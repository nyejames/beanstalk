//! HIR statements.
//!
//! WHAT: effectful operations inside HIR blocks.
//! WHY: statements are where assignment, calls, side-effect expressions, and runtime fragment pushes
//! become explicit before borrow validation and backend lowering.

use crate::compiler_frontend::hir::expressions::HirExpression;
use crate::compiler_frontend::hir::ids::{HirNodeId, LocalId};
use crate::compiler_frontend::hir::places::HirPlace;
use crate::compiler_frontend::host_functions::CallTarget;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

#[derive(Debug, Clone)]
pub struct HirStatement {
    pub id: HirNodeId,
    pub kind: HirStatementKind,
    pub location: SourceLocation,
}

#[derive(Debug, Clone)]
pub enum HirStatementKind {
    Assign {
        target: HirPlace,
        value: HirExpression,
    },

    // HIR construction flattens nested calls.
    // Single-call expressions don't need explicit assignment in the source
    Call {
        target: CallTarget,
        args: Vec<HirExpression>,
        result: Option<LocalId>,
    },

    /// Expression evaluated only for side effects.
    Expr(HirExpression),

    /// Accumulate one runtime string value into the entry start() fragment vec.
    ///
    /// WHAT: explicit HIR primitive that lowers from `NodeKind::PushStartRuntimeFragment`.
    /// WHY: backends handle fragment accumulation without needing to inspect the entry start
    /// function body for heuristic push patterns.
    PushRuntimeFragment {
        /// The local holding the Vec<String> accumulator inside entry start().
        vec_local: LocalId,
        /// Expression that produces the string value to push.
        value: HirExpression,
    },

    /// Explicit deterministic drop.
    #[allow(dead_code)] // Planned: explicit drop statements after ownership lowering matures.
    Drop(LocalId),
}
