use crate::backends::js::{JsEmitter, JsIdent};
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_nodes::{HirPlace, HirStmt};

/// Internal representation of simple JS statements
#[derive(Debug)]
pub enum JsStmt {
    /// `let x = expr;`
    Let { name: JsIdent, value: String },

    /// `x = expr;`
    Assign { name: JsIdent, value: String },

    /// Standalone expression statement
    Expr(String),
}

// ============================================================================
// Statement Lowering Methods
// ============================================================================

impl<'hir> JsEmitter<'hir> {
    /// Emits a statement to the output buffer with proper indentation
    ///
    /// This method handles all statement types and ensures proper formatting:
    /// - Let declarations: `let x = value;`
    /// - Assignments: `x = value;`
    /// - Expression statements: `expr;`
    ///
    /// Statements are emitted with proper indentation based on the current
    /// indentation depth, and a semicolon is added at the end.
    pub fn emit_stmt(&mut self, stmt: &JsStmt) {
        self.emit_indent();

        match stmt {
            JsStmt::Let { name, value } => {
                if self.config.pretty {
                    self.emit(&format!("let {} = {};", name.0, value));
                } else {
                    self.emit(&format!("let {}={};", name.0, value));
                }
            }

            JsStmt::Assign { name, value } => {
                if self.config.pretty {
                    self.emit(&format!("{} = {};", name.0, value));
                } else {
                    self.emit(&format!("{}={};", name.0, value));
                }
            }

            JsStmt::Expr(expr) => {
                self.emit(&format!("{};", expr));
            }
        }
    }

    /// Emits multiple statements in sequence
    ///
    /// This is a convenience method for emitting a list of statements,
    /// such as the prelude from a JsExpr.
    pub fn emit_stmts(&mut self, stmts: &[JsStmt]) {
        for stmt in stmts {
            self.emit_stmt(stmt);
        }
    }

    /// Lowers a HIR statement to JavaScript
    ///
    /// This method handles all HIR statement types and converts them to
    /// JavaScript statements. It manages variable declaration tracking to
    /// distinguish between `let` declarations and reassignments.
    ///
    /// **Assignment Handling**:
    /// - First assignment to a variable: `let x = value;`
    /// - Subsequent assignments: `x = value;`
    /// - The `is_mutable` flag is ignored in GC semantics (JavaScript handles mutability)
    ///
    /// **Memory Management**:
    /// - `PossibleDrop` statements are no-ops in GC semantics
    /// - All memory management is delegated to JavaScript's garbage collector
    ///
    /// Returns an error if an unsupported statement is encountered or if lowering fails.
    pub fn lower_stmt(&mut self, stmt: &HirStmt) -> Result<(), CompilerError> {
        match stmt {
            // === Variable Assignment ===
            HirStmt::Assign {
                target,
                value,
                is_mutable: _,
            } => {
                // Lower the value expression
                let value_expr = self.lower_expr(value)?;

                // Emit any prelude statements from the value expression
                self.emit_stmts(&value_expr.prelude);

                // Get the target variable name
                // For now, we only support simple variable assignments (not field or index assignments)
                let target_name = match target {
                    HirPlace::Var(name) => {
                        let js_ident = self.make_js_ident(*name);
                        js_ident.0
                    }
                    HirPlace::Field { base, field } => {
                        // Field assignment: base.field = value
                        let base_js = self.lower_place(base)?;
                        let field_name = self.string_table.resolve(*field);
                        format!("{}.{}", base_js, field_name)
                    }
                    HirPlace::Index { base, index } => {
                        // Index assignment: base[index] = value
                        let base_js = self.lower_place(base)?;
                        let index_expr = self.lower_expr(index)?;

                        // Emit index expression prelude
                        self.emit_stmts(&index_expr.prelude);

                        format!("{}[{}]", base_js, index_expr.value)
                    }
                };

                // Determine if this is a declaration or reassignment
                // Only simple variables can be declared with `let`
                // Field and index assignments are always reassignments
                let is_declaration = matches!(target, HirPlace::Var(_))
                    && !self.declared_vars.contains(&target_name);

                if is_declaration {
                    // First assignment - emit `let` declaration
                    self.declared_vars.insert(target_name.clone());
                    let stmt = JsStmt::Let {
                        name: JsIdent(target_name),
                        value: value_expr.value,
                    };
                    self.emit_stmt(&stmt);
                } else {
                    // Reassignment - emit plain assignment
                    let stmt = JsStmt::Assign {
                        name: JsIdent(target_name),
                        value: value_expr.value,
                    };
                    self.emit_stmt(&stmt);
                }
            }

            // === Function Calls ===
            HirStmt::Call { target, args } => {
                let call_expr = self.lower_call(target, args)?;

                // Emit prelude statements
                self.emit_stmts(&call_expr.prelude);

                // Emit the call as a statement
                let stmt = JsStmt::Expr(call_expr.value);
                self.emit_stmt(&stmt);
            }

            // === Memory Management (No-op in GC semantics) ===
            HirStmt::PossibleDrop(_) => {
                // No-op: GC handles all memory management
                // This statement is ignored in JavaScript backend
            }

            // === Expression Statement ===
            HirStmt::ExprStmt(expr) => {
                let js_expr = self.lower_expr(expr)?;

                // Emit prelude statements
                self.emit_stmts(&js_expr.prelude);

                // Emit the expression as a statement
                let stmt = JsStmt::Expr(js_expr.value);
                self.emit_stmt(&stmt);
            }

            // === Function and Template Definitions ===
            HirStmt::FunctionDef {
                name,
                signature,
                body,
            } => {
                self.lower_function_def(*name, signature, *body)?;
            }

            HirStmt::TemplateFn { name, params, body } => {
                self.lower_template_fn(*name, params, *body)?;
            }

            // === Runtime Template Calls ===
            HirStmt::RuntimeTemplateCall {
                template_fn,
                captures,
                id: _,
            } => {
                // Runtime template calls are function calls that return strings
                let call_expr = self.lower_function_call(*template_fn, captures)?;

                // Emit prelude statements
                self.emit_stmts(&call_expr.prelude);

                // Emit the call as a statement
                let stmt = JsStmt::Expr(call_expr.value);
                self.emit_stmt(&stmt);
            }

            // === Struct Definitions ===
            HirStmt::StructDef { name: _, fields: _ } => {
                // Struct definitions don't need to emit any JavaScript code
                // In JavaScript, structs are just object literals created at construction time
                // No class or prototype definition is needed
            }
        }

        Ok(())
    }
}
