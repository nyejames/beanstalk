//! JavaScript backend for Beanstalk.
//!
//! This backend lowers HIR into readable JavaScript using GC semantics.
//! Borrowing and ownership are optimization concerns and therefore ignored here.

mod emitter;
mod js_expr;
mod js_function;
mod js_statement;
mod libraries;
mod runtime;
mod symbols;
mod utils;

#[cfg(test)]
pub(crate) mod test_symbol_helpers;
#[cfg(test)]
mod tests;

pub(crate) use emitter::JsEmitter;
pub use emitter::lower_hir_to_js;

use crate::compiler_frontend::external_packages::{ExternalFunctionId, ExternalPackageRegistry};
use crate::compiler_frontend::hir::ids::FunctionId;
use std::collections::{HashMap, HashSet};

/// Configuration for JS lowering.
#[derive(Debug, Clone)]
pub struct JsLoweringConfig {
    /// Emit human-readable formatting.
    pub pretty: bool,

    /// Emit source location comments.
    pub emit_locations: bool,

    /// Automatically invoke the module start function.
    pub auto_invoke_start: bool,

    /// External package registry for resolving backend lowering metadata.
    pub external_package_registry: ExternalPackageRegistry,
    /// Allow provider-created ES module exports to lower through generated HTML glue.
    ///
    /// WHY: only the HTML builder can emit the matching ES module glue. Direct JS backend
    /// lowering must reject these exports unless that builder path explicitly opts in.
    pub external_module_export_glue_enabled: bool,
}

impl JsLoweringConfig {
    /// Standard HTML builder lowering config.
    pub fn standard_html(release_build: bool) -> Self {
        JsLoweringConfig {
            pretty: !release_build,
            emit_locations: false,
            auto_invoke_start: false,
            external_package_registry: ExternalPackageRegistry::new(),
            external_module_export_glue_enabled: false,
        }
    }
}

/// Deterministic JS identifier for a generated glue wrapper.
///
/// WHAT: maps stable external function IDs to safe wrapper function names.
/// WHY: the JS backend and the HTML glue generator must agree without duplicating naming logic.
pub(crate) fn external_module_export_glue_function_name(id: ExternalFunctionId) -> String {
    match id {
        ExternalFunctionId::Synthetic(n) => format!("__bs_glue_fn{n}"),
        other => format!("__bs_glue_{}", other.name()),
    }
}

/// Result of lowering a HIR module to JavaScript.
#[derive(Debug, Clone)]
pub struct JsModule {
    /// Complete JS source code.
    pub source: String,
    pub function_name_by_id: HashMap<FunctionId, String>,
    /// Set of external function IDs referenced during lowering.
    /// WHY: the HTML builder uses this to decide which generated glue wrappers to emit.
    pub referenced_external_functions: HashSet<ExternalFunctionId>,
}
