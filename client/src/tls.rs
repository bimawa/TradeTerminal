use anyhow::Result;
use rustls::pki_types::{CertificateDer, ServerName, UnixTime};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::{ClientConfig, DigitallySignedStruct, Error, SignatureScheme};
use sha2::{Digest, Sha256};
use std::sync::Arc;

#[derive(Debug)]
pub struct FingerprintVerifier {
    expected_fingerprint: String,
}

impl FingerprintVerifier {
    pub fn new(expected_fingerprint: String) -> Self {
        Self {
            expected_fingerprint: expected_fingerprint.to_lowercase(),
        }
    }

    fn calculate_fingerprint(&self, cert: &CertificateDer<'_>) -> String {
        let mut hasher = Sha256::new();
        hasher.update(cert.as_ref());
        hex::encode(hasher.finalize())
    }
}

impl ServerCertVerifier for FingerprintVerifier {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp_response: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, Error> {
        let actual_fingerprint = self.calculate_fingerprint(end_entity);

        if actual_fingerprint == self.expected_fingerprint {
            tracing::info!(
                fingerprint = %actual_fingerprint,
                "Certificate fingerprint verified successfully"
            );
            Ok(ServerCertVerified::assertion())
        } else {
            tracing::error!(
                expected = %self.expected_fingerprint,
                actual = %actual_fingerprint,
                "Certificate fingerprint mismatch"
            );
            Err(Error::InvalidCertificate(
                rustls::CertificateError::ApplicationVerificationFailure,
            ))
        }
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, Error> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::ECDSA_NISTP521_SHA512,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
        ]
    }
}

pub fn create_tls_connector(expected_fingerprint: &str) -> Result<tokio_rustls::TlsConnector> {
    let verifier = Arc::new(FingerprintVerifier::new(expected_fingerprint.to_string()));

    let config = ClientConfig::builder()
        .dangerous()
        .with_custom_certificate_verifier(verifier)
        .with_no_client_auth();

    Ok(tokio_rustls::TlsConnector::from(Arc::new(config)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fingerprint_verifier_creation() {
        let verifier = FingerprintVerifier::new("ABCD1234".to_string());
        assert_eq!(verifier.expected_fingerprint, "abcd1234");
    }

    #[test]
    fn test_create_tls_connector() {
        let result = create_tls_connector("abcd1234567890abcdef");
        assert!(result.is_ok());
    }
}
