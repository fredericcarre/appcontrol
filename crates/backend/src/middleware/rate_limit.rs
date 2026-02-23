use axum::{
    extract::{ConnectInfo, Request, State},
    http::StatusCode,
    middleware::Next,
    response::Response,
};
use dashmap::DashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

use crate::AppState;

/// Tracks request counts per key within a sliding window.
/// In-memory implementation — used as fallback when Redis is unavailable.
pub struct RateLimiter {
    /// Key → (window_start, count)
    entries: DashMap<String, (Instant, u32)>,
    window_secs: u64,
}

impl RateLimiter {
    pub fn new(window_secs: u64) -> Self {
        Self {
            entries: DashMap::new(),
            window_secs,
        }
    }

    /// Check if the key is within its rate limit. Returns true if allowed.
    pub fn check(&self, key: &str, max_requests: u32) -> bool {
        let now = Instant::now();
        let mut entry = self.entries.entry(key.to_string()).or_insert((now, 0));

        let (window_start, count) = entry.value_mut();

        // Reset window if expired
        if now.duration_since(*window_start).as_secs() >= self.window_secs {
            *window_start = now;
            *count = 1;
            return true;
        }

        if *count >= max_requests {
            return false;
        }

        *count += 1;
        true
    }

    /// Periodic cleanup of stale entries (call from a background task).
    pub fn cleanup(&self) {
        let now = Instant::now();
        self.entries
            .retain(|_, (start, _)| now.duration_since(*start).as_secs() < self.window_secs * 2);
    }
}

/// Rate limiter state shared across the application.
/// Uses Redis when available (HA-safe across multiple backend replicas),
/// falls back to in-memory DashMap when Redis is not configured.
pub struct RateLimitState {
    /// Per-IP limiter for auth endpoints
    pub auth: RateLimiter,
    /// Per-user limiter for write/operation endpoints
    pub operations: RateLimiter,
    /// Per-user limiter for read endpoints
    pub reads: RateLimiter,
}

impl RateLimitState {
    pub fn new() -> Self {
        Self {
            auth: RateLimiter::new(60),       // 1-minute window
            operations: RateLimiter::new(60), // 1-minute window
            reads: RateLimiter::new(60),      // 1-minute window
        }
    }
}

impl Default for RateLimitState {
    fn default() -> Self {
        Self::new()
    }
}

/// Check rate limit using Redis (HA-safe) if available, otherwise fall back to in-memory.
/// Uses a simple INCR + EXPIRE pattern with a fixed-window approach.
async fn check_rate_limit_with_redis(
    redis: &Option<redis::aio::ConnectionManager>,
    fallback: &RateLimiter,
    key: &str,
    max_requests: u32,
    window_secs: u64,
) -> bool {
    if let Some(redis) = redis {
        let redis_key = format!("rl:{}", key);
        let mut conn = redis.clone();

        // INCR + conditional EXPIRE in a pipeline
        let result: Result<(i64,), _> = redis::pipe()
            .atomic()
            .cmd("INCR")
            .arg(&redis_key)
            .cmd("EXPIRE")
            .arg(&redis_key)
            .arg(window_secs as i64)
            .ignore()
            .query_async(&mut conn)
            .await;

        match result {
            Ok((count,)) => {
                return count <= max_requests as i64;
            }
            Err(e) => {
                tracing::debug!("Redis rate limit check failed, falling back to in-memory: {}", e);
                // Fall through to in-memory
            }
        }
    }

    fallback.check(key, max_requests)
}

/// Rate limiting middleware for auth endpoints (keyed by IP).
pub async fn rate_limit_auth(
    State(state): State<Arc<AppState>>,
    connect_info: Option<ConnectInfo<SocketAddr>>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let ip = connect_info
        .map(|ci| ci.0.ip().to_string())
        .unwrap_or_else(|| "unknown".to_string());

    let key = format!("auth:{}", ip);
    if !check_rate_limit_with_redis(
        &state.redis,
        &state.rate_limiter.auth,
        &key,
        state.config.rate_limit_auth,
        60,
    )
    .await
    {
        tracing::warn!(ip = %ip, "Auth rate limit exceeded");
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    Ok(next.run(request).await)
}

/// Rate limiting middleware for operation endpoints (keyed by user ID).
pub async fn rate_limit_operations(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    // Extract user_id from request extensions (set by auth middleware)
    let user = request
        .extensions()
        .get::<crate::auth::AuthUser>()
        .map(|u| u.user_id.to_string())
        .unwrap_or_else(|| "anonymous".to_string());

    let key = format!("ops:{}", user);
    if !check_rate_limit_with_redis(
        &state.redis,
        &state.rate_limiter.operations,
        &key,
        state.config.rate_limit_operations,
        60,
    )
    .await
    {
        tracing::warn!(user = %user, "Operations rate limit exceeded");
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    Ok(next.run(request).await)
}

/// Rate limiting middleware for read endpoints (keyed by user ID).
pub async fn rate_limit_reads(
    State(state): State<Arc<AppState>>,
    request: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let user = request
        .extensions()
        .get::<crate::auth::AuthUser>()
        .map(|u| u.user_id.to_string())
        .unwrap_or_else(|| "anonymous".to_string());

    let key = format!("read:{}", user);
    if !check_rate_limit_with_redis(
        &state.redis,
        &state.rate_limiter.reads,
        &key,
        state.config.rate_limit_reads,
        60,
    )
    .await
    {
        tracing::warn!(user = %user, "Read rate limit exceeded");
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rate_limiter_allows_within_limit() {
        let limiter = RateLimiter::new(60);
        for _ in 0..10 {
            assert!(limiter.check("test-key", 10));
        }
        // 11th request should be denied
        assert!(!limiter.check("test-key", 10));
    }

    #[test]
    fn test_rate_limiter_different_keys_independent() {
        let limiter = RateLimiter::new(60);
        for _ in 0..5 {
            assert!(limiter.check("key-a", 5));
        }
        assert!(!limiter.check("key-a", 5));
        // key-b should still be allowed
        assert!(limiter.check("key-b", 5));
    }

    #[test]
    fn test_rate_limiter_cleanup() {
        let limiter = RateLimiter::new(0); // 0-second window = immediately expired
        limiter.check("test", 10);
        limiter.cleanup();
        // After cleanup, entries should be removed
        assert_eq!(limiter.entries.len(), 0);
    }

    #[tokio::test]
    async fn test_redis_fallback_when_none() {
        let limiter = RateLimiter::new(60);
        // When Redis is None, should use in-memory limiter
        let result = check_rate_limit_with_redis(&None, &limiter, "test-key", 5, 60).await;
        assert!(result);
    }
}
