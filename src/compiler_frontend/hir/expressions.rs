//! HIR expressions.
//!
//! WHAT: typed value-producing nodes used by statements, terminators, and pattern matching.
//! WHY: HIR keeps normal value construction as expression trees while control flow stays explicit.

use crate::compiler_frontend::hir::hir_datatypes::TypeId;
use crate::compiler_frontend::hir::ids::{ChoiceId, FieldId, HirValueId, RegionId, StructId};
use crate::compiler_frontend::hir::operators::{HirBinOp, HirUnaryOp};
use crate::compiler_frontend::hir::places::HirPlace;
use crate::compiler_frontend::symbols::string_interning::StringId;

/// Shared carrier tag for variant construction in HIR.
///
/// WHY: Choices, Options, and Results all construct variant-shaped values. A shared carrier
/// keeps backend lowering uniform while preserving distinct semantic type identity.
/// Phase 3 uses this for choices; Phase 6 will migrate Option/Result.
#[derive(Debug, Clone)]
pub enum HirVariantCarrier {
    Choice { choice_id: ChoiceId },
}

/// One field inside a `VariantConstruct`.
///
/// WHY: payload field names are part of the runtime carrier shape for JS and future backends.
#[derive(Debug, Clone)]
pub struct HirVariantField {
    pub name: Option<StringId>,
    pub value: HirExpression,
}

#[derive(Debug, Clone)]
pub struct HirExpression {
    pub id: HirValueId,
    pub kind: HirExpressionKind,
    pub ty: TypeId,
    pub value_kind: ValueKind,
    pub region: RegionId,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueKind {
    /// Refers to a memory location.
    Place,

    /// Produces a value.
    RValue,

    /// Compile-time constant.
    Const,
}

#[derive(Debug, Clone)]
pub enum HirExpressionKind {
    // --------------------------------------------------------
    // Literals
    // --------------------------------------------------------
    Int(i64),
    Float(f64),
    Bool(bool),
    Char(char),
    StringLiteral(String),

    // --------------------------------------------------------
    // Memory
    // --------------------------------------------------------
    Load(HirPlace),
    Copy(HirPlace),

    // --------------------------------------------------------
    // Operations
    // --------------------------------------------------------
    BinOp {
        left: Box<HirExpression>,
        op: HirBinOp,
        right: Box<HirExpression>,
    },

    UnaryOp {
        op: HirUnaryOp,
        operand: Box<HirExpression>,
    },

    // --------------------------------------------------------
    // Construction
    // --------------------------------------------------------
    StructConstruct {
        struct_id: StructId,
        fields: Vec<(FieldId, HirExpression)>,
    },

    Collection(Vec<HirExpression>),

    Range {
        start: Box<HirExpression>,
        end: Box<HirExpression>,
    },

    /// Construct a tuple value (for multi-return)
    /// Example: return (42, "hello")
    /// EMPTY TUPLE IS THE UNIT TYPE ()
    /// EMPTY TUPLE == DataType::None
    TupleConstruct {
        elements: Vec<HirExpression>,
    },

    /// Project a tuple slot by flat index.
    TupleGet {
        tuple: Box<HirExpression>,
        index: usize,
    },

    ///Construct an Option value
    /// - Some variant: value must be Some(expr)
    /// - None variant: value must be None
    OptionConstruct {
        variant: OptionVariant,
        value: Option<Box<HirExpression>>, // None for None variant, Some for Some variant
    },

    /// Construct a Result value
    /// Example: Ok(42) or Err("error")
    ResultConstruct {
        variant: ResultVariant,
        value: Box<HirExpression>, // The wrapped value
    },

    /// Unwraps an internal Result value for `call(...)!` propagation:
    /// - Ok(v)  => evaluates to v
    /// - Err(e) => propagates through the current function's error channel
    ResultPropagate {
        result: Box<HirExpression>,
    },

    /// Checks whether an internal Result carrier currently holds an Ok value.
    ResultIsOk {
        result: Box<HirExpression>,
    },

    /// Extracts the Ok payload from an internal Result carrier.
    ResultUnwrapOk {
        result: Box<HirExpression>,
    },

    /// Extracts the Err payload from an internal Result carrier.
    ResultUnwrapErr {
        result: Box<HirExpression>,
    },

    BuiltinCast {
        kind: HirBuiltinCastKind,
        value: Box<HirExpression>,
    },

    /// Construct a variant value through a shared carrier.
    ///
    /// WHY: unifies choice/option/result construction in HIR while keeping type kinds distinct.
    /// Phase 3: used for choices. Phase 6: will expand to Option/Result.
    VariantConstruct {
        carrier: HirVariantCarrier,
        variant_index: usize,
        fields: Vec<HirVariantField>,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum OptionVariant {
    #[allow(dead_code)]
    // Kept until alpha Option<T> lowering emits explicit Some carriers.
    Some,
    None,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HirBuiltinCastKind {
    Int,
    Float,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ResultVariant {
    Ok,
    Err,
}
