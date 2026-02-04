use crate::compiler::codegen::js::JsEmitter;
use crate::compiler::compiler_messages::compiler_errors::CompilerError;
use crate::compiler::hir::nodes::{BlockId, HirStmt};
use crate::compiler::parsers::statements::functions::FunctionSignature;
use crate::compiler::string_interning::InternedString;

/// What kind of callable is currently being lowered
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CallableKind {
    Function,
    Template,
}

/// Context for lowering a single function or template
pub struct CallableContext {
    pub kind: CallableKind,
    pub name: InternedString,
    pub body: BlockId,
}

// ============================================================================
// Function Definition Lowering Methods
// ============================================================================

impl<'hir> JsEmitter<'hir> {
    /// Lowers a regular function definition to JavaScript
    ///
    /// Converts HIR function definitions to JavaScript function declarations.
    /// Handles parameter lists and return value processing.
    ///
    /// **Function Translation**:
    /// - Function name is converted to a safe JavaScript identifier
    /// - Parameters are converted to JavaScript parameter list
    /// - Function body is emitted recursively
    /// - Return values are handled by the Return terminator in the body
    ///
    /// **Example**:
    /// ```beanstalk
    /// add |x Int, y Int| -> Int:
    ///     return x + y
    /// ;
    /// ```
    /// becomes:
    /// ```javascript
    /// function add(x, y) {
    ///     return x + y;
    /// }
    /// ```
    ///
    /// Returns an error if lowering fails.
    pub fn lower_function_def(
        &mut self,
        name: InternedString,
        signature: &FunctionSignature,
        body: BlockId,
    ) -> Result<(), CompilerError> {
        // Get the function name as a safe JavaScript identifier
        let func_name = self.make_js_ident(name);

        // Convert parameters to JavaScript parameter list
        let mut param_names = Vec::new();
        for param in &signature.parameters {
            let param_name = self.make_js_ident(param.id);
            param_names.push(param_name.0.clone());

            // Mark parameter as declared so it doesn't get `let` prefix in the body
            self.declared_vars.insert(param_name.0);
        }

        // Emit the function declaration
        self.emit_indent();
        if self.config.pretty {
            self.emit(&format!(
                "function {}({}) {{",
                func_name.0,
                param_names.join(", ")
            ));
        } else {
            self.emit(&format!(
                "function {}({}){{",
                func_name.0,
                param_names.join(",")
            ));
        }

        // Increase indentation for the function body
        self.indent();

        // Emit the function body recursively
        self.emit_block(body)?;

        // Decrease indentation
        self.dedent();

        // Close the function
        self.emit_indent();
        self.emit("}");

        Ok(())
    }

    /// Lowers a template function definition to JavaScript
    ///
    /// Converts HIR template functions to JavaScript string-building functions.
    /// Template functions return strings and handle template captures.
    ///
    /// **Template Translation**:
    /// - Template function name is converted to a safe JavaScript identifier
    /// - Parameters (captures) are converted to JavaScript parameter list
    /// - Template body is emitted as string concatenation operations
    /// - Function returns the built string
    ///
    /// **Example**:
    /// ```beanstalk
    /// template greeting |name String|:
    ///     [: Hello, [name]! ]
    /// ;
    /// ```
    /// becomes:
    /// ```javascript
    /// function greeting(name) {
    ///     return "Hello, " + name + "!";
    /// }
    /// ```
    ///
    /// Returns an error if lowering fails.
    pub fn lower_template_fn(
        &mut self,
        name: InternedString,
        params: &[(InternedString, crate::compiler::datatypes::DataType)],
        body: BlockId,
    ) -> Result<(), CompilerError> {
        // Get the template function name as a safe JavaScript identifier
        let func_name = self.make_js_ident(name);

        // Convert parameters to JavaScript parameter list
        let mut param_names = Vec::new();
        for (param_name, _param_type) in params {
            let js_param = self.make_js_ident(*param_name);
            param_names.push(js_param.0.clone());

            // Mark the parameter as declared so it doesn't get `let` prefix in the body
            self.declared_vars.insert(js_param.0);
        }

        // Emit the template function declaration
        self.emit_indent();
        if self.config.pretty {
            self.emit(&format!(
                "function {}({}) {{",
                func_name.0,
                param_names.join(", ")
            ));
        } else {
            self.emit(&format!(
                "function {}({}){{",
                func_name.0,
                param_names.join(",")
            ));
        }

        // Increase indentation for the function body
        self.indent();

        // Emit the function body recursively
        // Template functions build strings, so the body should contain
        // string concatenation operations and return a string
        self.emit_block(body)?;

        // Decrease indentation
        self.dedent();

        // Close the function
        self.emit_indent();
        self.emit("}");

        Ok(())
    }

    /// Lowers a HIR statement that is a function or template definition
    ///
    /// This method is called from the main statement lowering logic to handle
    /// function and template definitions that appear as statements in the HIR.
    ///
    /// Returns an error if lowering fails.
    pub fn lower_function_or_template_stmt(&mut self, stmt: &HirStmt) -> Result<(), CompilerError> {
        match stmt {
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

            _ => {
                // Not a function or template definition
                // This should not be called for other statement types
            }
        }

        Ok(())
    }
}
