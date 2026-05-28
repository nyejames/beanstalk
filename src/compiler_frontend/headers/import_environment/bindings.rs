//! Header-stage import environment data shapes.
//!
//! WHAT: defines `FileVisibility` and `HeaderImportEnvironment`, the per-file visibility maps
//! produced by header import preparation and consumed by dependency sorting and AST.
//! WHY: after header parsing, every source file needs a stable, complete visibility snapshot
//! so later stages do not rebuild import bindings or rediscover top-level symbols.
//! MUST NOT: parse executable bodies, fold constants, or perform AST semantic validation.

use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::external_packages::{ExternalFunctionId, ExternalSymbolId};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringId;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use rustc_hash::{FxHashMap, FxHashSet};

/// Per-file visible-name environment.
///
/// WHAT: maps the names visible in one source file to their resolved targets.
/// WHY: AST `ScopeContext` consumes this directly instead of rebuilding import visibility.
///
/// Includes same-file declarations, source imports, external symbols, type aliases, and
/// builtin/prelude reservations. Name collision policy is enforced during construction.
/// A member of a namespace record that is valid in value/expression context.
#[derive(Clone, Debug)]
pub(crate) enum NamespaceValueMember {
    SourceDeclaration(InternedPath),
    ExternalSymbol(ExternalSymbolId),
}

/// A member of a namespace record that is valid in type context.
#[derive(Clone, Debug)]
pub(crate) enum NamespaceTypeMember {
    SourceDeclaration(InternedPath),
    ExternalSymbol(ExternalSymbolId),
}

/// Where a namespace record originated, for diagnostics and HIR boundary checks.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum NamespaceRecordSource {
    SourceFile(InternedPath),
    ExternalPackage(StringId),
}

/// A shallow field-access-only import record built from one import surface.
///
/// WHAT: maps member names to their resolved targets so AST can resolve
/// `namespace.member` and `namespace.Type` without rebuilding import visibility.
/// WHY: namespace imports expose value and type members separately; mixing them
/// in the wrong context produces targeted diagnostics.
#[derive(Clone, Debug)]
pub(crate) struct NamespaceRecord {
    pub(crate) value_members: FxHashMap<StringId, NamespaceValueMember>,
    pub(crate) type_members: FxHashMap<StringId, NamespaceTypeMember>,
}

/// One receiver method made visible to a source file.
///
/// WHAT: stores the canonical function path plus the import/declaration location that made the
/// method visible.
/// WHY: receiver methods live in the receiver-call namespace rather than the ordinary value
/// namespace, so they need their own visibility entries and diagnostics.
#[derive(Clone, Debug)]
pub(crate) struct ReceiverMethodVisibility {
    pub(crate) function_path: InternedPath,
    pub(crate) location: SourceLocation,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct FileVisibility {
    /// Declaration paths that are visible in this file (including builtins).
    /// Used as an access gate for permission checks.
    pub(crate) visible_declaration_paths: FxHashSet<InternedPath>,

    /// Source-visible names → canonical declaration path.
    /// Includes same-file declarations and imported source symbols (aliased or not).
    pub(crate) visible_source_names: FxHashMap<StringId, InternedPath>,

    /// Type aliases: local visible name → canonical type alias path.
    pub(crate) visible_type_alias_names: FxHashMap<StringId, InternedPath>,

    /// External package functions/types/constants visible from this file.
    /// Populated by explicit virtual-package imports and prelude symbols.
    pub(crate) visible_external_symbols: FxHashMap<StringId, ExternalSymbolId>,

    /// External receiver methods visible in this file.
    ///
    /// WHY: receiver methods are callable only through receiver syntax. Keeping them outside
    /// `visible_external_symbols` prevents grouped or namespace imports from accidentally making
    /// `method(...)` a valid free-function call.
    pub(crate) visible_external_receiver_methods: FxHashMap<StringId, Vec<ExternalFunctionId>>,

    /// Namespace import records visible in this file.
    /// Populated by bare `import @path` and `import @path as alias` syntax.
    pub(crate) visible_namespace_records: FxHashMap<StringId, NamespaceRecord>,

    /// Receiver methods visible in this file.
    /// Key is the local method name (may be aliased for grouped imports).
    /// Value is the list of canonical function paths with that local name.
    /// WHY: receiver methods are callable only through receiver syntax, not as free
    ///      functions or namespace-record value members. Import preparation routes
    ///      them here so AST lookup can filter the module-wide catalog by file.
    ///      Multiple paths per name are needed because different receiver types can
    ///      share a method name (e.g. String.length and Array.length).
    pub(crate) visible_receiver_methods: FxHashMap<StringId, Vec<ReceiverMethodVisibility>>,
}

/// Header-built import environment for the entire module.
///
/// WHAT: collects one `FileVisibility` per parsed source file.
/// WHY: dependency sorting and AST need stable per-file visibility without rebuilding import
/// semantics in later stages.
#[derive(Clone, Debug, Default)]
pub(crate) struct HeaderImportEnvironment {
    pub(crate) file_visibility_by_source: FxHashMap<InternedPath, FileVisibility>,
    pub(crate) warnings: Vec<CompilerDiagnostic>,
}

impl HeaderImportEnvironment {
    /// Return the visibility map for a parsed source file.
    ///
    /// WHY: missing visibility means header preparation failed to populate its stage contract.
    /// This should only happen if a file was added to `module_file_paths` without running
    /// import environment construction.
    pub(crate) fn visibility_for(
        &self,
        source_file: &InternedPath,
    ) -> Result<&FileVisibility, CompilerError> {
        self.file_visibility_by_source.get(source_file).ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "Missing visibility entry for source file. This is a compiler bug: header parsing did not produce a visibility map for '{source_file:?}'."
            ))
        })
    }
}
