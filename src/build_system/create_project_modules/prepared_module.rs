//! Retained module preparation payload.
//!
//! WHAT: the build-system-owned handoff between Stage 0 source-file preparation and semantic
//!       module compilation. Carries the canonical `StableModuleOriginIdentity`, the
//!       provider-independent `PreparedHeaderSyntax`, the deterministic module string-table
//!       context, source identities, preparation warnings, and the input-size facts semantic
//!       compilation needs for arena capacity estimation.
//! WHY: the compiler design overview requires `PreparedHeaderSyntax` to be produced before the
//!      provider graph is compiled and retained so semantic compilation begins with
//!      provider-dependent `bind_module_headers` without retokenizing or reparsing source.
//!      This type makes that phase boundary unrepresentable as an invalid state: semantic
//!      compilation consumes retained syntax and a string table, never `PreparedSourceInput`,
//!      source text or tokens.

use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::headers::parse_file_headers::PreparedHeaderSyntax;
use crate::compiler_frontend::semantic_identity::StableModuleOriginIdentity;
use crate::compiler_frontend::symbols::identity::SourceFileTable;
use crate::compiler_frontend::symbols::string_interning::StringTable;

/// Retained result of preparing one module's source files and aggregating header syntax.
///
/// Construct this only from the module-preparation path. The `string_table` is the local module
/// fork built during file preparation; every `StringId` in `prepared_header_syntax` and
/// `source_files` is valid in it. Semantic compilation consumes this payload and continues
/// mutating the same string table through binding, AST, HIR and borrow validation.
///
/// This payload carries no source text or token streams, so semantic compilation cannot rerun
/// file preparation or retokenize source. It carries the module's canonical stable origin so the
/// semantic module-compilation boundary receives a graph-assigned identity instead of
/// reconstructing one from an entry path. The shape is ready for Phase 5 dependency-ordered
/// provider scheduling: preparation and binding are independently schedulable around the retained
/// syntax, string-table context and stable identity.
pub(crate) struct PreparedModule {
    /// The graph-assigned (or synthetic single-file) cross-build origin identity for this module.
    ///
    /// Travels from discovery through preparation to semantic compilation so the compiler receives
    /// a canonical module identity by name rather than re-deriving it from `entry_file_path`,
    /// `SourceFileTable` or absolute paths. Semantic compilation consumes it as the exporting-module
    /// and declaration-origin component of the `DefinedPublicExportOrigins` identity component.
    pub(crate) stable_origin: StableModuleOriginIdentity,
    /// Provider-independent retained header syntax, produced before provider interfaces exist.
    pub(crate) prepared_header_syntax: PreparedHeaderSyntax,
    /// Local module string table forked for this module during file preparation.
    pub(crate) string_table: StringTable,
    /// Source identities built from the prepared source paths.
    pub(crate) source_files: SourceFileTable,
    /// Warnings accumulated during file preparation.
    pub(crate) warnings: Vec<CompilerDiagnostic>,
    /// Number of source files in the module, for arena capacity estimation.
    pub(crate) source_file_count: usize,
    /// Total source byte count, for arena capacity estimation.
    pub(crate) source_byte_count: usize,
}
