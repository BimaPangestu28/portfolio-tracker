//! AES-256-GCM token encryption for Upwork, reusing the google::crypto
//! primitives. Only the key source differs (UPWORK_TOKEN_ENC_KEY). Fail closed.

pub use crate::google::crypto::{decrypt, encrypt};

/// Parse a base64-encoded 32-byte key value. Separated from env reading so it
/// can be tested without mutating process environment (mirrors google::crypto).
pub fn key_from_env_value(b64: &str) -> anyhow::Result<[u8; 32]> {
    let raw = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64.trim())
        .map_err(|_| anyhow::anyhow!("UPWORK_TOKEN_ENC_KEY is not valid base64"))?;
    raw.try_into()
        .map_err(|_| anyhow::anyhow!("UPWORK_TOKEN_ENC_KEY must decode to exactly 32 bytes"))
}

/// Read + parse the base64 32-byte key from UPWORK_TOKEN_ENC_KEY. Err when unset
/// or malformed (fail closed: callers treat this as "cannot connect").
pub fn key_from_env() -> anyhow::Result<[u8; 32]> {
    let b64 = std::env::var("UPWORK_TOKEN_ENC_KEY")
        .map_err(|_| anyhow::anyhow!("UPWORK_TOKEN_ENC_KEY is not set"))?;
    key_from_env_value(&b64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encrypt_decrypt_round_trips() {
        let k = [9u8; 32];
        let ct = encrypt("oauth-secret", &k).unwrap();
        assert_ne!(ct, "oauth-secret");
        assert_eq!(decrypt(&ct, &k).unwrap(), "oauth-secret");
    }

    #[test]
    fn key_from_env_requires_var() {
        std::env::remove_var("UPWORK_TOKEN_ENC_KEY");
        assert!(key_from_env().is_err());
    }

    #[test]
    fn key_from_value_validates_length_and_encoding() {
        use base64::Engine;
        let good = base64::engine::general_purpose::STANDARD.encode([1u8; 32]);
        assert!(key_from_env_value(&good).is_ok());
        let short = base64::engine::general_purpose::STANDARD.encode([1u8; 16]);
        assert!(key_from_env_value(&short).is_err());
        assert!(key_from_env_value("not-base64!!!").is_err());
    }
}
