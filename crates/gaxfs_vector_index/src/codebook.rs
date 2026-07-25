//! Precomputed Lloyd-Max Quantization Codebooks for Beta-Distributed Coordinates
//!
//! Provides optimal boundaries and centroids for scalar quantization over rotated coordinates.

/// Precomputed Lloyd-Max centroids for 4-bit (16 levels) quantization on Beta marginals
pub const CENTROIDS_4BIT_1536: [f32; 16] = [
    -0.1245, -0.0982, -0.0761, -0.0571, -0.0398, -0.0236, -0.0078, 0.0078, 0.0236, 0.0398, 0.0571,
    0.0761, 0.0982, 0.1245, 0.1580, 0.2050,
];

/// Precomputed Lloyd-Max decision boundaries for 4-bit (15 decision thresholds)
pub const BOUNDARIES_4BIT_1536: [f32; 15] = [
    -0.11135, -0.08715, -0.06660, -0.04845, -0.03170, -0.01570, 0.00000, 0.01570, 0.03170, 0.04845,
    0.06660, 0.08715, 0.11135, 0.14125, 0.18150,
];

/// Finds the quantized level index (0..15) for a given coordinate value
pub fn quantize_scalar_4bit(val: f32) -> u8 {
    let mut idx = 0;
    for &boundary in &BOUNDARIES_4BIT_1536 {
        if val > boundary {
            idx += 1;
        } else {
            break;
        }
    }
    idx
}

/// Dequantizes a 4-bit level index back to its centroid float
pub fn dequantize_scalar_4bit(idx: u8) -> f32 {
    let i = (idx as usize).min(15);
    CENTROIDS_4BIT_1536[i]
}
