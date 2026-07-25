//! Three-Layer Query Architecture (`query_planner`)
//!
//! Provides the primary GaxQL query engine implementing the 3-layer architecture specified in
//! `docs/architecture/gaxfs_indexing_architecture.md`:
//! - Layer 1: Platform-Independent GaxQL AST (`ast.rs`).
//! - Layer 2: Cost-Based Query Planner & Provider Router (`planner.rs`, `cost_model.rs`).
//! - Layer 3: Language Bindings Builder (`builder.rs`).

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

pub mod ast;
pub mod builder;
pub mod cost_model;
pub mod planner;

pub use ast::{DomainOp, QueryExpr, QueryPredicate};
pub use builder::QueryBuilder;
pub use cost_model::{PlanCost, estimate_op_cost, estimate_predicate_cost};
pub use planner::QueryPlanner;
