//! Data-sensitivity classification — the first half of the sovereignty story.
//!
//! Before any text is sent to a model, we classify how sensitive it is. The
//! router then decides whether it may go to a hosted frontier model or must stay
//! on a local one. Sovereignty is *primordiale*: secrets never leave.

use serde::{Deserialize, Serialize};

/// How sensitive a piece of context is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sensitivity {
    /// Generic, non-identifying text. May go anywhere.
    Public,
    /// Internal topology shapes (tech types, roles, ports). May go to frontier
    /// once redacted.
    Internal,
    /// Hostnames, IPs, file paths, usernames. Local by default.
    Sensitive,
    /// Credentials, tokens, connection strings, keys. **Never** leaves local.
    Secret,
}

/// Classifies text into a [`Sensitivity`] level using cheap heuristics.
///
/// Deliberately conservative: when in doubt it errs *upward* (more sensitive),
/// because the cost of leaking a secret to a hosted model is far higher than the
/// cost of routing to a local model.
#[derive(Debug, Default, Clone)]
pub struct SensitivityClassifier;

impl SensitivityClassifier {
    pub fn classify(&self, text: &str) -> Sensitivity {
        let lower = text.to_lowercase();

        // Secrets: anything that looks like a credential or a DSN with auth.
        const SECRET_MARKERS: &[&str] = &[
            "password",
            "passwd",
            "secret",
            "api_key",
            "apikey",
            "token",
            "private_key",
            "-----begin",
            "authorization:",
            "aws_secret",
            "client_secret",
        ];
        if SECRET_MARKERS.iter().any(|m| lower.contains(m)) {
            return Sensitivity::Secret;
        }
        // user:pass@host style auth embedded in a URL/DSN.
        if has_userinfo_credentials(text) {
            return Sensitivity::Secret;
        }

        // Sensitive: identifies real infrastructure.
        let looks_like_path = text.contains("/etc/")
            || text.contains("/opt/")
            || text.contains("/var/")
            || text.contains(":\\");
        let looks_like_ip = contains_ipv4(text);
        if looks_like_path || looks_like_ip {
            return Sensitivity::Sensitive;
        }

        // Internal: mentions technologies / ports but nothing identifying.
        const INTERNAL_MARKERS: &[&str] = &[
            "postgres", "mysql", "redis", "rabbitmq", "kafka", "nginx", "java", "oracle",
            "mongodb", ":80", ":443", ":5432", ":6379", "port",
        ];
        if INTERNAL_MARKERS.iter().any(|m| lower.contains(m)) {
            return Sensitivity::Internal;
        }

        Sensitivity::Public
    }
}

/// True if `text` contains a `scheme://user:pass@host` style credential.
fn has_userinfo_credentials(text: &str) -> bool {
    for token in text.split_whitespace() {
        if let Some(after_scheme) = token.split_once("://").map(|(_, rest)| rest) {
            if let Some((userinfo, _)) = after_scheme.split_once('@') {
                if userinfo.contains(':') {
                    return true;
                }
            }
        }
    }
    false
}

/// Crude IPv4 detection (no regex dependency): finds `a.b.c.d` with numeric octets.
fn contains_ipv4(text: &str) -> bool {
    text.split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .any(|chunk| {
            let parts: Vec<&str> = chunk.split('.').collect();
            parts.len() == 4
                && parts
                    .iter()
                    .all(|p| !p.is_empty() && p.parse::<u8>().is_ok())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_secrets() {
        let c = SensitivityClassifier;
        assert_eq!(
            c.classify("spring.datasource.password=hunter2"),
            Sensitivity::Secret
        );
        assert_eq!(
            c.classify("amqp://user:pass@rabbit:5672/vhost"),
            Sensitivity::Secret
        );
        assert_eq!(c.classify("Authorization: Bearer x"), Sensitivity::Secret);
    }

    #[test]
    fn detects_sensitive_infra() {
        let c = SensitivityClassifier;
        assert_eq!(c.classify("/opt/order-api/app.yml"), Sensitivity::Sensitive);
        assert_eq!(c.classify("connects to 10.0.0.3"), Sensitivity::Sensitive);
    }

    #[test]
    fn detects_internal_and_public() {
        let c = SensitivityClassifier;
        assert_eq!(
            c.classify("a postgres service on port 5432"),
            Sensitivity::Internal
        );
        assert_eq!(c.classify("just some words"), Sensitivity::Public);
    }
}
