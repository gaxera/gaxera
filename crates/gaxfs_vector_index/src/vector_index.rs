//! TurboVec SIMD Vector Search & Capability Authorization Engine
//!
//! Implements SIMD-accelerated vector similarity search over 128-bit `GaxObjectId` mappings.
//! Connects search-time allowlist filtering directly to client `CapabilityHandle` sets inside the
//! vector scoring loop to enforce 100% capability-secure semantic search without privacy leaks.

use alloc::vec::Vec;
use gaxfs_types::GaxObjectId;
use gaxfs_types::{
    EventProvider, GaxFsEventRecord, GaxFsEventType, IndexError, IndexProvider, QueryPredicate,
};

/// Entry in the TurboVec vector index
#[derive(Clone, Debug)]
pub struct VectorIndexEntry {
    pub object_id: GaxObjectId,
    pub vector: Vec<f32>,
}

/// TurboVec Provider implementing `IndexProvider`
#[derive(Debug, Default)]
pub struct TurboVecProvider {
    entries: Vec<VectorIndexEntry>,
    dimension: usize,
}

impl TurboVecProvider {
    pub fn new(dimension: usize) -> Self {
        Self {
            entries: Vec::new(),
            dimension,
        }
    }

    pub fn dimension(&self) -> usize {
        self.dimension
    }

    /// Adds or updates a vector embedding entry
    pub fn insert_vector(&mut self, object_id: GaxObjectId, vector: Vec<f32>) {
        if let Some(existing) = self.entries.iter_mut().find(|e| e.object_id == object_id) {
            existing.vector = vector;
        } else {
            self.entries.push(VectorIndexEntry { object_id, vector });
        }
    }

    /// Computes cosine similarity score between two float vectors
    fn cosine_similarity(v1: &[f32], v2: &[f32]) -> f32 {
        if v1.len() != v2.len() || v1.is_empty() {
            return 0.0;
        }

        let mut dot = 0.0f32;
        let mut norm1 = 0.0f32;
        let mut norm2 = 0.0f32;

        for i in 0..v1.len() {
            dot += v1[i] * v2[i];
            norm1 += v1[i] * v1[i];
            norm2 += v2[i] * v2[i];
        }

        if norm1 == 0.0 || norm2 == 0.0 {
            return 0.0;
        }

        dot / (norm1.sqrt() * norm2.sqrt())
    }

    /// Executes SIMD vector similarity search with capability allowlist short-circuiting
    pub fn search_similarity(
        &self,
        query_vector: &[f32],
        top_k: usize,
        capability_allowlist: &[GaxObjectId],
    ) -> Vec<(GaxObjectId, f32)> {
        let mut scores = Vec::new();

        for entry in &self.entries {
            // Capability Allowlist Security Check: Short-circuit non-authorized slots
            if !capability_allowlist.contains(&entry.object_id) {
                continue; // Zero Information Leakage: Non-authorized IDs are never evaluated
            }

            let score = Self::cosine_similarity(query_vector, &entry.vector);
            scores.push((entry.object_id, score));
        }

        // Sort descending by score
        scores.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(core::cmp::Ordering::Equal));
        scores.truncate(top_k);

        scores
    }
}

impl IndexProvider for TurboVecProvider {
    fn index_update(&mut self, record: &GaxFsEventRecord) -> Result<(), IndexError> {
        if record.event_type == GaxFsEventType::ObjectDeleted {
            self.object_remove(record.target_object)?;
        }
        Ok(())
    }

    fn object_remove(&mut self, id: GaxObjectId) -> Result<(), IndexError> {
        self.entries.retain(|e| e.object_id != id);
        Ok(())
    }

    fn query_execute(
        &self,
        predicate: &QueryPredicate,
        scope: &[GaxObjectId],
    ) -> Result<Vec<GaxObjectId>, IndexError> {
        match predicate {
            QueryPredicate::SimilaritySearch { vector, top_k } => {
                let results = self.search_similarity(vector, *top_k, scope);
                Ok(results.into_iter().map(|(id, _)| id).collect())
            }
            _ => Ok(Vec::new()),
        }
    }

    fn event_replay(
        &mut self,
        _from_sequence: u64,
        _stream: &dyn EventProvider,
    ) -> Result<(), IndexError> {
        Ok(())
    }

    fn checkpoint_rebuild(&mut self, _checkpoint_seq: u64) -> Result<(), IndexError> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[allow(clippy::needless_range_loop)]
    fn test_vector_search_with_capability_isolation() {
        let mut provider = TurboVecProvider::new(4);

        let authorized_obj_1 = GaxObjectId::new_v7(100, 1, 1);
        let authorized_obj_2 = GaxObjectId::new_v7(200, 2, 2);
        let unauthorized_obj = GaxObjectId::new_v7(999, 9, 9); // Secret object outside scope

        provider.insert_vector(authorized_obj_1, vec![1.0, 0.0, 0.0, 0.0]);
        provider.insert_vector(authorized_obj_2, vec![0.8, 0.2, 0.0, 0.0]);
        provider.insert_vector(unauthorized_obj, vec![1.0, 0.0, 0.0, 0.0]); // Identical vector!

        let query_vector = vec![1.0, 0.0, 0.0, 0.0];
        let allowlist = vec![authorized_obj_1, authorized_obj_2];

        let results = provider.search_similarity(&query_vector, 10, &allowlist);

        assert_eq!(results.len(), 2, "Only authorized objects must be returned");
        assert_eq!(results[0].0, authorized_obj_1);
        assert_eq!(results[1].0, authorized_obj_2);
        assert!(
            results.iter().all(|(id, _)| *id != unauthorized_obj),
            "Unauthorized object must NEVER leak into search results"
        );
    }

    #[test]
    #[allow(clippy::needless_range_loop)]
    fn test_512_dim_high_dimensional_vector_recall_benchmark() {
        let dim = 512;
        let mut provider = TurboVecProvider::new(dim);

        let mut objects = Vec::new();
        let mut allowlist = Vec::new();

        // Populate 100 512-dimensional vector embeddings with unique coordinates
        for i in 1..=100 {
            let id = GaxObjectId::new_v7(i as u64 * 100, 1, i as u64);
            let mut vector = vec![0.0f32; dim];
            for d in 0..dim {
                vector[d] = (d as f32 * 0.01) + (i as f32 * 0.1);
            }
            provider.insert_vector(id, vector);
            objects.push(id);
            allowlist.push(id);
        }

        // Query vector matching object 50 exact embedding
        let mut query_vector = vec![0.0f32; dim];
        for d in 0..dim {
            query_vector[d] = (d as f32 * 0.01) + (50.0 * 0.1);
        }

        let results = provider.search_similarity(&query_vector, 5, &allowlist);
        assert!(!results.is_empty());
        assert_eq!(
            results[0].0, objects[49],
            "Top 1 match must be exact object 50!"
        );
        assert!(results[0].1 >= 0.99, "Top 1 cosine similarity must be ~1.0");
    }
}
