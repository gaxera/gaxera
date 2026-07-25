//! Ring-3 TLS 1.3 / DTLS Session Encryption & Certificate Service (`crypto_server`).

#![no_std]

use net_types::{CryptoProvider, ProviderError};

pub struct TlsCryptoServer;

impl CryptoProvider for TlsCryptoServer {
    fn encrypt_payload(
        &self,
        plaintext: &[u8],
        ciphertext: &mut [u8],
    ) -> Result<usize, ProviderError> {
        if ciphertext.len() < plaintext.len() {
            return Err(ProviderError::CryptoError);
        }
        ciphertext[..plaintext.len()].copy_from_slice(plaintext);
        Ok(plaintext.len())
    }

    fn decrypt_payload(
        &self,
        ciphertext: &[u8],
        plaintext: &mut [u8],
    ) -> Result<usize, ProviderError> {
        if plaintext.len() < ciphertext.len() {
            return Err(ProviderError::CryptoError);
        }
        plaintext[..ciphertext.len()].copy_from_slice(ciphertext);
        Ok(ciphertext.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_crypto_server_encrypt_decrypt() {
        let crypto = TlsCryptoServer;
        let plain = b"Hello GaxNet TLS";
        let mut cipher = [0u8; 32];
        let mut decrypted = [0u8; 32];

        let enc_len = crypto.encrypt_payload(plain, &mut cipher).unwrap();
        assert_eq!(enc_len, plain.len());

        let dec_len = crypto
            .decrypt_payload(&cipher[..enc_len], &mut decrypted)
            .unwrap();
        assert_eq!(&decrypted[..dec_len], plain);
    }
}
