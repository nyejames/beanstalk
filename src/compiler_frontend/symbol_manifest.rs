//! Frontend-owned top-level symbol manifest.
//!
//! WHAT: builds the module-wide symbol table from sorted headers before AST construction.
//! WHY: header parsing owns top-level declaration discovery; AST should consume a pre-built
//! manifest rather than recollect declarations itself. This restores the correct ownership
//! split described in the compiler design overview.

use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::module_ast::canonical_source_file_for_header;
use crate::compiler_frontend::ast::statements::functions::{
    FunctionReturn, FunctionSignature, ReturnSlot,
};
use crate::compiler_frontend::builtins::error_type::{
    is_reserved_builtin_symbol, register_builtin_error_types,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::headers::parse_file_headers::{FileImport, Header, HeaderKind};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::symbols::identifier_policy::ensure_not_keyword_shadow_identifier;
use crate::projects::settings::IMPLICIT_START_FUNC_NAME;
use rustc_hash::{FxHashMap, FxHashSet};

/// Module-wide top-level symbol manifest, built from sorted headers before AST construction.
///
/// WHAT: contains every top-level declaration stub, visibility table, per-file import metadata,
/// and builtin type data needed by all AST passes.
/// WHY: AST receives this as a complete, pre-built package and does not re-iterate headers to
/// discover symbols. Ownership of declaration discovery stays in the header/dependency stages.
pub(crate) struct SymbolManifest {
    // From headers.
    pub(crate) declarations: Vec<Declaration>,
    pub(crate) canonical_source_by_symbol_path: FxHashMap<InternedPath, InternedPath>,
    pub(crate) module_file_paths: FxHashSet<InternedPath>,
    pub(crate) file_imports_by_source: FxHashMap<InternedPath, Vec<FileImport>>,
    pub(crate) importable_symbol_exported: FxHashMap<InternedPath, bool>,
    pub(crate) declared_paths_by_file: FxHashMap<InternedPath, FxHashSet<InternedPath>>,
    pub(crate) declared_names_by_file: FxHashMap<InternedPath, FxHashSet<StringId>>,
    // From builtins — merged once here so AST does not re-absorb.
    pub(crate) builtin_visible_symbol_paths: FxHashSet<InternedPath>,
    pub(crate) builtin_struct_ast_nodes: Vec<AstNode>,
    pub(crate) resolved_struct_fields_by_path: FxHashMap<InternedPath, Vec<Declaration>>,
    pub(crate) struct_source_by_path: FxHashMap<InternedPath, InternedPath>,
}

/// Build the module-wide symbol manifest from dependency-sorted headers.
///
/// WHAT: validates symbol names, registers every top-level declaration into the manifest tables,
/// and absorbs the builtin error type manifest.
/// WHY: this is the same work previously done by `pass_declarations` inside AST. Moving it here
/// restores the ownership split: headers/dependency-sort own discovery, AST owns lowering.
pub(crate) fn build_symbol_manifest(
    sorted_headers: &[Header],
    string_table: &mut StringTable,
) -> Result<SymbolManifest, CompilerMessages> {
    let mut manifest = SymbolManifest {
        declarations: Vec::new(),
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
    };

    for header in sorted_headers {
        if let Some(symbol_name) = header.tokens.src_path.name() {
            let symbol_name_text = string_table.resolve(symbol_name).to_owned();

            ensure_not_keyword_shadow_identifier(
                &symbol_name_text,
                header.name_location.to_owned(),
                "Module Declaration Collection",
            )
            .map_err(|error| CompilerMessages::from_error_ref(error, string_table))?;

            if is_reserved_builtin_symbol(&symbol_name_text) {
                return Err(CompilerMessages::from_error_ref(
                    CompilerError::new_rule_error(
                        format!("'{symbol_name_text}' is reserved as a builtin language type."),
                        header.name_location.to_owned(),
                    ),
                    string_table,
                ));
            }
        }

        manifest
            .module_file_paths
            .insert(header.source_file.to_owned());
        manifest.canonical_source_by_symbol_path.insert(
            header.tokens.src_path.to_owned(),
            canonical_source_file_for_header(header, string_table),
        );
        manifest
            .file_imports_by_source
            .entry(header.source_file.to_owned())
            .or_insert_with(|| header.file_imports.to_owned());

        match &header.kind {
            HeaderKind::Function { signature } => {
                manifest.declarations.push(Declaration {
                    id: header.tokens.src_path.to_owned(),
                    value: Expression::new(
                        ExpressionKind::NoValue,
                        header.name_location.to_owned(),
                        DataType::Function(Box::new(None), signature.to_owned()),
                        Ownership::ImmutableReference,
                    ),
                });
                register_declared_symbol(
                    &mut manifest,
                    &header.tokens.src_path,
                    &header.source_file,
                    Some(header.exported),
                );
            }
            HeaderKind::Struct { .. } => {
                register_declared_symbol(
                    &mut manifest,
                    &header.tokens.src_path,
                    &header.source_file,
                    Some(header.exported),
                );
            }
            HeaderKind::Choice { metadata } => {
                let variants = metadata
                    .variants
                    .iter()
                    .map(|variant| Declaration {
                        id: header.tokens.src_path.append(variant.name),
                        value: Expression::no_value(
                            variant.location.to_owned(),
                            DataType::None,
                            Ownership::ImmutableOwned,
                        ),
                    })
                    .collect::<Vec<_>>();

                manifest.declarations.push(Declaration {
                    id: header.tokens.src_path.to_owned(),
                    value: Expression::new(
                        ExpressionKind::NoValue,
                        header.name_location.to_owned(),
                        DataType::Choices(variants),
                        Ownership::ImmutableReference,
                    ),
                });
                register_declared_symbol(
                    &mut manifest,
                    &header.tokens.src_path,
                    &header.source_file,
                    Some(header.exported),
                );
            }
            HeaderKind::StartFunction => {
                let start_name = header
                    .source_file
                    .join_str(IMPLICIT_START_FUNC_NAME, string_table);
                // WHAT: entry start() signature uses Collection(StringSlice, MutableOwned),
                //       which is the Beanstalk frontend type for Vec<String>.
                // WHY: must match the signature emitted by pass_emit_nodes for the same
                //      implicit start function so call-site type checking succeeds.
                manifest.declarations.push(Declaration {
                    id: start_name.to_owned(),
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
                });
                // No exported flag for start — matches original pass_declarations behaviour.
                register_declared_symbol(&mut manifest, &start_name, &header.source_file, None);
            }
            HeaderKind::Constant { .. } => {
                register_declared_symbol(
                    &mut manifest,
                    &header.tokens.src_path,
                    &header.source_file,
                    Some(header.exported),
                );
            }
            _ => {}
        }
    }

    let builtin_manifest = register_builtin_error_types(string_table);
    manifest
        .builtin_visible_symbol_paths
        .extend(builtin_manifest.visible_symbol_paths.iter().cloned());
    manifest.declarations.extend(builtin_manifest.declarations);
    manifest
        .resolved_struct_fields_by_path
        .extend(builtin_manifest.resolved_struct_fields_by_path);
    manifest
        .struct_source_by_path
        .extend(builtin_manifest.struct_source_by_path);
    manifest
        .builtin_struct_ast_nodes
        .extend(builtin_manifest.ast_struct_nodes);

    Ok(manifest)
}

/// Register a symbol into the manifest's declared-path and declared-name tables.
/// When `exported` is `Some`, also records the symbol's export visibility.
fn register_declared_symbol(
    manifest: &mut SymbolManifest,
    symbol_path: &InternedPath,
    source_file: &InternedPath,
    exported: Option<bool>,
) {
    if let Some(is_exported) = exported {
        manifest
            .importable_symbol_exported
            .insert(symbol_path.to_owned(), is_exported);
    }
    manifest
        .declared_paths_by_file
        .entry(source_file.to_owned())
        .or_default()
        .insert(symbol_path.to_owned());
    if let Some(name) = symbol_path.name() {
        manifest
            .declared_names_by_file
            .entry(source_file.to_owned())
            .or_default()
            .insert(name);
    }
}
