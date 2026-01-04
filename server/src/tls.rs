use anyhow::{Context, Result};
use rcgen::{CertificateParams, DistinguishedName, DnType, SanType};
use rustls::ServerConfig;
use rustls_pemfile::{certs, pkcs8_private_keys};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::BufReader;
use std::time::SystemTime;

pub fn generate_self_signed_cert(ip: &str) -> Result<(String, String, String)> {
    let mut params = CertificateParams::default();

    let mut dn = DistinguishedName::new();
    dn.push(DnType::CommonName, "TradeTerminal Server");
    dn.push(DnType::OrganizationName, "TradeTerminal");
    params.distinguished_name = dn;

    params.subject_alt_names = vec![
        SanType::IpAddress(
            ip.parse()
                .with_context(|| format!("Invalid IP address: {}", ip))?,
        ),
        SanType::DnsName(
            "localhost"
                .try_into()
                .context("Failed to convert to Ia5String")?,
        ),
    ];

    let not_before = SystemTime::now();
    let not_after = not_before + std::time::Duration::from_secs(10 * 365 * 24 * 60 * 60);
    params.not_before = not_before.into();
    params.not_after = not_after.into();

    let key_pair = rcgen::KeyPair::generate()?;
    let key_pem = key_pair.serialize_pem();
    let cert = params.self_signed(&key_pair)?;
    let cert_pem = cert.pem();
    let fingerprint = calculate_fingerprint(&cert_pem)?;

    Ok((cert_pem, key_pem, fingerprint))
}

pub fn calculate_fingerprint(cert_pem: &str) -> Result<String> {
    let cert_der = pem::parse(cert_pem)
        .context("Failed to parse certificate PEM")?
        .into_contents();

    let mut hasher = Sha256::new();
    hasher.update(&cert_der);
    let hash = hasher.finalize();

    Ok(hex::encode(hash))
}

pub fn load_tls_config(cert_path: &str, key_path: &str) -> Result<ServerConfig> {
    let cert_file = File::open(cert_path)
        .with_context(|| format!("Failed to open certificate file: {}", cert_path))?;
    let mut cert_reader = BufReader::new(cert_file);

    let certs: Vec<_> = certs(&mut cert_reader)
        .collect::<Result<Vec<_>, _>>()
        .context("Failed to parse certificates")?;

    let key_file = File::open(key_path)
        .with_context(|| format!("Failed to open private key file: {}", key_path))?;
    let mut key_reader = BufReader::new(key_file);

    let keys: Vec<_> = pkcs8_private_keys(&mut key_reader)
        .collect::<Result<Vec<_>, _>>()
        .context("Failed to parse private keys")?;

    let key = keys
        .into_iter()
        .next()
        .context("No private key found in key file")?;

    let config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key.into())
        .context("Failed to create TLS config")?;

    Ok(config)
}

pub fn save_cert_to_files(
    cert_pem: &str,
    key_pem: &str,
    fingerprint: &str,
    output_dir: &std::path::Path,
) -> Result<()> {
    std::fs::create_dir_all(output_dir)?;

    std::fs::write(output_dir.join("server.crt"), cert_pem)?;
    std::fs::write(output_dir.join("server.key"), key_pem)?;
    std::fs::write(output_dir.join("fingerprint.txt"), fingerprint)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_certificate() {
        let (cert_pem, key_pem, fingerprint) = generate_self_signed_cert("127.0.0.1").unwrap();

        assert!(cert_pem.contains("BEGIN CERTIFICATE"));
        assert!(key_pem.contains("BEGIN PRIVATE KEY"));
        assert_eq!(fingerprint.len(), 64);
    }

    #[test]
    fn test_fingerprint_calculation() {
        let (cert_pem, _, _) = generate_self_signed_cert("127.0.0.1").unwrap();
        let fp = calculate_fingerprint(&cert_pem).unwrap();

        assert_eq!(fp.len(), 64);
        assert!(fp.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_fingerprint_consistency() {
        let (cert_pem, _, _) = generate_self_signed_cert("127.0.0.1").unwrap();
        let fp1 = calculate_fingerprint(&cert_pem).unwrap();
        let fp2 = calculate_fingerprint(&cert_pem).unwrap();

        assert_eq!(fp1, fp2);
    }
}
