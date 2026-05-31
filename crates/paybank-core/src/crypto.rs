//! Application-layer AES-256-GCM encryption for PII at rest (bank account /
//! routing numbers). Key from `PII_ENCRYPTION_KEY` (32-byte; hex or base64).
//! Ciphertext is stored as `enc:v1:` + base64(nonce(12) || ciphertext) in the
//! existing TEXT columns — no schema change.
//!
//! Backward-compatible and fail-open for availability (never lose access to a
//! row): `decrypt` returns the input unchanged for legacy plaintext or when no
//! key is configured, and `encrypt` passes through (with a warning) when no key
//! is set so local/dev still works. Set the key in every real environment.

use aes_gcm::aead::rand_core::RngCore;
use aes_gcm::aead::{Aead, KeyInit, OsRng};
use aes_gcm::{Aes256Gcm, Key, Nonce};
use base64::{engine::general_purpose::STANDARD, Engine};
use std::sync::OnceLock;

const PREFIX: &str = "enc:v1:";

fn cipher() -> Option<&'static Aes256Gcm> {
    static CIPHER: OnceLock<Option<Aes256Gcm>> = OnceLock::new();
    CIPHER
        .get_or_init(|| {
            let raw = match std::env::var("PII_ENCRYPTION_KEY") {
                Ok(v) => v,
                Err(_) => {
                    tracing::warn!(
                        "PII_ENCRYPTION_KEY not set — bank account/routing numbers will be stored UNENCRYPTED (dev only)"
                    );
                    return None;
                }
            };
            let bytes = if raw.len() == 64 && raw.chars().all(|c| c.is_ascii_hexdigit()) {
                (0..32)
                    .map(|i| u8::from_str_radix(&raw[i * 2..i * 2 + 2], 16).unwrap_or(0))
                    .collect::<Vec<u8>>()
            } else {
                STANDARD.decode(raw.as_bytes()).unwrap_or_default()
            };
            if bytes.len() != 32 {
                tracing::error!("PII_ENCRYPTION_KEY must decode to 32 bytes; PII encryption disabled");
                return None;
            }
            Some(Aes256Gcm::new(Key::<Aes256Gcm>::from_slice(&bytes)))
        })
        .as_ref()
}

/// Encrypt a value for storage. Empty string in → empty string out.
pub fn encrypt(plaintext: &str) -> String {
    if plaintext.is_empty() {
        return String::new();
    }
    let Some(c) = cipher() else {
        return plaintext.to_string();
    };
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);
    match c.encrypt(nonce, plaintext.as_bytes()) {
        Ok(ct) => {
            let mut blob = nonce_bytes.to_vec();
            blob.extend_from_slice(&ct);
            format!("{}{}", PREFIX, STANDARD.encode(blob))
        }
        Err(_) => plaintext.to_string(),
    }
}

/// Decrypt a stored value. Returns the input unchanged for legacy plaintext or
/// when the key/ciphertext can't be used (fail-open on read).
pub fn decrypt(stored: &str) -> String {
    let Some(rest) = stored.strip_prefix(PREFIX) else {
        return stored.to_string();
    };
    let Some(c) = cipher() else {
        return stored.to_string();
    };
    let Ok(blob) = STANDARD.decode(rest.as_bytes()) else {
        return stored.to_string();
    };
    if blob.len() < 12 {
        return stored.to_string();
    }
    let (nonce_bytes, ct) = blob.split_at(12);
    match c.decrypt(Nonce::from_slice(nonce_bytes), ct) {
        Ok(pt) => String::from_utf8(pt).unwrap_or_else(|_| stored.to_string()),
        Err(_) => stored.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // All crypto assertions live in one test fn because the cipher key is cached
    // in a process-wide OnceLock — the key must be set before the first crypto
    // call, and we cannot rely on test execution ordering across multiple fns.
    #[test]
    fn crypto_round_trip_and_fail_open() {
        // 64-char hex key (32 bytes). Set BEFORE any crypto call so the OnceLock
        // initializes with a real cipher.
        std::env::set_var(
            "PII_ENCRYPTION_KEY",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
        );

        // Sanity: the cipher is actually configured for this test run. If some
        // other test in this binary initialized the OnceLock without a key first,
        // these assertions would not hold — but crypto tests are confined to this
        // file, so this is the first crypto call.
        assert!(cipher().is_some(), "cipher should be configured");

        // --- ciphertext differs from plaintext and carries the prefix ---
        let plaintext = "123456789";
        let ct = encrypt(plaintext);
        assert_ne!(ct, plaintext, "ciphertext must not equal plaintext");
        assert!(
            ct.starts_with(PREFIX),
            "ciphertext must start with the enc prefix, got: {ct}"
        );

        // --- decrypt(encrypt(x)) == x ---
        assert_eq!(decrypt(&ct), plaintext, "round-trip must recover plaintext");

        // Nonce is random => two encryptions of the same input differ as ciphertext
        // but both decrypt back to the original.
        let ct2 = encrypt(plaintext);
        assert_ne!(ct, ct2, "random nonce should produce distinct ciphertexts");
        assert_eq!(decrypt(&ct2), plaintext);

        // Round-trip a value containing the prefix string itself and unicode.
        let tricky = "enc:v1:not-really-encrypted — ünïcödé 🙂";
        let ct_tricky = encrypt(tricky);
        assert!(ct_tricky.starts_with(PREFIX));
        assert_eq!(decrypt(&ct_tricky), tricky);

        // --- legacy plaintext (no prefix) decrypts to itself unchanged ---
        let legacy = "987654321";
        assert!(!legacy.starts_with(PREFIX));
        assert_eq!(
            decrypt(legacy),
            legacy,
            "legacy plaintext without prefix must be returned unchanged"
        );

        // --- empty string round-trips to empty ---
        assert_eq!(encrypt(""), "", "empty input encrypts to empty");
        assert_eq!(decrypt(""), "", "empty input decrypts to empty");

        // --- fail-open on malformed ciphertext (has prefix, garbage body) ---
        let malformed = format!("{PREFIX}not-valid-base64!!!");
        assert_eq!(
            decrypt(&malformed),
            malformed,
            "undecodable ciphertext must fail open and return the input"
        );

        // Prefix present, valid base64, but too short to hold a 12-byte nonce.
        let too_short = format!("{}{}", PREFIX, STANDARD.encode([0u8; 4]));
        assert_eq!(
            decrypt(&too_short),
            too_short,
            "blob shorter than the nonce must fail open"
        );

        // Prefix present, valid base64, correct length but wrong key/tag → GCM
        // auth fails, decrypt returns input unchanged rather than panicking.
        let tampered = format!("{}{}", PREFIX, STANDARD.encode([7u8; 28]));
        assert_eq!(
            decrypt(&tampered),
            tampered,
            "GCM auth failure must fail open"
        );
    }
}
