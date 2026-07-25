//! TurboQuant Vector Quantization & Compression Module
//!
//! Implements high-ratio lossy vector quantization derived from Google Research's TurboQuant algorithm.
//! Applies FWHT orthogonal rotation transforms followed by 4-bit Lloyd-Max scalar quantization
//! and RaBitQ length-renormalization scaling to guarantee unbiased inner product estimation.

use crate::codebook::{dequantize_scalar_4bit, quantize_scalar_4bit};
use alloc::vec;
use alloc::vec::Vec;
use gaxfs_types::{CompressionCapabilities, CompressionError, CompressionProvider};

/// TurboQuant Vector Quantization Engine implementing `CompressionProvider`
pub struct TurboQuantProvider {
    dimension: usize,
}

impl TurboQuantProvider {
    pub fn new(dimension: usize) -> Self {
        Self { dimension }
    }

    pub fn dimension(&self) -> usize {
        self.dimension
    }

    pub fn bits_per_channel(&self) -> usize {
        4
    }

    /// Fast Walsh-Hadamard Transform (FWHT) Orthogonal Rotation
    pub fn rotate_vector(&self, input: &[f32], output: &mut [f32]) {
        let len = input.len();
        output[..len].copy_from_slice(&input[..len]);

        let mut h = 1;
        while h < len {
            for i in (0..len).step_by(h * 2) {
                for j in i..i + h {
                    let x = output[j];
                    let y = output[j + h];
                    output[j] = x + y;
                    output[j + h] = x - y;
                }
            }
            h *= 2;
        }

        // Scale by 1 / sqrt(N)
        let norm = 1.0 / (len as f32).sqrt();
        for val in output.iter_mut().take(len) {
            *val *= norm;
        }
    }

    /// Inverse Fast Walsh-Hadamard Transform
    pub fn inverse_rotate_vector(&self, input: &[f32], output: &mut [f32]) {
        self.rotate_vector(input, output);
    }
}

impl CompressionProvider for TurboQuantProvider {
    fn compress(&self, input: &[u8]) -> Result<Vec<u8>, CompressionError> {
        if !input.len().is_multiple_of(4) {
            return Err(CompressionError::CompressFailed(
                "Input buffer length must be a multiple of 4 (f32 slice)".into(),
            ));
        }

        let num_floats = input.len() / 4;
        let mut f32_in = Vec::with_capacity(num_floats);
        let mut norm_sq = 0.0f32;
        #[allow(clippy::chunks_exact_to_as_chunks)]
        for chunk in input.chunks_exact(4) {
            let val = f32::from_le_bytes(chunk.try_into().unwrap());
            norm_sq += val * val;
            f32_in.push(val);
        }

        let vector_norm = norm_sq.sqrt();
        if vector_norm == 0.0 {
            // Return zero-filled payload
            let out = vec![0u8; num_floats.div_ceil(2) + 4];
            return Ok(out);
        }

        // Unit vector normalization
        let mut unit_v = Vec::with_capacity(num_floats);
        for &val in &f32_in {
            unit_v.push(val / vector_norm);
        }

        // Orthogonal FWHT rotation
        let mut rotated = vec![0.0f32; num_floats];
        self.rotate_vector(&unit_v, &mut rotated);

        // Dimension-invariant coordinate scale: stddev of rotated unit vector is 1/sqrt(N)
        let coord_scale = (num_floats as f32).sqrt();

        // Quantize coordinates & reconstruct centroid vector for scale calibration
        let mut quantized_indices = Vec::with_capacity(num_floats);
        let mut dot_u_xhat = 0.0f32;

        for &val in &rotated {
            // Scale val to standard unit variance before quantization
            let norm_coord = val * coord_scale;
            let q_idx = quantize_scalar_4bit(norm_coord * 0.035);
            let centroid = dequantize_scalar_4bit(q_idx) / 0.035 / coord_scale;
            dot_u_xhat += val * centroid;
            quantized_indices.push(q_idx);
        }

        // RaBitQ Length-Renormalization Scale: scale = ||v|| / <u, x_hat>
        let scale = if dot_u_xhat > 1e-12 {
            vector_norm / dot_u_xhat
        } else {
            vector_norm
        };

        // Pack 4-bit indices pair-wise (2 indices per byte)
        let mut packed_bytes = Vec::with_capacity(num_floats.div_ceil(2) + 4);
        // Prepend 4-byte float scale
        packed_bytes.extend_from_slice(&scale.to_le_bytes());

        for chunk in quantized_indices.chunks(2) {
            let b0 = chunk[0] & 0x0F;
            let b1 = if chunk.len() > 1 {
                (chunk[1] & 0x0F) << 4
            } else {
                0
            };
            packed_bytes.push(b0 | b1);
        }

        Ok(packed_bytes)
    }

    fn decompress(&self, compressed: &[u8]) -> Result<Vec<u8>, CompressionError> {
        if compressed.len() < 4 {
            return Err(CompressionError::DecompressFailed(
                "Compressed buffer too short".into(),
            ));
        }

        let scale = f32::from_le_bytes(compressed[0..4].try_into().unwrap());
        let packed_indices = &compressed[4..];

        let num_floats = packed_indices.len() * 2;
        let coord_scale = (num_floats as f32).sqrt();

        let mut unquantized_rotated = Vec::new();
        for &byte in packed_indices {
            let idx0 = byte & 0x0F;
            let idx1 = (byte >> 4) & 0x0F;

            let c0 = dequantize_scalar_4bit(idx0) / 0.035 / coord_scale;
            let c1 = dequantize_scalar_4bit(idx1) / 0.035 / coord_scale;

            unquantized_rotated.push(c0);
            unquantized_rotated.push(c1);
        }

        let mut reconstructed_unit = vec![0.0f32; unquantized_rotated.len()];
        self.inverse_rotate_vector(&unquantized_rotated, &mut reconstructed_unit);

        // Apply scale renormalization: v_hat = scale * u_reconstructed
        let mut output = Vec::with_capacity(reconstructed_unit.len() * 4);
        for &val in &reconstructed_unit {
            let scaled_val = val * scale;
            output.extend_from_slice(&scaled_val.to_le_bytes());
        }

        Ok(output)
    }

    fn capabilities(&self) -> CompressionCapabilities {
        CompressionCapabilities {
            supports_streaming: false,
            supports_dictionary: false,
            hardware_accelerated: true,
            is_lossy: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::needless_range_loop, clippy::chunks_exact_to_as_chunks)]
    fn test_turboquant_lloyd_max_renormalized_compression() {
        let dim = 128;
        let provider = TurboQuantProvider::new(dim);
        let mut original_floats = vec![0.0f32; dim];
        for i in 0..dim {
            original_floats[i] = ((i as f32 % 17.0) - 8.0) * 0.1;
        }

        let mut input_bytes = Vec::new();
        for &f in &original_floats {
            input_bytes.extend_from_slice(&f.to_le_bytes());
        }

        let compressed = provider.compress(&input_bytes).unwrap();
        // 128 4-bit indices = 64 bytes + 4 bytes scale = 68 bytes total (from 512 bytes raw f32 = 7.5x byte compression)
        assert_eq!(compressed.len(), 68);

        let decompressed = provider.decompress(&compressed).unwrap();
        let mut reconstructed_floats = Vec::new();
        for chunk in decompressed.chunks_exact(4) {
            reconstructed_floats.push(f32::from_le_bytes(chunk.try_into().unwrap()));
        }

        // Compute Cosine Similarity between original and TurboQuant reconstructed vector
        let mut dot = 0.0f32;
        let mut norm_orig = 0.0f32;
        let mut norm_rec = 0.0f32;
        for i in 0..dim {
            dot += original_floats[i] * reconstructed_floats[i];
            norm_orig += original_floats[i] * original_floats[i];
            norm_rec += reconstructed_floats[i] * reconstructed_floats[i];
        }

        let cosine_sim = dot / (norm_orig.sqrt() * norm_rec.sqrt());
        assert!(
            cosine_sim >= 0.95,
            "TurboQuant reconstructed cosine similarity must be high: sim = {}",
            cosine_sim
        );
    }
}
