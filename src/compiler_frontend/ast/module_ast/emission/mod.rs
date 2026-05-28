//! AST node emission.
//!
//! WHAT: lowers executable header payloads and const templates after the semantic environment is
//! complete.
//! WHY: body parsing needs full visibility/type information, but should not mutate environment
//! construction state.

pub(in crate::compiler_frontend::ast) mod emitter;

pub(in crate::compiler_frontend::ast) use emitter::{AstEmission, AstEmitter};
