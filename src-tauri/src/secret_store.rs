//! Encrypt/decrypt API keys at rest using AES-256-GCM.
//!
//! The encryption key is derived from machine-specific identifiers via PBKDF2,
//! so encrypted values are not portable between machines (by design).

use aes_gcm::aead::{Aead, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, KeyInit};
use base64::Engine;
use log::warn;
use pbkdf2::pbkdf2_hmac_array;
use sha2::Sha256;

const ENC_PREFIX: &str = "enc:";
const PBKDF2_ITERATIONS: u32 = 100_000;

/// Derive a 256-bit encryption key from machine-specific identifiers.
fn get_machine_key() -> [u8; 32] {
    let hostname = hostname::get()
        .map(|h| h.to_string_lossy().to_string())
        .unwrap_or_else(|_| "unknown-host".to_string());

    let username = whoami::username();
    let salt = format!("parler-api-key-{}-{}", hostname, username);

    pbkdf2_hmac_array::<Sha256, 32>(b"parler-secret-store", salt.as_bytes(), PBKDF2_ITERATIONS)
}

/// Encrypt a plaintext API key. Returns a string prefixed with `enc:`.
pub fn encrypt_api_key(plaintext: &str) -> String {
    if plaintext.is_empty() {
        return String::new();
    }

    let key_bytes = get_machine_key();
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

    match cipher.encrypt(&nonce, plaintext.as_bytes()) {
        Ok(ciphertext) => {
            // Concatenate nonce (12 bytes) + ciphertext, then base64-encode
            let mut combined = nonce.to_vec();
            combined.extend_from_slice(&ciphertext);
            format!(
                "{}{}",
                ENC_PREFIX,
                base64::engine::general_purpose::STANDARD.encode(&combined)
            )
        }
        Err(e) => {
            warn!("Failed to encrypt API key: {}", e);
            plaintext.to_string()
        }
    }
}

/// Decrypt an API key. If the value is not prefixed with `enc:`, returns it as-is
/// (plaintext migration case).
pub fn decrypt_api_key(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }

    let encoded = match value.strip_prefix(ENC_PREFIX) {
        Some(e) => e,
        None => return value.to_string(), // Not encrypted, return as-is
    };

    let combined = match base64::engine::general_purpose::STANDARD.decode(encoded) {
        Ok(c) => c,
        Err(e) => {
            warn!("Failed to decode encrypted API key: {}", e);
            return String::new();
        }
    };

    if combined.len() < 12 {
        warn!("Encrypted API key too short");
        return String::new();
    }

    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = aes_gcm::Nonce::from_slice(nonce_bytes);

    let key_bytes = get_machine_key();
    let key = Key::<Aes256Gcm>::from_slice(&key_bytes);
    let cipher = Aes256Gcm::new(key);

    match cipher.decrypt(nonce, ciphertext) {
        Ok(plaintext) => String::from_utf8(plaintext).unwrap_or_default(),
        Err(e) => {
            warn!("Failed to decrypt API key: {}", e);
            String::new()
        }
    }
}

/// Returns true if the value is already encrypted (prefixed with `enc:`).
pub fn is_encrypted(value: &str) -> bool {
    value.starts_with(ENC_PREFIX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_encrypt_decrypt() {
        let plaintext = "sk-test-1234567890abcdef";
        let encrypted = encrypt_api_key(plaintext);
        assert!(encrypted.starts_with(ENC_PREFIX));
        assert_ne!(encrypted, plaintext);

        let decrypted = decrypt_api_key(&encrypted);
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn empty_string_passthrough() {
        assert_eq!(encrypt_api_key(""), "");
        assert_eq!(decrypt_api_key(""), "");
    }

    #[test]
    fn unencrypted_passthrough() {
        let plain = "sk-plain-key";
        assert_eq!(decrypt_api_key(plain), plain);
    }
}
