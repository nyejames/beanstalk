use crate::compiler_frontend::codegen::js::JsEmitter;
use crate::compiler_frontend::codegen::js::js_host_functions::{
    HostFunctionId, get_host_function_str,
};
use crate::compiler_frontend::codegen::js::js_statement::JsStmt;
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::nodes::{BinOp, HirExpr, HirExprKind, UnaryOp};
use crate::compiler_frontend::host_functions::registry::CallTarget;
use crate::compiler_frontend::string_interning::InternedString;

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

impl JsExpr {
    /// Creates a simple expression with no prelude statements
    ///
    /// Use this for pure expressions like literals, variable references,
    /// or simple operations that don't require temporary variables.
    ///
    /// # Example
    /// ```ignore
    /// let expr = JsExpr::simple("42".to_string());
    /// assert!(expr.prelude.is_empty());
    /// assert_eq!(expr.value, "42");
    /// ```
    pub fn simple(value: String) -> Self {
        JsExpr {
            prelude: Vec::new(),
            value,
        }
    }

    /// Creates an expression with prelude statements
    ///
    /// Use this when the expression requires setup code to run first,
    /// such as storing intermediate results in temporary variables.
    ///
    /// # Example
    /// ```ignore
    /// let prelude = vec![JsStmt::Let {
    ///     name: JsIdent("_t0".to_string()),
    ///     value: "compute()".to_string(),
    /// }];
    /// let expr = JsExpr::with_prelude(prelude, "_t0".to_string());
    /// ```
    pub fn with_prelude(prelude: Vec<JsStmt>, value: String) -> Self {
        JsExpr { prelude, value }
    }

    /// Combines two expressions, merging their preludes
    ///
    /// The preludes are concatenated in order: first `self.prelude`,
    /// then `other.prelude`. This ensures statements execute in the
    /// correct sequence.
    ///
    /// The resulting value is constructed by applying the provided
    /// combiner function to both expression values.
    ///
    /// # Example
    /// ```ignore
    /// let left = JsExpr::simple("a".to_string());
    /// let right = JsExpr::simple("b".to_string());
    /// let combined = left.combine(right, |l, r| format!("{} + {}", l, r));
    /// assert_eq!(combined.value, "a + b");
    /// ```
    pub fn combine<F>(mut self, other: JsExpr, combiner: F) -> Self
    where
        F: FnOnce(&str, &str) -> String,
    {
        // Merge preludes: self's statements first, then other's
        self.prelude.extend(other.prelude);

        // Combine the values using the provided function
        let combined_value = combiner(&self.value, &other.value);

        JsExpr {
            prelude: self.prelude,
            value: combined_value,
        }
    }

    /// Adds additional prelude statements to this expression
    ///
    /// The new statements are prepended to the existing prelude,
    /// ensuring they execute before any existing setup code.
    ///
    /// # Example
    /// ```ignore
    /// let mut expr = JsExpr::simple("x".to_string());
    /// expr.prepend_prelude(vec![JsStmt::Expr("setup()".to_string())]);
    /// ```
    pub fn prepend_prelude(&mut self, mut statements: Vec<JsStmt>) {
        statements.append(&mut self.prelude);
        self.prelude = statements;
    }

    /// Adds additional prelude statements after existing ones
    ///
    /// The new statements are appended to the existing prelude,
    /// ensuring they execute after any existing setup code.
    ///
    /// # Example
    /// ```ignore
    /// let mut expr = JsExpr::simple("x".to_string());
    /// expr.append_prelude(vec![JsStmt::Expr("validate()".to_string())]);
    /// ```
    pub fn append_prelude(&mut self, statements: Vec<JsStmt>) {
        self.prelude.extend(statements);
    }

    /// Transforms the value of this expression while preserving the prelude
    ///
    /// Use this to wrap or modify the expression value without affecting
    /// the setup statements.
    ///
    /// # Example
    /// ```ignore
    /// let expr = JsExpr::simple("x".to_string());
    /// let wrapped = expr.map_value(|v| format!("({})", v));
    /// assert_eq!(wrapped.value, "(x)");
    /// ```
    pub fn map_value<F>(mut self, mapper: F) -> Self
    where
        F: FnOnce(String) -> String,
    {
        self.value = mapper(self.value);
        self
    }

    /// Checks if this expression has any prelude statements
    ///
    /// Returns `true` if the expression is pure (no setup required),
    /// `false` if it requires prelude statements to execute first.
    pub fn is_pure(&self) -> bool {
        self.prelude.is_empty()
    }

    /// Consumes this expression and returns its components
    ///
    /// Useful when you need to process the prelude and value separately.
    pub fn into_parts(self) -> (Vec<JsStmt>, String) {
        (self.prelude, self.value)
    }
}

// ============================================================================
// Expression Lowering Methods
// ============================================================================

impl<'hir> JsEmitter<'hir> {
    /// Lowers a HIR expression to JavaScript
    ///
    /// Returns a JsExpr containing any necessary prelude statements
    /// and the final expression value, or an error if lowering fails.
    pub fn lower_expr(&mut self, expr: &HirExpr) -> Result<JsExpr, CompilerError> {
        match &expr.kind {
            // === Literals ===
            HirExprKind::Int(value) => Ok(JsExpr::simple(value.to_string())),

            HirExprKind::Float(value) => {
                // Handle special float values
                if value.is_nan() {
                    Ok(JsExpr::simple("NaN".to_string()))
                } else if value.is_infinite() {
                    if value.is_sign_positive() {
                        Ok(JsExpr::simple("Infinity".to_string()))
                    } else {
                        Ok(JsExpr::simple("-Infinity".to_string()))
                    }
                } else {
                    Ok(JsExpr::simple(value.to_string()))
                }
            }

            HirExprKind::Bool(value) => Ok(JsExpr::simple(value.to_string())),

            HirExprKind::StringLiteral(s) | HirExprKind::HeapString(s) => {
                // Both string types become JavaScript strings in GC semantics
                let string_value = self.string_table.resolve(*s);
                Ok(JsExpr::simple(escape_js_string(string_value)))
            }

            HirExprKind::Char(c) => {
                // Characters become single-character JavaScript strings
                Ok(JsExpr::simple(escape_js_char(*c)))
            }

            // === Binary Operations ===
            HirExprKind::BinOp { left, op, right } => self.lower_binop(left, *op, right),

            // === Unary Operations ===
            HirExprKind::UnaryOp { op, operand } => self.lower_unary_op(*op, operand),

            // === Variable Access ===
            // In GC semantics, Load and Move are identical - both create references
            // to GC-managed data. The distinction is purely for optimization in
            // ownership-aware backends.
            HirExprKind::Load(place) | HirExprKind::Move(place) => {
                let js_ref = self.lower_place(place)?;
                Ok(JsExpr::simple(js_ref))
            }

            // === Function Calls ===
            HirExprKind::Call { target, args } => self.lower_call(target, args),

            HirExprKind::MethodCall {
                receiver,
                method,
                args,
            } => self.lower_method_call(receiver, *method, args),

            // === Constructors ===
            HirExprKind::StructConstruct {
                type_name: _,
                fields,
            } => self.lower_struct_construct(fields),

            HirExprKind::Collection(elements) => self.lower_collection(elements),

            // === Field Access ===
            HirExprKind::Field { base, field } => {
                let base_name = self.string_table.resolve(*base);
                let field_name = self.string_table.resolve(*field);
                Ok(JsExpr::simple(format!("{}.{}", base_name, field_name)))
            }

            // Unsupported expression types - return descriptive errors
            unsupported => Err(CompilerError::compiler_error(format!(
                "JavaScript backend: Unsupported HIR expression type: {:?}. This indicates missing implementation in the JavaScript backend.",
                unsupported
            ))),
        }
    }

    /// Lowers a HIR place (variable, field access, or index) to JavaScript
    ///
    /// Places represent memory locations that can be read from or written to.
    /// In GC semantics, all places are references to GC-managed data.
    ///
    /// Returns an error if the place contains unsupported constructs.
    pub(crate) fn lower_place(
        &mut self,
        place: &crate::compiler_frontend::hir::nodes::HirPlace,
    ) -> Result<String, CompilerError> {
        use crate::compiler_frontend::hir::nodes::HirPlace;

        match place {
            // Simple variable reference
            HirPlace::Var(name) => {
                let js_ident = self.make_js_ident(*name);
                Ok(js_ident.0)
            }

            // Field access: base.field
            HirPlace::Field { base, field } => {
                let base_js = self.lower_place(base)?;
                let field_name = self.string_table.resolve(*field);
                Ok(format!("{}.{}", base_js, field_name))
            }

            // Index access: base[index]
            HirPlace::Index { base, index } => {
                let base_js = self.lower_place(base)?;
                let index_expr = self.lower_expr(index)?;

                // If the index expression has a prelude, we need to handle it
                // For now, we'll just use the value directly
                // TODO: Handle preludes properly when we implement statement emission
                Ok(format!("{}[{}]", base_js, index_expr.value))
            }
        }
    }

    /// Lowers a binary operation to JavaScript
    ///
    /// Handles operator precedence by wrapping operands in parentheses when necessary.
    /// Combines preludes from both operands to ensure proper evaluation order.
    fn lower_binop(
        &mut self,
        left: &HirExpr,
        op: BinOp,
        right: &HirExpr,
    ) -> Result<JsExpr, CompilerError> {
        // Lower both operands
        let left_expr = self.lower_expr(left)?;
        let right_expr = self.lower_expr(right)?;

        // Map Beanstalk operators to JavaScript operators
        let js_op = match op {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Mod => "%",
            BinOp::Eq => "===",
            BinOp::Ne => "!==",
            BinOp::Lt => "<",
            BinOp::Le => "<=",
            BinOp::Gt => ">",
            BinOp::Ge => ">=",
            BinOp::And => "&&",
            BinOp::Or => "||",
            BinOp::Exponent => "**",
            BinOp::Root => {
                // Root operation: a root b = b^(1/a)
                // In JavaScript: Math.pow(b, 1/a)
                return Ok(
                    left_expr.combine(right_expr, |l, r| format!("Math.pow({}, 1 / {})", r, l))
                );
            }
        };

        // Combine the expressions with proper operator precedence
        // Wrap operands in parentheses to ensure correct evaluation order
        Ok(left_expr.combine(right_expr, |l, r| format!("({} {} {})", l, js_op, r)))
    }

    /// Lowers a unary operation to JavaScript
    ///
    /// Handles negation and logical not operations.
    fn lower_unary_op(&mut self, op: UnaryOp, operand: &HirExpr) -> Result<JsExpr, CompilerError> {
        // Lower the operand
        let operand_expr = self.lower_expr(operand)?;

        // Map Beanstalk unary operators to JavaScript operators
        let js_op = match op {
            UnaryOp::Neg => "-",
            UnaryOp::Not => "!",
        };

        // Apply the operator with parentheses for clarity
        Ok(operand_expr.map_value(|v| format!("({}{})", js_op, v)))
    }

    /// Lowers a function call to JavaScript
    ///
    /// Handles regular function calls with proper argument handling.
    /// All arguments are evaluated in order, with their preludes combined.
    pub(crate) fn lower_call(
        &mut self,
        target: &CallTarget,
        args: &[HirExpr],
    ) -> Result<JsExpr, CompilerError> {
        match target {
            CallTarget::UserFunction(name) => self.lower_function_call(*name, args),

            CallTarget::HostFunction(host_id) => self.lower_host_fn(*host_id, args),
        }
    }

    pub(crate) fn lower_function_call(
        &mut self,
        target: InternedString,
        args: &[HirExpr],
    ) -> Result<JsExpr, CompilerError> {
        let func_name = self.make_js_ident(target);

        // Lower all arguments
        let mut all_preludes = Vec::new();
        let mut arg_values = Vec::new();

        for arg in args {
            let arg_expr = self.lower_expr(arg)?;

            // Collect preludes from all arguments
            all_preludes.extend(arg_expr.prelude);

            // Collect the argument value
            arg_values.push(arg_expr.value);
        }

        // Build the function call expression
        let call_expr = if self.config.pretty {
            format!("{}({})", func_name.0, arg_values.join(", "))
        } else {
            format!("{}({})", func_name.0, arg_values.join(","))
        };

        Ok(JsExpr::with_prelude(all_preludes, call_expr))
    }

    /// Lowers a method call to JavaScript
    ///
    /// Handles method calls with proper receiver and argument handling.
    /// The receiver is evaluated first, followed by all arguments in order.
    fn lower_method_call(
        &mut self,
        receiver: &HirExpr,
        method: InternedString,
        args: &[HirExpr],
    ) -> Result<JsExpr, CompilerError> {
        // Lower the receiver
        let receiver_expr = self.lower_expr(receiver)?;

        // Get the method name
        let method_name = self.string_table.resolve(method);

        // Lower all arguments
        let mut all_preludes = receiver_expr.prelude;
        let mut arg_values = Vec::new();

        for arg in args {
            let arg_expr = self.lower_expr(arg)?;

            // Collect preludes from all arguments
            all_preludes.extend(arg_expr.prelude);

            // Collect the argument value
            arg_values.push(arg_expr.value);
        }

        // Build the method call expression
        let call_expr = if self.config.pretty {
            format!(
                "{}.{}({})",
                receiver_expr.value,
                method_name,
                arg_values.join(", ")
            )
        } else {
            format!(
                "{}.{}({})",
                receiver_expr.value,
                method_name,
                arg_values.join(",")
            )
        };

        Ok(JsExpr::with_prelude(all_preludes, call_expr))
    }

    fn lower_host_fn(
        &mut self,
        host_fn: HostFunctionId,
        args: &[HirExpr],
    ) -> Result<JsExpr, CompilerError> {
        // Lower all arguments first (same semantics as normal calls)
        let mut all_preludes = Vec::new();
        let mut arg_values = Vec::new();

        for arg in args {
            let arg_expr = self.lower_expr(arg)?;
            all_preludes.extend(arg_expr.prelude);
            arg_values.push(arg_expr.value);
        }

        let js_target = get_host_function_str(host_fn);

        let call_expr = if self.config.pretty {
            format!("{}({})", js_target, arg_values.join(", "))
        } else {
            format!("{}({})", js_target, arg_values.join(","))
        };

        Ok(JsExpr::with_prelude(all_preludes, call_expr))
    }

    /// Lowers a struct construction to a JavaScript object literal
    ///
    /// Struct construction in Beanstalk becomes a JavaScript object literal.
    /// All fields are evaluated in order, with their preludes combined.
    ///
    /// Example:
    /// ```beanstalk
    /// Person(name = "Alice", age = 30)
    /// ```
    /// becomes:
    /// ```javascript
    /// { name: "Alice", age: 30 }
    /// ```
    fn lower_struct_construct(
        &mut self,
        fields: &[(InternedString, HirExpr)],
    ) -> Result<JsExpr, CompilerError> {
        // Lower all field values
        let mut all_preludes = Vec::new();
        let mut field_pairs = Vec::new();

        for (field_name, field_value) in fields {
            let field_expr = self.lower_expr(field_value)?;

            // Collect preludes from all field values
            all_preludes.extend(field_expr.prelude);

            // Get the field name as a string
            let name = self.string_table.resolve(*field_name);

            // Build the field pair: name: value
            if self.config.pretty {
                field_pairs.push(format!("{}: {}", name, field_expr.value));
            } else {
                field_pairs.push(format!("{}:{}", name, field_expr.value));
            }
        }

        // Build the object literal
        let object_literal = if field_pairs.is_empty() {
            "{}".to_string()
        } else if self.config.pretty {
            format!("{{ {} }}", field_pairs.join(", "))
        } else {
            format!("{{{}}}", field_pairs.join(","))
        };

        Ok(JsExpr::with_prelude(all_preludes, object_literal))
    }

    /// Lowers a collection construction to a JavaScript array literal
    ///
    /// Collection construction in Beanstalk becomes a JavaScript array literal.
    /// All elements are evaluated in order, with their preludes combined.
    ///
    /// Example:
    /// ```beanstalk
    /// {1, 2, 3}
    /// ```
    /// becomes:
    /// ```javascript
    /// [1, 2, 3]
    /// ```
    fn lower_collection(&mut self, elements: &[HirExpr]) -> Result<JsExpr, CompilerError> {
        // Lower all elements
        let mut all_preludes = Vec::new();
        let mut element_values = Vec::new();

        for element in elements {
            let element_expr = self.lower_expr(element)?;

            // Collect preludes from all elements
            all_preludes.extend(element_expr.prelude);

            // Collect the element value
            element_values.push(element_expr.value);
        }

        // Build the array literal
        let array_literal = if element_values.is_empty() {
            "[]".to_string()
        } else if self.config.pretty {
            format!("[{}]", element_values.join(", "))
        } else {
            format!("[{}]", element_values.join(","))
        };

        Ok(JsExpr::with_prelude(all_preludes, array_literal))
    }
}

// ============================================================================
// String Escaping Utilities
// ============================================================================

/// Escapes a string for use as a JavaScript string literal
///
/// Handles:
/// - Backslash escaping
/// - Quote escaping
/// - Newlines, tabs, and other control characters
/// - Unicode characters
fn escape_js_string(s: &str) -> String {
    let mut result = String::from("\"");

    for ch in s.chars() {
        match ch {
            '\\' => result.push_str("\\\\"),
            '"' => result.push_str("\\\""),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            '\0' => result.push_str("\\0"),
            // Control characters (0x00-0x1F, excluding those handled above)
            c if c.is_control() && c != '\n' && c != '\r' && c != '\t' && c != '\0' => {
                result.push_str(&format!("\\x{:02x}", c as u32));
            }
            // Regular characters
            c => result.push(c),
        }
    }

    result.push('"');
    result
}

/// Escapes a character for use as a JavaScript string literal
///
/// Characters in Beanstalk become single-character JavaScript strings.
fn escape_js_char(c: char) -> String {
    let mut result = String::from("\"");

    match c {
        '\\' => result.push_str("\\\\"),
        '"' => result.push_str("\\\""),
        '\n' => result.push_str("\\n"),
        '\r' => result.push_str("\\r"),
        '\t' => result.push_str("\\t"),
        '\0' => result.push_str("\\0"),
        // Control characters
        ch if ch.is_control() && ch != '\n' && ch != '\r' && ch != '\t' && ch != '\0' => {
            result.push_str(&format!("\\x{:02x}", ch as u32));
        }
        // Regular characters
        ch => result.push(ch),
    }

    result.push('"');
    result
}
