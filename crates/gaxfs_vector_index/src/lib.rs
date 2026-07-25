//! GaxFS Vector Indexing & Compression Crate (`gaxfs_vector_index`)
//!
//! Integrates SIMD-accelerated vector similarity search (`TurboVecProvider`) and
//! high-ratio lossy vector quantization (`TurboQuantProvider`) into GaxFS provider trait interfaces.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub mod codebook;
pub mod quantization;
pub mod vector_index;

pub use quantization::TurboQuantProvider;
pub use vector_index::{TurboVecProvider, VectorIndexEntry};
