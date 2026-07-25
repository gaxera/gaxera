//! Layer 1 — GaxQL Declarative Abstract Syntax Tree (AST)
//!
//! Language-agnostic, platform-independent query representation.
//! Expresses pure user intent without execution hints or provider names.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;
use gaxfs_types::{CapabilityHandle, GaxObjectId};

/// Domain Operations supported in GaxQL AST expressions
#[derive(Clone, Debug, PartialEq)]
pub enum DomainOp {
    PathEquals(String),
    AttributeEquals { key: String, value: String },
    SimilarTo { vector: Vec<f32>, top_k: usize },
    References(GaxObjectId),
    DependsOn(GaxObjectId),
    Contains(GaxObjectId),
}

/// GaxQL AST Predicate Expression Tree
#[derive(Clone, Debug, PartialEq)]
pub enum QueryPredicate {
    Leaf(DomainOp),
    And(Vec<QueryPredicate>),
    Or(Vec<QueryPredicate>),
    Not(Box<QueryPredicate>),
}

/// Complete GaxQL Query AST
#[derive(Clone, Debug, PartialEq)]
pub struct QueryExpr {
    pub scope: CapabilityHandle,
    pub predicate: QueryPredicate,
    pub limit: Option<usize>,
    pub offset: Option<usize>,
}

impl QueryExpr {
    pub fn new(scope: CapabilityHandle, predicate: QueryPredicate) -> Self {
        Self {
            scope,
            predicate,
            limit: None,
            offset: None,
        }
    }
}
