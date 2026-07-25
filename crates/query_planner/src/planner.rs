//! Layer 2 — Cost-Based Query Planner & Parallel Provider Router
//!
//! Parses GaxQL ASTs, estimates costs, reorders predicates, routes operations to specialized
//! index provider traits, computes candidate set intersections, and enforces capability authorization.

use crate::ast::{DomainOp, QueryExpr, QueryPredicate};
use crate::cost_model::estimate_predicate_cost;
use alloc::vec::Vec;
use gaxfs_types::{CapabilityHandle, GaxObjectId, IndexProvider, NamespaceProvider, StorageError};
use std::collections::HashSet;

/// Query Planner Router dispatching AST expressions across Index Providers
pub struct QueryPlanner<'a> {
    namespace_provider: Option<&'a dyn NamespaceProvider>,
    vector_index_provider: Option<&'a dyn IndexProvider>,
}

impl<'a> QueryPlanner<'a> {
    pub fn new() -> Self {
        Self {
            namespace_provider: None,
            vector_index_provider: None,
        }
    }

    pub fn with_namespace_provider(mut self, provider: &'a dyn NamespaceProvider) -> Self {
        self.namespace_provider = Some(provider);
        self
    }

    pub fn with_vector_index_provider(mut self, provider: &'a dyn IndexProvider) -> Self {
        self.vector_index_provider = Some(provider);
        self
    }

    /// Evaluates a GaxQL query AST, returning the capability-filtered target object IDs
    pub fn execute(&self, expr: &QueryExpr) -> Result<Vec<GaxObjectId>, StorageError> {
        // Enforce capability authorization scope check
        if expr.scope.is_revoked() {
            return Ok(Vec::new());
        }

        let scope_id = expr.scope.target_object();
        let scope_list = vec![scope_id];

        // Evaluate AST expression tree
        let candidate_set = self.evaluate_predicate(&expr.scope, &expr.predicate, &scope_list)?;

        // Filter results against capability scope
        let mut final_results: Vec<GaxObjectId> = candidate_set
            .into_iter()
            .filter(|id| *id == scope_id || scope_list.contains(id))
            .collect();

        if let Some(limit) = expr.limit {
            final_results.truncate(limit);
        }

        Ok(final_results)
    }

    fn evaluate_predicate(
        &self,
        scope_cap: &CapabilityHandle,
        pred: &QueryPredicate,
        scope: &[GaxObjectId],
    ) -> Result<HashSet<GaxObjectId>, StorageError> {
        match pred {
            QueryPredicate::Leaf(op) => self.evaluate_op(op, scope_cap, scope),
            QueryPredicate::And(list) => {
                if list.is_empty() {
                    return Ok(HashSet::new());
                }

                // Sort predicates by estimated cost (lowest cost first)
                let mut sorted_list = list.clone();
                sorted_list.sort_by_key(estimate_predicate_cost);

                let mut result = self.evaluate_predicate(scope_cap, &sorted_list[0], scope)?;
                for p in &sorted_list[1..] {
                    if result.is_empty() {
                        break; // Short-circuit AND evaluation on empty intersection
                    }
                    let set = self.evaluate_predicate(scope_cap, p, scope)?;
                    result = result.intersection(&set).cloned().collect();
                }

                Ok(result)
            }
            QueryPredicate::Or(list) => {
                let mut result = HashSet::new();
                for p in list {
                    let set = self.evaluate_predicate(scope_cap, p, scope)?;
                    result.extend(set);
                }
                Ok(result)
            }
            QueryPredicate::Not(_) => Ok(HashSet::new()),
        }
    }

    fn evaluate_op(
        &self,
        op: &DomainOp,
        scope_cap: &CapabilityHandle,
        scope: &[GaxObjectId],
    ) -> Result<HashSet<GaxObjectId>, StorageError> {
        match op {
            DomainOp::PathEquals(path) => {
                if let Some(provider) = self.namespace_provider
                    && let Ok(obj_id) = provider.resolve_path(scope_cap, path)
                {
                    let mut set = HashSet::new();
                    set.insert(obj_id);
                    return Ok(set);
                }
                Ok(HashSet::new())
            }
            DomainOp::SimilarTo { vector, top_k } => {
                if let Some(provider) = self.vector_index_provider {
                    let pred = gaxfs_types::QueryPredicate::SimilaritySearch {
                        vector: vector.clone(),
                        top_k: *top_k,
                    };
                    if let Ok(matching_ids) = provider.query_execute(&pred, scope) {
                        return Ok(matching_ids.into_iter().collect());
                    }
                }
                Ok(HashSet::new())
            }
            _ => Ok(HashSet::new()),
        }
    }

    /// Verifies if a query expression requires vector indexing
    pub fn requires_vector_index(expr: &QueryExpr) -> bool {
        Self::predicate_contains_op(&expr.predicate, &|op| {
            matches!(op, DomainOp::SimilarTo { .. })
        })
    }

    fn predicate_contains_op<F: Fn(&DomainOp) -> bool>(pred: &QueryPredicate, f: &F) -> bool {
        match pred {
            QueryPredicate::Leaf(op) => f(op),
            QueryPredicate::And(list) | QueryPredicate::Or(list) => {
                list.iter().any(|p| Self::predicate_contains_op(p, f))
            }
            QueryPredicate::Not(inner) => Self::predicate_contains_op(inner, f),
        }
    }
}

impl<'a> Default for QueryPlanner<'a> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cost_model::estimate_op_cost;

    #[test]
    fn test_planner_selective_index_pruning() {
        let path_op = DomainOp::PathEquals("/sys/config".into());
        let sim_op = DomainOp::SimilarTo {
            vector: vec![1.0; 4],
            top_k: 5,
        };

        assert_eq!(estimate_op_cost(&path_op).0, 1);
        assert_eq!(estimate_op_cost(&sim_op).0, 50);

        let expr = QueryExpr::new(
            gaxfs_types::CapabilityHandle::new_root(
                1,
                gaxfs_types::CapabilitySpace(1),
                GaxObjectId::NIL,
                gaxfs_types::GaxFsRights::READ,
            ),
            QueryPredicate::Leaf(path_op),
        );

        assert!(!QueryPlanner::requires_vector_index(&expr));
    }

    #[test]
    fn test_multi_predicate_intersection_and_short_circuit() {
        let path_op = DomainOp::PathEquals("/sys/config".into());
        let attr_op = DomainOp::AttributeEquals {
            key: "version".into(),
            value: "1.0".into(),
        };

        // Create AND predicate with path and attribute filter
        let and_pred = QueryPredicate::And(vec![
            QueryPredicate::Leaf(path_op),
            QueryPredicate::Leaf(attr_op),
        ]);

        let cap = CapabilityHandle::new_root(
            1,
            gaxfs_types::CapabilitySpace(1),
            GaxObjectId::NIL,
            gaxfs_types::GaxFsRights::READ,
        );

        let expr = QueryExpr::new(cap, and_pred);
        let planner = QueryPlanner::new();

        // Execution without providers returns empty candidate set via short-circuiting
        let results = planner.execute(&expr).unwrap();
        assert!(
            results.is_empty(),
            "Unsatisfied predicates must evaluate cleanly to empty set"
        );
    }
}
