//! JavaScript backend for Beanstalk.
//!
//! This backend lowers HIR into readable JavaScript using GC semantics.
//! Borrowing and ownership are optimization concerns and therefore ignored here.

mod emitter;
mod js_expr;
mod js_function;
mod js_host_functions;
mod js_statement;
mod runtime;
mod symbols;
mod utils;

#[cfg(test)]
pub(crate) mod test_symbol_helpers;
#[cfg(test)]
mod tests;

pub(crate) use emitter::JsEmitter;
pub use emitter::lower_hir_to_js;

use crate::compiler_frontend::hir::hir_nodes::FunctionId;
use std::collections::HashMap;

/// Configuration for JS lowering.
#[derive(Debug, Clone)]
pub struct JsLoweringConfig {
    /// Emit human-readable formatting.
    pub pretty: bool,

    /// Emit source location comments.
    pub emit_locations: bool,

    /// Automatically invoke the module start function.
    pub auto_invoke_start: bool,
}

impl JsLoweringConfig {
    /// Standard HTML builder lowering config.
    pub fn standard_html(release_build: bool) -> Self {
        JsLoweringConfig {
            pretty: !release_build,
            emit_locations: false,
            auto_invoke_start: false,
        }
    }
}

/// Result of lowering a HIR module to JavaScript.
#[derive(Debug, Clone)]
pub struct JsModule {
    /// Complete JS source code.
    pub source: String,
    pub function_name_by_id: HashMap<FunctionId, String>,
}
