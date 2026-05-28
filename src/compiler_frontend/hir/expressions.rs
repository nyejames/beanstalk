//! HIR expressions.
//!
//! WHAT: typed value-producing nodes used by statements, terminators, and pattern matching.
//! WHY: HIR keeps normal value construction as expression trees while control flow stays explicit.

use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::hir::ids::{ChoiceId, FieldId, HirValueId, RegionId, StructId};
use crate::compiler_frontend::hir::operators::{HirBinOp, HirUnaryOp};
use crate::compiler_frontend::hir::places::HirPlace;
use crate::compiler_frontend::symbols::string_interning::StringId;

/// Shared carrier tag for variant construction in HIR.
///
/// WHY: Choices, Options, and Results all construct variant-shaped values. A shared carrier
/// keeps backend lowering uniform while preserving distinct semantic type identity.
#[derive(Debug, Clone)]
pub enum HirVariantCarrier {
    Choice { choice_id: ChoiceId },
    Option,
    Fallible,
}

/// Variant index used for `some` inside `HirVariantCarrier::Option`.
pub const OPTION_SOME_VARIANT_INDEX: usize = 1;

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
    // -------------------------
    //  Literals
    // -------------------------
    Int(i64),
    Float(f64),
    Bool(bool),
    Char(char),
    StringLiteral(String),

    // -------------------------
    //  Memory & Data Flow
    // -------------------------
    Load(HirPlace),
    Copy(HirPlace),

    // -------------------------
    //  Operations
    // -------------------------
    BinOp {
        left: Box<HirExpression>,
        op: HirBinOp,
        right: Box<HirExpression>,
    },

    UnaryOp {
        op: HirUnaryOp,
        operand: Box<HirExpression>,
    },

    // -------------------------
    //  Object Construction
    // -------------------------
    StructConstruct {
        struct_id: StructId,
        fields: Vec<(FieldId, HirExpression)>,
    },

    Collection(Vec<HirExpression>),

    Range {
        start: Box<HirExpression>,
        end: Box<HirExpression>,
    },

    // -------------------------
    //  Tuple Operations
    // -------------------------
    /// Construct a tuple value (for multi-return)
    /// Example: return (42, "hello")
    /// EMPTY TUPLE IS THE UNIT TYPE ()
    /// EMPTY TUPLE == the builtin none `TypeId`.
    TupleConstruct {
        elements: Vec<HirExpression>,
    },

    /// Project a tuple slot by flat index.
    TupleGet {
        tuple: Box<HirExpression>,
        index: usize,
    },

    // -------------------------
    //  Fallible Carrier Handling
    // -------------------------
    /// Extracts the success payload from an internal fallible carrier.
    FallibleUnwrapSuccess {
        result: Box<HirExpression>,
    },

    /// Extracts the error payload from an internal fallible carrier.
    FallibleUnwrapError {
        result: Box<HirExpression>,
    },

    // -------------------------
    //  Type Conversion
    // -------------------------
    BuiltinCast {
        kind: HirBuiltinCastKind,
        value: Box<HirExpression>,
    },

    // -------------------------
    //  Variant Operations
    // -------------------------
    /// Construct a variant value through a shared carrier.
    ///
    /// WHY: unifies choice/option/result construction in HIR while keeping type kinds distinct.
    VariantConstruct {
        carrier: HirVariantCarrier,
        variant_index: usize,
        fields: Vec<HirVariantField>,
    },

    /// Extract a payload field from a variant-shaped value.
    ///
    /// WHY: match-arm capture bindings are materialized as local assignments from the
    /// scrutinee. Using a dedicated HIR expression keeps backend lowering uniform and
    /// preserves the carrier type for field-name resolution.
    VariantPayloadGet {
        carrier: HirVariantCarrier,
        source: Box<HirExpression>,
        variant_index: usize,
        field_index: usize,
    },
}

// Option none/some are represented through VariantConstruct with HirVariantCarrier::Option.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HirBuiltinCastKind {
    Int,
    Float,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FallibleCarrierVariant {
    Success,
    Error,
}
