//! AST semantic environment construction.
//!
//! WHAT: resolves imports, aliases, nominal declarations, constants, signatures, and receiver
//! methods before executable bodies are parsed.
//! WHY: body emission should read one complete semantic environment instead of relying on
//! partially valid accumulator fields.

pub(in crate::compiler_frontend::ast) mod builder;
mod function_signatures;
mod import_environment;
mod type_aliases;
mod type_resolution;

pub(in crate::compiler_frontend::ast) use builder::{
    AstModuleEnvironment, AstModuleEnvironmentBuilder,
};
