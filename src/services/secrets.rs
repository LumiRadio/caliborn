//! AES-256-GCM symmetric encryption for at-rest secrets (Discord OAuth tokens
//! today, anything similar tomorrow).
//!
//! Master key is loaded once at startup from `CALIBORN_TOKEN_ENCRYPTION_KEY`
//! (64 hex chars = 32 bytes). Each `seal()` call generates a fresh 12-byte
//! random nonce; the nonce is stored alongside the ciphertext in the DB.
//!
//! Without the master key Caliborn cannot decrypt tokens, so the env var must
//! be present in production (and rotated atomically by re-encrypting the
//! `discord_oauth_tokens` table — out of scope here).

use aes_gcm::{
    Aes256Gcm, Key, Nonce,
    aead::{Aead, KeyInit, OsRng, rand_core::RngCore},
};

#[derive(thiserror::Error, Debug)]
pub enum SealerError {
    #[error("Master key environment variable not set")]
    KeyEnvMissing,
    #[error("Master key must be 64 hex characters (32 bytes)")]
    InvalidKeyLength,
    #[error("Master key is not valid hex: {0}")]
    InvalidKeyHex(#[from] hex::FromHexError),
    #[error("Encryption failed")]
    Encrypt,
    #[error("Decryption failed (bad key, tampered ciphertext, or wrong nonce)")]
    Decrypt,
    #[error("Stored nonce has wrong length (expected 12 bytes)")]
    InvalidNonce,
}

const NONCE_LEN: usize = 12;
const KEY_LEN: usize = 32;
const ENV_VAR: &str = "CALIBORN_TOKEN_ENCRYPTION_KEY";

#[derive(Clone)]
pub struct TokenSealer {
    cipher: Aes256Gcm,
}

impl TokenSealer {
    /// Load the master key from `CALIBORN_TOKEN_ENCRYPTION_KEY`.
    pub fn from_env() -> Result<Self, SealerError> {
        let hex_key = std::env::var(ENV_VAR).map_err(|_| SealerError::KeyEnvMissing)?;
        Self::from_hex(&hex_key)
    }

    pub fn from_hex(hex_key: &str) -> Result<Self, SealerError> {
        let bytes = hex::decode(hex_key.trim())?;
        if bytes.len() != KEY_LEN {
            return Err(SealerError::InvalidKeyLength);
        }
        let key = Key::<Aes256Gcm>::from_slice(&bytes);
        Ok(Self {
            cipher: Aes256Gcm::new(key),
        })
    }

    /// Encrypt `plaintext`. Returns `(ciphertext_with_tag, nonce)` for
    /// storage. Both are required for [`Self::unseal`].
    pub fn seal(&self, plaintext: &[u8]) -> Result<(Vec<u8>, [u8; NONCE_LEN]), SealerError> {
        let mut nonce_bytes = [0u8; NONCE_LEN];
        OsRng.fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let ciphertext = self
            .cipher
            .encrypt(nonce, plaintext)
            .map_err(|_| SealerError::Encrypt)?;
        Ok((ciphertext, nonce_bytes))
    }

    pub fn unseal(&self, ciphertext: &[u8], nonce: &[u8]) -> Result<Vec<u8>, SealerError> {
        if nonce.len() != NONCE_LEN {
            return Err(SealerError::InvalidNonce);
        }
        let nonce = Nonce::from_slice(nonce);
        self.cipher
            .decrypt(nonce, ciphertext)
            .map_err(|_| SealerError::Decrypt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_sealer() -> TokenSealer {
        // Fixed 32-byte key for tests; never use in production.
        TokenSealer::from_hex("00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff")
            .unwrap()
    }

    #[test]
    fn roundtrip_succeeds() {
        let s = test_sealer();
        let (ct, nonce) = s.seal(b"some-discord-refresh-token").unwrap();
        let pt = s.unseal(&ct, &nonce).unwrap();
        assert_eq!(pt, b"some-discord-refresh-token");
    }

    #[test]
    fn unseal_rejects_tampered_ciphertext() {
        let s = test_sealer();
        let (mut ct, nonce) = s.seal(b"hello").unwrap();
        ct[0] ^= 0xFF;
        assert!(s.unseal(&ct, &nonce).is_err());
    }

    #[test]
    fn unseal_rejects_wrong_nonce() {
        let s = test_sealer();
        let (ct, _) = s.seal(b"hello").unwrap();
        let bad_nonce = [0u8; 12];
        assert!(s.unseal(&ct, &bad_nonce).is_err());
    }

    #[test]
    fn unseal_rejects_short_nonce() {
        let s = test_sealer();
        let (ct, _) = s.seal(b"hello").unwrap();
        assert!(matches!(
            s.unseal(&ct, &[0u8; 8]),
            Err(SealerError::InvalidNonce)
        ));
    }

    #[test]
    fn invalid_key_hex_rejected() {
        assert!(matches!(
            TokenSealer::from_hex("not-hex"),
            Err(SealerError::InvalidKeyHex(_))
        ));
    }

    #[test]
    fn wrong_key_length_rejected() {
        assert!(matches!(
            TokenSealer::from_hex("0011"),
            Err(SealerError::InvalidKeyLength)
        ));
    }

    #[test]
    fn nonce_is_unique_per_seal() {
        let s = test_sealer();
        let (_, n1) = s.seal(b"x").unwrap();
        let (_, n2) = s.seal(b"x").unwrap();
        assert_ne!(n1, n2);
    }
}
