//! Cost Estimation & Selective Index Pruning Module
//!
//! Evaluates estimated execution costs per predicate node to prune unnecessary
//! index evaluation and reorder conjunctions (lowest cost predicates evaluated first).

use crate::ast::{DomainOp, QueryPredicate};

/// Estimated Query Plan Execution Cost
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct PlanCost(pub u32);

pub fn estimate_op_cost(op: &DomainOp) -> PlanCost {
    match op {
        DomainOp::PathEquals(_) => PlanCost(1), // Ultra-fast exact path lookup (Namespace Index)
        DomainOp::AttributeEquals { .. } => PlanCost(5), // Key-value BTree lookup (Metadata Index)
        DomainOp::References(_) => PlanCost(10), // Directed edge graph traversal (Graph Index)
        DomainOp::DependsOn(_) => PlanCost(10),
        DomainOp::Contains(_) => PlanCost(10),
        DomainOp::SimilarTo { .. } => PlanCost(50), // High-cost vector inner product scoring (Semantic Index)
    }
}

pub fn estimate_predicate_cost(pred: &QueryPredicate) -> PlanCost {
    match pred {
        QueryPredicate::Leaf(op) => estimate_op_cost(op),
        QueryPredicate::And(list) => {
            // In an AND, cost is dominated by evaluating the cheapest predicate first to narrow candidates
            list.iter()
                .map(estimate_predicate_cost)
                .min()
                .unwrap_or(PlanCost(100))
        }
        QueryPredicate::Or(list) => {
            // In an OR, cost is the sum of all branch costs
            let total: u32 = list.iter().map(|p| estimate_predicate_cost(p).0).sum();
            PlanCost(total)
        }
        QueryPredicate::Not(inner) => estimate_predicate_cost(inner),
    }
}
