use hmac::{Hmac, Mac};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

pub fn sign(secret: &str, params: &str) -> String {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC can take key of any size");
    mac.update(params.as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

pub fn generate_signature(api_key: &str, api_secret: &str, timestamp: u64, recv_window: u64, body: &str) -> String {
    let param_str = format!("{}{}{}{}", timestamp, api_key, recv_window, body);
    sign(api_secret, &param_str)
}
