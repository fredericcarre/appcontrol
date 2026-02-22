//! Per-agent rate limiting for the gateway.
//!
//! Prevents a rogue or misconfigured agent from flooding the gateway/backend
//! with excessive messages. Uses a sliding window counter per agent_id.

use dashmap::DashMap;
use uuid::Uuid;

/// Maximum messages per agent per window. At 20 components × 30s interval
/// this allows ~40 CheckResults + 1 Heartbeat per minute = plenty of headroom.
const MAX_MESSAGES_PER_WINDOW: u32 = 200;

/// Window duration in seconds.
const WINDOW_SECS: i64 = 60;

struct AgentCounter {
    count: u32,
    window_start: chrono::DateTime<chrono::Utc>,
}

/// Tracks per-agent message rates and enforces limits.
pub struct AgentRateLimiter {
    counters: DashMap<Uuid, AgentCounter>,
}

impl AgentRateLimiter {
    pub fn new() -> Self {
        Self {
            counters: DashMap::new(),
        }
    }

    /// Check if the agent is within rate limits. Returns true if allowed.
    /// Increments the counter for the current window.
    pub fn check(&self, agent_id: Uuid) -> bool {
        let now = chrono::Utc::now();

        let mut entry = self.counters.entry(agent_id).or_insert_with(|| AgentCounter {
            count: 0,
            window_start: now,
        });

        let counter = entry.value_mut();

        // Reset window if expired
        if (now - counter.window_start).num_seconds() >= WINDOW_SECS {
            counter.count = 0;
            counter.window_start = now;
        }

        counter.count += 1;
        counter.count <= MAX_MESSAGES_PER_WINDOW
    }

    /// Remove counters for disconnected agents (call periodically).
    pub fn cleanup_stale(&self, active_agents: &[Uuid]) {
        let active_set: std::collections::HashSet<Uuid> = active_agents.iter().copied().collect();
        self.counters.retain(|id, _| active_set.contains(id));
    }
}

impl Default for AgentRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allows_within_limit() {
        let limiter = AgentRateLimiter::new();
        let agent = Uuid::new_v4();

        for _ in 0..MAX_MESSAGES_PER_WINDOW {
            assert!(limiter.check(agent));
        }
    }

    #[test]
    fn test_blocks_over_limit() {
        let limiter = AgentRateLimiter::new();
        let agent = Uuid::new_v4();

        for _ in 0..MAX_MESSAGES_PER_WINDOW {
            limiter.check(agent);
        }
        // Next one should be blocked
        assert!(!limiter.check(agent));
    }

    #[test]
    fn test_different_agents_independent() {
        let limiter = AgentRateLimiter::new();
        let agent_a = Uuid::new_v4();
        let agent_b = Uuid::new_v4();

        for _ in 0..MAX_MESSAGES_PER_WINDOW {
            limiter.check(agent_a);
        }
        // agent_a is full
        assert!(!limiter.check(agent_a));
        // agent_b should still have quota
        assert!(limiter.check(agent_b));
    }

    #[test]
    fn test_cleanup_removes_stale() {
        let limiter = AgentRateLimiter::new();
        let active = Uuid::new_v4();
        let stale = Uuid::new_v4();

        limiter.check(active);
        limiter.check(stale);

        limiter.cleanup_stale(&[active]);

        assert_eq!(limiter.counters.len(), 1);
        assert!(limiter.counters.contains_key(&active));
    }
}
