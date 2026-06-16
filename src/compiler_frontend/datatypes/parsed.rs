//! Parsed type syntax before resolution.
//!
//! WHAT: AST type annotations start as `ParsedTypeRef` and are resolved
//!      into `TypeId` by the type-resolution pass.
//! WHY: unresolved names, inferred positions, and source spelling must not
//!      be confused with resolved semantic type identity.

use crate::compiler_frontend::symbols::string_interning::{StringId, StringIdRemap};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

/// Parsed fixed-capacity syntax before semantic folding.
///
/// WHAT: stores the narrow capacity shape the parser accepts: a positive integer literal
///       or a bare constant name. Arithmetic and other general expression forms are
///       rejected at parse time so type resolution only sees canonical capacity forms.
/// WHY: collection capacity identity requires a compile-time-known value, but the parser
///      enforces the literal-or-bare-const rule directly rather than carrying raw tokens.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ParsedCollectionCapacity {
    /// A positive integer literal such as `64`.
    Literal {
        value: i32,
        location: SourceLocation,
    },
    /// A bare visible constant name such as `capacity`.
    BareConstant {
        name: StringId,
        location: SourceLocation,
    },
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

    /// A dotted namespace-qualified type path such as `Canvas.Context` or
    /// `io.input.Input`.
    ///
    /// WHAT: stores the unresolved path components before type resolution walks the
    ///      visible namespace records.
    /// WHY: external package surfaces expose nested namespaces, so type position must
    ///      support arbitrary-depth dotted paths while keeping the syntax representation
    ///      separate from semantic type identity.
    Qualified {
        path: Vec<StringId>,
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
            ParsedTypeRef::Qualified { path, location } => {
                for component in path {
                    *component = remap.get(*component);
                }
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
                    match capacity {
                        ParsedCollectionCapacity::BareConstant { name, location } => {
                            location.remap_string_ids(remap);
                            *name = remap.get(*name);
                        }
                        ParsedCollectionCapacity::Literal { location, .. } => {
                            location.remap_string_ids(remap);
                        }
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
