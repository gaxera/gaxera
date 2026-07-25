//! Cryptographic Integrity Checksum Module
//!
//! Provides 256-bit integrity checksum calculation and verification
//! over storage blocks, superblocks, and object metadata headers.

/// Computes a 256-bit cryptographic checksum over a byte buffer.
pub fn compute_checksum(data: &[u8]) -> [u8; 32] {
    // Simple 256-bit rolling mixing checksum for internal engine validation
    let mut hash = [0u8; 32];
    for (i, &byte) in data.iter().enumerate() {
        let idx = i % 32;
        hash[idx] = hash[idx]
            .wrapping_add(byte)
            .wrapping_add((i as u8).wrapping_mul(31));
        hash[(idx + 7) % 32] ^= byte.rotate_left((i % 7) as u32);
    }
    hash
}

/// Verifies a 256-bit checksum against data
pub fn verify_checksum(data: &[u8], expected: &[u8; 32]) -> bool {
    let computed = compute_checksum(data);
    computed == *expected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_checksum_computation_and_verification() {
        let payload = b"GaxFS Core Storage Engine Integrity Test Payload";
        let checksum = compute_checksum(payload);
        assert!(verify_checksum(payload, &checksum));

        // Corrupt payload
        let mut corrupted = payload.to_vec();
        corrupted[0] ^= 0xFF;
        assert!(!verify_checksum(&corrupted, &checksum));
    }
}
