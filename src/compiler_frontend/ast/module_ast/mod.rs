//! AST module construction and scope-context helpers.
//!
//! WHAT: combines per-file headers into one typed AST, resolves file-scoped imports, lowers
//! function/struct/const bodies, and synthesizes top-level template fragments.
//! WHY: this stage is where module-wide symbol identity and per-file visibility are enforced
//! together, so diagnostics must preserve the full shared `StringTable` context.
//!
//! ## Pass sequence (see each sub-module for details)
//!
//! 1. `pass_import_bindings` — build per-file visibility gates
//! 2. `pass_type_alias_resolution` — resolve type alias targets
//! 3. `pass_type_resolution` — resolve constants and struct field types
//! 4. `pass_function_signatures` — resolve function signatures
//! 5. `build_receiver_catalog` — build receiver method index from resolved signatures
//! 6. `pass_emit_nodes` — lower function/template bodies into AST nodes
//! 7. `finalization` — normalize templates and assemble final [`Ast`]
//!
//! The entry point and final assembly live in [`crate::compiler_frontend::ast::Ast::new`].

pub(in crate::compiler_frontend::ast) mod build_state;
mod finalization;
mod pass_emit_nodes;
mod pass_function_signatures;
mod pass_import_bindings;
mod pass_type_alias_resolution;
mod pass_type_resolution;
pub(crate) mod scope_context;

// Internal re-exports so `ast/mod.rs` can surface the minimal public API.
//
// `Ast` and `AstBuildContext` live in `ast/mod.rs` (the strict module entry point).
// The types below are re-exported here only so `ast/mod.rs` can re-export them;
// callers should import through `ast::` directly.
#[cfg(test)]
pub(crate) use scope_context::{ReceiverMethodCatalog, ReceiverMethodEntry};

#[cfg(test)]
#[path = "../tests/module_ast_receiver_method_tests.rs"]
mod module_ast_receiver_method_tests;

#[cfg(test)]
#[path = "../tests/choice_expression_tests.rs"]
mod choice_expression_tests;

#[cfg(test)]
#[path = "../tests/scope_context_tests.rs"]
mod scope_context_tests;
