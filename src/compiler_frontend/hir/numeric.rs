//! HIR checked numeric operations.
//!
//! WHAT: defines the statement-level numeric operation surface used by HIR to expose checked
//!       arithmetic, division/modulo-by-zero handling, and recoverable vs trapping failure modes.
//! WHY: numeric failures are semantic effects that belong in HIR, not in source expression trees,
//!      so backends receive an explicit operation with a known failure mode instead of rediscovering
//!      source operator fallibility.

use crate::compiler_frontend::hir::expressions::HirExpression;

/// How a checked numeric operation should behave on failure.
///
/// WHAT: selects between returning a recoverable builtin `Error!` carrier and trapping.
/// WHY: the choice depends on the enclosing function's error return slot. A builtin `Error!`
///      function can recover numeric failures through the normal fallible-carrier path; any other
///      fallible channel or non-fallible context must trap because the failure cannot be represented
///      as a user-visible value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericFailureMode {
    /// Produce an internal fallible carrier (success value or builtin `Error`).
    ///
    /// WHAT: the enclosing function has builtin `Error!` exactly as its error return slot, so
    ///       numeric failures can be returned through the normal fallible-carrier path.
    /// WHY: this keeps recoverable numeric failures in the same control-flow shape as explicit
    ///      `cast!` propagation and lets later lowering emit `HirTerminator::FallibleBranch`.
    ReturnError,

    /// Stop execution on failure.
    ///
    /// WHAT: the operation has no recoverable channel. The result local receives only the scalar
    ///       success value; failure is a runtime trap/throw.
    /// WHY: custom fallible channels, top-level `start()`, and non-fallible functions cannot
    ///      represent numeric failures as user values, so the backend must halt.
    Trap,
}

/// A checked numeric operation kind used in HIR.
///
/// WHAT: identifies the specific scalar arithmetic operation and its scalar kind.
/// WHY: backends must know both the operation (add, div, pow, ...) and whether the operands are
///      `Int` or `Float` so they can apply the correct checked runtime helper.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirNumericOp {
    IntAdd,
    IntSub,
    IntMul,
    IntDiv,
    IntMod,
    IntPow,
    IntNeg,
    FloatAdd,
    FloatSub,
    FloatMul,
    FloatDiv,
    FloatMod,
    FloatPow,
    FloatNeg,
}

impl HirNumericOp {
    /// Human-readable source-style name for debugging and HIR display.
    pub(crate) fn source_name(self) -> &'static str {
        match self {
            HirNumericOp::IntAdd => "int_add",
            HirNumericOp::IntSub => "int_sub",
            HirNumericOp::IntMul => "int_mul",
            HirNumericOp::IntDiv => "int_div",
            HirNumericOp::IntMod => "int_mod",
            HirNumericOp::IntPow => "int_pow",
            HirNumericOp::IntNeg => "int_neg",
            HirNumericOp::FloatAdd => "float_add",
            HirNumericOp::FloatSub => "float_sub",
            HirNumericOp::FloatMul => "float_mul",
            HirNumericOp::FloatDiv => "float_div",
            HirNumericOp::FloatMod => "float_mod",
            HirNumericOp::FloatPow => "float_pow",
            HirNumericOp::FloatNeg => "float_neg",
        }
    }

    /// Whether the operation takes one operand.
    pub(crate) fn is_unary(self) -> bool {
        matches!(self, HirNumericOp::IntNeg | HirNumericOp::FloatNeg)
    }
}

/// Operand carrier for a checked numeric operation.
///
/// WHAT: represents either a unary or binary numeric operation in one HIR-local shape.
/// WHY: keeps `HirStatementKind::NumericOp` a single variant while still distinguishing unary
///      negation from binary arithmetic for validation and backend lowering.
#[derive(Debug, Clone)]
pub enum HirNumericOperands {
    Unary {
        operand: HirExpression,
    },
    Binary {
        left: HirExpression,
        right: HirExpression,
    },
}
