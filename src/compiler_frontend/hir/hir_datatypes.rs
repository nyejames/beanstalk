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

use crate::compiler_frontend::hir::hir_nodes::StructId;

/// Stable identifier for a canonical HIR type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TypeId(pub u32);

/// Canonical type container.
///
/// Additional metadata (layout, drop flags, region bounds)
/// can be attached here later without changing the IR.
#[derive(Debug, Clone)]
pub struct HirType {
    pub kind: TypeKind,
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
#[derive(Debug, Default)]
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
}

// ============================================================
// Type Kinds
// ============================================================

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum TypeKind {
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
    Option { inner: TypeId },

    /// Result wraps any type (including Tuple)
    /// fn || -> Int, String!  becomes  Result { ok: Tuple { ... }, err: ErrorType }
    Result { ok: TypeId, err: TypeId },

    /// Tagged union.
    ///
    /// Variants are types, not AST-level "choices".
    Union {
        variants: Vec<TypeId>,
    },
}
