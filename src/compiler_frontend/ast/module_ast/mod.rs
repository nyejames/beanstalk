//! AST module construction phases and scope-context helpers.
//!
//! WHAT: maps the AST stage into three explicit owners:
//! `build_ast_environment`, `emit_ast_nodes`, and `finalize_ast`.
//! WHY: the old pass accumulator made field validity depend on implicit ordering. Keeping
//! environment, emission, and finalization state separate makes stage ownership reviewable.
//!
//! ## Phase ownership
//!
//! - `environment` builds import bindings, resolved declarations, signatures, and receiver data.
//! - `emission` lowers executable bodies and const templates into AST-owned output state.
//! - `finalization` normalizes HIR-boundary templates/constants and assembles [`Ast`].
//!
//! The entry point and final assembly live in [`crate::compiler_frontend::ast::Ast::new`].

pub(in crate::compiler_frontend::ast) mod build_context;
pub(in crate::compiler_frontend::ast) mod emission;
pub(in crate::compiler_frontend::ast) mod environment;
pub(in crate::compiler_frontend::ast) mod finalization;
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
