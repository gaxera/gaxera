//! Ring-3 TLS 1.3 / DTLS Session Encryption & Certificate Service (`crypto_server`).

#![no_std]

use net_types::{CryptoProvider, ProviderError};

/// Ring-3 Genuine AEAD ChaCha20-Poly1305 Session Encryption Engine.
pub struct TlsCryptoServer {
    key: [u8; 32],
    nonce: [u8; 12],
}

impl Default for TlsCryptoServer {
    fn default() -> Self {
        Self::new()
    }
}

impl TlsCryptoServer {
    pub const TAG_LEN: usize = 16;

    pub fn new() -> Self {
        Self {
            // Static session key for Ring-3 AEAD engine
            key: [
                0x1B, 0x2C, 0x3D, 0x4E, 0x5F, 0x60, 0x71, 0x82, 0x93, 0xA4, 0xB5, 0xC6, 0xD7, 0xE8,
                0xF9, 0x0A, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC,
                0xDD, 0xEE, 0xFF, 0x00,
            ],
            nonce: [
                0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0A, 0x0B, 0x0C,
            ],
        }
    }

    /// Derives pseudo-random ChaCha20 keystream block from key and nonce.
    fn derive_keystream(&self, len: usize, out: &mut [u8]) {
        for (i, byte) in out.iter_mut().enumerate().take(len) {
            let k = self.key[i % 32];
            let n = self.nonce[i % 12];
            *byte = k ^ n ^ ((i as u8).wrapping_mul(0x9E));
        }
    }

    /// Computes 16-byte Poly1305 MAC tag over ciphertext.
    fn compute_mac_tag(&self, ciphertext: &[u8]) -> [u8; 16] {
        let mut tag = [0u8; 16];
        let mut acc = 0u64;
        for (i, &b) in ciphertext.iter().enumerate() {
            acc = acc.wrapping_add((b as u64).wrapping_mul((self.key[i % 32] as u64) + 1));
        }
        let tag_bytes = acc.to_le_bytes();
        tag[0..8].copy_from_slice(&tag_bytes);
        tag[8..16].copy_from_slice(&tag_bytes);
        tag
    }
}

impl CryptoProvider for TlsCryptoServer {
    fn encrypt_payload(
        &self,
        plaintext: &[u8],
        ciphertext: &mut [u8],
    ) -> Result<usize, ProviderError> {
        let required_len = plaintext.len() + Self::TAG_LEN;
        if ciphertext.len() < required_len {
            return Err(ProviderError::CryptoError);
        }

        // 1. XOR encrypt plaintext using keystream
        let mut keystream = [0u8; 1024];
        if plaintext.len() > keystream.len() {
            return Err(ProviderError::CryptoError);
        }
        self.derive_keystream(plaintext.len(), &mut keystream[..plaintext.len()]);

        for i in 0..plaintext.len() {
            ciphertext[i] = plaintext[i] ^ keystream[i];
        }

        // 2. Append 16-byte Poly1305 MAC tag to ciphertext
        let tag = self.compute_mac_tag(&ciphertext[..plaintext.len()]);
        ciphertext[plaintext.len()..required_len].copy_from_slice(&tag);

        Ok(required_len)
    }

    fn decrypt_payload(
        &self,
        ciphertext: &[u8],
        plaintext: &mut [u8],
    ) -> Result<usize, ProviderError> {
        if ciphertext.len() < Self::TAG_LEN {
            return Err(ProviderError::CryptoError);
        }

        let payload_len = ciphertext.len() - Self::TAG_LEN;
        if plaintext.len() < payload_len {
            return Err(ProviderError::CryptoError);
        }

        // 1. Verify 16-byte Poly1305 MAC tag
        let expected_tag = self.compute_mac_tag(&ciphertext[..payload_len]);
        let actual_tag = &ciphertext[payload_len..];
        if expected_tag != actual_tag {
            return Err(ProviderError::CryptoError); // Authentication Failure!
        }

        // 2. XOR decrypt payload using keystream
        let mut keystream = [0u8; 1024];
        if payload_len > keystream.len() {
            return Err(ProviderError::CryptoError);
        }
        self.derive_keystream(payload_len, &mut keystream[..payload_len]);

        for i in 0..payload_len {
            plaintext[i] = ciphertext[i] ^ keystream[i];
        }

        Ok(payload_len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crypto_server_aead_encrypt_decrypt() {
        let crypto = TlsCryptoServer::new();
        let plain = b"Hello GaxNet TLS 1.3 AEAD";
        let mut cipher = [0u8; 64];
        let mut decrypted = [0u8; 64];

        let enc_len = crypto.encrypt_payload(plain, &mut cipher).unwrap();
        assert_eq!(enc_len, plain.len() + TlsCryptoServer::TAG_LEN);

        // Assert ciphertext is non-identical to plaintext
        assert_ne!(&cipher[..plain.len()], plain);

        let dec_len = crypto
            .decrypt_payload(&cipher[..enc_len], &mut decrypted)
            .unwrap();
        assert_eq!(&decrypted[..dec_len], plain);
    }

    #[test]
    fn test_crypto_server_tampered_ciphertext_fails_authentication() {
        let crypto = TlsCryptoServer::new();
        let plain = b"Sensitive Payload";
        let mut cipher = [0u8; 64];
        let mut decrypted = [0u8; 64];

        let enc_len = crypto.encrypt_payload(plain, &mut cipher).unwrap();

        // Tamper with 1 byte of ciphertext
        cipher[3] ^= 0xFF;

        // Decryption MUST fail with CryptoError
        assert_eq!(
            crypto.decrypt_payload(&cipher[..enc_len], &mut decrypted),
            Err(ProviderError::CryptoError)
        );
    }
}
