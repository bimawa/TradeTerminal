use anyhow::{Context, Result};
use std::path::PathBuf;

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();

    let mut ip = "127.0.0.1".to_string();
    let mut output_dir = PathBuf::from("./certs");

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--ip" => {
                i += 1;
                if i >= args.len() {
                    anyhow::bail!("--ip requires an argument");
                }
                ip = args[i].clone();
            }
            "--output-dir" => {
                i += 1;
                if i >= args.len() {
                    anyhow::bail!("--output-dir requires an argument");
                }
                output_dir = PathBuf::from(&args[i]);
            }
            "--help" | "-h" => {
                print_usage();
                return Ok(());
            }
            _ => {
                eprintln!("Unknown argument: {}", args[i]);
                print_usage();
                std::process::exit(1);
            }
        }
        i += 1;
    }

    println!("Generating self-signed certificate for IP: {}", ip);
    println!("Output directory: {}", output_dir.display());

    let (cert_pem, key_pem, fingerprint) =
        trade_server::tls::generate_self_signed_cert(&ip)
            .context("Failed to generate certificate")?;

    trade_server::tls::save_cert_to_files(&cert_pem, &key_pem, &fingerprint, &output_dir)
        .context("Failed to save certificate files")?;

    println!("\nCertificate generated successfully!");
    println!("  Certificate: {}", output_dir.join("server.crt").display());
    println!("  Private key: {}", output_dir.join("server.key").display());
    println!("  Fingerprint: {}", output_dir.join("fingerprint.txt").display());
    println!("\nSHA-256 Fingerprint:");
    println!("  {}", fingerprint);
    println!("\nAdd this to your client .env:");
    println!("  TLS_CERT_FINGERPRINT={}", fingerprint);

    Ok(())
}

fn print_usage() {
    println!("Usage: generate-cert [OPTIONS]");
    println!();
    println!("Options:");
    println!("  --ip <IP>           Server IP address (default: 127.0.0.1)");
    println!("  --output-dir <DIR>  Output directory for certificates (default: ./certs)");
    println!("  -h, --help          Print this help message");
}
