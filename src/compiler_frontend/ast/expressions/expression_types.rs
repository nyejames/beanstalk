//! Supporting expression type contracts shared by AST parsing, folding, and HIR lowering.
//!
//! WHAT: stores small semantic enums that describe constant classification,
//! fallible-expression handling, compiler built-in casts, and temporary fallible carrier variants.
//! WHY: these contracts are used across expression construction and lowering,
//! but they are not the expression value shape itself.

use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringIdRemap};

/// Value-level classification for const-record semantics.
///
/// WHAT: distinguishes a compile-time const-record value from a normal runtime value.
/// WHY: const-record status is a value fact, not a type identity. It must not be encoded
///      in `DataType` because the same struct type can produce both runtime instances and
///      const-record values depending on the construction context.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConstRecordState {
    RuntimeValue,
    ConstRecord,
}

/// Classification of an expression's compile-time foldability.
///
/// WHAT: distinguishes scalar literals, composite aggregates, template shapes, and
///       non-constant values so that const folding and HIR lowering can decide
///       whether an expression can be evaluated at compile time.
/// WHY: this is a value-shape classification, not a type identity. The same type
///      can produce both `Literal` and `NonConst` values depending on the expression.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConstValueKind {
    /// Atomic scalar literal (int, float, string slice, bool, char, or path).
    Literal,

    /// Aggregate value whose fields or elements are all compile-time constants.
    ///
    /// Covers struct instances, choice constructors, collections, ranges, and
    /// fallible carriers when every sub-expression is itself constant.
    Composite,

    /// Template that can be fully rendered to a constant string at compile time.
    RenderableTemplate,

    /// Template with unresolved slots that acts as a compile-time wrapper value.
    ///
    /// The wrapper can be filled later at an active fill site; it does not
    /// render to a backend-facing string on its own.
    TemplateWrapper,

    /// Slot-insertion template helper value.
    ///
    /// Produced by `TemplateType::SlotInsert` syntax. Valid only when consumed
    /// by an active wrapper fill site; otherwise treated as non-constant.
    SlotInsertTemplate,

    /// Not a compile-time constant.
    NonConst,
}

impl ConstValueKind {
    pub fn is_compile_time_value(self) -> bool {
        !matches!(self, Self::NonConst)
    }
}

/// How a fallible expression or call is handled at the use site.
///
/// WHAT: captures the two forms of fallible handling written by the user:
///       propagate (`!`) or catch with an optional error binding and fallback.
/// WHY: HIR lowering needs the full handling shape to build the correct
///      success/error branch structure without re-parsing source syntax.
#[derive(Clone, Debug)]
pub enum FallibleHandling {
    /// The `!` operator: propagate the error upward to the caller.
    Propagate,

    /// A `catch` block with an optional error binding and handler body.
    Handler {
        /// Optional name and path binding for the caught error value.
        error: Option<CatchErrorBinding>,

        /// Handler body nodes executed when the fallible value is an error.
        ///
        /// Value-producing catch blocks keep `ThenValue` statements in this body so HIR
        /// lowering can route them through the shared active value target.
        body: Vec<AstNode>,
    },
}

/// Name and path binding for a caught error in a `catch` handler.
#[derive(Clone, Debug)]
pub struct CatchErrorBinding {
    pub error_name: StringId,
    pub error_binding: InternedPath,
}

/// Built-in scalar cast targets for explicit or inferred conversions.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuiltinCastKind {
    /// Cast to a signed or unsigned integer.
    Int,

    /// Cast to a floating-point value.
    Float,
}

impl FallibleHandling {
    /// Remap error binding names and body AST nodes in this handling shape.
    ///
    /// Called by per-file frontend output remapping before module-wide dependency sorting.
    #[allow(dead_code)]
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        match self {
            FallibleHandling::Propagate => {}

            FallibleHandling::Handler { error, body } => {
                if let Some(error_binding) = error {
                    error_binding.remap_string_ids(remap);
                }
                for node in body {
                    node.remap_string_ids(remap);
                }
            }
        }
    }
}

impl CatchErrorBinding {
    /// Remap error name and binding path.
    ///
    /// Called by per-file frontend output remapping before module-wide dependency sorting.
    #[allow(dead_code)]
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.error_name = remap.get(self.error_name);
        self.error_binding.remap_string_ids(remap);
    }
}

/// Success or error variant for fallible carrier construction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FallibleCarrierVariant {
    Success,
    Error,
}
