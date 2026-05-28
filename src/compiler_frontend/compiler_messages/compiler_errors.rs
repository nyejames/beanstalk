//! # Compiler Error Handling System
//!
//! This module owns `CompilerError` (internal/tooling failures) and `CompilerMessages`
//! (render-boundary aggregation). User-facing source diagnostics live in
//! `compiler_diagnostic.rs` as `CompilerDiagnostic`.
//!
//! ## Architecture
//!
//! ```text
//! Frontend/compiler stages
//!   -> CompilerDiagnostic { kind, severity, primary_location, labels, payload }
//!   -> DiagnosticBag accumulates one or many diagnostics locally
//!   -> CompilerMessages owns ordered diagnostics + StringTable + optional render TypeEnvironment
//!      at stage/build boundaries
//!   -> renderers produce terminal/dev-server/terse output
//!
//! CompilerError
//!   -> target ownership: internal/tooling/compiler failure only
//!   -> printed through one central helper
//!   -> no normal Beanstalk source, syntax, type, rule, import, config-source,
//!      or borrow diagnostics
//! ```
//!
//! User-facing diagnostics must use typed `CompilerDiagnostic` constructors in
//! `compiler_diagnostic.rs`.
//!
//! ### What is still allowed
//! - `return_compiler_error!` — for internal compiler bugs only.
//! - `return_hir_transformation_error!` — for HIR lowering failures (compiler bugs).
//! - `return_file_error!` — for filesystem failures before source representation.
//!
//! ## Error Types
//!
//! `ErrorType` classifies internal/tooling failures that still use `CompilerError`.
//!
//! Categories:
//! - **HirTransformation / Backend** — compiler-internal lowering failures.
//! - **Compiler** — internal bugs (not user's fault).
//! - **File** — filesystem errors.
//! - **Config** — configuration file issues.
//! - **DevServer** — development server infrastructure failures.
//!
//! ## Design Principles
//!
//! ### Shared StringTable Context
//! Diagnostics preserve interned path scopes, so top-level renderers and file-adjacent helpers
//! resolve paths through the shared `StringTable` for the current build or parse lifecycle.
//!
//! ### Structured Payloads
//! `CompilerDiagnostic` carries typed payloads (`DiagnosticPayload`) instead of rendered strings.
//! Renderers at the boundary resolve interned IDs and enums into human prose.
//!
//! ### Consistent Patterns
//! - Stage-local accumulation: `DiagnosticBag`.
//! - Boundary transport: `CompilerMessages`.
//! - Internal failure: `CompilerError` + immediate print.
//!
//! ## Error Flow Through Compilation Pipeline
//!
//! ```text
//! Source Code
//!     ↓
//! Tokenizer → CompilerDiagnostic (Syntax)
//!     ↓
//! Header Parser → CompilerDiagnostic (Syntax / Import / Rule)
//!     ↓
//! Dependency Sort → CompilerDiagnostic (Rule)
//!     ↓
//! AST Builder → CompilerDiagnostic (Type / Rule)
//!     ↓
//! HIR Builder → CompilerError (HirTransformation) — internal only
//!     ↓
//! Borrow Checker → CompilerDiagnostic (Borrow) + side-table facts
//!     ↓
//! Backend Lowering → CompilerError (Backend) — internal only
//!     ↓
//! CompilerMessages (ordered diagnostics + StringTable)
//! ```

pub use crate::compiler_frontend::compiler_messages::source_location::SourceLocation;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, DiagnosticKind, DiagnosticPayload, DiagnosticSeverity,
    InfrastructureDiagnosticKind,
};
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringIdRemap, StringTable};
use std::collections::HashMap;
use std::path::Path;

// -----------------------
//  Compiler Message Set
// -----------------------

#[derive(Debug, Clone)]
pub struct CompilerMessages {
    /// Ordered diagnostics at a build/render boundary.
    ///
    /// WHAT: stores errors and warnings in the order the compiler produced them.
    /// WHY: renderers, tests, dev-server summaries, and CLI output all consume this one sequence
    /// instead of consulting parallel message stores.
    pub(crate) diagnostics: Vec<CompilerDiagnostic>,

    pub string_table: StringTable,

    /// Optional module-local type table used only by diagnostic renderers.
    ///
    /// WHAT: carries the semantic type lookup table beside diagnostics from one failed module.
    /// WHY: type diagnostics store `TypeId`s. Renderers need the matching module environment to
    /// turn those IDs into source-level names, but individual diagnostics must not own it.
    ///
    /// Boundary shape: this is intentionally owned by `CompilerMessages` only on failed module
    /// boundaries where diagnostics outlive the AST/HIR owner that still has the active
    /// `TypeEnvironment`. Successful builds carry the module type table in `Module`, not here.
    pub(crate) render_type_environment: Option<Box<TypeEnvironment>>,
}

impl CompilerMessages {
    pub fn empty(string_table: StringTable) -> Self {
        Self {
            diagnostics: Vec::new(),
            string_table,
            render_type_environment: None,
        }
    }

    pub(crate) fn from_diagnostics(
        diagnostics: Vec<CompilerDiagnostic>,
        string_table: StringTable,
    ) -> Self {
        Self {
            diagnostics,
            string_table,
            render_type_environment: None,
        }
    }

    pub fn has_errors(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == DiagnosticSeverity::Error)
    }

    pub fn has_warnings(&self) -> bool {
        self.diagnostics
            .iter()
            .any(|d| d.severity == DiagnosticSeverity::Warning)
    }

    /// Count diagnostics with `Error` severity.
    pub fn error_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == DiagnosticSeverity::Error)
            .count()
    }

    /// Count diagnostics with `Warning` severity.
    pub fn warning_count(&self) -> usize {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == DiagnosticSeverity::Warning)
            .count()
    }

    /// Iterate over every diagnostic in compiler production order.
    ///
    /// WHAT: exposes the single boundary diagnostic stream without implying an error-only mirror.
    /// WHY: renderers and reports often need to preserve ordering while applying their own
    /// severity policy locally.
    pub(crate) fn diagnostics(&self) -> impl Iterator<Item = &CompilerDiagnostic> {
        self.diagnostics.iter()
    }

    /// Borrow the ordered diagnostic stream for render helpers that need a slice.
    pub(crate) fn diagnostic_slice(&self) -> &[CompilerDiagnostic] {
        &self.diagnostics
    }

    /// Append already-structured diagnostics while preserving current order.
    pub(crate) fn extend_diagnostics(
        &mut self,
        diagnostics: impl IntoIterator<Item = CompilerDiagnostic>,
    ) {
        self.diagnostics.extend(diagnostics);
    }

    /// Consume the boundary container and return its ordered diagnostics.
    pub(crate) fn into_diagnostics(self) -> Vec<CompilerDiagnostic> {
        self.diagnostics
    }

    /// Take the optional type render context when aggregating message containers.
    pub(crate) fn take_render_type_environment(&mut self) -> Option<TypeEnvironment> {
        self.render_type_environment.take().map(|env| *env)
    }

    /// Iterate over diagnostics with `Error` severity.
    #[cfg(test)]
    pub(crate) fn error_diagnostics(&self) -> impl Iterator<Item = &CompilerDiagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == DiagnosticSeverity::Error)
    }

    /// Return the first error-severity diagnostic, preserving diagnostic order.
    #[cfg(test)]
    pub(crate) fn first_error(&self) -> Option<&CompilerDiagnostic> {
        self.error_diagnostics().next()
    }

    #[cfg(test)]
    pub(crate) fn first_infrastructure_error_for_tests(
        &self,
    ) -> Option<(&ErrorType, &str, &SourceLocation)> {
        let diagnostic = self.first_error()?;
        let DiagnosticPayload::InfrastructureError {
            msg, error_type, ..
        } = &diagnostic.payload
        else {
            return None;
        };

        Some((error_type, msg.as_str(), &diagnostic.primary_location))
    }

    /// Iterate over diagnostics with `Warning` severity.
    pub(crate) fn warnings(&self) -> impl Iterator<Item = &CompilerDiagnostic> {
        self.diagnostics
            .iter()
            .filter(|d| d.severity == DiagnosticSeverity::Warning)
    }

    /// Wrap a single `CompilerDiagnostic` into a `CompilerMessages` container with no warnings.
    ///
    /// WHY: frontend stages emit `CompilerDiagnostic` values directly and need a clean boundary
    /// conversion into the message container expected by build-system callers.
    pub fn from_diagnostic(diagnostic: CompilerDiagnostic, string_table: StringTable) -> Self {
        Self {
            diagnostics: vec![diagnostic],
            string_table,
            render_type_environment: None,
        }
    }

    /// Wrap a single `CompilerDiagnostic` while cloning the caller's active `StringTable`.
    pub fn from_diagnostic_ref(diagnostic: CompilerDiagnostic, string_table: &StringTable) -> Self {
        Self::from_diagnostic(diagnostic, string_table.clone())
    }

    /// Wrap a single `CompilerError` into a `CompilerMessages` container with no warnings.
    ///
    /// WHY: Several build/backend modules need to convert a `CompilerError` into the richer
    /// `CompilerMessages` type at a boundary. Centralising this avoids repeated inline struct
    /// literals scattered across callers.
    pub fn from_error(error: CompilerError, string_table: StringTable) -> Self {
        let diagnostic = compiler_error_to_diagnostic(&error);
        Self {
            diagnostics: vec![diagnostic],
            string_table,
            render_type_environment: None,
        }
    }

    /// Wrap one error while cloning the caller's active `StringTable`.
    ///
    /// WHAT: snapshots the current table state into the returned message container.
    /// WHY: frontend/build boundaries often only borrow the shared table, but diagnostics still
    /// need the full interned-path context accumulated so far.
    pub fn from_error_ref(error: CompilerError, string_table: &StringTable) -> Self {
        Self::from_error(error, string_table.clone())
    }

    /// Wrap already-collected warnings plus one infrastructure error while preserving table context.
    ///
    /// WHAT: carries forward the caller's warning set, appends the boundary failure, and clones
    /// the current `StringTable`.
    /// WHY: these helpers receive warnings that were produced before the failure. Keeping that
    /// order makes `CompilerMessages` a true production-order diagnostic stream.
    pub fn from_error_with_warnings(
        error: CompilerError,
        warning_diagnostics: Vec<CompilerDiagnostic>,
        string_table: &StringTable,
    ) -> Self {
        let mut diagnostics = warning_diagnostics;
        diagnostics.push(compiler_error_to_diagnostic(&error));
        Self {
            diagnostics,
            string_table: string_table.clone(),
            render_type_environment: None,
        }
    }

    /// Wrap already-collected warnings plus one typed diagnostic while preserving table context.
    ///
    /// WHAT: carries forward the caller's warning set and a clone of the current `StringTable`,
    /// then stores the typed boundary diagnostic directly in `diagnostics`.
    /// WHY: frontend stages that emit `CompilerDiagnostic` need to preserve structured payloads
    /// so that boundary renderers can resolve `StringId` values through the shared `StringTable`.
    /// The warning set was emitted before the failure, so it stays first.
    pub fn from_diagnostic_with_warnings(
        diagnostic: CompilerDiagnostic,
        warning_diagnostics: Vec<CompilerDiagnostic>,
        string_table: &StringTable,
    ) -> Self {
        let mut diagnostics = warning_diagnostics;
        diagnostics.push(diagnostic);
        Self {
            diagnostics,
            string_table: string_table.clone(),
            render_type_environment: None,
        }
    }

    /// Build a single file-scoped message set while preserving the caller's existing table state.
    ///
    /// WHAT: clones the current table, interns the failing path into that clone, and returns a
    /// message set that owns the resulting diagnostic context.
    /// WHY: file-system errors often arise after the current build already interned many other
    /// paths, so the returned diagnostics must preserve those older interned IDs as well.
    pub fn file_error(path: &Path, msg: impl Into<String>, string_table: &StringTable) -> Self {
        let mut error_string_table = string_table.clone();
        let error = CompilerError::file_error(path, msg, &mut error_string_table);
        Self::from_error(error, error_string_table)
    }

    pub(crate) fn with_render_type_environment(
        mut self,
        type_environment: TypeEnvironment,
    ) -> Self {
        // Keep the type table as boundary-owned render context. Diagnostics still carry only
        // `TypeId`s, and successful module results keep their `TypeEnvironment` on `Module`.
        self.render_type_environment = Some(Box::new(type_environment));
        self
    }

    pub(crate) fn render_type_environment(&self) -> Option<&TypeEnvironment> {
        self.render_type_environment.as_deref()
    }

    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        for diagnostic in self.diagnostics.iter_mut() {
            diagnostic.remap_string_ids(remap);
        }
        if let Some(type_environment) = self.render_type_environment.as_mut() {
            type_environment.remap_string_ids(remap);
        }
    }
}

#[derive(Debug, Eq, Hash, PartialEq, Clone)]
pub enum CompilerErrorMetadataKey {
    CompilationStage,

    // Optional guidance for direct internal/tooling error rendering.
    PrimarySuggestion,
    AlternativeSuggestion,
    SuggestedReplacement,
    SuggestedInsertion,
    SuggestedLocation,
}

// -------------------------
//  Internal Compiler Error
// -------------------------

#[derive(Debug, Clone)]
pub struct CompilerError {
    pub msg: String,

    // Stores the interned source scope for this diagnostic. Header-local scopes may include a
    // synthetic `.header` suffix and are resolved back to real file paths only at render time.
    pub location: SourceLocation,
    pub error_type: ErrorType,

    // Structured guidance for internal/tooling failures. User-facing diagnostics carry typed
    // payload facts on `CompilerDiagnostic` instead of using this string map.
    pub metadata: HashMap<CompilerErrorMetadataKey, String>,
}

impl CompilerError {
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.location.remap_string_ids(remap);
    }

    pub fn new(
        msg: impl Into<String>,
        location: SourceLocation,
        error_type: ErrorType,
    ) -> CompilerError {
        CompilerError {
            msg: msg.into(),
            location,
            error_type,
            metadata: HashMap::new(),
        }
    }

    /// Replace only the location scope path while preserving the existing span positions.
    ///
    /// WHAT: rewrites the interned path for a diagnostic without touching its line/column data.
    /// WHY: some helpers need to attach a resolved file path after building a precise span-based
    /// error, and downgrading that span to a file-level location would lose useful diagnostics.
    pub fn with_scope_path(mut self, file_path: &Path, string_table: &mut StringTable) -> Self {
        self.location.scope = InternedPath::from_path_buf(file_path, string_table);
        self
    }

    pub fn with_error_type(mut self, error_type: ErrorType) -> Self {
        self.error_type = error_type;
        self
    }

    pub fn new_metadata_entry(&mut self, key: CompilerErrorMetadataKey, value: String) {
        self.metadata.insert(key, value);
    }

    /// Create a thread panic error (internal compiler_frontend issue)
    pub fn new_thread_panic(msg: impl Into<String>) -> Self {
        CompilerError {
            msg: msg.into(),
            location: SourceLocation::default(),
            error_type: ErrorType::Compiler,
            metadata: HashMap::new(),
        }
    }

    /// Create a compiler_frontend error (internal bug, not user's fault)
    // Existing backend and frontend invariant checks use `CompilerError::compiler_error(...)`
    // as the direct constructor for infrastructure diagnostics.
    #[allow(clippy::self_named_constructors)]
    pub fn compiler_error(msg: impl Into<String>) -> Self {
        CompilerError {
            msg: msg.into(),
            location: SourceLocation::default(),
            error_type: ErrorType::Compiler,
            metadata: HashMap::new(),
        }
    }

    /// Create a file system error from a Path
    pub fn file_error(path: &Path, msg: impl Into<String>, string_table: &mut StringTable) -> Self {
        CompilerError {
            msg: msg.into(),
            location: SourceLocation::from_path(path, string_table),
            error_type: ErrorType::File,
            metadata: HashMap::new(),
        }
    }

    /// Create a file system error from Path with metadata
    pub fn new_file_error(
        path: &Path,
        msg: impl Into<String>,
        metadata: HashMap<CompilerErrorMetadataKey, String>,
        string_table: &mut StringTable,
    ) -> Self {
        CompilerError {
            msg: msg.into(),
            location: SourceLocation::from_path(path, string_table),
            error_type: ErrorType::File,
            metadata,
        }
    }
}

// Adds more information to the CompilerError
// So it knows the file path (possible specific part of the line soon)
// And the type of error
#[derive(PartialEq, Debug, Clone)]
pub enum ErrorType {
    File,
    Config,
    Compiler,
    DevServer,
    HirTransformation,
    Backend(crate::backends::error_types::BackendErrorType),
}

/// Convert a direct `CompilerError` into the boundary diagnostic sequence.
///
/// This exists only for infrastructure/tooling paths that still return `CompilerError`.
pub(crate) fn compiler_error_to_diagnostic(error: &CompilerError) -> CompilerDiagnostic {
    CompilerDiagnostic::with_severity(
        DiagnosticKind::Infrastructure(InfrastructureDiagnosticKind::InfrastructureFailure),
        DiagnosticSeverity::Error,
        error.location.clone(),
        DiagnosticPayload::InfrastructureError {
            msg: error.msg.clone(),
            error_type: error.error_type.clone(),
            metadata: error.metadata.clone(),
        },
    )
}

/// Return a filesystem infrastructure error.
///
/// Usage: `return_file_error!(path, "message", { metadata })`;
#[macro_export]
macro_rules! return_file_error {
    // Metadata usage for direct infrastructure rendering.
    ($string_table:expr, $path:expr, $msg:expr, { $( $key:ident => $value:expr ),* $(,)? }) => {{
        return Err($crate::compiler_frontend::compiler_errors::CompilerError::new_file_error(
            $path,
            $msg,
            {
                let mut map = std::collections::HashMap::new();
                $( map.insert($crate::compiler_frontend::compiler_errors::CompilerErrorMetadataKey::$key, $value.into()); )*
                map
            },
            $string_table,
        ));
    }};
    // Usage without guidance metadata.
    ($string_table:expr, $path:expr, $msg:expr) => {{
        return Err($crate::compiler_frontend::compiler_errors::CompilerError::file_error(
            $path,
            $msg,
            $string_table,
        ));
    }};
}

/// Returns a new CompilerError for internal compiler_frontend bugs.
///
/// Compiler errors indicate bugs in the compiler_frontend itself, not user code issues.
/// These provide the location of the bug in the compiler_frontend source code
#[macro_export]
macro_rules! return_compiler_error {
    // Variant with format string, arguments, and metadata (with semicolon separator)
    ($fmt:expr, $($arg:expr),+ ; { $( $key:ident => $value:expr ),* $(,)? }) => {{
        let mut error = $crate::compiler_frontend::compiler_errors::CompilerError::compiler_error(
            format!($fmt, $($arg),+)
        );
        $(
            error.new_metadata_entry(
                $crate::compiler_frontend::compiler_errors::CompilerErrorMetadataKey::$key,
                $value.into(),
            );
        )*
        return Err(error);
    }};
    // Variant with format string and arguments (no metadata)
    ($fmt:expr, $($arg:expr),+ $(,)?) => {{
        return Err($crate::compiler_frontend::compiler_errors::CompilerError::compiler_error(
            format!($fmt, $($arg),+)
        ));
    }};
    // Variant with message and metadata (with semicolon separator)
    ($msg:expr ; { $( $key:ident => $value:expr ),* $(,)? }) => {{
        let mut error = $crate::compiler_frontend::compiler_errors::CompilerError::compiler_error(
            $msg
        );
        $(
            error.new_metadata_entry(
                $crate::compiler_frontend::compiler_errors::CompilerErrorMetadataKey::$key,
                $value.into(),
            );
        )*
        return Err(error);
    }};
    // Simple variant with just a message (no metadata)
    ($msg:expr) => {{
        return Err($crate::compiler_frontend::compiler_errors::CompilerError::compiler_error(
            $msg
        ));
    }};
}

/// Returns a new CompilerError for HIR transformation failures.
///
/// HIR transformation errors indicate failures during AST to HIR conversion.
/// These are typically compiler_frontend bugs where the HIR infrastructure is missing
/// or incomplete for a particular language feature.
///
/// Usage: `return_hir_transformation_error!("Function '{}' transformation not yet implemented", func_name, location, {})`;
#[macro_export]
macro_rules! return_hir_transformation_error {
    // HIR failures may carry metadata for direct infrastructure rendering.
    ($msg:expr, $location:expr, { $( $key:ident => $value:expr ),* $(,)? }) => {
        let mut error = $crate::compiler_frontend::compiler_errors::CompilerError::new(
            $msg,
            $location,
            $crate::compiler_frontend::compiler_errors::ErrorType::HirTransformation,
        );
        $(
            error.new_metadata_entry(
                $crate::compiler_frontend::compiler_errors::CompilerErrorMetadataKey::$key,
                $value.into(),
            );
        )*
        return Err(error)
    };
    ($msg:expr, $location:expr) => {
        return Err($crate::compiler_frontend::compiler_errors::CompilerError::new(
            $msg,
            $location,
            $crate::compiler_frontend::compiler_errors::ErrorType::HirTransformation,
        ))
    };
}
