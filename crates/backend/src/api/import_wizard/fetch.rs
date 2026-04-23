//! Fetch an import document from a remote URL (server-side).
//!
//! Exposes `POST /api/v1/import/fetch` taking `{ "url": "https://..." }`
//! and returning `{ content, format, source_url, size_bytes, content_type }`.
//!
//! Client code uses this as an alternative to pasting/uploading: the fetched
//! content feeds directly into `/import/preview` and `/import/execute`.
//!
//! Safety:
//! - Scheme allowlist: `http`, `https` only.
//! - Method is GET; redirects limited to 3 hops.
//! - Request timeout: 15s (connect + read).
//! - Max body size: 5 MiB (prevents memory exhaustion).
//! - SSRF guard: resolved IP addresses must be public unless
//!   `IMPORT_FETCH_ALLOW_PRIVATE=true` (set in on-prem/dev, default off).
//! - Only authenticated users can fetch.

use axum::{
    extract::{Extension, State},
    response::Json,
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::net::IpAddr;
use std::sync::Arc;
use std::time::Duration;

use crate::auth::AuthUser;
use crate::error::ApiError;
use crate::middleware::audit::log_action;
use crate::AppState;

const MAX_BODY_BYTES: usize = 5 * 1024 * 1024; // 5 MiB
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);
const MAX_REDIRECTS: usize = 3;

#[derive(Debug, Deserialize)]
pub struct FetchImportRequest {
    pub url: String,
    /// Optional format hint. When absent, derived from Content-Type or URL suffix.
    #[serde(default)]
    pub format: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FetchImportResponse {
    pub content: String,
    pub format: String,
    pub source_url: String,
    pub size_bytes: usize,
    pub content_type: Option<String>,
}

/// POST /api/v1/import/fetch
pub async fn fetch_import(
    State(state): State<Arc<AppState>>,
    Extension(user): Extension<AuthUser>,
    Json(body): Json<FetchImportRequest>,
) -> Result<Json<FetchImportResponse>, ApiError> {
    let parsed = reqwest::Url::parse(body.url.trim())
        .map_err(|e| ApiError::Validation(format!("invalid URL: {}", e)))?;

    // 1. Scheme allowlist
    match parsed.scheme() {
        "http" | "https" => {}
        other => {
            return Err(ApiError::Validation(format!(
                "unsupported URL scheme '{}': only http and https are allowed",
                other
            )));
        }
    }

    let host = parsed
        .host_str()
        .ok_or_else(|| ApiError::Validation("URL has no host".to_string()))?
        .to_string();

    // 2. SSRF guard: resolve host and reject private/loopback/link-local
    // addresses. Opt-out via env var for on-prem deployments where internal
    // URLs (intranet artifact stores) are the legitimate use case.
    let allow_private = std::env::var("IMPORT_FETCH_ALLOW_PRIVATE")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    if !allow_private {
        let port = parsed.port_or_known_default().unwrap_or(443);
        let sock_addrs = tokio::net::lookup_host((host.as_str(), port))
            .await
            .map_err(|e| ApiError::Validation(format!("DNS lookup failed for host: {}", e)))?;
        let ips: Vec<IpAddr> = sock_addrs.map(|sa| sa.ip()).collect();
        if ips.is_empty() {
            return Err(ApiError::Validation(
                "host resolved to no IP addresses".to_string(),
            ));
        }
        for ip in &ips {
            if is_private_or_unsafe(ip) {
                return Err(ApiError::Validation(format!(
                    "URL resolves to a private/loopback IP ({}); set IMPORT_FETCH_ALLOW_PRIVATE=true to permit internal URLs",
                    ip
                )));
            }
        }
    }

    log_action(
        &state.db,
        user.user_id,
        "fetch_import",
        "application",
        uuid::Uuid::nil(),
        json!({ "url": body.url }),
    )
    .await
    .ok();

    // 3. HTTP fetch
    let client = reqwest::Client::builder()
        .timeout(REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::limited(MAX_REDIRECTS))
        .user_agent(concat!("AppControl/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|e| ApiError::Internal(format!("http client build: {}", e)))?;

    let resp = client
        .get(parsed.clone())
        .send()
        .await
        .map_err(|e| ApiError::Validation(format!("fetch failed: {}", e)))?;

    let status = resp.status();
    if !status.is_success() {
        return Err(ApiError::Validation(format!(
            "remote returned HTTP {}",
            status.as_u16()
        )));
    }

    let content_type = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    // 4. Cap body size: read chunk-by-chunk so we can abort before OOM.
    // Short-circuit on Content-Length first so we don't even start downloading
    // obviously oversized responses.
    if let Some(len) = resp.content_length() {
        if len > MAX_BODY_BYTES as u64 {
            return Err(ApiError::Validation(format!(
                "remote body ({} bytes) exceeds {} MiB limit",
                len,
                MAX_BODY_BYTES / (1024 * 1024)
            )));
        }
    }
    let mut resp = resp;
    let mut buf: Vec<u8> = Vec::with_capacity(64 * 1024);
    loop {
        let chunk = resp
            .chunk()
            .await
            .map_err(|e| ApiError::Validation(format!("body read error: {}", e)))?;
        let Some(chunk) = chunk else { break };
        if buf.len() + chunk.len() > MAX_BODY_BYTES {
            return Err(ApiError::Validation(format!(
                "remote body exceeds {} MiB limit",
                MAX_BODY_BYTES / (1024 * 1024)
            )));
        }
        buf.extend_from_slice(&chunk);
    }

    let content = String::from_utf8(buf)
        .map_err(|_| ApiError::Validation("remote body is not valid UTF-8".to_string()))?;

    // 5. Format detection: explicit hint > Content-Type > URL path suffix > JSON default.
    let format = body
        .format
        .map(|f| f.to_lowercase())
        .or_else(|| detect_format_from_content_type(content_type.as_deref()))
        .or_else(|| detect_format_from_url(&parsed))
        .unwrap_or_else(|| "json".to_string());

    Ok(Json(FetchImportResponse {
        size_bytes: content.len(),
        content,
        format,
        source_url: parsed.into(),
        content_type,
    }))
}

fn detect_format_from_content_type(ct: Option<&str>) -> Option<String> {
    let ct = ct?.to_ascii_lowercase();
    if ct.contains("yaml") || ct.contains("yml") {
        Some("yaml".to_string())
    } else if ct.contains("json") {
        Some("json".to_string())
    } else {
        None
    }
}

fn detect_format_from_url(url: &reqwest::Url) -> Option<String> {
    let path = url.path().to_lowercase();
    if path.ends_with(".yaml") || path.ends_with(".yml") {
        Some("yaml".to_string())
    } else if path.ends_with(".json") {
        Some("json".to_string())
    } else {
        None
    }
}

/// Returns true when the address must not be contacted by default:
/// loopback, private RFC1918/RFC4193 space, link-local, unspecified,
/// documentation/test ranges. Covers both IPv4 and IPv6.
fn is_private_or_unsafe(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                || v4.is_documentation()
                // 100.64.0.0/10 (carrier-grade NAT)
                || (v4.octets()[0] == 100 && (v4.octets()[1] & 0xC0) == 0x40)
        }
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                // Unique Local Address (fc00::/7)
                || (v6.segments()[0] & 0xfe00) == 0xfc00
                // Link-local (fe80::/10)
                || (v6.segments()[0] & 0xffc0) == 0xfe80
                // IPv4-mapped IPv6 covering private v4 ranges
                || v6.to_ipv4_mapped().is_some_and(|m| {
                    m.is_loopback() || m.is_private() || m.is_link_local() || m.is_unspecified()
                })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detect_format_from_content_type_json() {
        assert_eq!(
            detect_format_from_content_type(Some("application/json; charset=utf-8")),
            Some("json".to_string())
        );
    }

    #[test]
    fn detect_format_from_content_type_yaml() {
        assert_eq!(
            detect_format_from_content_type(Some("text/yaml")),
            Some("yaml".to_string())
        );
        assert_eq!(
            detect_format_from_content_type(Some("application/x-yaml")),
            Some("yaml".to_string())
        );
    }

    #[test]
    fn detect_format_from_content_type_none() {
        assert_eq!(detect_format_from_content_type(Some("text/plain")), None);
        assert_eq!(detect_format_from_content_type(None), None);
    }

    #[test]
    fn detect_format_from_url_extension() {
        let json = reqwest::Url::parse("https://ex.com/foo/bar.json").unwrap();
        assert_eq!(detect_format_from_url(&json), Some("json".to_string()));
        let yaml = reqwest::Url::parse("https://ex.com/maps/app.yaml").unwrap();
        assert_eq!(detect_format_from_url(&yaml), Some("yaml".to_string()));
        let yml = reqwest::Url::parse("https://ex.com/maps/app.yml").unwrap();
        assert_eq!(detect_format_from_url(&yml), Some("yaml".to_string()));
        let unknown = reqwest::Url::parse("https://ex.com/foo").unwrap();
        assert_eq!(detect_format_from_url(&unknown), None);
    }

    #[test]
    fn ssrf_blocks_loopback_v4() {
        assert!(is_private_or_unsafe(&"127.0.0.1".parse().unwrap()));
        assert!(is_private_or_unsafe(&"127.5.5.5".parse().unwrap()));
    }

    #[test]
    fn ssrf_blocks_rfc1918_v4() {
        assert!(is_private_or_unsafe(&"10.0.0.1".parse().unwrap()));
        assert!(is_private_or_unsafe(&"172.16.0.1".parse().unwrap()));
        assert!(is_private_or_unsafe(&"172.31.255.255".parse().unwrap()));
        assert!(is_private_or_unsafe(&"192.168.1.1".parse().unwrap()));
    }

    #[test]
    fn ssrf_blocks_link_local_v4() {
        assert!(is_private_or_unsafe(&"169.254.169.254".parse().unwrap()));
    }

    #[test]
    fn ssrf_blocks_cgnat() {
        assert!(is_private_or_unsafe(&"100.64.0.1".parse().unwrap()));
        assert!(is_private_or_unsafe(&"100.127.255.255".parse().unwrap()));
        // 100.63 is not CGNAT
        assert!(!is_private_or_unsafe(&"100.63.0.1".parse().unwrap()));
    }

    #[test]
    fn ssrf_allows_public_v4() {
        assert!(!is_private_or_unsafe(&"8.8.8.8".parse().unwrap()));
        assert!(!is_private_or_unsafe(&"1.1.1.1".parse().unwrap()));
    }

    #[test]
    fn ssrf_blocks_ipv6_loopback_and_ula() {
        assert!(is_private_or_unsafe(&"::1".parse().unwrap()));
        assert!(is_private_or_unsafe(&"fc00::1".parse().unwrap()));
        assert!(is_private_or_unsafe(&"fd12::3456".parse().unwrap()));
        assert!(is_private_or_unsafe(&"fe80::1".parse().unwrap()));
    }

    #[test]
    fn ssrf_allows_public_ipv6() {
        assert!(!is_private_or_unsafe(
            &"2606:4700:4700::1111".parse().unwrap()
        ));
    }

    #[test]
    fn ssrf_blocks_ipv4_mapped_private_v6() {
        assert!(is_private_or_unsafe(&"::ffff:10.0.0.1".parse().unwrap()));
        assert!(is_private_or_unsafe(&"::ffff:127.0.0.1".parse().unwrap()));
    }
}
