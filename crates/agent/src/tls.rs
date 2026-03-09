use sha2::Digest;
use std::io::BufReader;
use std::sync::Arc;

use crate::config::TlsSection;

/// Build a `tokio_rustls::TlsConnector` from the agent's TLS configuration.
///
/// This connector presents the agent's client certificate to the gateway (mTLS)
/// and validates the gateway's certificate against the configured CA.
pub fn build_tls_connector(tls: &TlsSection) -> anyhow::Result<tokio_rustls::TlsConnector> {
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};

    // Load CA certificates for server verification
    let ca_path = tls
        .ca_file
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("TLS enabled but ca_file not configured"))?;
    let ca_data = std::fs::read(ca_path)
        .map_err(|e| anyhow::anyhow!("Failed to read CA file {}: {}", ca_path, e))?;
    let mut ca_reader = BufReader::new(ca_data.as_slice());

    let mut root_store = rustls::RootCertStore::empty();
    for cert in rustls_pemfile::certs(&mut ca_reader) {
        let cert = cert.map_err(|e| anyhow::anyhow!("Failed to parse CA cert: {}", e))?;
        root_store
            .add(cert)
            .map_err(|e| anyhow::anyhow!("Failed to add CA cert to root store: {}", e))?;
    }

    // Load client certificate chain
    let cert_path = tls
        .cert_file
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("TLS enabled but cert_file not configured"))?;
    let cert_data = std::fs::read(cert_path)
        .map_err(|e| anyhow::anyhow!("Failed to read cert file {}: {}", cert_path, e))?;
    let mut cert_reader = BufReader::new(cert_data.as_slice());
    let client_certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut cert_reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| anyhow::anyhow!("Failed to parse client cert: {}", e))?;

    if client_certs.is_empty() {
        return Err(anyhow::anyhow!(
            "No certificates found in cert file: {}",
            cert_path
        ));
    }

    // Load client private key
    let key_path = tls
        .key_file
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("TLS enabled but key_file not configured"))?;
    let key_data = std::fs::read(key_path)
        .map_err(|e| anyhow::anyhow!("Failed to read key file {}: {}", key_path, e))?;
    let mut key_reader = BufReader::new(key_data.as_slice());

    let client_key: PrivateKeyDer<'static> = rustls_pemfile::private_key(&mut key_reader)
        .map_err(|e| anyhow::anyhow!("Failed to parse private key: {}", e))?
        .ok_or_else(|| anyhow::anyhow!("No private key found in key file: {}", key_path))?;

    // Build rustls ClientConfig with mTLS (client cert + CA verification)
    let config = rustls::ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_client_auth_cert(client_certs, client_key)
        .map_err(|e| anyhow::anyhow!("Failed to build TLS config with client cert: {}", e))?;

    Ok(tokio_rustls::TlsConnector::from(Arc::new(config)))
}

/// Certificate verifier that accepts any certificate (INSECURE).
/// Only use for development/testing with self-signed certificates.
#[derive(Debug)]
struct InsecureCertVerifier;

impl rustls::client::danger::ServerCertVerifier for InsecureCertVerifier {
    fn verify_server_cert(
        &self,
        _end_entity: &rustls::pki_types::CertificateDer<'_>,
        _intermediates: &[rustls::pki_types::CertificateDer<'_>],
        _server_name: &rustls::pki_types::ServerName<'_>,
        _ocsp_response: &[u8],
        _now: rustls::pki_types::UnixTime,
    ) -> Result<rustls::client::danger::ServerCertVerified, rustls::Error> {
        Ok(rustls::client::danger::ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &rustls::pki_types::CertificateDer<'_>,
        _dss: &rustls::DigitallySignedStruct,
    ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
        Ok(rustls::client::danger::HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
        vec![
            rustls::SignatureScheme::RSA_PKCS1_SHA256,
            rustls::SignatureScheme::RSA_PKCS1_SHA384,
            rustls::SignatureScheme::RSA_PKCS1_SHA512,
            rustls::SignatureScheme::ECDSA_NISTP256_SHA256,
            rustls::SignatureScheme::ECDSA_NISTP384_SHA384,
            rustls::SignatureScheme::ECDSA_NISTP521_SHA512,
            rustls::SignatureScheme::RSA_PSS_SHA256,
            rustls::SignatureScheme::RSA_PSS_SHA384,
            rustls::SignatureScheme::RSA_PSS_SHA512,
            rustls::SignatureScheme::ED25519,
        ]
    }
}

/// Build a TLS connector that presents client certificate (mTLS) but skips server
/// certificate verification. Use for self-signed gateway certificates in dev/containers.
/// WARNING: This is INSECURE and should not be used in production with untrusted networks.
pub fn build_tls_connector_insecure(
    tls: &TlsSection,
) -> anyhow::Result<tokio_rustls::TlsConnector> {
    use rustls::pki_types::{CertificateDer, PrivateKeyDer};

    // Load client certificate chain
    let cert_path = tls
        .cert_file
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("TLS enabled but cert_file not configured"))?;
    let cert_data = std::fs::read(cert_path)
        .map_err(|e| anyhow::anyhow!("Failed to read cert file {}: {}", cert_path, e))?;
    let mut cert_reader = BufReader::new(cert_data.as_slice());
    let client_certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut cert_reader)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| anyhow::anyhow!("Failed to parse client cert: {}", e))?;

    if client_certs.is_empty() {
        return Err(anyhow::anyhow!(
            "No certificates found in cert file: {}",
            cert_path
        ));
    }

    // Load client private key
    let key_path = tls
        .key_file
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("TLS enabled but key_file not configured"))?;
    let key_data = std::fs::read(key_path)
        .map_err(|e| anyhow::anyhow!("Failed to read key file {}: {}", key_path, e))?;
    let mut key_reader = BufReader::new(key_data.as_slice());

    let client_key: PrivateKeyDer<'static> = rustls_pemfile::private_key(&mut key_reader)
        .map_err(|e| anyhow::anyhow!("Failed to parse private key: {}", e))?
        .ok_or_else(|| anyhow::anyhow!("No private key found in key file: {}", key_path))?;

    // Build rustls ClientConfig with:
    // - Insecure server cert verification (accepts any certificate)
    // - mTLS client authentication (presents client cert)
    let config = rustls::ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(InsecureCertVerifier))
        .with_client_auth_cert(client_certs, client_key)
        .map_err(|e| anyhow::anyhow!("Failed to build TLS config with client cert: {}", e))?;

    Ok(tokio_rustls::TlsConnector::from(Arc::new(config)))
}

/// Compute SHA-256 fingerprint of the first certificate in the configured cert file.
pub fn compute_cert_fingerprint(tls: &TlsSection) -> Option<String> {
    let cert_path = tls.cert_file.as_deref()?;
    let cert_data = std::fs::read(cert_path).ok()?;
    let mut reader = BufReader::new(cert_data.as_slice());
    let cert = rustls_pemfile::certs(&mut reader).next()?.ok()?;
    let digest = sha2::Sha256::digest(cert.as_ref());
    Some(hex::encode(digest))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_tls_connector_missing_ca() {
        let tls = TlsSection {
            enabled: true,
            cert_file: Some("/tmp/nonexistent.pem".into()),
            key_file: Some("/tmp/nonexistent.key".into()),
            ca_file: None,
        };
        let result = build_tls_connector(&tls);
        match result {
            Err(e) => assert!(e.to_string().contains("ca_file")),
            Ok(_) => panic!("Expected error for missing ca_file"),
        }
    }

    #[test]
    fn test_build_tls_connector_missing_cert() {
        let tls = TlsSection {
            enabled: true,
            cert_file: None,
            key_file: Some("/tmp/nonexistent.key".into()),
            ca_file: Some("/tmp/nonexistent.pem".into()),
        };
        // Will fail at CA loading (file not found), which is before cert check
        let result = build_tls_connector(&tls);
        assert!(result.is_err());
    }

    #[test]
    fn test_compute_cert_fingerprint_missing_file() {
        let tls = TlsSection {
            enabled: true,
            cert_file: Some("/tmp/nonexistent.pem".into()),
            key_file: None,
            ca_file: None,
        };
        assert!(compute_cert_fingerprint(&tls).is_none());
    }
}
