use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PkiError {
    #[error("Certificate file not found: {0}")]
    CertNotFound(String),
    #[error("Key file not found: {0}")]
    KeyNotFound(String),
    #[error("CA file not found: {0}")]
    CaNotFound(String),
    #[error("TLS configuration error: {0}")]
    TlsConfig(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// TLS configuration for mTLS connections.
#[derive(Debug, Clone)]
pub struct TlsConfig {
    pub cert_path: String,
    pub key_path: String,
    pub ca_path: String,
    pub verify_clients: bool,
}

impl TlsConfig {
    /// Create a new TLS config, validating that files exist.
    pub fn new(cert_path: &str, key_path: &str, ca_path: &str, verify_clients: bool) -> Result<Self, PkiError> {
        if !Path::new(cert_path).exists() {
            return Err(PkiError::CertNotFound(cert_path.to_string()));
        }
        if !Path::new(key_path).exists() {
            return Err(PkiError::KeyNotFound(key_path.to_string()));
        }
        if !Path::new(ca_path).exists() {
            return Err(PkiError::CaNotFound(ca_path.to_string()));
        }
        Ok(Self {
            cert_path: cert_path.to_string(),
            key_path: key_path.to_string(),
            ca_path: ca_path.to_string(),
            verify_clients,
        })
    }

    /// Create a config without file existence validation (for testing or deferred loading).
    pub fn new_unchecked(cert_path: &str, key_path: &str, ca_path: &str, verify_clients: bool) -> Self {
        Self {
            cert_path: cert_path.to_string(),
            key_path: key_path.to_string(),
            ca_path: ca_path.to_string(),
            verify_clients,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tls_config_missing_cert() {
        let result = TlsConfig::new("/nonexistent/cert.pem", "/nonexistent/key.pem", "/nonexistent/ca.pem", true);
        assert!(matches!(result, Err(PkiError::CertNotFound(_))));
    }

    #[test]
    fn test_tls_config_unchecked() {
        let config = TlsConfig::new_unchecked("/some/cert.pem", "/some/key.pem", "/some/ca.pem", false);
        assert_eq!(config.cert_path, "/some/cert.pem");
        assert!(!config.verify_clients);
    }
}
