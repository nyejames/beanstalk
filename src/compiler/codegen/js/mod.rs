//! JavaScript backend for Beanstalk
//!
//! This backend lowers HIR into **structured JavaScript** with pure GC semantics.
//! Ownership, drops, and borrow annotations are ignored entirely.
//!
//! Design goals:
//! - Readable JS output
//! - Structured control flow (no block dispatch)
//! - Semantics-faithful, not CFG-faithful
//! - Minimal runtime scaffolding

mod js_expr;
mod js_function;
mod js_statement;

use crate::compiler::codegen::js::js_statement::JsStmt;
use crate::compiler::hir::nodes::{BlockId, HirBlock, HirModule};
use crate::compiler::string_interning::InternedString;
use std::collections::{HashMap, HashSet};

/// Configuration for JS lowering
#[derive(Debug, Clone)]
pub struct JsLoweringConfig {
    /// Emit human-readable formatting (indentation, newlines)
    pub pretty: bool,

    /// Emit source location comments
    pub emit_locations: bool,
}

/// Result of lowering a HIR module to JavaScript
#[derive(Debug, Clone)]
pub struct JsModule {
    /// Complete JS source code
    pub source: String,
}

/// Internal state for JS emission
///
/// This is *not* an IR. It is a structured printer with context.
pub struct JsEmitter<'hir> {
    /// Source HIR module
    pub hir: &'hir HirModule,

    /// Output buffer
    pub out: String,

    /// Current indentation depth
    pub indent: usize,

    /// Lowering configuration
    pub config: JsLoweringConfig,

    /// Map of block IDs to their blocks
    ///
    /// Cached for fast lookup during recursive emission.
    pub blocks: HashMap<BlockId, &'hir HirBlock>,

    /// Active loop labels for `break` / `continue`
    ///
    /// Maps loop block IDs to JS label names.
    pub loop_labels: HashMap<BlockId, JsLabel>,

    /// Names that are already used in the current JS scope
    ///
    /// Used to avoid collisions when generating temporaries.
    pub used_names: HashSet<String>,

    /// Counter for generating unique temporary names
    pub temp_counter: usize,
}

/// A JS identifier that is safe to emit directly
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct JsIdent(pub String);

/// A JS label used for breaking / continuing loops
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct JsLabel(pub String);

/// Result of lowering a HIR expression to JS
///
/// JS expressions may require:
/// - a pure expression (`a + b`)
/// - or a sequence of statements + a final value
#[derive(Debug)]
pub struct JsExpr {
    /// Statements that must run before the value is available
    pub prelude: Vec<JsStmt>,

    /// JS expression string representing the value
    pub value: String,
}
