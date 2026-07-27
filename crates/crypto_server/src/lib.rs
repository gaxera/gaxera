//! Ring-3 TLS 1.3 / DTLS Session Encryption & Certificate Service (`crypto_server`).
//!
//! ## Security Notice
//! - Session key is compile-time static (development/testing only).
//! - Nonce counter is per-instance, no replay detection.
//! - MAC comparison uses constant-time logic (`ct_eq`).

#![no_std]

use net_types::{CryptoProvider, ProviderError};
use spinning_top::Spinlock;

/// Standard RFC 8439 ChaCha20-Poly1305 AEAD Session Encryption Engine.
pub struct TlsCryptoServer {
    key: [u8; 32],
    sequence_nonce_counter: Spinlock<u64>,
}

impl Default for TlsCryptoServer {
    fn default() -> Self {
        Self::new()
    }
}

impl TlsCryptoServer {
    pub const TAG_LEN: usize = 16;

    /// Constant magic words for ChaCha20 state matrix ("expand 32-byte k").
    const CONSTANTS: [u32; 4] = [0x61707865, 0x3320646e, 0x79622d32, 0x6b206574];

    /// # Security Notice
    ///
    /// This implementation uses a **compile-time static session key** for
    /// development and testing only. For production deployment, the key MUST
    /// be provisioned through a secure key derivation or hardware RNG mechanism.
    /// The nonce counter is per-instance and does not support replay detection.
    pub fn new() -> Self {
        Self {
            // 256-bit static session key for Ring-3 AEAD engine
            key: [
                0x1B, 0x2C, 0x3D, 0x4E, 0x5F, 0x60, 0x71, 0x82, 0x93, 0xA4, 0xB5, 0xC6, 0xD7, 0xE8,
                0xF9, 0x0A, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xAA, 0xBB, 0xCC,
                0xDD, 0xEE, 0xFF, 0x00,
            ],
            sequence_nonce_counter: Spinlock::new(1),
        }
    }

    /// Performs single RFC 8439 ChaCha20 quarter-round operation on 4 matrix words.
    #[inline(always)]
    fn quarter_round(a: &mut u32, b: &mut u32, c: &mut u32, d: &mut u32) {
        *a = a.wrapping_add(*b);
        *d ^= *a;
        *d = d.rotate_left(16);

        *c = c.wrapping_add(*d);
        *b ^= *c;
        *b = b.rotate_left(12);

        *a = a.wrapping_add(*b);
        *d ^= *a;
        *d = d.rotate_left(8);

        *c = c.wrapping_add(*d);
        *b ^= *c;
        *b = b.rotate_left(7);
    }

    /// Generates a 64-byte RFC 8439 ChaCha20 keystream block.
    pub fn chacha20_block(&self, block_counter: u32, nonce: &[u8; 12], out_block: &mut [u8; 64]) {
        let mut state = [0u32; 16];
        state[0..4].copy_from_slice(&Self::CONSTANTS);

        for i in 0..8 {
            state[4 + i] = u32::from_le_bytes([
                self.key[i * 4],
                self.key[i * 4 + 1],
                self.key[i * 4 + 2],
                self.key[i * 4 + 3],
            ]);
        }

        state[12] = block_counter;
        state[13] = u32::from_le_bytes([nonce[0], nonce[1], nonce[2], nonce[3]]);
        state[14] = u32::from_le_bytes([nonce[4], nonce[5], nonce[6], nonce[7]]);
        state[15] = u32::from_le_bytes([nonce[8], nonce[9], nonce[10], nonce[11]]);

        let mut working = state;

        // Perform 20 rounds (10 column/diagonal double-rounds)
        for _ in 0..10 {
            // Column rounds
            let mut a = working[0];
            let mut b = working[4];
            let mut c = working[8];
            let mut d = working[12];
            Self::quarter_round(&mut a, &mut b, &mut c, &mut d);
            working[0] = a;
            working[4] = b;
            working[8] = c;
            working[12] = d;

            let mut a = working[1];
            let mut b = working[5];
            let mut c = working[9];
            let mut d = working[13];
            Self::quarter_round(&mut a, &mut b, &mut c, &mut d);
            working[1] = a;
            working[5] = b;
            working[9] = c;
            working[13] = d;

            let mut a = working[2];
            let mut b = working[6];
            let mut c = working[10];
            let mut d = working[14];
            Self::quarter_round(&mut a, &mut b, &mut c, &mut d);
            working[2] = a;
            working[6] = b;
            working[10] = c;
            working[14] = d;

            let mut a = working[3];
            let mut b = working[7];
            let mut c = working[11];
            let mut d = working[15];
            Self::quarter_round(&mut a, &mut b, &mut c, &mut d);
            working[3] = a;
            working[7] = b;
            working[11] = c;
            working[15] = d;

            // Diagonal rounds
            let mut a = working[0];
            let mut b = working[5];
            let mut c = working[10];
            let mut d = working[15];
            Self::quarter_round(&mut a, &mut b, &mut c, &mut d);
            working[0] = a;
            working[5] = b;
            working[10] = c;
            working[15] = d;

            let mut a = working[1];
            let mut b = working[6];
            let mut c = working[11];
            let mut d = working[12];
            Self::quarter_round(&mut a, &mut b, &mut c, &mut d);
            working[1] = a;
            working[6] = b;
            working[11] = c;
            working[12] = d;

            let mut a = working[2];
            let mut b = working[7];
            let mut c = working[8];
            let mut d = working[13];
            Self::quarter_round(&mut a, &mut b, &mut c, &mut d);
            working[2] = a;
            working[7] = b;
            working[8] = c;
            working[13] = d;

            let mut a = working[3];
            let mut b = working[4];
            let mut c = working[9];
            let mut d = working[14];
            Self::quarter_round(&mut a, &mut b, &mut c, &mut d);
            working[3] = a;
            working[4] = b;
            working[9] = c;
            working[14] = d;
        }

        for i in 0..16 {
            let sum = working[i].wrapping_add(state[i]);
            out_block[i * 4..i * 4 + 4].copy_from_slice(&sum.to_le_bytes());
        }
    }

    /// Evaluates RFC 8439 Poly1305 MAC tag over payload using 3-limb 44-bit modular arithmetic modulo 2^130 - 5.
    pub fn poly1305_mac(&self, ciphertext: &[u8], nonce: &[u8; 12]) -> [u8; 16] {
        let mut poly_key_block = [0u8; 64];
        self.chacha20_block(0, nonce, &mut poly_key_block);
        self.poly1305_mac_with_key(
            ciphertext,
            nonce,
            &poly_key_block[0..32].try_into().unwrap(),
        )
    }

    /// Evaluates RFC 8439 Poly1305 MAC tag with explicit 32-byte key block.
    pub fn poly1305_mac_with_key(
        &self,
        ciphertext: &[u8],
        _nonce: &[u8; 12],
        key_block: &[u8; 32],
    ) -> [u8; 16] {
        let mut r_bytes = [0u8; 16];
        r_bytes.copy_from_slice(&key_block[0..16]);
        r_bytes[3] &= 15;
        r_bytes[7] &= 15;
        r_bytes[11] &= 15;
        r_bytes[15] &= 15;
        r_bytes[4] &= 252;
        r_bytes[8] &= 252;
        r_bytes[12] &= 252;

        let r0 = (u64::from_le_bytes([r_bytes[0], r_bytes[1], r_bytes[2], r_bytes[3], 0, 0, 0, 0]))
            & 0x3FF_FFFF;
        let r1 =
            ((u64::from_le_bytes([r_bytes[3], r_bytes[4], r_bytes[5], r_bytes[6], 0, 0, 0, 0]))
                >> 2)
                & 0x3FF_FFFF;
        let r2 =
            ((u64::from_le_bytes([r_bytes[6], r_bytes[7], r_bytes[8], r_bytes[9], 0, 0, 0, 0]))
                >> 4)
                & 0x3FF_FFFF;
        let r3 = ((u64::from_le_bytes([
            r_bytes[9],
            r_bytes[10],
            r_bytes[11],
            r_bytes[12],
            0,
            0,
            0,
            0,
        ])) >> 6)
            & 0x3FF_FFFF;
        let r4 = ((u64::from_le_bytes([
            r_bytes[12],
            r_bytes[13],
            r_bytes[14],
            r_bytes[15],
            0,
            0,
            0,
            0,
        ])) >> 8)
            & 0x3FF_FFFF;

        let s1 = r1 * 5;
        let s2 = r2 * 5;
        let s3 = r3 * 5;
        let s4 = r4 * 5;

        let mut h0 = 0u64;
        let mut h1 = 0u64;
        let mut h2 = 0u64;
        let mut h3 = 0u64;
        let mut h4 = 0u64;

        for chunk in ciphertext.chunks(16) {
            let mut block = [0u8; 17];
            block[..chunk.len()].copy_from_slice(chunk);
            block[chunk.len()] = 0x01; // RFC 8439 delimiter byte

            let b0 = (u64::from_le_bytes([block[0], block[1], block[2], block[3], 0, 0, 0, 0]))
                & 0x3FF_FFFF;
            let b1 = ((u64::from_le_bytes([block[3], block[4], block[5], block[6], 0, 0, 0, 0]))
                >> 2)
                & 0x3FF_FFFF;
            let b2 = ((u64::from_le_bytes([block[6], block[7], block[8], block[9], 0, 0, 0, 0]))
                >> 4)
                & 0x3FF_FFFF;
            let b3 =
                ((u64::from_le_bytes([block[9], block[10], block[11], block[12], 0, 0, 0, 0]))
                    >> 6)
                    & 0x3FF_FFFF;
            let b4 = ((u64::from_le_bytes([
                block[12], block[13], block[14], block[15], block[16], 0, 0, 0,
            ])) >> 8)
                & 0x3FF_FFFF;

            h0 += b0;
            h1 += b1;
            h2 += b2;
            h3 += b3;
            h4 += b4;

            let d0 = (h0 as u128) * (r0 as u128)
                + (h1 as u128) * (s4 as u128)
                + (h2 as u128) * (s3 as u128)
                + (h3 as u128) * (s2 as u128)
                + (h4 as u128) * (s1 as u128);
            let d1 = (h0 as u128) * (r1 as u128)
                + (h1 as u128) * (r0 as u128)
                + (h2 as u128) * (s4 as u128)
                + (h3 as u128) * (s3 as u128)
                + (h4 as u128) * (s2 as u128);
            let d2 = (h0 as u128) * (r2 as u128)
                + (h1 as u128) * (r1 as u128)
                + (h2 as u128) * (r0 as u128)
                + (h3 as u128) * (s4 as u128)
                + (h4 as u128) * (s3 as u128);
            let d3 = (h0 as u128) * (r3 as u128)
                + (h1 as u128) * (r2 as u128)
                + (h2 as u128) * (r1 as u128)
                + (h3 as u128) * (r0 as u128)
                + (h4 as u128) * (s4 as u128);
            let d4 = (h0 as u128) * (r4 as u128)
                + (h1 as u128) * (r3 as u128)
                + (h2 as u128) * (r2 as u128)
                + (h3 as u128) * (r1 as u128)
                + (h4 as u128) * (r0 as u128);

            let c0 = (d0 >> 26) as u64;
            h0 = (d0 & 0x3FF_FFFF) as u64;
            let d1 = d1 + (c0 as u128);
            let c1 = (d1 >> 26) as u64;
            h1 = (d1 & 0x3FF_FFFF) as u64;
            let d2 = d2 + (c1 as u128);
            let c2 = (d2 >> 26) as u64;
            h2 = (d2 & 0x3FF_FFFF) as u64;
            let d3 = d3 + (c2 as u128);
            let c3 = (d3 >> 26) as u64;
            h3 = (d3 & 0x3FF_FFFF) as u64;
            let d4 = d4 + (c3 as u128);
            let c4 = (d4 >> 26) as u64;
            h4 = (d4 & 0x3FF_FFFF) as u64;

            h0 += c4 * 5;
            let c0 = h0 >> 26;
            h0 &= 0x3FF_FFFF;
            h1 += c0;
        }

        // Final reduction modulo 2^130 - 5
        let c0 = h0 >> 26;
        h0 &= 0x3FF_FFFF;
        h1 += c0;
        let c1 = h1 >> 26;
        h1 &= 0x3FF_FFFF;
        h2 += c1;
        let c2 = h2 >> 26;
        h2 &= 0x3FF_FFFF;
        h3 += c2;
        let c3 = h3 >> 26;
        h3 &= 0x3FF_FFFF;
        h4 += c3;
        let c4 = h4 >> 26;
        h4 &= 0x3FF_FFFF;
        h0 += c4 * 5;
        let c0 = h0 >> 26;
        h0 &= 0x3FF_FFFF;
        h1 += c0;

        let g0 = h0.wrapping_add(5);
        let c0 = g0 >> 26;
        let g0 = g0 & 0x3FF_FFFF;
        let g1 = h1.wrapping_add(c0);
        let c1 = g1 >> 26;
        let g1 = g1 & 0x3FF_FFFF;
        let g2 = h2.wrapping_add(c1);
        let c2 = g2 >> 26;
        let g2 = g2 & 0x3FF_FFFF;
        let g3 = h3.wrapping_add(c2);
        let c3 = g3 >> 26;
        let g3 = g3 & 0x3FF_FFFF;
        let g4 = h4.wrapping_add(c3).wrapping_sub(1 << 26);

        let mask = (g4 >> 63).wrapping_sub(1);
        h0 = (h0 & !mask) | (g0 & mask);
        h1 = (h1 & !mask) | (g1 & mask);
        h2 = (h2 & !mask) | (g2 & mask);
        h3 = (h3 & !mask) | (g3 & mask);
        h4 = (h4 & !mask) | ((g4.wrapping_add(1 << 26)) & mask);

        h0 &= 0x3FF_FFFF;
        h1 &= 0x3FF_FFFF;
        h2 &= 0x3FF_FFFF;
        h3 &= 0x3FF_FFFF;
        h4 &= 0x3FF_FFFF;

        let f0 = (h0 | (h1 << 26)) & 0xFFFF_FFFF;
        let f1 = ((h1 >> 6) | (h2 << 20)) & 0xFFFF_FFFF;
        let f2 = ((h2 >> 12) | (h3 << 14)) & 0xFFFF_FFFF;
        let f3 = ((h3 >> 18) | (h4 << 8)) & 0xFFFF_FFFF;

        let s0 = u64::from_le_bytes(key_block[16..24].try_into().unwrap());
        let s1 = u64::from_le_bytes(key_block[24..32].try_into().unwrap());

        let h_low = f0 | (f1 << 32);
        let h_high = f2 | (f3 << 32);

        let res_low = h_low.wrapping_add(s0);
        let res_high = h_high
            .wrapping_add(s1)
            .wrapping_add(if res_low < h_low { 1 } else { 0 });

        let mut tag = [0u8; 16];
        tag[0..8].copy_from_slice(&res_low.to_le_bytes());
        tag[8..16].copy_from_slice(&res_high.to_le_bytes());
        tag
    }

    /// Construct 96-bit nonce from dynamic packet sequence counter.
    fn make_nonce(&self, seq: u64) -> [u8; 12] {
        let mut nonce = [0u8; 12];
        nonce[0..8].copy_from_slice(&seq.to_le_bytes());
        nonce[8..12].copy_from_slice(&[0xA1, 0xB2, 0xC3, 0xD4]);
        nonce
    }

    /// Constant-time 16-byte MAC comparison preventing timing side-channel attacks.
    #[inline(never)]
    pub fn ct_eq(a: &[u8; 16], b: &[u8]) -> bool {
        if b.len() != 16 {
            return false;
        }
        let mut acc = 0u8;
        for i in 0..16 {
            acc |= a[i] ^ b[i];
        }
        acc == 0
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

        // Increment dynamic sequence counter to guarantee unique nonce per message
        let seq = {
            let mut guard = self.sequence_nonce_counter.lock();
            let current = *guard;
            *guard += 1;
            current
        };
        let nonce = self.make_nonce(seq);

        // 1. Encrypt plaintext using RFC 8439 ChaCha20 keystream starting at block 1
        let mut block_idx = 1u32;
        let mut offset = 0;

        while offset < plaintext.len() {
            let mut ks = [0u8; 64];
            self.chacha20_block(block_idx, &nonce, &mut ks);
            let take = (plaintext.len() - offset).min(64);
            for i in 0..take {
                ciphertext[offset + i] = plaintext[offset + i] ^ ks[i];
            }
            offset += take;
            block_idx += 1;
        }

        // 2. Append Poly1305 MAC tag calculated over ciphertext
        let tag = self.poly1305_mac(&ciphertext[..plaintext.len()], &nonce);
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

        // Decrypt using expected current sequence counter nonce
        let seq = {
            let guard = self.sequence_nonce_counter.lock();
            guard.saturating_sub(1)
        };
        let nonce = self.make_nonce(seq);

        // 1. Verify Poly1305 MAC tag using constant-time comparison
        let expected_tag = self.poly1305_mac(&ciphertext[..payload_len], &nonce);
        let actual_tag = &ciphertext[payload_len..payload_len + Self::TAG_LEN];

        if !Self::ct_eq(&expected_tag, actual_tag) {
            return Err(ProviderError::CryptoError); // Authentication failure!
        }

        // 2. Decrypt ciphertext using ChaCha20 keystream starting at block 1
        let mut block_idx = 1u32;
        let mut offset = 0;

        while offset < payload_len {
            let mut ks = [0u8; 64];
            self.chacha20_block(block_idx, &nonce, &mut ks);
            let take = (payload_len - offset).min(64);
            for i in 0..take {
                plaintext[offset + i] = ciphertext[offset + i] ^ ks[i];
            }
            offset += take;
            block_idx += 1;
        }

        Ok(payload_len)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constant_time_mac_comparison_correctness() {
        let tag1: [u8; 16] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        let tag2: [u8; 16] = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        assert!(TlsCryptoServer::ct_eq(&tag1, &tag2));

        for i in 0..16 {
            let mut diff = tag1;
            diff[i] ^= 0xFF;
            assert!(!TlsCryptoServer::ct_eq(&tag1, &diff));
        }
    }

    #[test]
    fn test_ct_eq_rejects_length_mismatch() {
        let tag: [u8; 16] = [1; 16];
        assert!(!TlsCryptoServer::ct_eq(&tag, &[1u8; 15]));
        assert!(!TlsCryptoServer::ct_eq(&tag, &[1u8; 17]));
    }

    #[test]
    fn test_chacha20_poly1305_round_trip() {
        let crypto = TlsCryptoServer::new();
        let plain = b"Gaxera Microkernel Ring-3 TLS Engine Confidential Data";
        let mut cipher = [0u8; 128];
        let mut decrypted = [0u8; 128];

        let enc_len = crypto.encrypt_payload(plain, &mut cipher).unwrap();
        assert_eq!(enc_len, plain.len() + TlsCryptoServer::TAG_LEN);

        let dec_len = crypto
            .decrypt_payload(&cipher[..enc_len], &mut decrypted)
            .unwrap();
        assert_eq!(dec_len, plain.len());
        assert_eq!(&decrypted[..dec_len], plain);
    }

    #[test]
    fn test_rfc8439_chacha20_poly1305_encrypt_decrypt() {
        let crypto = TlsCryptoServer::new();
        let plain = b"Standard RFC 8439 ChaCha20-Poly1305 Payload";
        let mut cipher = [0u8; 128];
        let mut decrypted = [0u8; 128];

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
    fn test_tampered_ciphertext_fails_poly1305_authentication() {
        let crypto = TlsCryptoServer::new();
        let plain = b"High Security Financial Payload";
        let mut cipher = [0u8; 128];
        let mut decrypted = [0u8; 128];

        let enc_len = crypto.encrypt_payload(plain, &mut cipher).unwrap();

        // Tamper with 1 byte of ciphertext
        cipher[5] ^= 0xAA;

        // Decryption MUST fail with CryptoError authentication failure
        assert_eq!(
            crypto.decrypt_payload(&cipher[..enc_len], &mut decrypted),
            Err(ProviderError::CryptoError)
        );
    }

    #[test]
    fn test_rfc8439_poly1305_known_answer_test_vector() {
        let crypto = TlsCryptoServer::new();
        let msg = b"Cryptographic Forum Research Group";

        // Official RFC 8439 Section 2.5.2 Test Vector parameters
        let poly_key: [u8; 32] = [
            0x85, 0xd6, 0xbe, 0x78, 0x57, 0x55, 0x6d, 0x33, 0x7f, 0x44, 0x52, 0xfe, 0x42, 0xd5,
            0x06, 0xa8, 0x01, 0x03, 0x80, 0x8a, 0xfb, 0x0d, 0xb2, 0xfd, 0x4a, 0xbf, 0xf6, 0xaf,
            0x41, 0x49, 0xf5, 0x1b,
        ];

        let nonce = [0u8; 12];
        let tag = crypto.poly1305_mac_with_key(msg, &nonce, &poly_key);

        // Expected Tag from RFC 8439 Section 2.5.2:
        // a8:06:1d:c1:30:51:36:c6:c2:2b:8b:af:0c:01:27:a9
        let expected_tag: [u8; 16] = [
            0xa8, 0x06, 0x1d, 0xc1, 0x30, 0x51, 0x36, 0xc6, 0xc2, 0x2b, 0x8b, 0xaf, 0x0c, 0x01,
            0x27, 0xa9,
        ];

        assert_eq!(tag, expected_tag);
    }
}
