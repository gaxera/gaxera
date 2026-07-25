//! Layer 3 — Fluent Language Bindings & Query Builder
//!
//! Provides an ergonomic, type-safe Rust builder API for constructing GaxQL ASTs.

use crate::ast::{DomainOp, QueryExpr, QueryPredicate};
use alloc::string::String;
use alloc::vec::Vec;
use gaxfs_types::{CapabilityHandle, GaxObjectId};

/// Fluent Query Builder
pub struct QueryBuilder {
    scope: CapabilityHandle,
    predicates: Vec<QueryPredicate>,
    limit: Option<usize>,
    offset: Option<usize>,
}

impl QueryBuilder {
    pub fn with_scope(scope: CapabilityHandle) -> Self {
        Self {
            scope,
            predicates: Vec::new(),
            limit: None,
            offset: None,
        }
    }

    pub fn where_path(mut self, path: impl Into<String>) -> Self {
        self.predicates
            .push(QueryPredicate::Leaf(DomainOp::PathEquals(path.into())));
        self
    }

    pub fn where_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.predicates
            .push(QueryPredicate::Leaf(DomainOp::AttributeEquals {
                key: key.into(),
                value: value.into(),
            }));
        self
    }

    pub fn where_similar_to(mut self, vector: Vec<f32>, top_k: usize) -> Self {
        self.predicates
            .push(QueryPredicate::Leaf(DomainOp::SimilarTo { vector, top_k }));
        self
    }

    pub fn where_references(mut self, target_id: GaxObjectId) -> Self {
        self.predicates
            .push(QueryPredicate::Leaf(DomainOp::References(target_id)));
        self
    }

    pub fn limit(mut self, limit: usize) -> Self {
        self.limit = Some(limit);
        self
    }

    pub fn build(self) -> QueryExpr {
        let root_predicate = if self.predicates.len() == 1 {
            self.predicates.into_iter().next().unwrap()
        } else {
            QueryPredicate::And(self.predicates)
        };

        QueryExpr {
            scope: self.scope,
            predicate: root_predicate,
            limit: self.limit,
            offset: self.offset,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gaxfs_types::{CapabilitySpace, GaxFsRights};

    #[test]
    fn test_query_builder_fluent_api() {
        let cap =
            CapabilityHandle::new_root(1, CapabilitySpace(1), GaxObjectId::NIL, GaxFsRights::READ);
        let query = QueryBuilder::with_scope(cap)
            .where_path("/data/report")
            .where_similar_to(vec![0.5; 8], 10)
            .limit(5)
            .build();

        assert_eq!(query.limit, Some(5));
        match query.predicate {
            QueryPredicate::And(preds) => {
                assert_eq!(preds.len(), 2);
            }
            _ => panic!("Expected AND root predicate"),
        }
    }
}
