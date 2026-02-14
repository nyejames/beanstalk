// This is early prototype code, so ignore placeholder unused stuff for now
#![allow(unused)]

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
mod js_host_functions;
mod js_statement;

// Re-export types that are used in tests and external code
pub use js_expr::JsExpr;
pub use js_statement::JsStmt;

use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::hir::nodes::{BlockId, HirBlock, HirExpr, HirKind, HirModule};
use crate::compiler_frontend::string_interning::{InternedString, StringTable};
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
    pub fn new(
        hir: &'hir HirModule,
        string_table: &'hir StringTable,
        config: JsLoweringConfig,
    ) -> Self {
        // Build block lookup table for fast access
        let blocks: HashMap<BlockId, &'hir HirBlock> =
            hir.blocks.iter().map(|block| (block.id, block)).collect();

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

    /// Emits a source location comment if configured
    ///
    /// When `emit_locations` is enabled in the configuration, this method
    /// emits a comment indicating the original Beanstalk source location
    /// for the current HIR node.
    ///
    /// Format: `/* Line X:Y-Z */` where X is the line number, Y is the start column,
    /// and Z is the end column.
    ///
    /// Location comments are only emitted when:
    /// - `emit_locations` is enabled in the configuration
    /// - Pretty printing is enabled (for readability)
    ///
    /// In compact mode, location comments are suppressed to minimize output size.
    pub(crate) fn emit_location_comment(
        &mut self,
        location: &crate::compiler_frontend::parsers::tokenizer::tokens::TextLocation,
    ) {
        // Only emit location comments in pretty mode with emit_locations enabled
        if self.config.emit_locations && self.config.pretty {
            let line = location.start_pos.line_number + 1; // Convert to 1-based
            let start_col = location.start_pos.char_column;
            let end_col = location.end_pos.char_column;

            self.emit(&format!(" /* Line {}:{}-{} */", line, start_col, end_col));
        }
    }

    /// Emits a newline and proper indentation
    ///
    /// When pretty printing is enabled, this emits a newline followed by
    /// indentation (4 spaces per level). When pretty printing is disabled,
    /// this emits nothing, resulting in compact output.
    pub(crate) fn emit_indent(&mut self) {
        if self.config.pretty {
            self.out.push('\n');
            for _ in 0..self.indent {
                self.out.push_str("    "); // 4 spaces per indent level
            }
        }
    }

    /// Emits a space if pretty printing is enabled
    ///
    /// Use this to add spacing between tokens in pretty mode while
    /// keeping output compact in non-pretty mode.
    pub(crate) fn emit_space(&mut self) {
        if self.config.pretty {
            self.out.push(' ');
        }
    }

    /// Emits a newline if pretty printing is enabled
    ///
    /// Use this to add blank lines between major sections in pretty mode
    /// while keeping output compact in non-pretty mode.
    pub(crate) fn emit_newline(&mut self) {
        if self.config.pretty {
            self.out.push('\n');
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

    /// Emits a block recursively
    ///
    /// This method processes all statements in a block sequentially,
    /// then handles the block's terminator. Child blocks referenced by
    /// the terminator are emitted recursively.
    ///
    /// **Block Processing Algorithm**:
    /// 1. Process all statements in the block sequentially
    /// 2. Handle the block's terminator
    /// 3. Recursively emit referenced child blocks
    /// 4. Maintain proper indentation and scoping
    ///
    /// **Debug Output**:
    /// - When `emit_locations` is enabled, source location comments are added
    /// - Comments indicate the original Beanstalk source location for each node
    ///
    /// Returns an error if the block is not found or if lowering fails.
    pub fn emit_block(&mut self, block_id: BlockId) -> Result<(), CompilerError> {
        // Look up the block
        let block = match self.blocks.get(&block_id) {
            Some(block) => *block,
            None => {
                // Block not found - this is a compiler_frontend bug
                return Err(CompilerError::compiler_error(format!(
                    "JavaScript backend: Block {} not found in HIR module. This indicates a bug in HIR construction.",
                    block_id
                )));
            }
        };

        // Process all statements in the block
        for node in &block.nodes {
            // Emit location comment if configured
            if self.config.emit_locations {
                self.emit_location_comment(&node.location);
            }

            match &node.kind {
                crate::compiler_frontend::hir::nodes::HirKind::Stmt(stmt) => {
                    self.lower_stmt(stmt)?;
                }
                crate::compiler_frontend::hir::nodes::HirKind::Terminator(term) => {
                    self.lower_terminator(term)?;
                }
            }
        }

        Ok(())
    }

    /// Lowers a HIR terminator to JavaScript
    ///
    /// Terminators control the flow of execution and may reference other blocks.
    /// This method handles all terminator types and ensures proper control flow
    /// structure in the generated JavaScript.
    ///
    /// Returns an error if an unsupported terminator is encountered or if lowering fails.
    pub fn lower_terminator(
        &mut self,
        terminator: &crate::compiler_frontend::hir::nodes::HirTerminator,
    ) -> Result<(), CompilerError> {
        use crate::compiler_frontend::hir::nodes::HirTerminator;

        match terminator {
            HirTerminator::If {
                condition,
                then_block,
                else_block,
            } => {
                self.lower_if_terminator(condition, *then_block, *else_block)?;
            }

            HirTerminator::Loop {
                label,
                binding,
                iterator,
                body,
                index_binding,
            } => {
                self.lower_loop_terminator(*label, binding, iterator, *body, index_binding)?;
            }

            HirTerminator::Break { target } => {
                self.lower_break_terminator(*target);
            }

            HirTerminator::Continue { target } => {
                self.lower_continue_terminator(*target);
            }

            HirTerminator::Return(values) => {
                self.lower_return_terminator(values)?;
            }

            HirTerminator::Match {
                scrutinee,
                arms,
                default_block,
            } => {
                self.lower_match_terminator(scrutinee, arms, *default_block)?;
            }

            HirTerminator::ReturnError(expr) => {
                // Error returns become JavaScript throw statements
                self.emit_indent();
                let error_expr = self.lower_expr(expr)?;
                self.emit_stmts(&error_expr.prelude);
                self.emit_indent();
                self.emit(&format!("throw {};", error_expr.value));
            }

            HirTerminator::Panic { message } => {
                // Panics become JavaScript throw statements
                self.emit_indent();
                if let Some(msg) = message {
                    let msg_expr = self.lower_expr(msg)?;
                    self.emit_stmts(&msg_expr.prelude);
                    self.emit_indent();
                    self.emit(&format!("throw new Error({});", msg_expr.value));
                } else {
                    self.emit("throw new Error(\"panic\");");
                }
            }
        }

        Ok(())
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
///
/// Returns a Result containing either the generated JavaScript module or a compilation error.
/// Errors can occur when encountering unsupported HIR nodes or invalid constructs.
pub fn lower_hir_to_js(
    hir: &HirModule,
    string_table: &StringTable,
    config: JsLoweringConfig,
) -> Result<JsModule, CompilerError> {
    let mut emitter = JsEmitter::new(hir, string_table, config);

    // Emit all function definitions from the HIR module
    // Functions are stored as nodes in the module's functions list
    for func_node in &hir.functions {
        match &func_node.kind {
            HirKind::Stmt(stmt) => {
                emitter.lower_function_or_template_stmt(stmt)?;
                emitter.emit_indent(); // Add a blank line between functions
            }
            _ => {
                // Functions should always be statements
                // If we encounter a non-statement, this is a compiler_frontend bug
                return Err(CompilerError::compiler_error(
                    "JavaScript backend: Function node is not a statement. This is a compiler_frontend bug - HIR should only contain statement nodes in the functions list.",
                ));
            }
        }
    }

    // Emit the entry block (main execution)
    // The entry block contains the top-level code that runs when the module loads
    if !hir.blocks.is_empty() {
        emitter.emit_indent();
        if emitter.config.pretty {
            emitter.emit("// Main execution");
        }
        emitter.emit_block(hir.entry_block)?;
    }

    Ok(JsModule {
        source: emitter.out,
    })
}

// ============================================================================
// Terminator Lowering Methods (Stubs - to be implemented in subtasks)
// ============================================================================

impl<'hir> JsEmitter<'hir> {
    /// Lowers an If terminator to JavaScript if-else statement
    fn lower_if_terminator(
        &mut self,
        condition: &HirExpr,
        then_block: BlockId,
        else_block: Option<BlockId>,
    ) -> Result<(), CompilerError> {
        // Lower the condition expression
        let cond_expr = self.lower_expr(condition)?;

        // Emit any prelude statements from the condition
        self.emit_stmts(&cond_expr.prelude);

        // Emit the if statement
        self.emit_indent();
        if self.config.pretty {
            self.emit(&format!("if ({}) {{", cond_expr.value));
        } else {
            self.emit(&format!("if({}){{", cond_expr.value));
        }

        // Increase indentation for the then block
        self.indent();

        // Emit the then block recursively
        self.emit_block(then_block)?;

        // Decrease indentation
        self.dedent();

        // Handle the else block if present
        if let Some(else_block_id) = else_block {
            self.emit_indent();
            if self.config.pretty {
                self.emit("} else {");
            } else {
                self.emit("}else{");
            }

            // Increase indentation for the else block
            self.indent();

            // Emit the else block recursively
            self.emit_block(else_block_id)?;

            // Decrease indentation
            self.dedent();
        }

        // Close the if statement
        self.emit_indent();
        self.emit("}");

        Ok(())
    }

    /// Lowers a Loop terminator to JavaScript loop
    fn lower_loop_terminator(
        &mut self,
        label: BlockId,
        binding: &Option<(InternedString, DataType)>,
        iterator: &Option<HirExpr>,
        body: BlockId,
        index_binding: &Option<InternedString>,
    ) -> Result<(), CompilerError> {
        // Generate a unique loop label for break/continue targeting
        let loop_label = self.gen_loop_label(label);

        // Store the label in the loop_labels map for break/continue resolution
        self.loop_labels.insert(label, loop_label.clone());

        // Emit the loop based on whether we have an iterator
        if let Some(iter_expr) = iterator {
            // Loop with iterator: for-of loop in JavaScript
            let iter_js = self.lower_expr(iter_expr)?;

            // Emit any prelude statements from the iterator
            self.emit_stmts(&iter_js.prelude);

            self.emit_indent();

            // Check if we have a loop variable binding
            if let Some((var_name, _data_type)) = binding {
                let js_var = self.make_js_ident(*var_name);

                // Check if we also have an index binding
                if let Some(index_name) = index_binding {
                    let js_index = self.make_js_ident(*index_name);

                    // For loops with both value and index, we need to use entries()
                    // JavaScript: for (const [index, value] of array.entries())
                    if self.config.pretty {
                        self.emit(&format!(
                            "{}: for (const [{}, {}] of {}.entries()) {{",
                            loop_label.0, js_index.0, js_var.0, iter_js.value
                        ));
                    } else {
                        self.emit(&format!(
                            "{}:for(const [{},{}] of {}.entries()){{",
                            loop_label.0, js_index.0, js_var.0, iter_js.value
                        ));
                    }
                } else {
                    // For loops with just the value
                    // JavaScript: for (const value of array)
                    if self.config.pretty {
                        self.emit(&format!(
                            "{}: for (const {} of {}) {{",
                            loop_label.0, js_var.0, iter_js.value
                        ));
                    } else {
                        self.emit(&format!(
                            "{}:for(const {} of {}){{",
                            loop_label.0, js_var.0, iter_js.value
                        ));
                    }
                }
            } else {
                // Loop without binding - just iterate for side effects
                // JavaScript: for (const _ of array)
                if self.config.pretty {
                    self.emit(&format!(
                        "{}: for (const _ of {}) {{",
                        loop_label.0, iter_js.value
                    ));
                } else {
                    self.emit(&format!(
                        "{}:for(const _ of {}){{",
                        loop_label.0, iter_js.value
                    ));
                }
            }
        } else {
            // Infinite loop: while(true) in JavaScript
            self.emit_indent();
            if self.config.pretty {
                self.emit(&format!("{}: while (true) {{", loop_label.0));
            } else {
                self.emit(&format!("{}:while(true){{", loop_label.0));
            }
        }

        // Increase indentation for the loop body
        self.indent();

        // Emit the loop body recursively
        self.emit_block(body)?;

        // Decrease indentation
        self.dedent();

        // Close the loop
        self.emit_indent();
        self.emit("}");

        // Remove the label from the map after the loop is complete
        self.loop_labels.remove(&label);

        Ok(())
    }

    /// Lowers a Break terminator to JavaScript break statement
    fn lower_break_terminator(&mut self, target: BlockId) {
        self.emit_indent();

        // Look up the loop label for the target block
        if let Some(label) = self.loop_labels.get(&target) {
            // Break with label to target the correct loop
            if self.config.pretty {
                self.emit(&format!("break {};", label.0));
            } else {
                self.emit(&format!("break {};", label.0));
            }
        } else {
            // If no label found, emit a plain break (should not happen in well-formed HIR)
            // This is a fallback for debugging
            if self.config.pretty {
                self.emit("break; /* WARNING: No label found for target */");
            } else {
                self.emit("break;");
            }
        }
    }

    /// Lowers a Continue terminator to JavaScript continue statement
    fn lower_continue_terminator(&mut self, target: BlockId) {
        self.emit_indent();

        // Look up the loop label for the target block
        if let Some(label) = self.loop_labels.get(&target) {
            // Continue with label to target the correct loop
            if self.config.pretty {
                self.emit(&format!("continue {};", label.0));
            } else {
                self.emit(&format!("continue {};", label.0));
            }
        } else {
            // If no label found, emit a plain continue (should not happen in well-formed HIR)
            // This is a fallback for debugging
            if self.config.pretty {
                self.emit("continue; /* WARNING: No label found for target */");
            } else {
                self.emit("continue;");
            }
        }
    }

    /// Lowers a Return terminator to JavaScript return statement
    fn lower_return_terminator(&mut self, values: &[HirExpr]) -> Result<(), CompilerError> {
        self.emit_indent();

        if values.is_empty() {
            // Return with no values
            self.emit("return;");
        } else if values.len() == 1 {
            // Single return value
            let value_expr = self.lower_expr(&values[0])?;

            // Emit any prelude statements
            self.emit_stmts(&value_expr.prelude);

            // Emit the return statement
            self.emit_indent();
            if self.config.pretty {
                self.emit(&format!("return {};", value_expr.value));
            } else {
                self.emit(&format!("return {};", value_expr.value));
            }
        } else {
            // Multiple return values - return as an array
            let mut all_preludes = Vec::new();
            let mut value_strings = Vec::new();

            for value in values {
                let value_expr = self.lower_expr(value)?;
                all_preludes.extend(value_expr.prelude);
                value_strings.push(value_expr.value);
            }

            // Emit all preludes
            self.emit_stmts(&all_preludes);

            // Emit the return statement with array
            self.emit_indent();
            if self.config.pretty {
                self.emit(&format!("return [{}];", value_strings.join(", ")));
            } else {
                self.emit(&format!("return [{}];", value_strings.join(",")));
            }
        }

        Ok(())
    }

    /// Lowers a Match terminator to JavaScript switch or if-else chain
    fn lower_match_terminator(
        &mut self,
        scrutinee: &HirExpr,
        arms: &[crate::compiler_frontend::hir::nodes::HirMatchArm],
        default_block: Option<BlockId>,
    ) -> Result<(), CompilerError> {
        use crate::compiler_frontend::hir::nodes::HirPattern;

        // Lower the scrutinee expression
        let scrutinee_expr = self.lower_expr(scrutinee)?;

        // Emit any prelude statements
        self.emit_stmts(&scrutinee_expr.prelude);

        // Store the scrutinee in a temporary variable for repeated comparison
        let temp_var = self.gen_temp();
        self.emit_indent();
        if self.config.pretty {
            self.emit(&format!("const {} = {};", temp_var.0, scrutinee_expr.value));
        } else {
            self.emit(&format!("const {}={};", temp_var.0, scrutinee_expr.value));
        }

        // Determine if we can use a switch statement or need if-else chain
        // For now, we'll use if-else chain for simplicity and correctness
        // TODO: Optimize to use switch when all patterns are simple literals

        let mut first_arm = true;

        for arm in arms {
            // Emit if/else if based on whether this is the first arm
            if first_arm {
                self.emit_indent();
                first_arm = false;
            } else {
                if self.config.pretty {
                    self.emit(" else ");
                } else {
                    self.emit("else ");
                }
            }

            // Generate the condition for this arm
            let condition = match &arm.pattern {
                HirPattern::Literal(lit_expr) => {
                    let lit_js = self.lower_expr(lit_expr)?;
                    // Emit any prelude from the literal (shouldn't be any, but just in case)
                    self.emit_stmts(&lit_js.prelude);
                    if self.config.pretty {
                        format!("{} === {}", temp_var.0, lit_js.value)
                    } else {
                        format!("{}==={}", temp_var.0, lit_js.value)
                    }
                }

                HirPattern::Range { start, end } => {
                    let start_js = self.lower_expr(start)?;
                    let end_js = self.lower_expr(end)?;

                    // Emit any preludes
                    self.emit_stmts(&start_js.prelude);
                    self.emit_stmts(&end_js.prelude);

                    if self.config.pretty {
                        format!(
                            "{} >= {} && {} <= {}",
                            temp_var.0, start_js.value, temp_var.0, end_js.value
                        )
                    } else {
                        format!(
                            "{}>={}&& {}<={}",
                            temp_var.0, start_js.value, temp_var.0, end_js.value
                        )
                    }
                }

                HirPattern::Wildcard => {
                    // Wildcard matches everything - this should be the last arm
                    "true".to_string()
                }
            };

            // Add guard condition if present
            let full_condition = if let Some(guard) = &arm.guard {
                let guard_js = self.lower_expr(guard)?;
                self.emit_stmts(&guard_js.prelude);
                if self.config.pretty {
                    format!("({}) && ({})", condition, guard_js.value)
                } else {
                    format!("({})&&({})", condition, guard_js.value)
                }
            } else {
                condition
            };

            // Emit the if statement
            if self.config.pretty {
                self.emit(&format!("if ({}) {{", full_condition));
            } else {
                self.emit(&format!("if({}){{", full_condition));
            }

            // Increase indentation for the arm body
            self.indent();

            // Emit the arm body block
            self.emit_block(arm.body)?;

            // Decrease indentation
            self.dedent();

            self.emit_indent();
            self.emit("}");
        }

        // Handle default block if present and no wildcard was found
        if let Some(default_id) = default_block {
            if self.config.pretty {
                self.emit(" else {");
            } else {
                self.emit("else{");
            }

            // Increase indentation for the default block
            self.indent();

            // Emit the default block
            self.emit_block(default_id)?;

            // Decrease indentation
            self.dedent();

            self.emit_indent();
            self.emit("}");
        }

        Ok(())
    }
}
