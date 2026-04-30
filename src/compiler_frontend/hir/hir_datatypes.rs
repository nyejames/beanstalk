// ============================================================
// HIR Type System
// ============================================================
//
// This is the canonical type representation used by HIR.
// All types are fully resolved and interned.
// No inference, no AST residue, no surface syntax artifacts.
//
// Types are referenced by TypeId and stored in a TypeContext.
//
// ============================================================

use crate::compiler_frontend::external_packages::ExternalTypeId;
use crate::compiler_frontend::hir::ids::StructId;

/// Stable identifier for a canonical HIR type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeId(pub u32);

/// Canonical type container.
///
/// Additional metadata (layout, drop flags, region bounds)
/// can be attached here later without changing the IR.
#[derive(Debug, Clone)]
pub struct HirType {
    pub kind: HirTypeKind,
    // Future extension points:
    //
    // - Cached layout information
    // - Drop semantics flags
    // - Region bounds
    // - ABI classification
    //
    // Keep empty for now.
}

/// Central type storage.
///
/// Guarantees canonical identity of all types.
/// If two expressions have the same TypeId,
/// they are exactly the same type.
#[derive(Debug, Clone, Default)]
pub struct TypeContext {
    types: Vec<HirType>,
}

impl TypeContext {
    /// Inserts a new canonical type.
    /// Caller is responsible for interning/deduplicating if desired.
    pub fn insert(&mut self, ty: HirType) -> TypeId {
        let id = TypeId(self.types.len() as u32);
        self.types.push(ty);
        id
    }

    pub fn get(&self, id: TypeId) -> &HirType {
        &self.types[id.0 as usize]
    }

    pub fn contains(&self, id: TypeId) -> bool {
        (id.0 as usize) < self.types.len()
    }

    #[allow(dead_code)] // Planned: diagnostics/debug type-table sizing helpers.
    pub fn len(&self) -> usize {
        self.types.len()
    }
}

// ============================================================
// Type Kinds
// ============================================================

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum HirTypeKind {
    // --------------------------------------------------------
    // Primitive Types
    // --------------------------------------------------------
    Bool,
    Int,
    Float,
    Decimal,
    Char,
    String,
    Range,

    /// The unit type (no value).
    /// Replaces AST-level `None` pseudo-types.
    Unit,

    // --------------------------------------------------------
    // Compound Types
    // --------------------------------------------------------
    /// Multiple return values (Go-style)
    /// This is the CANONICAL representation of `fn || -> Int, String`
    Tuple {
        fields: Vec<TypeId>,
    },

    /// Dynamically sized homogeneous collection.
    Collection {
        element: TypeId,
    },

    /// Fully resolved struct type.
    Struct {
        struct_id: StructId,
    },

    /// Function type.
    ///
    /// `receiver` is present for method-style functions.
    Function {
        receiver: Option<TypeId>,
        params: Vec<TypeId>,
        returns: Vec<TypeId>,
    },

    /// Option wraps any type (including Tuple)
    /// fn || -> Int, String?  becomes  Option { inner: Tuple { ... } }
    Option {
        inner: TypeId,
    },

    /// Result wraps any type (including Tuple)
    /// fn || -> Int, String!  becomes  Result { ok: Tuple { ... }, err: ErrorType }
    #[allow(dead_code)] // Planned: typed Result<T, E> lowering in later HIR passes.
    Result {
        ok: TypeId,
        err: TypeId,
    },

    /// Nominal choice type.
    ///
    /// WHY: choices are closed nominal variant carriers. Payload metadata lives in the
    /// module choice registry so expressions can keep a compact `ChoiceId`.
    Choice {
        choice_id: crate::compiler_frontend::hir::ids::ChoiceId,
    },

    /// Opaque external type provided by a platform package.
    ///
    /// WHY: external types are nominal and sealed; they carry no field or variant
    /// information in HIR. Backends classify them as `HeapAllocated`.
    External {
        type_id: ExternalTypeId,
    },
}

/// Backend-agnostic classification of a HIR type.
///
/// WHAT: collapses the full `HirTypeKind` taxonomy into the coarse categories backends care about
/// (scalar vs heap vs void vs function) so each backend only needs a small match table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HirTypeClass {
    Unit,
    Bool,
    Char,
    Int,
    Float,
    Decimal,
    Function,
    HeapAllocated,
}

/// Classifies a `HirTypeKind` into a backend-agnostic category.
pub fn classify_hir_type(kind: &HirTypeKind) -> HirTypeClass {
    match kind {
        HirTypeKind::Unit => HirTypeClass::Unit,
        HirTypeKind::Bool => HirTypeClass::Bool,
        HirTypeKind::Char => HirTypeClass::Char,
        HirTypeKind::Int => HirTypeClass::Int,
        HirTypeKind::Float => HirTypeClass::Float,
        HirTypeKind::Decimal => HirTypeClass::Decimal,
        HirTypeKind::Function { .. } => HirTypeClass::Function,
        HirTypeKind::String
        | HirTypeKind::Range
        | HirTypeKind::Tuple { .. }
        | HirTypeKind::Collection { .. }
        | HirTypeKind::Struct { .. }
        | HirTypeKind::Option { .. }
        | HirTypeKind::Result { .. }
        | HirTypeKind::Choice { .. }
        | HirTypeKind::External { .. } => HirTypeClass::HeapAllocated,
    }
}
