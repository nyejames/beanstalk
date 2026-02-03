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

use crate::compiler::hir::nodes::{BlockId, HirBlock, HirModule};
use crate::compiler::string_interning::{InternedString, StringTable};
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

    /// String table for resolving interned strings
    pub string_table: &'hir StringTable,

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

    /// Tracks which variables have been declared in the current scope
    ///
    /// Used to distinguish between `let` declarations and reassignments.
    /// When a variable is assigned for the first time, we emit `let x = value;`
    /// For subsequent assignments to the same variable, we emit `x = value;`
    pub declared_vars: HashSet<String>,
}

/// A JS identifier that is safe to emit directly
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct JsIdent(pub String);

/// A JS label used for breaking / continuing loops
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct JsLabel(pub String);

impl<'hir> JsEmitter<'hir> {
    /// Creates a new JsEmitter for the given HIR module
    ///
    /// This constructor:
    /// - Builds a block lookup table for fast access during emission
    /// - Initializes indentation tracking
    /// - Sets up name hygiene and temporary counters
    /// - Configures output formatting options
    pub fn new(hir: &'hir HirModule, string_table: &'hir StringTable, config: JsLoweringConfig) -> Self {
        // Build block lookup table for fast access
        let blocks: HashMap<BlockId, &'hir HirBlock> = hir
            .blocks
            .iter()
            .map(|block| (block.id, block))
            .collect();

        JsEmitter {
            hir,
            string_table,
            out: String::new(),
            indent: 0,
            config,
            blocks,
            loop_labels: HashMap::new(),
            used_names: HashSet::new(),
            temp_counter: 0,
            declared_vars: HashSet::new(),
        }
    }

    /// Emits a newline and proper indentation
    pub(crate) fn emit_indent(&mut self) {
        if self.config.pretty {
            self.out.push('\n');
            for _ in 0..self.indent {
                self.out.push_str("    "); // 4 spaces per indent level
            }
        }
    }

    /// Emits a string directly to the output buffer
    pub(crate) fn emit(&mut self, s: &str) {
        self.out.push_str(s);
    }

    /// Increases indentation depth
    pub fn indent(&mut self) {
        self.indent += 1;
    }

    /// Decreases indentation depth
    pub fn dedent(&mut self) {
        if self.indent > 0 {
            self.indent -= 1;
        }
    }

    /// Generates a unique temporary variable name
    pub fn gen_temp(&mut self) -> JsIdent {
        loop {
            let name = format!("_t{}", self.temp_counter);
            self.temp_counter += 1;
            
            if !self.used_names.contains(&name) {
                self.used_names.insert(name.clone());
                return JsIdent(name);
            }
        }
    }

    /// Converts a Beanstalk identifier to a safe JavaScript identifier
    ///
    /// Handles:
    /// - JavaScript reserved words
    /// - Name collision avoidance
    /// - Special character escaping
    pub fn make_js_ident(&mut self, name: InternedString) -> JsIdent {
        let name_str = self.string_table.resolve(name);
        
        // Check if it's a JavaScript reserved word
        let safe_name = if is_js_reserved(name_str) {
            format!("_{}", name_str)
        } else {
            name_str.to_string()
        };

        // Track usage for collision detection
        self.used_names.insert(safe_name.clone());
        
        JsIdent(safe_name)
    }

    /// Generates a unique loop label
    pub fn gen_loop_label(&mut self, block_id: BlockId) -> JsLabel {
        let label = format!("loop_{}", block_id);
        JsLabel(label)
    }
}

/// Checks if a string is a JavaScript reserved word or keyword
fn is_js_reserved(name: &str) -> bool {
    matches!(
        name,
        // JavaScript keywords
        "break" | "case" | "catch" | "class" | "const" | "continue" | "debugger" |
        "default" | "delete" | "do" | "else" | "export" | "extends" | "finally" |
        "for" | "function" | "if" | "import" | "in" | "instanceof" | "new" |
        "return" | "super" | "switch" | "this" | "throw" | "try" | "typeof" |
        "var" | "void" | "while" | "with" | "yield" |
        // Future reserved words
        "enum" | "implements" | "interface" | "let" | "package" | "private" |
        "protected" | "public" | "static" | "await" | "abstract" | "boolean" |
        "byte" | "char" | "double" | "final" | "float" | "goto" | "int" | "long" |
        "native" | "short" | "synchronized" | "throws" | "transient" | "volatile" |
        // Common globals that should be avoided
        "undefined" | "null" | "true" | "false" | "NaN" | "Infinity" |
        "eval" | "arguments" | "Array" | "Object" | "String" | "Number" |
        "Boolean" | "Date" | "Math" | "JSON" | "console"
    )
}

/// Main entry point for lowering HIR to JavaScript
pub fn lower_hir_to_js(hir: &HirModule, string_table: &StringTable, config: JsLoweringConfig) -> JsModule {
    let emitter = JsEmitter::new(hir, string_table, config);
    
    // TODO: Implement the actual lowering logic
    // This will be implemented in subsequent tasks
    
    JsModule {
        source: emitter.out,
    }
}
