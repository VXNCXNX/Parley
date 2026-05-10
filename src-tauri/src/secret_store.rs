//! Encrypt/decrypt API keys at rest using AES-256-GCM.

use aes_gcm::aead::{Aead, OsRng};
use aes_gcm::{AeadCore, Aes256Gcm, Key, KeyInit};
use base64::Engine;
use log::warn;
use pbkdf2::pbkdf2_hmac_array;
use sha2::Sha256;
use std::sync::OnceLock;

const ENC_PREFIX: &str = "enc2:";
const LEGACY_ENC_PREFIX: &str = "enc:";
const PBKDF2_ITERATIONS: u32 = 100_000;

/// Cached stable key (derived once via PBKDF2, reused for all operations).
static STABLE_KEY: OnceLock<[u8; 32]> = OnceLock::new();
/// Cached legacy keys for decrypting values written before enc2.
static LEGACY_MACHINE_KEYS: OnceLock<Vec<[u8; 32]>> = OnceLock::new();

fn derive_key(salt: &str) -> [u8; 32] {
    pbkdf2_hmac_array::<Sha256, 32>(b"parler-secret-store", salt.as_bytes(), PBKDF2_ITERATIONS)
}

/// Derive a stable 256-bit encryption key. Avoid hostname here: macOS can
/// report a different hostname after network changes or sleep/wake, which made
/// persisted credentials undecryptable.
fn get_stable_key() -> &'static [u8; 32] {
    STABLE_KEY.get_or_init(|| {
        let username = whoami::username();
        let salt = format!("parler-api-key-v2-{}", username);
        derive_key(&salt)
    })
}

/// Derive the pre-enc2 key so old settings can be migrated when still readable.
fn get_legacy_machine_keys() -> &'static Vec<[u8; 32]> {
    LEGACY_MACHINE_KEYS.get_or_init(|| {
        let username = whoami::username();
        legacy_hostname_candidates()
            .into_iter()
            .map(|hostname| derive_key(&format!("parler-api-key-{}-{}", hostname, username)))
            .collect()
    })
}

fn legacy_hostname_candidates() -> Vec<String> {
    let mut candidates = Vec::new();
    if let Ok(hostname) = hostname::get() {
        push_unique_candidate(&mut candidates, hostname.to_string_lossy().to_string());
    }

    #[cfg(target_os = "macos")]
    for key in ["LocalHostName", "ComputerName", "HostName"] {
        if let Ok(output) = std::process::Command::new("scutil")
            .args(["--get", key])
            .output()
        {
            if output.status.success() {
                push_unique_candidate(
                    &mut candidates,
                    String::from_utf8_lossy(&output.stdout).to_string(),
                );
            }
        }
    }

    if candidates.is_empty() {
        candidates.push("unknown-host".to_string());
    }
    candidates
}

fn push_unique_candidate(candidates: &mut Vec<String>, candidate: String) {
    let candidate = candidate.trim().to_string();
    if !candidate.is_empty() && !candidates.contains(&candidate) {
        candidates.push(candidate);
    }
}

/// Encrypt a plaintext API key. Returns a string prefixed with `enc2:`.
pub fn encrypt_api_key(plaintext: &str) -> String {
    if plaintext.is_empty() {
        return String::new();
    }

    let key = Key::<Aes256Gcm>::from_slice(get_stable_key());
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

fn decrypt_with_key(encoded: &str, key_bytes: &[u8; 32]) -> Result<String, String> {
    let combined = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .map_err(|e| format!("decode failed: {}", e))?;

    if combined.len() < 12 {
        return Err("encrypted value too short".to_string());
    }

    let (nonce_bytes, ciphertext) = combined.split_at(12);
    let nonce = aes_gcm::Nonce::from_slice(nonce_bytes);

    let key = Key::<Aes256Gcm>::from_slice(key_bytes);
    let cipher = Aes256Gcm::new(key);

    let plaintext = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|e| format!("decrypt failed: {}", e))?;
    String::from_utf8(plaintext).map_err(|e| format!("invalid utf8: {}", e))
}

/// Decrypt an API key. If the value is not prefixed with an encryption marker,
/// returns it as-is for plaintext migration.
pub fn decrypt_api_key(value: &str) -> String {
    if value.is_empty() {
        return String::new();
    }

    if let Some(encoded) = value.strip_prefix(ENC_PREFIX) {
        return match decrypt_with_key(encoded, get_stable_key()) {
            Ok(plaintext) => plaintext,
            Err(e) => {
                warn!("Failed to decrypt API key: {}", e);
                String::new()
            }
        };
    }

    if let Some(encoded) = value.strip_prefix(LEGACY_ENC_PREFIX) {
        let mut last_err = None;
        for key in get_legacy_machine_keys() {
            match decrypt_with_key(encoded, key) {
                Ok(plaintext) => return plaintext,
                Err(e) => last_err = Some(e),
            }
        }
        warn!(
            "Failed to decrypt legacy API key: {}",
            last_err.unwrap_or_else(|| "no legacy keys available".to_string())
        );
        return String::new();
    }

    value.to_string()
}

/// Returns true if the value is already encrypted.
pub fn is_encrypted(value: &str) -> bool {
    value.starts_with(ENC_PREFIX) || value.starts_with(LEGACY_ENC_PREFIX)
}

/// Returns true when a value uses the hostname-derived legacy key.
pub fn uses_legacy_encryption(value: &str) -> bool {
    value.starts_with(LEGACY_ENC_PREFIX)
}

#[cfg(test)]
fn encrypt_api_key_legacy(plaintext: &str) -> String {
    if plaintext.is_empty() {
        return String::new();
    }

    let key = Key::<Aes256Gcm>::from_slice(&get_legacy_machine_keys()[0]);
    let cipher = Aes256Gcm::new(key);
    let nonce = Aes256Gcm::generate_nonce(&mut OsRng);

    match cipher.encrypt(&nonce, plaintext.as_bytes()) {
        Ok(ciphertext) => {
            let mut combined = nonce.to_vec();
            combined.extend_from_slice(&ciphertext);
            format!(
                "{}{}",
                LEGACY_ENC_PREFIX,
                base64::engine::general_purpose::STANDARD.encode(&combined)
            )
        }
        Err(e) => {
            warn!("Failed to encrypt API key: {}", e);
            plaintext.to_string()
        }
    }
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

    #[test]
    fn legacy_encrypted_values_still_decrypt() {
        let plaintext = "legacy-secret";
        let encrypted = encrypt_api_key_legacy(plaintext);
        assert!(encrypted.starts_with(LEGACY_ENC_PREFIX));
        assert!(uses_legacy_encryption(&encrypted));
        assert_eq!(decrypt_api_key(&encrypted), plaintext);
    }

    #[test]
    fn invalid_encrypted_value_returns_empty_string() {
        assert_eq!(decrypt_api_key("enc2:not-valid-base64"), "");
        assert_eq!(decrypt_api_key("enc:not-valid-base64"), "");
    }
}
