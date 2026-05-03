//! Header-owned module symbol package.
//!
//! WHAT: defines `ModuleSymbols`, the header-owned symbol metadata package built during header
//! parsing. Dependency sorting fills its complete sorted declaration placeholder list.
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
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::generics::GenericParameterList;
use crate::compiler_frontend::declaration_syntax::declaration_shell::DeclarationSyntax;
use crate::compiler_frontend::external_packages::ExternalSymbolId;
use crate::compiler_frontend::headers::parse_file_headers::{FileImport, Header, HeaderKind};
use crate::compiler_frontend::headers::types::FileReExport;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::value_mode::ValueMode;
use crate::projects::settings::IMPLICIT_START_FUNC_NAME;
use rustc_hash::{FxHashMap, FxHashSet};

/// Resolved target of a facade export entry.
///
/// WHAT: a facade export can expose either a source declaration (via `#` or re-export)
/// or an external package symbol (via re-export of a virtual package).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum FacadeExportTarget {
    Source(InternedPath),
    External(ExternalSymbolId),
}

/// One exported symbol in a module facade.
///
/// WHAT: records the name that external importers use and the resolved target.
/// WHY: re-export aliases can expose a symbol under a different name than its canonical path.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FacadeExportEntry {
    pub export_name: StringId,
    pub target: FacadeExportTarget,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum GenericDeclarationKind {
    Function,
    Struct,
    Choice,
    TypeAlias,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GenericDeclarationMetadata {
    pub(crate) kind: GenericDeclarationKind,
    pub(crate) parameters: GenericParameterList,
    pub(crate) declaration_location: SourceLocation,
}

/// Header-owned module symbol package.
///
/// WHAT: carries top-level declaration placeholders, per-file import/export metadata, and builtin
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
    pub(crate) type_alias_paths: FxHashSet<InternedPath>,
    pub(crate) generic_declarations_by_path: FxHashMap<InternedPath, GenericDeclarationMetadata>,

    // Builtin data merged during header parsing.
    pub(crate) builtin_visible_symbol_paths: FxHashSet<InternedPath>,
    pub(crate) builtin_struct_ast_nodes: Vec<AstNode>,
    pub(crate) resolved_struct_fields_by_path: FxHashMap<InternedPath, Vec<Declaration>>,
    pub(crate) struct_source_by_path: FxHashMap<InternedPath, InternedPath>,

    // Facade data: maps source-library import prefix to exported module-facade entries.
    // Each entry records the export name (which may differ from the target path name via alias)
    // and the resolved target (source symbol path or external symbol id).
    pub(crate) facade_exports: FxHashMap<String, FxHashSet<FacadeExportEntry>>,
    // Maps source file logical path to its library prefix, if the file belongs to a source library.
    pub(crate) file_library_membership: FxHashMap<InternedPath, String>,
    // Re-export clauses collected from each source file during header parsing.
    // Only `#mod.bst` files should contain entries; others are rejected during parsing.
    pub(crate) file_re_exports_by_source: FxHashMap<InternedPath, Vec<FileReExport>>,

    // Module root membership for entry-root files (not source libraries).
    // Maps file path (logical or canonical) to its module root path.
    pub(crate) file_module_membership: FxHashMap<InternedPath, InternedPath>,
    // Facade exports for entry-root module roots, keyed by module root path.
    pub(crate) module_root_facade_exports: FxHashMap<InternedPath, FxHashSet<FacadeExportEntry>>,
    // Module root prefixes relative to the entry root, sorted longest first.
    // Used for intercepting cross-module imports before file resolution.
    pub(crate) module_root_prefixes: Vec<(InternedPath, InternedPath)>,
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
            builtin_visible_symbol_paths: FxHashSet::default(),
            builtin_struct_ast_nodes: Vec::new(),
            resolved_struct_fields_by_path: FxHashMap::default(),
            struct_source_by_path: FxHashMap::default(),
            type_alias_paths: FxHashSet::default(),
            generic_declarations_by_path: FxHashMap::default(),
            facade_exports: FxHashMap::default(),
            file_library_membership: FxHashMap::default(),
            file_re_exports_by_source: FxHashMap::default(),
            file_module_membership: FxHashMap::default(),
            module_root_facade_exports: FxHashMap::default(),
            module_root_prefixes: Vec::new(),
        }
    }

    /// Build the complete sorted declaration placeholder list from topologically ordered headers
    /// and append staged builtin declarations.
    ///
    /// WHAT: iterates the already-sorted headers to create `Declaration` placeholders for every
    /// top-level header kind (except ConstTemplate), then appends the builtin declarations that
    /// were staged during header parsing.
    ///
    /// WHY: declarations must be in the same topological order as the sorted headers so that
    /// all AST passes see dependencies before dependents. The order-independent maps were already
    /// built during `parse_headers`; only this Vec requires sorted input.
    pub(crate) fn build_sorted_declarations(
        &mut self,
        sorted_headers: &[Header],
        string_table: &mut StringTable,
    ) {
        self.declarations.clear();

        for header in sorted_headers {
            if let Some(declaration) = declaration_from_header(header, string_table) {
                self.declarations.push(declaration);
            }
        }

        // Append staged builtin declarations after all user-defined declarations.
        self.declarations.append(&mut self.builtin_declarations);
    }
}

fn declaration_from_header(header: &Header, string_table: &mut StringTable) -> Option<Declaration> {
    match &header.kind {
        HeaderKind::Function { signature, .. } => Some(Declaration {
            id: header.tokens.src_path.to_owned(),
            value: Expression::new(
                ExpressionKind::NoValue,
                header.name_location.to_owned(),
                DataType::Function(Box::new(None), signature.to_owned()),
                ValueMode::ImmutableReference,
            ),
        }),
        HeaderKind::Constant { declaration, .. } => Some(constant_declaration_placeholder(
            &header.tokens.src_path,
            declaration,
            &header.name_location,
        )),
        HeaderKind::Struct { fields, .. } => Some(Declaration {
            id: header.tokens.src_path.to_owned(),
            value: Expression::new(
                ExpressionKind::NoValue,
                header.name_location.to_owned(),
                DataType::runtime_struct(header.tokens.src_path.to_owned(), fields.to_owned()),
                ValueMode::ImmutableReference,
            ),
        }),
        HeaderKind::Choice { variants, .. } => Some(Declaration {
            id: header.tokens.src_path.to_owned(),
            value: Expression::new(
                ExpressionKind::NoValue,
                header.name_location.to_owned(),
                DataType::Choices {
                    nominal_path: header.tokens.src_path.to_owned(),
                    variants: variants.to_owned(),
                    generic_instance_key: None,
                },
                ValueMode::ImmutableReference,
            ),
        }),
        HeaderKind::StartFunction => {
            let start_name = header
                .source_file
                .join_str(IMPLICIT_START_FUNC_NAME, string_table);
            Some(Declaration {
                id: start_name.to_owned(),
                value: Expression::new(
                    ExpressionKind::NoValue,
                    header.name_location.to_owned(),
                    DataType::Function(
                        Box::new(None),
                        FunctionSignature {
                            parameters: vec![],
                            returns: vec![ReturnSlot::success(FunctionReturn::Value(
                                DataType::collection(DataType::StringSlice),
                            ))],
                        },
                    ),
                    ValueMode::ImmutableReference,
                ),
            })
        }
        HeaderKind::TypeAlias { .. } => None,
        HeaderKind::ConstTemplate => None,
    }
}

fn constant_declaration_placeholder(
    path: &InternedPath,
    declaration: &DeclarationSyntax,
    location: &crate::compiler_frontend::tokenizer::tokens::SourceLocation,
) -> Declaration {
    Declaration {
        id: path.to_owned(),
        value: Expression::new(
            ExpressionKind::NoValue,
            location.to_owned(),
            declaration.semantic_type(),
            declaration.value_mode(),
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
