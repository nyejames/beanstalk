//! Header-stage import environment data shapes.
//!
//! WHAT: defines `FileVisibility` and `HeaderImportEnvironment`, the per-file visibility maps
//! produced by header import preparation and consumed by dependency sorting and AST.
//! WHY: after header parsing, every source file needs a stable, complete visibility snapshot
//! so later stages do not rebuild import bindings or rediscover top-level symbols.
//! MUST NOT: parse executable bodies, fold constants, or perform AST semantic validation.

use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::external_packages::ExternalSymbolId;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringId;
use rustc_hash::{FxHashMap, FxHashSet};

/// Per-file visible-name environment.
///
/// WHAT: maps the names visible in one source file to their resolved targets.
/// WHY: AST `ScopeContext` consumes this directly instead of rebuilding import visibility.
///
/// Includes same-file declarations, source imports, external symbols, type aliases, and
/// builtin/prelude reservations. Name collision policy is enforced during construction.
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
}

/// Header-built import environment for the entire module.
///
/// WHAT: collects one `FileVisibility` per parsed source file.
/// WHY: dependency sorting and AST need stable per-file visibility without rebuilding import
/// semantics in later stages.
#[derive(Clone, Debug, Default)]
#[allow(dead_code)]
pub(crate) struct HeaderImportEnvironment {
    pub(crate) file_visibility_by_source: FxHashMap<InternedPath, FileVisibility>,
    pub(crate) warnings: Vec<CompilerWarning>,
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
                "Missing visibility entry for source file. This is a compiler bug: header parsing did not produce a visibility map for '{:?}'.",
                source_file
            ))
        })
    }
}
