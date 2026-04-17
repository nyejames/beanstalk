//! Header-owned module symbol package.
//!
//! WHAT: defines `ModuleSymbols`, the top-level symbol collection built during header parsing and
//! finalized (sorted declarations) during dependency sorting.
//! WHY: top-level symbol discovery is owned by the header stage. `ModuleSymbols` carries that
//! knowledge forward so dependency sorting and AST construction consume it directly without
//! re-iterating headers or running a separate manifest-building pass.
//!
//! ## Ownership split
//!
//! Header parsing owns:
//! - Top-level symbol discovery and metadata collection
//! - Builtin/reserved symbol registration
//!
//! Dependency sorting owns:
//! - Reconstruction of `declarations` in topologically sorted header order
//!
//! AST owns:
//! - Import visibility resolution
//! - Type/constant/signature resolution
//! - Receiver-method catalog construction
//! - Body lowering and template normalization

use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::statements::functions::{
    FunctionReturn, FunctionSignature, ReturnSlot,
};
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::declaration_syntax::declaration_shell::DeclarationSyntax;
use crate::compiler_frontend::headers::parse_file_headers::{FileImport, Header, HeaderKind};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::projects::settings::IMPLICIT_START_FUNC_NAME;
use rustc_hash::{FxHashMap, FxHashSet};

/// Header-owned module symbol package.
///
/// WHAT: carries every top-level declaration stub, per-file import/export metadata, and builtin
/// type data needed by all AST passes.
///
/// WHY: header parsing discovers top-level symbols once; dependency sorting finalises the
/// `declarations` order; AST receives this as a complete, pre-built package and does not
/// re-iterate headers to discover symbols.
///
/// ## Field lifetimes
///
/// - All order-independent maps are populated by `parse_headers` and stay unchanged thereafter.
/// - `builtin_declarations` is populated by `parse_headers` and consumed (appended into
///   `declarations`) by `resolve_module_dependencies`.
/// - `declarations` is empty after `parse_headers` and filled by `resolve_module_dependencies`.
#[derive(Debug)]
pub(crate) struct ModuleSymbols {
    // Declarations in sorted-header order.
    // Empty until resolve_module_dependencies completes; do not read before sorting.
    pub(crate) declarations: Vec<Declaration>,

    // Staging: builtin declarations collected during header parsing.
    // Consumed by resolve_module_dependencies (appended to declarations). Empty after sorting.
    pub(crate) builtin_declarations: Vec<Declaration>,

    // Order-independent maps built during header parsing.
    pub(crate) canonical_source_by_symbol_path: FxHashMap<InternedPath, InternedPath>,
    pub(crate) module_file_paths: FxHashSet<InternedPath>,
    pub(crate) file_imports_by_source: FxHashMap<InternedPath, Vec<FileImport>>,
    pub(crate) importable_symbol_exported: FxHashMap<InternedPath, bool>,
    pub(crate) declared_paths_by_file: FxHashMap<InternedPath, FxHashSet<InternedPath>>,
    pub(crate) declared_names_by_file: FxHashMap<InternedPath, FxHashSet<StringId>>,
    pub(crate) declaration_stubs_by_path: FxHashMap<InternedPath, DeclarationStub>,

    // Builtin data merged during header parsing.
    pub(crate) builtin_visible_symbol_paths: FxHashSet<InternedPath>,
    pub(crate) builtin_struct_ast_nodes: Vec<AstNode>,
    pub(crate) resolved_struct_fields_by_path: FxHashMap<InternedPath, Vec<Declaration>>,
    pub(crate) struct_source_by_path: FxHashMap<InternedPath, InternedPath>,
}

impl ModuleSymbols {
    pub(crate) fn empty() -> Self {
        Self {
            declarations: Vec::new(),
            builtin_declarations: Vec::new(),
            canonical_source_by_symbol_path: FxHashMap::default(),
            module_file_paths: FxHashSet::default(),
            file_imports_by_source: FxHashMap::default(),
            importable_symbol_exported: FxHashMap::default(),
            declared_paths_by_file: FxHashMap::default(),
            declared_names_by_file: FxHashMap::default(),
            declaration_stubs_by_path: FxHashMap::default(),
            builtin_visible_symbol_paths: FxHashSet::default(),
            builtin_struct_ast_nodes: Vec::new(),
            resolved_struct_fields_by_path: FxHashMap::default(),
            struct_source_by_path: FxHashMap::default(),
        }
    }

    /// Build declarations in sorted-header order and append the staged builtin declarations.
    ///
    /// WHAT: iterates the already-sorted headers to create `Declaration` stubs for Function,
    /// Choice, and StartFunction headers, then appends the builtin declarations that were staged
    /// during header parsing.
    ///
    /// WHY: declarations must be in the same topological order as the sorted headers so that
    /// all AST passes see dependencies before dependents. The order-independent maps were already
    /// built during `parse_headers`; only this Vec requires sorted input.
    pub(crate) fn build_sorted_declarations(
        &mut self,
        sorted_headers: &[Header],
        string_table: &mut StringTable,
    ) {
        for header in sorted_headers {
            if let Some(stub) = declaration_stub_from_header(header, string_table)
                && matches!(
                    stub.kind,
                    DeclarationStubKind::Function
                        | DeclarationStubKind::Choice
                        | DeclarationStubKind::StartFunction
                )
            {
                self.declarations.push(stub.declaration);
            }
        }

        // Append staged builtin declarations after all user-defined declarations.
        self.declarations.append(&mut self.builtin_declarations);
    }

    pub(crate) fn seed_declaration_stubs(
        &mut self,
        headers: &[Header],
        string_table: &mut StringTable,
    ) {
        self.declaration_stubs_by_path.clear();

        for header in headers {
            if let Some(stub) = declaration_stub_from_header(header, string_table) {
                self.declaration_stubs_by_path
                    .insert(stub.path.to_owned(), stub);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DeclarationStubKind {
    Function,
    Constant,
    Struct,
    Choice,
    StartFunction,
}

#[derive(Debug, Clone)]
pub(crate) struct DeclarationStub {
    pub(crate) path: InternedPath,
    pub(crate) kind: DeclarationStubKind,
    pub(crate) declaration: Declaration,
}

fn declaration_stub_from_header(
    header: &Header,
    string_table: &mut StringTable,
) -> Option<DeclarationStub> {
    match &header.kind {
        HeaderKind::Function { signature } => Some(DeclarationStub {
            path: header.tokens.src_path.to_owned(),
            kind: DeclarationStubKind::Function,
            declaration: Declaration {
                id: header.tokens.src_path.to_owned(),
                value: Expression::new(
                    ExpressionKind::NoValue,
                    header.name_location.to_owned(),
                    DataType::Function(Box::new(None), signature.to_owned()),
                    Ownership::ImmutableReference,
                ),
            },
        }),
        HeaderKind::Constant { declaration } => Some(DeclarationStub {
            path: header.tokens.src_path.to_owned(),
            kind: DeclarationStubKind::Constant,
            declaration: constant_declaration_stub(
                &header.tokens.src_path,
                declaration,
                &header.name_location,
            ),
        }),
        HeaderKind::Struct { fields } => Some(DeclarationStub {
            path: header.tokens.src_path.to_owned(),
            kind: DeclarationStubKind::Struct,
            declaration: Declaration {
                id: header.tokens.src_path.to_owned(),
                value: Expression::new(
                    ExpressionKind::NoValue,
                    header.name_location.to_owned(),
                    DataType::runtime_struct(
                        header.tokens.src_path.to_owned(),
                        fields.to_owned(),
                        Ownership::MutableOwned,
                    ),
                    Ownership::ImmutableReference,
                ),
            },
        }),
        HeaderKind::Choice { variants } => Some(DeclarationStub {
            path: header.tokens.src_path.to_owned(),
            kind: DeclarationStubKind::Choice,
            declaration: Declaration {
                id: header.tokens.src_path.to_owned(),
                value: Expression::new(
                    ExpressionKind::NoValue,
                    header.name_location.to_owned(),
                    DataType::Choices {
                        nominal_path: header.tokens.src_path.to_owned(),
                        variants: variants.to_owned(),
                    },
                    Ownership::ImmutableReference,
                ),
            },
        }),
        HeaderKind::StartFunction => {
            let start_name = header
                .source_file
                .join_str(IMPLICIT_START_FUNC_NAME, string_table);
            Some(DeclarationStub {
                path: start_name.to_owned(),
                kind: DeclarationStubKind::StartFunction,
                declaration: Declaration {
                    id: start_name,
                    value: Expression::new(
                        ExpressionKind::NoValue,
                        header.name_location.to_owned(),
                        DataType::Function(
                            Box::new(None),
                            FunctionSignature {
                                parameters: vec![],
                                returns: vec![ReturnSlot::success(FunctionReturn::Value(
                                    DataType::Collection(
                                        Box::new(DataType::StringSlice),
                                        Ownership::MutableOwned,
                                    ),
                                ))],
                            },
                        ),
                        Ownership::ImmutableReference,
                    ),
                },
            })
        }
        HeaderKind::ConstTemplate => None,
    }
}

fn constant_declaration_stub(
    path: &InternedPath,
    declaration: &DeclarationSyntax,
    location: &crate::compiler_frontend::tokenizer::tokens::SourceLocation,
) -> Declaration {
    let ownership = if declaration.mutable_marker {
        Ownership::MutableOwned
    } else {
        Ownership::ImmutableOwned
    };

    Declaration {
        id: path.to_owned(),
        value: Expression::new(
            ExpressionKind::NoValue,
            location.to_owned(),
            declaration.to_data_type(&ownership),
            ownership,
        ),
    }
}

/// Register a symbol into the declared-path and declared-name tables.
/// When `exported` is `Some`, also records the symbol's export visibility.
pub(crate) fn register_declared_symbol(
    module_symbols: &mut ModuleSymbols,
    symbol_path: &InternedPath,
    source_file: &InternedPath,
    exported: Option<bool>,
) {
    if let Some(is_exported) = exported {
        module_symbols
            .importable_symbol_exported
            .insert(symbol_path.to_owned(), is_exported);
    }
    module_symbols
        .declared_paths_by_file
        .entry(source_file.to_owned())
        .or_default()
        .insert(symbol_path.to_owned());
    if let Some(name) = symbol_path.name() {
        module_symbols
            .declared_names_by_file
            .entry(source_file.to_owned())
            .or_default()
            .insert(name);
    }
}
