//! AST semantic environment construction.
//!
//! WHAT: resolves imports, aliases, nominal declarations, constants, signatures, and receiver
//! methods into a stable declaration table before executable bodies are parsed.
//! WHY: body emission should read one complete semantic environment instead of relying on
//! partially valid accumulator fields.

pub(in crate::compiler_frontend::ast) mod builder;
pub(in crate::compiler_frontend::ast) mod constant_resolution;
pub(in crate::compiler_frontend::ast) mod declaration_table;
mod function_signatures;
mod type_aliases;
mod type_resolution;

// --------------------------
//  Re-exports
// --------------------------

pub(in crate::compiler_frontend::ast) use builder::{
    AstEnvironmentInput, AstModuleEnvironment, AstModuleEnvironmentBuilder,
};
pub(crate) use declaration_table::TopLevelDeclarationTable;
