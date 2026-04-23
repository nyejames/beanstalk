//! Result-handler control-flow termination checks.
//!
//! WHAT: computes whether a handler body can fall through.
//! WHY: named handlers without fallback must terminate on all reachable paths when the surrounding
//! expression still requires success values.

use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};

fn statement_guarantees_termination(statement: &AstNode) -> bool {
    match &statement.kind {
        NodeKind::Return(_) | NodeKind::ReturnError(_) => true,
        NodeKind::If(_, then_body, Some(else_body)) => {
            body_guarantees_termination(then_body) && body_guarantees_termination(else_body)
        }
        NodeKind::Match(_, arms, maybe_default_body) => {
            // Match parsing enforces exhaustiveness for all non-choice scrutinees and for
            // choice scrutinees without an explicit `else =>` arm. Guarded choice matches
            // must also include `else =>`, so `None` here is still exhaustive by construction.
            let all_arms_terminate = arms
                .iter()
                .all(|arm| body_guarantees_termination(&arm.body));
            if !all_arms_terminate {
                return false;
            }

            match maybe_default_body {
                Some(default_body) => body_guarantees_termination(default_body),
                None => true,
            }
        }
        _ => false,
    }
}

pub(super) fn body_guarantees_termination(body: &[AstNode]) -> bool {
    body.iter().any(statement_guarantees_termination)
}
