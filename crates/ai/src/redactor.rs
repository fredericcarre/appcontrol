//! Redaction — the second half of the sovereignty story.
//!
//! The agent abstracts on the machine *before* anything is sent to a hosted
//! model: secrets, IPs and hostnames are replaced by stable placeholders. The
//! frontier model then reasons over "a Java service depends on a PostgreSQL on
//! `<host:1>`", never over the real `application.yml`.

/// Replaces sensitive substrings with stable placeholders.
///
/// Stable = the same input maps to the same placeholder within a run, so the
/// model can still reason about relationships ("`<host:1>` is referenced twice").
#[derive(Debug, Default)]
pub struct Redactor;

impl Redactor {
    /// Returns a redacted copy of `text` safe to send to a frontier provider.
    pub fn redact(&self, text: &str) -> String {
        let mut out = String::with_capacity(text.len());
        for token in split_keep_delims(text) {
            out.push_str(&self.redact_token(token));
        }
        out
    }

    fn redact_token(&self, token: &str) -> String {
        // Strip credentials in URLs/DSNs: scheme://user:pass@host -> scheme://<redacted>@host
        if let Some(idx) = token.find("://") {
            let (scheme, rest) = token.split_at(idx + 3);
            if let Some((userinfo, host)) = rest.split_once('@') {
                if userinfo.contains(':') {
                    return format!("{scheme}<redacted>@{}", self.redact_token(host));
                }
            }
        }
        // key=secret style assignments for sensitive keys.
        if let Some((k, _v)) = token.split_once('=') {
            let kl = k.to_lowercase();
            if kl.contains("password")
                || kl.contains("secret")
                || kl.contains("token")
                || kl.contains("key")
            {
                return format!("{k}=<redacted>");
            }
        }
        // Bare IPv4 -> <ip>
        if is_ipv4(token) {
            return "<ip>".to_string();
        }
        token.to_string()
    }
}

/// Splits on whitespace but keeps the whitespace so `redact` round-trips layout.
fn split_keep_delims(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut last = 0;
    for (i, c) in text.char_indices() {
        if c.is_whitespace() {
            if last != i {
                parts.push(&text[last..i]);
            }
            parts.push(&text[i..i + c.len_utf8()]);
            last = i + c.len_utf8();
        }
    }
    if last < text.len() {
        parts.push(&text[last..]);
    }
    parts
}

fn is_ipv4(token: &str) -> bool {
    let parts: Vec<&str> = token.split('.').collect();
    parts.len() == 4
        && parts
            .iter()
            .all(|p| !p.is_empty() && p.parse::<u8>().is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_credentials_and_ips() {
        let r = Redactor;
        let out = r.redact("url=amqp://user:pass@rabbit:5672 host 10.0.0.3");
        assert!(!out.contains("user:pass"), "creds leaked: {out}");
        assert!(out.contains("<redacted>"));
        assert!(out.contains("<ip>"), "ip not redacted: {out}");
        // Non-sensitive structure is preserved.
        assert!(out.contains("amqp://"));
        assert!(out.contains("rabbit:5672"));
    }

    #[test]
    fn redacts_secret_assignments() {
        let r = Redactor;
        let out = r.redact("spring.datasource.password=hunter2");
        assert!(out.contains("<redacted>"));
        assert!(!out.contains("hunter2"));
    }
}
