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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sign_basic() {
        let signature = sign("my_secret", "test_params");
        assert_eq!(signature.len(), 64);
        assert!(signature.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_sign_deterministic() {
        let sig1 = sign("secret", "params");
        let sig2 = sign("secret", "params");
        assert_eq!(sig1, sig2);
    }

    #[test]
    fn test_sign_different_secrets() {
        let sig1 = sign("secret1", "params");
        let sig2 = sign("secret2", "params");
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn test_sign_different_params() {
        let sig1 = sign("secret", "params1");
        let sig2 = sign("secret", "params2");
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn test_sign_empty_params() {
        let signature = sign("secret", "");
        assert_eq!(signature.len(), 64);
    }

    #[test]
    fn test_sign_known_value() {
        let signature = sign("test_secret", "test_message");
        assert_eq!(
            signature,
            "65a20917b5d689b407c1b6e0c7aa9de40221efc481ffe58f2485c51d822acece"
        );
    }

    #[test]
    fn test_generate_signature_format() {
        let sig = generate_signature(
            "api_key_123",
            "api_secret_456",
            1704067200000,
            5000,
            r#"{"symbol":"BTCUSDT"}"#,
        );
        assert_eq!(sig.len(), 64);
    }

    #[test]
    fn test_generate_signature_deterministic() {
        let sig1 = generate_signature("key", "secret", 1000, 5000, "body");
        let sig2 = generate_signature("key", "secret", 1000, 5000, "body");
        assert_eq!(sig1, sig2);
    }

    #[test]
    fn test_generate_signature_timestamp_affects_result() {
        let sig1 = generate_signature("key", "secret", 1000, 5000, "body");
        let sig2 = generate_signature("key", "secret", 2000, 5000, "body");
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn test_generate_signature_recv_window_affects_result() {
        let sig1 = generate_signature("key", "secret", 1000, 5000, "body");
        let sig2 = generate_signature("key", "secret", 1000, 10000, "body");
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn test_generate_signature_body_affects_result() {
        let sig1 = generate_signature("key", "secret", 1000, 5000, r#"{"a":1}"#);
        let sig2 = generate_signature("key", "secret", 1000, 5000, r#"{"a":2}"#);
        assert_ne!(sig1, sig2);
    }

    #[test]
    fn test_generate_signature_empty_body() {
        let sig = generate_signature("key", "secret", 1000, 5000, "");
        assert_eq!(sig.len(), 64);
    }

    #[test]
    fn test_generate_signature_param_order() {
        let sig = generate_signature("mykey", "mysecret", 1704067200000, 5000, "test");
        let expected_param_str = "1704067200000mykey5000test";
        let expected_sig = sign("mysecret", expected_param_str);
        assert_eq!(sig, expected_sig);
    }
}
