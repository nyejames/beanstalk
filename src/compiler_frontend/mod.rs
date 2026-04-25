//! Compiler frontend pipeline.
//!
//! WHAT: tokenization, header parsing, dependency sorting, AST/HIR construction, and borrow
//! validation, wired into the stage flow described in the compiler design overview.

pub(crate) mod ast;
pub(crate) mod declaration_syntax;
pub(crate) mod headers;
pub(crate) mod style_directives;
pub(crate) mod tokenizer;
pub(crate) mod optimizers {
    pub(crate) mod constant_folding;
}

pub(crate) mod module_dependencies;

pub(crate) mod basic_utility_functions;
pub(crate) mod builtins;
pub(crate) mod deferred_feature_diagnostics;

pub(crate) mod reserved_trait_syntax;

pub(crate) mod compiler_messages {
    pub(crate) mod compiler_dev_logging;
    pub(crate) mod compiler_errors;
    pub(crate) mod compiler_warnings;
    pub(crate) mod display_messages;
    pub(crate) mod source_location;
}

pub(crate) mod symbols {
    pub(crate) mod identifier_policy;
    pub(crate) mod identity;
    pub(crate) mod string_interning;
}

pub(crate) use compiler_messages::compiler_errors;
pub(crate) use compiler_messages::compiler_warnings;
pub(crate) use compiler_messages::display_messages;
pub(crate) mod datatypes;
pub(crate) mod interned_path;
pub(crate) mod syntax_errors;
pub(crate) mod token_scan;
pub(crate) mod type_coercion;

pub(crate) mod host_functions;

pub(crate) mod hir;

pub(crate) mod analysis;

pub(crate) mod paths;

mod pipeline;

pub use pipeline::CompilerFrontend;

/// Flags change the behavior of the core `compiler_frontend` pipeline.
#[derive(PartialEq, Debug, Clone)]
pub enum Flag {
    Release,
    DisableWarnings,
    DisableTimers,
    HtmlWasm,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrontendBuildProfile {
    Dev,
    Release,
}

#[cfg(test)]
pub(crate) mod tests {
    pub(crate) mod test_support;
}
