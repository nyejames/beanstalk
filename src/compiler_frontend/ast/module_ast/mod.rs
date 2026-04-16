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
//! 2. `pass_type_resolution` — resolve constants and struct field types
//! 3. `pass_function_signatures` — resolve function signatures; build receiver catalog
//! 4. `pass_emit_nodes` — lower function/template bodies into AST nodes
//! 5. `orchestrate` — normalize templates; assemble final `Ast`

mod build_state;
mod finalization;
mod orchestrate;
mod pass_emit_nodes;
mod pass_import_bindings;
mod pass_function_signatures;
mod pass_type_resolution;
pub(crate) mod scope_context;

// Public AST surface consumed by later compiler stages.
#[cfg(test)]
pub use crate::compiler_frontend::ast::templates::top_level_templates::AstDocFragment;
pub use crate::compiler_frontend::ast::templates::top_level_templates::AstDocFragmentKind;
pub use orchestrate::{Ast, AstBuildContext};
pub use scope_context::{ContextKind, ScopeContext};
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
