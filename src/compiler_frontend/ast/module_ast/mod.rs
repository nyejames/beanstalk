//! AST module construction and scope-context helpers.
//!
//! WHAT: combines per-file headers into one typed AST, resolves file-scoped imports, lowers
//! function/struct/const bodies, and synthesizes top-level template fragments.
//! WHY: this stage is where module-wide symbol identity and per-file visibility are enforced
//! together, so diagnostics must preserve the full shared `StringTable` context.
//!
//! ## Pass sequence (see each sub-module for details)
//!
//! 1. `pass_declarations`      — register all symbols module-wide
//! 2. `pass_import_bindings`   — build per-file visibility gates
//! 3. `pass_type_resolution`   — resolve constants and struct field types
//! 4. `pass_function_signatures` — resolve function signatures; build receiver catalog
//! 5. `pass_emit_nodes`        — lower function/template bodies into AST nodes
//! 6. `pass_finalize`          — synthesize templates; assemble final `Ast`

mod build_state;
mod pass_declarations;
mod pass_emit_nodes;
mod pass_finalize;
mod pass_function_signatures;
mod pass_import_bindings;
mod pass_type_resolution;
pub(crate) mod scope_context;

// Public surface: re-export everything that callers currently import from this module.
// WHY: these items are used by HIR, borrow-checker, and test code through the
// `ast::ast::*` path. The linter cannot detect cross-file usage of pub re-exports,
// so false-positive "unused" warnings are suppressed here.
#[allow(unused_imports)]
pub use crate::compiler_frontend::ast::templates::top_level_templates::{
    AstDocFragment, AstDocFragmentKind, AstStartTemplateItem,
};
#[allow(unused_imports)]
pub use pass_finalize::{Ast, ModuleExport};
pub use scope_context::{ContextKind, ScopeContext};
#[allow(unused_imports)]
pub(crate) use scope_context::{ReceiverMethodCatalog, ReceiverMethodEntry};

use crate::compiler_frontend::headers::parse_file_headers::Header;
use crate::compiler_frontend::string_interning::StringTable;

/// Returns the canonical (real OS) filesystem path for the source file that owns this header.
/// Falls back to the logical source-file path when no OS path is recorded.
/// WHY: const-template scopes use synthetic paths; the canonical path is needed for
/// project-path-resolver lookups and rendered-path-usage tracking.
pub(super) fn canonical_source_file_for_header(
    header: &Header,
    string_table: &mut StringTable,
) -> crate::compiler_frontend::interned_path::InternedPath {
    header
        .tokens
        .canonical_os_path
        .as_ref()
        .map(|canonical_path| {
            crate::compiler_frontend::interned_path::InternedPath::from_path_buf(
                canonical_path,
                string_table,
            )
        })
        .unwrap_or_else(|| header.source_file.to_owned())
}

#[cfg(test)]
#[path = "../tests/module_ast_receiver_method_tests.rs"]
mod module_ast_receiver_method_tests;

#[cfg(test)]
#[path = "../tests/choice_expression_tests.rs"]
mod choice_expression_tests;

#[cfg(test)]
#[path = "../tests/scope_context_tests.rs"]
mod scope_context_tests;
