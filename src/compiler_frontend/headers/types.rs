//! Header-stage data contracts.
//!
//! WHAT: shared structs/enums produced by header parsing and consumed by dependency sorting,
//! AST construction, and module symbol collection.
//! WHY: keeping these types separate from parser control flow makes the header-stage API obvious
//! and avoids making `parse_file_headers.rs` the dumping ground for every header concern.

use crate::compiler_frontend::ast::TopLevelDeclarationIndex;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::declaration_syntax::choice::ChoiceVariant;
use crate::compiler_frontend::declaration_syntax::declaration_shell::DeclarationSyntax;
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::headers::module_symbols::ModuleSymbols;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::identity::FileId;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation};
use rustc_hash::FxHashMap;
use std::collections::HashSet;
use std::fmt::Display;
use std::rc::Rc;

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
#[derive(Clone)]
pub struct HeaderParseOptions {
    pub entry_file_id: Option<FileId>,
    pub project_path_resolver: Option<ProjectPathResolver>,
    pub path_format_config: PathStringFormatConfig,
    pub style_directives: StyleDirectiveRegistry,
}

impl Default for HeaderParseOptions {
    fn default() -> Self {
        Self {
            entry_file_id: None,
            project_path_resolver: None,
            path_format_config: PathStringFormatConfig::default(),
            style_directives: StyleDirectiveRegistry::built_ins(),
        }
    }
}

#[derive(Clone, Debug)]
pub enum HeaderKind {
    Function {
        signature: FunctionSignature,
    },
    Constant {
        declaration: DeclarationSyntax,
    },
    Struct {
        fields: Vec<Declaration>,
    },
    Choice {
        variants: Vec<ChoiceVariant>,
    },
    TypeAlias {
        target: DataType,
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
    pub exported: bool,
    // Module-level dependency edges required before AST construction can lower this header.
    pub dependencies: HashSet<InternedPath>,
    pub name_location: SourceLocation,

    // Token Body (for functions / templates) and info about canonical_os_path
    pub tokens: FileTokens,

    pub source_file: InternedPath,
    pub file_imports: Vec<FileImport>,
    pub file_re_exports: Vec<FileReExport>,
}

impl Display for Header {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Header kind: {:#?}", self.kind)
    }
}

impl Header {
    /// Returns the canonical (real OS) filesystem path for the source file that owns this header.
    /// Falls back to the logical source-file path when no OS path is recorded.
    ///
    /// WHY: const-template scopes use synthetic paths; the canonical path is needed for
    /// project-path-resolver lookups and rendered-path-usage tracking.
    pub(crate) fn canonical_source_file(&self, string_table: &mut StringTable) -> InternedPath {
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
}

impl FileImport {
    // NOTE: `local_name()` was intended for import alias binding but the
    // resolution logic inlines `alias.unwrap_or(symbol_name)` to use the
    // resolved symbol name rather than the raw path name.
}

/// Re-export clause item parsed from `#import @path/to/symbol` in a `#mod.bst` facade.
///
/// WHAT: carries the same shape as `FileImport` because re-export syntax mirrors import syntax.
/// WHY: keeping them separate makes it explicit that re-exports do not create local bindings
/// and do not contribute dependency edges.
#[derive(Clone, Debug)]
pub struct FileReExport {
    pub header_path: InternedPath,
    pub alias: Option<StringId>,
    pub location: SourceLocation,
    pub path_location: SourceLocation,
    pub alias_location: Option<SourceLocation>,
}

/// Classification of a source file's role within the module.
///
/// WHAT: distinguishes entry files, normal source files, and library facade files.
/// WHY: each role has different rules for runtime code, exports, and visibility.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileRole {
    /// The module entry file. Has an implicit start function.
    Entry,
    /// A normal source file. No top-level runtime code allowed.
    Normal,
    /// A library facade file (`#mod.bst`). Defines the public export surface.
    /// No top-level runtime code allowed. Exported declarations are visible externally.
    ModuleFacade,
}

// Shared file-level state that stays live while one source file is being split into headers.
pub(super) struct HeaderParseContext<'a> {
    pub external_package_registry: &'a ExternalPackageRegistry,
    pub style_directives: &'a StyleDirectiveRegistry,
    pub warnings: &'a mut Vec<CompilerWarning>,
    pub file_role: FileRole,
    pub project_path_resolver: Option<ProjectPathResolver>,
    pub path_format_config: PathStringFormatConfig,
    pub string_table: &'a mut StringTable,
    pub const_template_number: &'a mut usize,
    /// Count of runtime (non-exported) top-level templates seen so far in the entry file.
    /// Used as the runtime_insertion_index for the next const fragment.
    pub runtime_fragment_count: &'a mut usize,
    pub top_level_const_fragments: &'a mut Vec<TopLevelConstFragment>,
    pub file_re_exports_by_source: &'a mut FxHashMap<InternedPath, Vec<FileReExport>>,
}

// Shared per-header builder inputs that stay stable while one declaration is classified.
pub(super) struct HeaderBuildContext<'a> {
    pub external_package_registry: &'a ExternalPackageRegistry,
    pub style_directives: &'a StyleDirectiveRegistry,
    pub warnings: &'a mut Vec<CompilerWarning>,
    pub project_path_resolver: Option<ProjectPathResolver>,
    pub path_format_config: PathStringFormatConfig,
    pub visible_constant_placeholders: Rc<TopLevelDeclarationIndex>,
    pub source_file: &'a InternedPath,
    pub file_imports: &'a HashSet<InternedPath>,
    pub file_import_entries: &'a [FileImport],
    pub file_re_export_entries: &'a [FileReExport],
    pub file_constant_order: &'a mut usize,
    pub string_table: &'a mut StringTable,
}
