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
    #[error("Certificate generation error: {0}")]
    Generation(String),
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
    pub fn new(
        cert_path: &str,
        key_path: &str,
        ca_path: &str,
        verify_clients: bool,
    ) -> Result<Self, PkiError> {
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
    pub fn new_unchecked(
        cert_path: &str,
        key_path: &str,
        ca_path: &str,
        verify_clients: bool,
    ) -> Self {
        Self {
            cert_path: cert_path.to_string(),
            key_path: key_path.to_string(),
            ca_path: ca_path.to_string(),
            verify_clients,
        }
    }
}

// ---------------------------------------------------------------------------
// Certificate generation (rcgen-based)
// ---------------------------------------------------------------------------

/// Output of CA generation: PEM-encoded certificate and private key.
#[derive(Debug, Clone)]
pub struct CaBundle {
    pub cert_pem: String,
    pub key_pem: String,
}

/// Output of certificate issuance: PEM-encoded cert, key, and CA chain.
#[derive(Debug, Clone)]
pub struct IssuedCert {
    pub cert_pem: String,
    pub key_pem: String,
    pub ca_pem: String,
}

/// Generate a self-signed CA certificate for AppControl.
///
/// The CA is valid for `validity_days` (default 3650 = 10 years).
/// This CA will sign all gateway and agent certificates.
pub fn generate_ca(org_name: &str, validity_days: u32) -> Result<CaBundle, PkiError> {
    use rcgen::{BasicConstraints, CertificateParams, IsCa, KeyPair, KeyUsagePurpose};
    use time::{Duration, OffsetDateTime};

    let mut params = CertificateParams::default();
    params
        .distinguished_name
        .push(rcgen::DnType::OrganizationName, org_name);
    params.distinguished_name.push(
        rcgen::DnType::CommonName,
        format!("{} AppControl CA", org_name),
    );
    params.is_ca = IsCa::Ca(BasicConstraints::Unconstrained);
    params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    params.not_before = OffsetDateTime::now_utc();
    params.not_after = OffsetDateTime::now_utc() + Duration::days(validity_days as i64);

    let key_pair = KeyPair::generate()
        .map_err(|e| PkiError::Generation(format!("CA key generation failed: {}", e)))?;
    let cert = params
        .self_signed(&key_pair)
        .map_err(|e| PkiError::Generation(format!("CA cert generation failed: {}", e)))?;

    Ok(CaBundle {
        cert_pem: cert.pem(),
        key_pem: key_pair.serialize_pem(),
    })
}

/// Issue a server certificate (for the gateway) signed by the CA.
///
/// `cn` is typically the gateway hostname or a wildcard.
/// `san_dns` lists additional DNS SANs (e.g., `["gateway.example.com", "localhost"]`).
/// `san_ips` lists IP SANs (e.g., `["127.0.0.1", "10.0.1.5"]`).
pub fn issue_gateway_cert(
    ca_cert_pem: &str,
    ca_key_pem: &str,
    cn: &str,
    san_dns: &[String],
    san_ips: &[String],
    validity_days: u32,
) -> Result<IssuedCert, PkiError> {
    use rcgen::{CertificateParams, ExtendedKeyUsagePurpose, KeyPair, KeyUsagePurpose, SanType};
    use time::{Duration, OffsetDateTime};

    let ca_key = KeyPair::from_pem(ca_key_pem)
        .map_err(|e| PkiError::Generation(format!("Failed to parse CA key: {}", e)))?;
    let ca_params = CertificateParams::from_ca_cert_pem(ca_cert_pem)
        .map_err(|e| PkiError::Generation(format!("Failed to parse CA cert: {}", e)))?;
    let ca_cert = ca_params
        .self_signed(&ca_key)
        .map_err(|e| PkiError::Generation(format!("Failed to rebuild CA cert: {}", e)))?;

    let mut params = CertificateParams::default();
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, cn);
    params.key_usages = vec![
        KeyUsagePurpose::DigitalSignature,
        KeyUsagePurpose::KeyEncipherment,
    ];
    params.extended_key_usages = vec![
        ExtendedKeyUsagePurpose::ServerAuth,
        ExtendedKeyUsagePurpose::ClientAuth,
    ];
    params.not_before = OffsetDateTime::now_utc();
    params.not_after = OffsetDateTime::now_utc() + Duration::days(validity_days as i64);

    // Add SANs
    let mut sans = vec![SanType::DnsName(cn.try_into().map_err(|e| {
        PkiError::Generation(format!("Invalid CN as DNS name: {}", e))
    })?)];
    for dns in san_dns {
        if dns != cn {
            sans.push(SanType::DnsName(dns.as_str().try_into().map_err(|e| {
                PkiError::Generation(format!("Invalid SAN DNS name '{}': {}", dns, e))
            })?));
        }
    }
    for ip_str in san_ips {
        let ip: std::net::IpAddr = ip_str
            .parse()
            .map_err(|e| PkiError::Generation(format!("Invalid SAN IP '{}': {}", ip_str, e)))?;
        sans.push(SanType::IpAddress(ip));
    }
    params.subject_alt_names = sans;

    let key_pair = KeyPair::generate()
        .map_err(|e| PkiError::Generation(format!("Gateway key generation failed: {}", e)))?;
    let cert = params
        .signed_by(&key_pair, &ca_cert, &ca_key)
        .map_err(|e| PkiError::Generation(format!("Gateway cert signing failed: {}", e)))?;

    Ok(IssuedCert {
        cert_pem: cert.pem(),
        key_pem: key_pair.serialize_pem(),
        ca_pem: ca_cert_pem.to_string(),
    })
}

/// Issue a client certificate (for an agent) signed by the CA.
///
/// `hostname` is set as both the CN and a DNS SAN.
pub fn issue_agent_cert(
    ca_cert_pem: &str,
    ca_key_pem: &str,
    hostname: &str,
    validity_days: u32,
) -> Result<IssuedCert, PkiError> {
    use rcgen::{CertificateParams, ExtendedKeyUsagePurpose, KeyPair, KeyUsagePurpose, SanType};
    use time::{Duration, OffsetDateTime};

    let ca_key = KeyPair::from_pem(ca_key_pem)
        .map_err(|e| PkiError::Generation(format!("Failed to parse CA key: {}", e)))?;
    let ca_params = CertificateParams::from_ca_cert_pem(ca_cert_pem)
        .map_err(|e| PkiError::Generation(format!("Failed to parse CA cert: {}", e)))?;
    let ca_cert = ca_params
        .self_signed(&ca_key)
        .map_err(|e| PkiError::Generation(format!("Failed to rebuild CA cert: {}", e)))?;

    let mut params = CertificateParams::default();
    params
        .distinguished_name
        .push(rcgen::DnType::CommonName, hostname);
    params.key_usages = vec![KeyUsagePurpose::DigitalSignature];
    params.extended_key_usages = vec![ExtendedKeyUsagePurpose::ClientAuth];
    params.not_before = OffsetDateTime::now_utc();
    params.not_after = OffsetDateTime::now_utc() + Duration::days(validity_days as i64);
    params.subject_alt_names =
        vec![SanType::DnsName(hostname.try_into().map_err(|e| {
            PkiError::Generation(format!("Invalid hostname as DNS name: {}", e))
        })?)];

    let key_pair = KeyPair::generate()
        .map_err(|e| PkiError::Generation(format!("Agent key generation failed: {}", e)))?;
    let cert = params
        .signed_by(&key_pair, &ca_cert, &ca_key)
        .map_err(|e| PkiError::Generation(format!("Agent cert signing failed: {}", e)))?;

    Ok(IssuedCert {
        cert_pem: cert.pem(),
        key_pem: key_pair.serialize_pem(),
        ca_pem: ca_cert_pem.to_string(),
    })
}

/// Compute SHA-256 fingerprint of a PEM certificate string.
pub fn fingerprint_pem(cert_pem: &str) -> Option<String> {
    use sha2::Digest;

    // Extract the DER bytes from PEM
    let pem_lines: Vec<&str> = cert_pem.lines().collect();
    let b64: String = pem_lines
        .iter()
        .filter(|l| !l.starts_with("-----"))
        .copied()
        .collect();
    let der = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, &b64).ok()?;
    let digest = sha2::Sha256::digest(&der);
    Some(hex::encode(digest))
}

/// Validate that a CA certificate and private key are a valid keypair.
///
/// This is used when importing an external CA to ensure:
/// 1. The certificate can be parsed
/// 2. The private key can be parsed
/// 3. They form a valid pair (can sign certificates)
pub fn validate_ca_keypair(ca_cert_pem: &str, ca_key_pem: &str) -> Result<(), PkiError> {
    use rcgen::{CertificateParams, KeyPair};

    // Parse the private key
    let ca_key = KeyPair::from_pem(ca_key_pem)
        .map_err(|e| PkiError::Generation(format!("Invalid CA private key: {}", e)))?;

    // Parse the certificate and verify it works with the key
    let ca_params = CertificateParams::from_ca_cert_pem(ca_cert_pem)
        .map_err(|e| PkiError::Generation(format!("Invalid CA certificate: {}", e)))?;

    // Try to rebuild the cert with the key - this validates they match
    ca_params
        .self_signed(&ca_key)
        .map_err(|e| PkiError::Generation(format!("CA cert and key do not match: {}", e)))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Enrollment tokens
// ---------------------------------------------------------------------------

/// Generate a cryptographically random enrollment token.
///
/// Format: `ac_enroll_<32 random hex chars>` (easy to identify, easy to paste).
pub fn generate_enrollment_token() -> String {
    use rand::Rng;
    let mut rng = rand::thread_rng();
    let bytes: [u8; 16] = rng.gen();
    format!("ac_enroll_{}", hex::encode(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tls_config_missing_cert() {
        let result = TlsConfig::new(
            "/nonexistent/cert.pem",
            "/nonexistent/key.pem",
            "/nonexistent/ca.pem",
            true,
        );
        assert!(matches!(result, Err(PkiError::CertNotFound(_))));
    }

    #[test]
    fn test_tls_config_unchecked() {
        let config =
            TlsConfig::new_unchecked("/some/cert.pem", "/some/key.pem", "/some/ca.pem", false);
        assert_eq!(config.cert_path, "/some/cert.pem");
        assert!(!config.verify_clients);
    }

    #[test]
    fn test_generate_ca() {
        let ca = generate_ca("TestOrg", 365).unwrap();
        assert!(ca.cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(ca.key_pem.contains("BEGIN PRIVATE KEY"));
    }

    #[test]
    fn test_issue_gateway_cert() {
        let ca = generate_ca("TestOrg", 365).unwrap();
        let gw = issue_gateway_cert(
            &ca.cert_pem,
            &ca.key_pem,
            "gateway.example.com",
            &["localhost".to_string()],
            &["127.0.0.1".to_string()],
            365,
        )
        .unwrap();
        assert!(gw.cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(gw.key_pem.contains("BEGIN PRIVATE KEY"));
        assert_eq!(gw.ca_pem, ca.cert_pem);
    }

    #[test]
    fn test_issue_agent_cert() {
        let ca = generate_ca("TestOrg", 365).unwrap();
        let agent = issue_agent_cert(&ca.cert_pem, &ca.key_pem, "server01.prod", 365).unwrap();
        assert!(agent.cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(agent.key_pem.contains("BEGIN PRIVATE KEY"));
    }

    #[test]
    fn test_fingerprint_pem() {
        let ca = generate_ca("TestOrg", 365).unwrap();
        let fp = fingerprint_pem(&ca.cert_pem);
        assert!(fp.is_some());
        let fp = fp.unwrap();
        // SHA-256 = 64 hex chars
        assert_eq!(fp.len(), 64);
    }

    #[test]
    fn test_generate_enrollment_token() {
        let token = generate_enrollment_token();
        assert!(token.starts_with("ac_enroll_"));
        assert_eq!(token.len(), "ac_enroll_".len() + 32);

        // Tokens should be unique
        let token2 = generate_enrollment_token();
        assert_ne!(token, token2);
    }

    #[test]
    fn test_gateway_cert_with_ip_san() {
        let ca = generate_ca("TestOrg", 365).unwrap();
        let gw = issue_gateway_cert(
            &ca.cert_pem,
            &ca.key_pem,
            "gw.local",
            &[],
            &["10.0.1.5".to_string(), "::1".to_string()],
            365,
        )
        .unwrap();
        assert!(gw.cert_pem.contains("BEGIN CERTIFICATE"));
    }

    #[test]
    fn test_full_chain_ca_gateway_agent() {
        // Generate CA
        let ca = generate_ca("Acme Corp", 3650).unwrap();

        // Issue gateway cert
        let gw = issue_gateway_cert(
            &ca.cert_pem,
            &ca.key_pem,
            "gateway.acme.com",
            &["gw.acme.com".to_string()],
            &["10.0.0.1".to_string()],
            365,
        )
        .unwrap();

        // Issue two agent certs
        let agent1 = issue_agent_cert(&ca.cert_pem, &ca.key_pem, "web01.acme.com", 365).unwrap();
        let agent2 = issue_agent_cert(&ca.cert_pem, &ca.key_pem, "db01.acme.com", 365).unwrap();

        // All certs are different
        assert_ne!(gw.cert_pem, agent1.cert_pem);
        assert_ne!(agent1.cert_pem, agent2.cert_pem);

        // All share the same CA
        assert_eq!(gw.ca_pem, ca.cert_pem);
        assert_eq!(agent1.ca_pem, ca.cert_pem);
        assert_eq!(agent2.ca_pem, ca.cert_pem);

        // All have unique fingerprints
        let fp_gw = fingerprint_pem(&gw.cert_pem).unwrap();
        let fp_a1 = fingerprint_pem(&agent1.cert_pem).unwrap();
        let fp_a2 = fingerprint_pem(&agent2.cert_pem).unwrap();
        assert_ne!(fp_gw, fp_a1);
        assert_ne!(fp_a1, fp_a2);
    }
}
