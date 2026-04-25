//! HIR pattern matching data.
//!
//! WHAT: lowered pattern arms for HIR match terminators.
//! WHY: AST validates patterns and exhaustiveness; HIR preserves the validated matching contract for
//! backend lowering.

use crate::compiler_frontend::hir::expressions::HirExpression;
use crate::compiler_frontend::hir::ids::ChoiceId;

#[derive(Debug, Clone)]
pub struct HirMatchArm {
    pub pattern: HirPattern,
    pub guard: Option<HirExpression>,
    pub body: crate::compiler_frontend::hir::ids::BlockId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirRelationalPatternOp {
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
}

#[derive(Debug, Clone)]
pub enum HirPattern {
    Literal(HirExpression),
    Wildcard,
    Relational {
        op: HirRelationalPatternOp,
        value: HirExpression,
    },
    ChoiceVariant {
        choice_id: ChoiceId,
        variant_index: usize,
    },
}
