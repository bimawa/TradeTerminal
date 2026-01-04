use std::time::Duration;

pub fn validate_auth_key(received: &str, expected: &str) -> bool {
    if received.len() != expected.len() {
        return false;
    }

    let mut result = 0u8;
    for (a, b) in received.bytes().zip(expected.bytes()) {
        result |= a ^ b;
    }

    result == 0
}

pub const AUTH_TIMEOUT: Duration = Duration::from_secs(5);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auth_key_validation_success() {
        let key = "abcd1234efgh5678";
        assert!(validate_auth_key("abcd1234efgh5678", key));
    }

    #[test]
    fn test_auth_key_validation_failure() {
        let key = "abcd1234efgh5678";
        assert!(!validate_auth_key("wrong_key_here!", key));
    }

    #[test]
    fn test_auth_key_length_mismatch() {
        let key = "abcd1234";
        assert!(!validate_auth_key("short", key));
        assert!(!validate_auth_key("abcd1234_longer", key));
    }

    #[test]
    fn test_auth_key_empty_strings() {
        assert!(validate_auth_key("", ""));
    }

    #[test]
    fn test_auth_key_case_sensitive() {
        assert!(!validate_auth_key("ABCD", "abcd"));
    }
}
