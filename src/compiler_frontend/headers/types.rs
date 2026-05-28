//! Header-stage data contracts.
//!
//! WHAT: shared structs/enums produced by header parsing and consumed by dependency sorting,
//! AST construction, and module symbol collection.
//! WHY: keeping these types separate from parser control flow makes the header-stage API obvious
//! and avoids making `parse_file_headers.rs` the dumping ground for every header concern.

use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::datatypes::generic_parameters::GenericParameterList;
use crate::compiler_frontend::datatypes::parsed::ParsedTypeRef;
use crate::compiler_frontend::declaration_syntax::choice::ChoiceVariantSyntax;
use crate::compiler_frontend::declaration_syntax::declaration_shell::DeclarationSyntax;
use crate::compiler_frontend::declaration_syntax::signature_members::{
    FunctionSignatureSyntax, SignatureMemberSyntax,
};
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::headers::import_environment::HeaderImportEnvironment;
use crate::compiler_frontend::headers::module_symbols::ModuleSymbols;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::symbols::identity::FileId;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringIdRemap, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation};
use std::collections::HashSet;
use std::fmt::Display;

/// Parsed headers for one module plus const-fragment placement metadata for the entry file.
///
/// WHY: const fragments carry runtime insertion indices so the builder can merge them with the
/// runtime fragment list returned by entry `start()`. Runtime fragments are not tracked here —
/// they are evaluated directly inside `start()` in source order.
///
/// `module_symbols` carries all order-independent top-level symbol metadata collected during
/// header parsing. `declarations` inside it is empty until dependency sorting completes.
pub struct Headers {
    pub headers: Vec<Header>,
    pub top_level_const_fragments: Vec<TopLevelConstFragment>,
    /// Number of top-level runtime templates in the entry file.
    ///
    /// WHY: only the entry file produces runtime slots; header parsing is the single authoritative
    /// counter so builders do not need to re-scan HIR for `PushRuntimeFragment` statements.
    pub entry_runtime_fragment_count: usize,
    /// Header-owned module symbol package.
    ///
    /// WHY: top-level symbol discovery is owned by the header stage; dependency sorting and AST
    /// construction consume this directly without a separate manifest-building step.
    pub module_symbols: ModuleSymbols,
    /// Header-built per-file import visibility environment.
    ///
    /// WHY: import binding and visibility construction is owned by the header stage; AST
    /// consumes this directly without rebuilding import bindings or rediscovering visibility.
    pub import_environment: HeaderImportEnvironment,
}

/// Placement metadata for one compile-time top-level template in the entry file.
///
/// WHAT: records where a const fragment should be inserted relative to runtime fragments
/// in the final merged output.
/// WHY: only const fragments carry insertion metadata; runtime fragments are returned by
/// `start()` in source order and need no separate metadata.
#[derive(Clone, Debug)]
pub struct TopLevelConstFragment {
    /// Number of runtime fragments seen before this const fragment in source order.
    /// Used by the builder to insert the const string at the correct position.
    pub runtime_insertion_index: usize,
    pub header_path: InternedPath,
    pub location: SourceLocation,
}

/// Optional settings that affect module header parsing.
///
/// WHAT: bundles optional entry identity and path-resolution behavior for one parse invocation.
/// WHY: the parser is called from both production and tests, and grouping these keeps the API concise.
#[derive(Clone, Default)]
pub struct HeaderParseOptions {
    pub entry_file_id: Option<FileId>,
    pub project_path_resolver: Option<ProjectPathResolver>,
}

#[derive(Clone, Debug)]
pub enum HeaderKind {
    Function {
        generic_parameters: GenericParameterList,
        signature: FunctionSignatureSyntax,
    },
    Constant {
        declaration: DeclarationSyntax,
        source_order: usize,
    },
    Struct {
        generic_parameters: GenericParameterList,
        fields: Vec<SignatureMemberSyntax>,
    },
    Choice {
        generic_parameters: GenericParameterList,
        variants: Vec<ChoiceVariantSyntax>,
    },
    TypeAlias {
        generic_parameters: GenericParameterList,
        target: ParsedTypeRef,
    },

    ConstTemplate,

    /// The entry-file start function for non-header top-level statements.
    ///
    /// WHAT: captures top-level executable statements that are not declarations.
    /// WHY: only the module entry file produces a start function. Non-entry files with
    /// non-trivial top-level executable code are rejected as a rule error.
    /// Start functions are build-system-only; they are not importable or callable from modules.
    StartFunction,
}

#[derive(Clone, Debug)]
pub struct Header {
    pub kind: HeaderKind,
    /// The role of the source file that produced this header.
    ///
    /// WHAT: distinguishes entry files, normal source files, and module facades.
    /// WHY: visibility and export decisions now depend on file role and declaration kind,
    /// not just the old `exported` boolean.
    pub file_role: FileRole,
    // Module-level dependency edges required before AST construction can lower this header.
    pub dependencies: HashSet<InternedPath>,
    pub name_location: SourceLocation,

    // Token Body (for functions / templates) and info about canonical_os_path
    pub tokens: FileTokens,

    pub source_file: InternedPath,
    pub file_imports: Vec<FileImport>,
}

impl Display for Header {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Header kind: {:#?}", self.kind)
    }
}

impl TopLevelConstFragment {
    /// Remap every interned string owned by this fragment into the merged global string table.
    ///
    /// WHY: per-file frontend preparation uses local string tables; merging them into the module
    /// table requires shifting every `StringId`, `InternedPath`, and `SourceLocation` so later
    /// stages resolve names through the global table.
    // Called when merging per-file frontend outputs into the module-wide compilation.
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.header_path.remap_string_ids(remap);
        self.location.remap_string_ids(remap);
    }
}

impl FileImport {
    /// Remap every interned string owned by this import into the merged global string table.
    ///
    /// WHY: per-file frontend preparation uses local string tables; merging them into the module
    /// table requires shifting every `StringId`, `InternedPath`, and `SourceLocation` so later
    /// stages resolve names through the global table.
    // Called when merging per-file frontend outputs into the module-wide compilation.
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        remap_import_like_header_fields(
            &mut self.header_path,
            &mut self.alias,
            &mut self.location,
            &mut self.path_location,
            &mut self.alias_location,
            remap,
        );
    }
}

fn remap_import_like_header_fields(
    header_path: &mut InternedPath,
    alias: &mut Option<StringId>,
    location: &mut SourceLocation,
    path_location: &mut SourceLocation,
    alias_location: &mut Option<SourceLocation>,
    remap: &StringIdRemap,
) {
    header_path.remap_string_ids(remap);

    if let Some(alias) = alias {
        *alias = remap.get(*alias);
    }

    location.remap_string_ids(remap);
    path_location.remap_string_ids(remap);

    if let Some(alias_location) = alias_location {
        alias_location.remap_string_ids(remap);
    }
}

impl HeaderKind {
    /// Remap every interned string owned by this header kind into the merged global string table.
    ///
    /// WHAT: dispatches to nested remap methods for function signatures, declaration shells,
    ///       struct fields, choice variants, and type-alias targets.
    /// WHY: per-file frontend preparation uses local string tables; merging them into the module
    ///      table requires shifting every `StringId`, `InternedPath`, and `SourceLocation` so later
    ///      stages resolve names through the global table.
    // Called when merging per-file frontend outputs into the module-wide compilation.
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        match self {
            HeaderKind::Function {
                generic_parameters,
                signature,
            } => {
                generic_parameters.remap_string_ids(remap);
                signature.remap_string_ids(remap);
            }

            HeaderKind::Constant { declaration, .. } => {
                declaration.remap_string_ids(remap);
            }

            HeaderKind::Struct {
                generic_parameters,
                fields,
            } => {
                generic_parameters.remap_string_ids(remap);
                for field in fields {
                    field.remap_string_ids(remap);
                }
            }

            HeaderKind::Choice {
                generic_parameters,
                variants,
            } => {
                generic_parameters.remap_string_ids(remap);
                for variant in variants {
                    variant.remap_string_ids(remap);
                }
            }

            HeaderKind::TypeAlias {
                generic_parameters,
                target,
            } => {
                generic_parameters.remap_string_ids(remap);
                target.remap_string_ids(remap);
            }

            HeaderKind::ConstTemplate | HeaderKind::StartFunction => {}
        }
    }
}

impl Header {
    /// Remap every interned string owned by this header into the merged global string table.
    ///
    /// WHAT: remaps the kind payload, dependency paths, source locations, token stream,
    ///       source file, and imports.
    /// WHY: per-file frontend preparation uses local string tables; merging them into the module
    ///      table requires shifting every `StringId`, `InternedPath`, and `SourceLocation` so later
    ///      stages resolve names through the global table.
    // Called when merging per-file frontend outputs into the module-wide compilation.
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.kind.remap_string_ids(remap);

        // Rebuild the dependency set after remapping because InternedPath hash values
        // depend on their component StringIds, which change during remapping.
        let mut remapped_dependencies = HashSet::with_capacity(self.dependencies.len());
        for mut path in self.dependencies.drain() {
            path.remap_string_ids(remap);
            remapped_dependencies.insert(path);
        }
        self.dependencies = remapped_dependencies;

        self.name_location.remap_string_ids(remap);
        self.tokens.remap_string_ids(remap);
        self.source_file.remap_string_ids(remap);

        for import in &mut self.file_imports {
            import.remap_string_ids(remap);
        }
    }

    /// Returns the canonical (real OS) filesystem path for the source file that owns this header.
    /// Falls back to the logical source-file path when no OS path is recorded.
    ///
    /// WHY: const-template scopes use synthetic paths; the canonical path is needed for
    /// project-path-resolver lookups and rendered-path-usage tracking.
    pub(crate) fn canonical_source_file(&self, string_table: &mut StringTable) -> InternedPath {
        // Mutation: canonical filesystem paths are project-derived inputs that must be interned
        // before downstream stages can use them as InternedPath values.
        self.tokens
            .canonical_os_path
            .as_ref()
            .map(|canonical_path| InternedPath::from_path_buf(canonical_path, string_table))
            .unwrap_or_else(|| self.source_file.to_owned())
    }
}

#[derive(Clone, Debug)]
pub struct FileImport {
    pub header_path: InternedPath,
    pub alias: Option<StringId>,
    pub location: SourceLocation,
    pub path_location: SourceLocation,
    pub alias_location: Option<SourceLocation>,
    pub from_grouped: bool,
}

/// Classification of a source file's role within the module.
///
/// WHAT: distinguishes entry files, normal source files, and module facade files.
/// WHY: each role has different rules for runtime code, exports, and visibility.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileRole {
    /// The module entry file. Has an implicit start function.
    Entry,
    /// A normal source file. No top-level runtime code allowed.
    Normal,
    /// A module facade file (`#mod.bst`). Defines the public export surface.
    /// No top-level runtime code allowed. Exported declarations are visible externally.
    ModuleFacade,
}

/// Per-file output produced by header parsing before module-wide aggregation.
///
/// WHAT: carries all data produced from a single source file during header parsing so that
/// `parse_headers` can aggregate per-file outputs deterministically instead of relying on
/// shared mutable buffers during the file loop.
/// WHY: explicit per-file boundaries are required before tokenization/header parsing can run
/// in parallel; each file must be self-contained so later phases can merge/remap outputs.
pub struct FileFrontendPrepareOutput {
    pub source_file: InternedPath,
    /// Preserved for later parallel phases that need stable file identity before aggregation.
    #[allow(dead_code)]
    pub file_id: Option<FileId>,
    pub headers: Vec<Header>,
    pub top_level_const_fragments: Vec<TopLevelConstFragment>,
    /// Number of const templates parsed in this file.
    ///
    /// WHY: const-template synthetic names must remain unique across the module while per-file
    /// parsing reports its contribution separately from module aggregation.
    // Phase 6 parallel preparation keeps this contribution explicit for validation and future
    // fragment instrumentation, even though Alpha currently permits const templates only in the
    // single entry file.
    #[allow(dead_code)]
    pub const_template_count: usize,
    /// Number of runtime fragments contributed by this file.
    pub runtime_fragment_count: usize,
    /// Warnings emitted while parsing this file.
    ///
    /// WHY: per-file preparation must be self-contained; warnings are merged into the caller's
    /// warning vector in deterministic file iteration order before module-wide aggregation.
    pub warnings: Vec<CompilerDiagnostic>,
}

/// Failed per-file header preparation plus warnings emitted before the failure.
///
/// WHY: warnings are produced while parsing declarations before a later token in the same file can
/// fail. The module parser must keep those warnings even when the file contributes no headers.
#[derive(Debug)]
pub struct FileFrontendPrepareError {
    pub warnings: Vec<CompilerDiagnostic>,
    pub diagnostic: CompilerDiagnostic,
}

impl FileFrontendPrepareOutput {
    /// Remap every interned string owned by this per-file output into the merged global string table.
    ///
    /// WHAT: remaps source file, headers, const fragments, and warnings.
    /// WHY: per-file frontend preparation uses local string tables; merging them into the module
    ///      table requires shifting every `StringId`, `InternedPath`, and `SourceLocation` so later
    ///      stages resolve names through the global table.
    // Called when merging per-file frontend outputs into the module-wide compilation.
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.source_file.remap_string_ids(remap);

        for header in &mut self.headers {
            header.remap_string_ids(remap);
        }

        for fragment in &mut self.top_level_const_fragments {
            fragment.remap_string_ids(remap);
        }

        for warning in &mut self.warnings {
            warning.remap_string_ids(remap);
        }
    }
}

impl FileFrontendPrepareError {
    /// Remap every interned string owned by this failed per-file output into the merged global
    /// string table.
    ///
    /// WHAT: remaps warnings and the primary diagnostic.
    /// WHY: per-file frontend preparation uses local string tables; even failed files may have
    ///      emitted warnings before the error, and those strings must resolve through the global table.
    // Called when merging per-file frontend outputs into the module-wide compilation.
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        for warning in &mut self.warnings {
            warning.remap_string_ids(remap);
        }

        self.diagnostic.remap_string_ids(remap);
    }
}

// Shared file-level state that stays live while one source file is being split into headers.
pub(super) struct HeaderParseContext<'a> {
    pub external_package_registry: &'a ExternalPackageRegistry,
    pub file_role: FileRole,
    pub string_table: &'a mut StringTable,
    /// Module-wide base offset for const-template synthetic names in this file.
    ///
    /// WHY: const-template names must be unique across the module; each file's parser
    /// starts numbering from this offset so later aggregation does not need to renumber.
    pub const_template_offset: usize,
    /// Entry-file base offset for runtime-fragment insertion indices in this file.
    ///
    /// WHY: only entry files produce runtime fragments, but passing the offset keeps
    /// per-file preparation deterministic even if the caller changes ordering later.
    pub runtime_fragment_offset: usize,
}

// Shared per-header builder inputs that stay stable while one declaration is classified.
pub(super) struct HeaderBuildContext<'a> {
    pub external_package_registry: &'a ExternalPackageRegistry,
    pub warnings: &'a mut Vec<CompilerDiagnostic>,
    pub source_file: &'a InternedPath,
    pub file_imports: &'a HashSet<InternedPath>,
    pub file_import_entries: &'a [FileImport],
    pub file_constant_order: &'a mut usize,
    pub string_table: &'a mut StringTable,
    pub file_role: FileRole,
}

#[cfg(test)]
#[path = "tests/header_remap_tests.rs"]
mod header_remap_tests;
