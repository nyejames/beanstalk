//! Parsed type syntax before resolution.
//!
//! WHAT: AST type annotations start as `ParsedTypeRef` and are resolved
//!      into `TypeId` by the type-resolution pass.
//! WHY: unresolved names, inferred positions, and source spelling must not
//!      be confused with resolved semantic type identity.

use crate::compiler_frontend::symbols::string_interning::{StringId, StringIdRemap};
use crate::compiler_frontend::tokenizer::tokens::{SourceLocation, Token};

/// Parsed fixed-capacity expression syntax before semantic folding.
///
/// WHAT: stores the exact token slice and primary source location for a collection capacity
///       expression such as `64` or `capacity + 16`.
/// WHY: the parser owns syntax shape only. AST type resolution later folds these tokens into
///      the canonical fixed collection capacity once constants and types are available.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ParsedCollectionCapacity {
    pub(crate) tokens: Vec<Token>,
    pub(crate) location: SourceLocation,
}

/// Parsed type annotation before resolution.
///
/// Does NOT represent semantic type identity.
#[derive(Debug, Clone, PartialEq)]
pub enum ParsedTypeRef {
    // -----------------
    //  Meta-types
    // -----------------
    Inferred,

    Named {
        name: StringId,
        location: SourceLocation,
    },

    Namespaced {
        namespace: StringId,
        name: StringId,
        location: SourceLocation,
    },

    Applied {
        base: Box<ParsedTypeRef>,
        arguments: Vec<ParsedTypeRef>,
        location: SourceLocation,
    },

    // -----------------
    //  Builtin Types
    // -----------------
    BuiltinBool {
        location: SourceLocation,
    },

    BuiltinInt {
        location: SourceLocation,
    },

    BuiltinFloat {
        location: SourceLocation,
    },

    #[allow(dead_code)] // Planned: explicit Decimal literal/type surface.
    BuiltinDecimal {
        location: SourceLocation,
    },

    BuiltinString {
        location: SourceLocation,
    },

    BuiltinChar {
        location: SourceLocation,
    },

    #[allow(dead_code)] // Planned: explicit None literal/type flows.
    BuiltinNone {
        location: SourceLocation,
    },

    // -----------------
    //  Trait-local Types
    // -----------------
    This {
        location: SourceLocation,
    },

    // -----------------
    //  Constructed Types
    // -----------------
    Collection {
        element: Box<ParsedTypeRef>,
        location: SourceLocation,
        fixed_capacity: Option<ParsedCollectionCapacity>,
    },

    Map {
        key: Box<ParsedTypeRef>,
        value: Box<ParsedTypeRef>,
        location: SourceLocation,
    },

    Optional {
        inner: Box<ParsedTypeRef>,
        location: SourceLocation,
    },

    #[allow(dead_code)] // Planned: explicit Result<T, E> type syntax.
    Result {
        ok: Box<ParsedTypeRef>,
        err: Box<ParsedTypeRef>,
        location: SourceLocation,
    },
}

impl ParsedTypeRef {
    /// Remap all interned string IDs in this parsed type reference into a merged string table.
    ///
    /// WHAT: updates `name` IDs and every `SourceLocation` recursively through `Applied`,
    ///       `Collection`, `Optional`, and `Result` variants.
    /// WHY: per-file header parsing produces `ParsedTypeRef` values using local string tables;
    ///      remapping keeps them valid after merge into the module/global table.
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        match self {
            ParsedTypeRef::Inferred => {}

            ParsedTypeRef::Named { name, location } => {
                *name = remap.get(*name);
                location.remap_string_ids(remap);
            }
            ParsedTypeRef::Namespaced {
                namespace,
                name,
                location,
            } => {
                *namespace = remap.get(*namespace);
                *name = remap.get(*name);
                location.remap_string_ids(remap);
            }

            ParsedTypeRef::Applied {
                base,
                arguments,
                location,
            } => {
                base.remap_string_ids(remap);
                for argument in arguments {
                    argument.remap_string_ids(remap);
                }
                location.remap_string_ids(remap);
            }

            ParsedTypeRef::BuiltinBool { location }
            | ParsedTypeRef::BuiltinInt { location }
            | ParsedTypeRef::BuiltinFloat { location }
            | ParsedTypeRef::BuiltinDecimal { location }
            | ParsedTypeRef::BuiltinString { location }
            | ParsedTypeRef::BuiltinChar { location }
            | ParsedTypeRef::BuiltinNone { location }
            | ParsedTypeRef::This { location } => {
                location.remap_string_ids(remap);
            }

            ParsedTypeRef::Collection {
                element,
                location,
                fixed_capacity,
            } => {
                element.remap_string_ids(remap);
                location.remap_string_ids(remap);
                if let Some(capacity) = fixed_capacity {
                    capacity.location.remap_string_ids(remap);
                    for token in &mut capacity.tokens {
                        token.remap_string_ids(remap);
                    }
                }
            }

            ParsedTypeRef::Map {
                key,
                value,
                location,
            } => {
                key.remap_string_ids(remap);
                value.remap_string_ids(remap);
                location.remap_string_ids(remap);
            }

            ParsedTypeRef::Optional { inner, location } => {
                inner.remap_string_ids(remap);
                location.remap_string_ids(remap);
            }

            ParsedTypeRef::Result { ok, err, location } => {
                ok.remap_string_ids(remap);
                err.remap_string_ids(remap);
                location.remap_string_ids(remap);
            }
        }
    }
}
