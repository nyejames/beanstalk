//! HIR block terminators.
//!
//! WHAT: explicit control-flow exits for each block.
//! WHY: control flow must be structured enough for borrow validation and backend lowering.

use crate::compiler_frontend::hir::expressions::HirExpression;
use crate::compiler_frontend::hir::ids::{BlockId, LocalId};
use crate::compiler_frontend::hir::patterns::HirMatchArm;

#[derive(Debug, Clone)]
pub enum HirTerminator {
    Jump {
        target: BlockId,
        args: Vec<LocalId>, // Not SSA - just passing current local values
    },

    If {
        condition: HirExpression,
        then_block: BlockId,
        else_block: BlockId, // Required, must jump or return somewhere (Could just be continuation)
    },

    Match {
        scrutinee: HirExpression,
        arms: Vec<HirMatchArm>, // Each arm's body block must end with Jump or Return
    },

    Break {
        target: BlockId,
    },

    Continue {
        target: BlockId,
    },

    Return(HirExpression),

    Panic {
        message: Option<HirExpression>,
    },
}
