//! Function Transformer Component
//!
//! This module implements the FunctionTransformer component for the HIR builder.
//! It handles the transformation of AST function definitions and calls into HIR representation.
//!
//! ## Responsibilities
//! - Transform function definitions into HIR function blocks
//! - Handle function parameters and return value management
//! - Convert function calls to HIR call instructions
//! - Transform host function calls with proper import information
//! - Prepare arguments for Beanstalk's unified ABI

use crate::backends::function_registry::{CallTarget, HostFunctionId};
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind, Var};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::hir::build_hir::HirBuilderContext;
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, HirExpr, HirExprKind, HirKind, HirNode, HirStmt, HirTerminator,
};
use crate::compiler_frontend::string_interning::InternedString;
use crate::compiler_frontend::tokenizer::tokens::TextLocation;
use crate::return_hir_transformation_error;
use saying::say;

/// The FunctionTransformer component handles transformation of functions from AST to HIR.
///
/// This component operates on borrowed HirBuilderContext and does not maintain
///  an independent state. All transformations are coordinated through the context.
pub struct FunctionTransformer;

impl FunctionTransformer {
    /// Creates a new FunctionTransformer
    pub fn new() -> Self {
        FunctionTransformer
    }

    /// Transforms an AST function definition into HIR representation.
    ///
    /// This creates a HIR function block with proper parameter handling and return management.
    /// The function body is transformed into a separate block that the function references.
    ///
    /// # Arguments
    /// * `name` - The function name
    /// * `signature` - The function signature with parameters and return types
    /// * `body` - The AST nodes that make up the function body
    /// * `ctx` - The HIR builder context
    /// * `location` - Source location for error reporting
    ///
    /// # Returns
    /// A HIR node representing the function definition
    pub fn transform_function_definition(
        &mut self,
        name: InternedString,
        signature: FunctionSignature,
        body: &[AstNode],
        ctx: &mut HirBuilderContext,
        location: TextLocation,
    ) -> Result<HirNode, CompilerError> {
        // Register the function signature in the context
        ctx.register_function(name, signature.clone());

        // Create a new block for the function body
        let body_block_id = ctx.create_block();

        // Enter function scope
        ctx.enter_scope_with_block(
            crate::compiler_frontend::hir::build_hir::ScopeType::Function,
            body_block_id,
        );

        // Set the current function
        let previous_function = ctx.current_function;
        ctx.current_function = Some(name);

        // Handle function parameters - they become local declarations in the function body
        self.handle_function_parameters(&signature.parameters, ctx, body_block_id)?;

        // Transform the function body
        for node in body {
            let hir_nodes = self.transform_function_body_node(node, ctx)?;
            for hir_node in hir_nodes {
                ctx.add_node_to_block(body_block_id, hir_node);
            }
        }

        // Ensure the function body block has a terminator
        // If the last node isn't a return, add an implicit return
        if let Some(block) = ctx.get_block(body_block_id) {
            let has_terminator = block
                .nodes
                .last()
                .map(|n| {
                    matches!(
                        n.kind,
                        HirKind::Terminator(HirTerminator::Return(_))
                            | HirKind::Terminator(HirTerminator::ReturnError(_))
                    )
                })
                .unwrap_or(false);

            if !has_terminator {
                // Add implicit return based on function signature
                let return_values = if signature.returns.is_empty() {
                    vec![]
                } else {
                    // For functions with return values, we need explicit returns
                    // This is an error case that should be caught earlier, but we handle it gracefully
                    vec![]
                };

                let return_node = HirNode {
                    kind: HirKind::Terminator(HirTerminator::Return(return_values)),
                    location: location.clone(),
                    id: ctx.allocate_node_id(),
                };
                ctx.add_node_to_block(body_block_id, return_node);
            }
        }

        // Exit function scope
        let _dropped_vars = ctx.exit_scope();

        // Restore previous function
        ctx.current_function = previous_function;

        // Create the function definition HIR node
        let node_id = ctx.allocate_node_id();
        let func_node = HirNode {
            kind: HirKind::Stmt(HirStmt::FunctionDef {
                name,
                signature,
                body: body_block_id,
            }),
            location,
            id: node_id,
        };

        Ok(func_node)
    }

    /// Handles function parameters by creating local declarations in the function body.
    ///
    /// Parameters are treated as local variables that are initialized with the argument values.
    fn handle_function_parameters(
        &mut self,
        params: &[Var],
        ctx: &mut HirBuilderContext,
        body_block_id: BlockId,
    ) -> Result<(), CompilerError> {
        for param in params {
            // Parameters are implicitly declared at the start of the function
            // We mark them as potentially owned since they come from the caller
            ctx.mark_potentially_owned(param.id);

            // Add parameter to the block's parameter list
            if let Some(block) = ctx.get_block_mut(body_block_id) {
                block.params.push(param.id);
            }
        }
        Ok(())
    }

    /// Transforms a single AST node from a function body into HIR nodes.
    ///
    /// This delegates to the appropriate transformation method based on the node type.
    fn transform_function_body_node(
        &mut self,
        node: &AstNode,
        ctx: &mut HirBuilderContext,
    ) -> Result<Vec<HirNode>, CompilerError> {
        match &node.kind {
            NodeKind::Return(exprs) => self.transform_return(exprs, ctx, &node.location),
            NodeKind::FunctionCall {
                name,
                args,
                returns,
                location,
            } => self.transform_function_call_as_stmt(*name, args, ctx, location),
            NodeKind::HostFunctionCall {
                host_function_id,
                args,
                returns,
                location,
            } => self.transform_host_function_call_as_stmt(*host_function_id, args, ctx, location),
            NodeKind::VariableDeclaration(arg) => {
                // Use the context's helper method to process variable declarations
                ctx.process_variable_declaration(arg, &node.location)
            }

            // Used by the host environment / build system for special purposes,
            // For example, in the HTML build system
            // these are separated from the rest of the JS/Wasm codegen to become the actual HTML page content.
            // But this is up to the build system to decide how these are used and lowered,
            // so it can be separated from the rest of the HIR nodes for the build system's convenience
            NodeKind::TopLevelTemplate(expr) => {
                if let ExpressionKind::Template(ref template) = expr.kind {
                    // TODO:
                    /*

                       ## Templates are just expressions with capture
                       * every file has an implicit `start() -> String`
                       * all top-level templates contribute to that return value
                       * templates can capture variables declared earlier in `start`

                       > **Lower top-level templates exactly like any other runtime template expression, except their results are accumulated into the implicit return of `start`.**

                       The build system can later decide:
                       * whether the returned string is HTML
                       * whether it’s written to disk
                       * whether it’s concatenated with something else

                       So HIR’s job is just: *produce a string value*.

                       ## Concrete lowering strategy (recommended)

                       ### Step 1: Make `start` return a string always
                       If there are no templates, return `""`.

                       ### Step 2: Introduce a hidden accumulator local

                       Inside `start`, introduce a compiler_frontend-generated local, conceptually:

                       ```text
                       __start_result : String
                       ```

                       Initialization:

                       * Empty string literal (or whatever your canonical empty template lowers to)

                       This happens **before** lowering any body nodes.

                       This fits well with your current HIR philosophy:

                       * “No temporaries at the language level”
                       * Compiler-introduced locals are normal locals

                       ---

                       ### Step 3: Lower each `NodeKind::TopLevelTemplate` as:

                       1. Lower the template expression *as an expression*, not as a side-channel
                       2. Append its result into `__start_result`

                       In pseudocode terms:

                       ```beanstalk
                       __start_result = __start_result + (lowered template expression)
                       ```

                       In HIR terms, this likely becomes:
                       * Call to a string concat intrinsic or runtime helper
                       * Assignment back into the accumulator

                       ---

                       ### Step 4: Emit a final return

                       At the end of `start`:

                       ```text
                       return __start_result
                       ```

                       What you want here is something like:

                       * Lower `expr` → `(Vec<HirNode>, HirExpr)`
                       * Emit nodes to evaluate the template
                       * Emit a call / assignment that mutates `__start_result`

                       That suggests two small architectural nudges:

                       ### A. `FunctionTransformer` needs awareness of “current start accumulator”

                       Not as a special case, but as *context*.

                       For example, in `HirBuilderContext`:

                       * Optional `current_start_accumulator: Option<VarId>`

                       * `ExpressionKind::Template` already lowers to either:

                         * `HirExprKind::StringLiteral` (folded case)
                         * `HirExprKind::Call { target: runtime_template_fn }`

                       * `start` is a normal function scope
                       * Template expressions reference variables by `InternedString`
                       * HIR loads variables via `HirPlace::Var(name)`
                       * Any runtime template function call receives captured values as arguments (explicitly or implicitly, depending on your template model)

                       No closure machinery is required *at HIR level* unless templates outlive the function (they don’t here).
                    */
                } else {
                    // This should never happen, but show a warning if it does
                    say!(Red
                        "Compiler Bug: Top level template has the wrong type expression inside it for some reason: ",
                        #expr.kind
                    );
                }

                // Don't add anything to the actual codegen
                Ok(Vec::new())
            }

            _ => {
                // For other node types, delegate to the main context processing
                // This will be handled by expression linearizer, control flow linearizer, etc.
                return_hir_transformation_error!(
                    format!(
                        "Function body node type not yet implemented in FunctionTransformer: {:?}",
                        node.kind
                    ),
                    node.location.to_error_location_without_table(),
                    {
                        CompilationStage => "HIR Generation - Function Transformation",
                        PrimarySuggestion => "This node type should be handled by other components"
                    }
                )
            }
        }
    }

    /// Transforms a return statement into HIR.
    ///
    /// Return statements become HIR Return terminators with the return values.
    pub fn transform_return(
        &mut self,
        exprs: &[Expression],
        ctx: &mut HirBuilderContext,
        location: &TextLocation,
    ) -> Result<Vec<HirNode>, CompilerError> {
        // Transform each return expression
        let mut hir_exprs = Vec::new();
        for expr in exprs {
            let hir_expr = self.transform_expression_for_return(expr, ctx)?;
            hir_exprs.push(hir_expr);
        }

        // Create the return terminator
        let node_id = ctx.allocate_node_id();
        let return_node = HirNode {
            kind: HirKind::Terminator(HirTerminator::Return(hir_exprs)),
            location: location.clone(),
            id: node_id,
        };

        Ok(vec![return_node])
    }

    /// Transforms an expression for use in a return statement.
    ///
    /// This is a simplified transformation that handles basic expression types.
    /// Complex expressions should be linearized by the expression linearizer first.
    fn transform_expression_for_return(
        &mut self,
        expr: &Expression,
        _ctx: &mut HirBuilderContext,
    ) -> Result<HirExpr, CompilerError> {
        let hir_expr_kind = match &expr.kind {
            ExpressionKind::Int(val) => HirExprKind::Int(*val),
            ExpressionKind::Float(val) => HirExprKind::Float(*val),
            ExpressionKind::Bool(val) => HirExprKind::Bool(*val),
            ExpressionKind::StringSlice(s) => HirExprKind::StringLiteral(*s),
            ExpressionKind::Char(c) => HirExprKind::Char(*c),
            ExpressionKind::Reference(name) => HirExprKind::Load(
                crate::compiler_frontend::hir::hir_nodes::HirPlace::Var(*name),
            ),
            _ => {
                return_hir_transformation_error!(
                    format!(
                        "Complex expression in return not yet supported: {:?}",
                        expr.kind
                    ),
                    expr.location.to_error_location_without_table(),
                    {
                        CompilationStage => "HIR Generation - Function Transformation",
                        PrimarySuggestion => "Simplify the return expression or use expression linearizer"
                    }
                )
            }
        };

        Ok(HirExpr {
            kind: hir_expr_kind,
            location: expr.location.clone(),
        })
    }

    /// Transforms a function call into HIR as a statement.
    ///
    /// This creates a HIR Call statement. If the function returns values,
    /// they can be assigned via separate Assign nodes.
    pub fn transform_function_call_as_stmt(
        &mut self,
        name: InternedString,
        args: &[Expression],
        ctx: &mut HirBuilderContext,
        location: &TextLocation,
    ) -> Result<Vec<HirNode>, CompilerError> {
        // Transform arguments
        let hir_args = self.transform_arguments(args, ctx)?;

        // Create the call statement
        let node_id = ctx.allocate_node_id();
        let call_node = HirNode {
            kind: HirKind::Stmt(HirStmt::Call {
                target: CallTarget::UserFunction(name),
                args: hir_args,
            }),
            location: location.clone(),
            id: node_id,
        };

        Ok(vec![call_node])
    }

    /// Transforms a function call into HIR as an expression.
    ///
    /// This creates a HIR Call expression that can be used in assignments or other expressions.
    pub fn transform_function_call(
        &mut self,
        name: InternedString,
        args: &[Expression],
        ctx: &mut HirBuilderContext,
        location: &TextLocation,
    ) -> Result<(Vec<HirNode>, HirExpr), CompilerError> {
        // Transform arguments
        let hir_args = self.transform_arguments(args, ctx)?;

        // Create the call expression
        let call_expr = HirExpr {
            kind: HirExprKind::Call {
                target: CallTarget::UserFunction(name),
                args: hir_args,
            },
            location: location.clone(),
        };

        Ok((vec![], call_expr))
    }

    /// Transforms a host function call into HIR as a statement.
    ///
    /// Host function calls include import information for the WASM module and function name.
    pub fn transform_host_function_call_as_stmt(
        &mut self,
        host_function_id: HostFunctionId,
        args: &[Expression],
        ctx: &mut HirBuilderContext,
        location: &TextLocation,
    ) -> Result<Vec<HirNode>, CompilerError> {
        // Transform arguments
        let hir_args = self.transform_arguments(args, ctx)?;

        // Create the host call statement
        let node_id = ctx.allocate_node_id();
        let call_node = HirNode {
            kind: HirKind::Stmt(HirStmt::Call {
                target: CallTarget::HostFunction(host_function_id),
                args: hir_args,
            }),
            location: location.clone(),
            id: node_id,
        };

        Ok(vec![call_node])
    }

    /// Transforms a host function call into HIR as an expression.
    ///
    /// This is used when the host function call result is needed in an expression context.
    pub fn transform_host_function_call(
        &mut self,
        id: HostFunctionId,
        args: &[Expression],
        return_types: &[DataType],
        ctx: &mut HirBuilderContext,
        location: &TextLocation,
    ) -> Result<(Vec<HirNode>, HirExpr), CompilerError> {
        // Transform arguments
        let hir_args = self.transform_arguments(args, ctx)?;

        // For host calls as expressions, we need to create a statement first
        // and then load the result. For now, we'll create a call expression directly.
        // This is a simplification - a proper implementation would use temporaries.
        let node_id = ctx.allocate_node_id();
        let call_stmt = HirNode {
            kind: HirKind::Stmt(HirStmt::Call {
                target: CallTarget::HostFunction(id),
                args: hir_args.clone(),
            }),
            location: location.clone(),
            id: node_id,
        };

        // Create a placeholder expression that represents the call result
        // In a full implementation, this would use a temporary variable
        let call_expr = HirExpr {
            kind: HirExprKind::Call {
                target: CallTarget::HostFunction(id),
                args: hir_args,
            },
            location: location.clone(),
        };

        Ok((vec![call_stmt], call_expr))
    }

    /// Transforms function arguments into HIR expressions.
    ///
    /// This prepares arguments for Beanstalk's unified ABI, where arguments can be
    /// either borrowed or owned (determined at runtime via ownership flags).
    fn transform_arguments(
        &mut self,
        args: &[Expression],
        _ctx: &mut HirBuilderContext,
    ) -> Result<Vec<HirExpr>, CompilerError> {
        let mut hir_args = Vec::new();

        for arg in args {
            let hir_arg = self.transform_argument(arg)?;
            hir_args.push(hir_arg);
        }

        Ok(hir_args)
    }

    /// Transforms a single argument expression into HIR.
    ///
    /// This is a simplified transformation for basic expression types.
    /// Complex expressions should be linearized first.
    pub fn transform_argument(&mut self, expr: &Expression) -> Result<HirExpr, CompilerError> {
        let hir_expr_kind = match &expr.kind {
            ExpressionKind::Int(val) => HirExprKind::Int(*val),
            ExpressionKind::Float(val) => HirExprKind::Float(*val),
            ExpressionKind::Bool(val) => HirExprKind::Bool(*val),
            ExpressionKind::StringSlice(s) => HirExprKind::StringLiteral(*s),
            ExpressionKind::Char(c) => HirExprKind::Char(*c),
            ExpressionKind::Reference(name) => HirExprKind::Load(
                crate::compiler_frontend::hir::hir_nodes::HirPlace::Var(*name),
            ),
            _ => {
                return_hir_transformation_error!(
                    format!(
                        "Complex expression in function argument not yet supported: {:?}",
                        expr.kind
                    ),
                    expr.location.to_error_location_without_table(),
                    {
                        CompilationStage => "HIR Generation - Function Transformation",
                        PrimarySuggestion => "Simplify the argument expression or use expression linearizer"
                    }
                )
            }
        };

        Ok(HirExpr {
            kind: hir_expr_kind,
            location: expr.location.clone(),
        })
    }

    /// Prepares arguments for Beanstalk's unified ABI.
    ///
    /// The unified ABI uses tagged pointers where the ownership bit indicates
    /// whether the argument is borrowed (0) or owned (1). This method marks
    /// arguments that could potentially transfer ownership.
    pub fn prepare_unified_abi_arguments(
        &mut self,
        args: &[Expression],
        ctx: &mut HirBuilderContext,
    ) -> Result<Vec<HirExpr>, CompilerError> {
        let mut hir_args = Vec::new();

        for arg in args {
            let mut hir_arg = self.transform_argument(arg)?;

            // Check if this argument could potentially transfer ownership
            // This is a conservative analysis - the borrow checker will finalize
            if let ExpressionKind::Reference(name) = &arg.kind {
                if ctx.is_potentially_owned(name) {
                    // Mark this as a potential move
                    hir_arg.kind = HirExprKind::Move(
                        crate::compiler_frontend::hir::hir_nodes::HirPlace::Var(*name),
                    );
                    ctx.mark_potentially_consumed(*name);
                }
            }

            hir_args.push(hir_arg);
        }

        Ok(hir_args)
    }
}

impl Default for FunctionTransformer {
    fn default() -> Self {
        Self::new()
    }
}
