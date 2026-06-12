//! AES-256-GCM encryption for OAuth tokens at rest. The key comes from
//! GOOGLE_TOKEN_ENC_KEY (base64-encoded 32 bytes). Fail closed: callers must
//! treat a missing/invalid key as "cannot connect", never store plaintext.

use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Nonce};
use base64::Engine;
use rand::RngCore;

/// Parse a base64-encoded 32-byte key (the GOOGLE_TOKEN_ENC_KEY value).
pub fn key_from_env_value(b64: &str) -> anyhow::Result<[u8; 32]> {
    let raw = base64::engine::general_purpose::STANDARD
        .decode(b64.trim())
        .map_err(|_| anyhow::anyhow!("GOOGLE_TOKEN_ENC_KEY is not valid base64"))?;
    let key: [u8; 32] = raw
        .try_into()
        .map_err(|_| anyhow::anyhow!("GOOGLE_TOKEN_ENC_KEY must decode to exactly 32 bytes"))?;
    Ok(key)
}

/// Read + parse the key from the environment. Err when unset (fail closed).
pub fn key_from_env() -> anyhow::Result<[u8; 32]> {
    let b64 = std::env::var("GOOGLE_TOKEN_ENC_KEY")
        .map_err(|_| anyhow::anyhow!("GOOGLE_TOKEN_ENC_KEY is not set"))?;
    key_from_env_value(&b64)
}

/// Encrypt to base64(nonce[12] || ciphertext).
pub fn encrypt(plaintext: &str, key: &[u8; 32]) -> anyhow::Result<String> {
    let cipher = Aes256Gcm::new(key.into());
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ct = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|e| anyhow::anyhow!("encrypt failed: {e}"))?;
    let mut blob = nonce_bytes.to_vec();
    blob.extend_from_slice(&ct);
    Ok(base64::engine::general_purpose::STANDARD.encode(blob))
}

/// Decrypt base64(nonce[12] || ciphertext).
pub fn decrypt(b64: &str, key: &[u8; 32]) -> anyhow::Result<String> {
    let blob = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|_| anyhow::anyhow!("ciphertext is not valid base64"))?;
    if blob.len() < 12 {
        anyhow::bail!("ciphertext too short");
    }
    let (nonce_bytes, ct) = blob.split_at(12);
    let cipher = Aes256Gcm::new(key.into());
    let pt = cipher
        .decrypt(Nonce::from_slice(nonce_bytes), ct)
        .map_err(|e| anyhow::anyhow!("decrypt failed: {e}"))?;
    Ok(String::from_utf8(pt)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> [u8; 32] { [7u8; 32] }

    #[test]
    fn round_trips() {
        let k = key();
        let ct = encrypt("ya29.secret-token", &k).unwrap();
        assert_ne!(ct, "ya29.secret-token");
        assert_eq!(decrypt(&ct, &k).unwrap(), "ya29.secret-token");
    }

    #[test]
    fn wrong_key_fails() {
        let ct = encrypt("secret", &key()).unwrap();
        let mut bad = key();
        bad[0] = 0;
        assert!(decrypt(&ct, &bad).is_err());
    }

    #[test]
    fn key_from_base64_validates_length() {
        use base64::Engine;
        let good = base64::engine::general_purpose::STANDARD.encode([1u8; 32]);
        assert!(key_from_env_value(&good).is_ok());
        let short = base64::engine::general_purpose::STANDARD.encode([1u8; 16]);
        assert!(key_from_env_value(&short).is_err());
        assert!(key_from_env_value("not-base64!!!").is_err());
    }
}
