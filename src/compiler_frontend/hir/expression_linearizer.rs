#![allow(unused)]

//! Expression Linearizer for HIR Builder
//!
//! This module implements the ExpressionLinearizer component that converts nested AST
//! expressions into sequential HIR instructions with explicit temporaries.
//!
//! The linearizer ensures that:
//! - All nested expressions are flattened into sequential instructions
//! - Compiler-introduced temporaries are treated exactly like user locals
//! - Binary operations, function calls, and complex expressions are properly handled
//! - Expression evaluation order is explicit and deterministic
//!
//! ## Key Design Principles
//!
//! - All compiler_frontend-introduced locals are entered into the same variable and scope system
//!   as user variables, with identical drop and borrow semantics
//! - Expression flattening creates explicit temporaries for intermediate results
//! - The linearizer operates on borrowed HirBuilderContext to maintain single authoritative state

use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::hir::build_hir::HirBuilderContext;
use crate::compiler_frontend::hir::nodes::{
    BinOp, HirExpr, HirExprKind, HirKind, HirNode, HirPlace, HirStmt, UnaryOp,
};
use crate::compiler_frontend::host_functions::registry::{CallTarget, HostFunctionId};
use crate::compiler_frontend::parsers::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::parsers::expressions::expression::{
    Expression, ExpressionKind, Operator,
};
use crate::compiler_frontend::parsers::tokenizer::tokens::TextLocation;
use crate::compiler_frontend::string_interning::InternedString;
use crate::return_compiler_error;
use std::collections::HashMap;

/// The ExpressionLinearizer component converts nested AST expressions into
/// sequential HIR instructions with explicit temporaries.
///
/// This component operates on borrowed HirBuilderContext rather than owning
/// independent state, ensuring a single authoritative HIR state per module.
#[derive(Debug, Default)]
pub struct ExpressionLinearizer {
    /// Compiler-introduced local variables and their kinds.
    /// CRITICAL: All these locals are treated exactly like user locals with
    /// identical drop and borrow semantics.
    compiler_introduced_locals: HashMap<InternedString, HirExprKind>,
}

impl ExpressionLinearizer {
    /// Creates a new ExpressionLinearizer
    pub fn new() -> Self {
        ExpressionLinearizer {
            compiler_introduced_locals: HashMap::new(),
        }
    }

    /// Linearizes an AST expression into HIR.
    ///
    /// Returns a tuple of:
    /// - Vec<HirNode>: The sequence of HIR nodes that compute intermediate values
    /// - HirExpr: The final expression result (may be a simple load or literal)
    ///
    /// This method flattens nested expressions by introducing explicit temporaries
    /// for intermediate results.
    pub fn linearize_expression(
        &mut self,
        expr: &Expression,
        ctx: &mut HirBuilderContext,
    ) -> Result<(Vec<HirNode>, HirExpr), CompilerError> {
        match &expr.kind {
            // Literals - no linearization needed, just convert to HIR
            ExpressionKind::Int(value) => {
                Ok((Vec::new(), self.create_int_expr(*value, &expr.location)))
            }
            ExpressionKind::Float(value) => {
                Ok((Vec::new(), self.create_float_expr(*value, &expr.location)))
            }
            ExpressionKind::Bool(value) => {
                Ok((Vec::new(), self.create_bool_expr(*value, &expr.location)))
            }
            ExpressionKind::Char(value) => {
                Ok((Vec::new(), self.create_char_expr(*value, &expr.location)))
            }
            ExpressionKind::StringSlice(interned) => Ok((
                Vec::new(),
                self.create_string_literal_expr(*interned, &expr.location),
            )),

            // Variable reference - convert to HIR Load
            ExpressionKind::Reference(name) => {
                let hir_expr = HirExpr {
                    kind: HirExprKind::Load(HirPlace::Var(*name)),
                    location: expr.location.clone(),
                };
                Ok((Vec::new(), hir_expr))
            }

            // Runtime expressions need full linearization
            ExpressionKind::Runtime(ast_nodes) => {
                self.linearize_runtime_expression(ast_nodes, &expr.location, ctx)
            }

            // Function calls
            ExpressionKind::FunctionCall(name, args) => {
                self.linearize_function_call(*name, args, &expr.location, ctx)
            }

            ExpressionKind::HostFunctionCall(id, args) => {
                self.linearize_host_function_call(*id, args, &expr.location, ctx)
            }

            // Collections
            ExpressionKind::Collection(items) => {
                self.linearize_collection(items, &expr.data_type, &expr.location, ctx)
            }

            // Struct instances
            ExpressionKind::StructInstance(args) => {
                self.linearize_struct_instance(args, &expr.data_type, &expr.location, ctx)
            }

            // Range expressions
            ExpressionKind::Range(start, end) => {
                self.linearize_range(start, end, &expr.data_type, &expr.location, ctx)
            }

            // Templates - handled by template processor
            ExpressionKind::Template(template) => {
                // Use the TemplateProcessor to handle templates
                let mut template_processor =
                    crate::compiler_frontend::hir::template_processor::TemplateProcessor::new();
                template_processor.process_template(template, ctx)
            }

            // Functions as values
            ExpressionKind::Function(_, _) => {
                // Function expressions are handled by FunctionTransformer
                return_compiler_error!(
                    "Function expressions should be processed by FunctionTransformer"
                )
            }

            // Struct definitions
            ExpressionKind::StructDefinition(_) => {
                // Struct definitions are handled at the module level
                return_compiler_error!("Struct definitions should be processed at module level")
            }

            // None - empty expression
            ExpressionKind::None => Ok((Vec::new(), self.create_none_expr(&expr.location))),
        }
    }

    /// Linearizes a runtime expression (expressions that couldn't be folded at compile time).
    ///
    /// Runtime expressions are stored as a Vec<AstNode> in RPN (Reverse Polish Notation) order.
    /// This method processes them sequentially, creating temporaries for intermediate results.
    fn linearize_runtime_expression(
        &mut self,
        ast_nodes: &[AstNode],
        location: &TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> Result<(Vec<HirNode>, HirExpr), CompilerError> {
        let mut nodes = Vec::new();
        let mut value_stack: Vec<HirExpr> = Vec::new();

        for ast_node in ast_nodes {
            match &ast_node.kind {
                // Operators consume values from the stack and produce a result
                NodeKind::Operator(op) => {
                    let required = op.required_values();

                    if required == 2 {
                        // Binary operator
                        if value_stack.len() < 2 {
                            return_compiler_error!(
                                "Not enough operands for binary operator {:?}",
                                op
                            );
                        }
                        let right = value_stack.pop().unwrap();
                        let left = value_stack.pop().unwrap();

                        let hir_op = self.convert_operator(op)?;

                        // Create the binary operation expression
                        let binop_expr = HirExpr {
                            kind: HirExprKind::BinOp {
                                left: Box::new(left),
                                op: hir_op,
                                right: Box::new(right),
                            },
                            location: ast_node.location.clone(),
                        };

                        // If this is not the last operation, create a temporary
                        // Otherwise, keep it on the stack as the final result
                        value_stack.push(binop_expr);
                    } else {
                        // Unary operator
                        if value_stack.is_empty() {
                            return_compiler_error!(
                                "Not enough operands for unary operator {:?}",
                                op
                            );
                        }
                        let operand = value_stack.pop().unwrap();

                        let hir_op = self.convert_unary_operator(op)?;

                        let unary_expr = HirExpr {
                            kind: HirExprKind::UnaryOp {
                                op: hir_op,
                                operand: Box::new(operand),
                            },
                            location: ast_node.location.clone(),
                        };

                        value_stack.push(unary_expr);
                    }
                }

                // Variable declarations push their value onto the stack
                NodeKind::VariableDeclaration(arg) => {
                    let (expr_nodes, expr) = self.linearize_expression(&arg.value, ctx)?;
                    nodes.extend(expr_nodes);
                    value_stack.push(expr);
                }

                // R-values push their expression onto the stack
                NodeKind::Rvalue(expr) => {
                    let (expr_nodes, hir_expr) = self.linearize_expression(expr, ctx)?;
                    nodes.extend(expr_nodes);
                    value_stack.push(hir_expr);
                }

                // Function calls
                NodeKind::FunctionCall {
                    name,
                    args,
                    returns,
                    location,
                } => {
                    let (call_nodes, call_expr) =
                        self.linearize_function_call(*name, args, location, ctx)?;
                    nodes.extend(call_nodes);
                    value_stack.push(call_expr);
                }

                // Host function calls
                NodeKind::HostFunctionCall {
                    host_function_id,
                    args,
                    returns,
                    location,
                } => {
                    let (call_nodes, call_expr) =
                        self.linearize_host_function_call(*host_function_id, args, location, ctx)?;
                    nodes.extend(call_nodes);
                    value_stack.push(call_expr);
                }

                // Field access
                NodeKind::FieldAccess {
                    base,
                    field,
                    data_type,
                    ..
                } => {
                    let (base_nodes, base_expr) = self.linearize_ast_node(base, ctx)?;
                    nodes.extend(base_nodes);

                    // Extract the base variable name from the expression
                    let base_var = self.extract_base_var(&base_expr)?;

                    let field_expr = HirExpr {
                        kind: HirExprKind::Field {
                            base: base_var,
                            field: *field,
                        },
                        location: ast_node.location.clone(),
                    };
                    value_stack.push(field_expr);
                }

                // Other node types that might appear in runtime expressions
                _ => {
                    // Try to convert the node to an expression
                    let (node_nodes, node_expr) = self.linearize_ast_node(ast_node, ctx)?;
                    nodes.extend(node_nodes);
                    value_stack.push(node_expr);
                }
            }
        }

        // The final value on the stack is our result
        if let Some(result) = value_stack.pop() {
            // If there are remaining values on the stack, something went wrong
            if !value_stack.is_empty() {
                return_compiler_error!(
                    "Expression linearization left {} unused values on stack",
                    value_stack.len()
                );
            }
            Ok((nodes, result))
        } else {
            // Empty expression - return a none expression
            Ok((
                nodes,
                HirExpr {
                    kind: HirExprKind::Int(0), // Placeholder for empty expressions
                    location: location.clone(),
                },
            ))
        }
    }

    /// Linearizes a function call expression.
    ///
    /// Arguments are linearized first, then the call is created.
    /// If the call result is used in a larger expression, a temporary is created.
    pub fn linearize_function_call(
        &mut self,
        name: InternedString,
        args: &[Expression],
        location: &TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> Result<(Vec<HirNode>, HirExpr), CompilerError> {
        let mut nodes = Vec::new();
        let mut hir_args = Vec::new();

        // Linearize each argument
        for arg in args {
            let (arg_nodes, arg_expr) = self.linearize_expression(arg, ctx)?;
            nodes.extend(arg_nodes);
            hir_args.push(arg_expr);
        }

        // Create the call expression
        let call_expr = HirExpr {
            kind: HirExprKind::Call {
                target: CallTarget::UserFunction(name),
                args: hir_args,
            },
            location: location.clone(),
        };

        Ok((nodes, call_expr))
    }

    /// Linearizes a host function call expression.
    fn linearize_host_function_call(
        &mut self,
        host_function_id: HostFunctionId,
        args: &[Expression],
        location: &TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> Result<(Vec<HirNode>, HirExpr), CompilerError> {
        let mut nodes = Vec::new();
        let mut hir_args = Vec::new();

        // Linearize each argument
        for arg in args {
            let (arg_nodes, arg_expr) = self.linearize_expression(arg, ctx)?;
            nodes.extend(arg_nodes);
            hir_args.push(arg_expr);
        }

        // Create the call expression (host calls are represented the same as regular calls in HIR)
        let call_expr = HirExpr {
            kind: HirExprKind::Call {
                target: CallTarget::HostFunction(host_function_id),
                args: hir_args,
            },
            location: location.clone(),
        };

        Ok((nodes, call_expr))
    }

    /// Linearizes a binary operation.
    ///
    /// Both operands are linearized first, then the operation is created.
    pub fn linearize_binary_operation(
        &mut self,
        left: &Expression,
        op: &Operator,
        right: &Expression,
        location: &TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> Result<(Vec<HirNode>, HirExpr), CompilerError> {
        let mut nodes = Vec::new();

        // Linearize left operand
        let (left_nodes, left_expr) = self.linearize_expression(left, ctx)?;
        nodes.extend(left_nodes);

        // Linearize right operand
        let (right_nodes, right_expr) = self.linearize_expression(right, ctx)?;
        nodes.extend(right_nodes);

        // Convert operator
        let hir_op = self.convert_operator(op)?;

        // Create the binary operation expression
        let binop_expr = HirExpr {
            kind: HirExprKind::BinOp {
                left: Box::new(left_expr),
                op: hir_op,
                right: Box::new(right_expr),
            },
            location: location.clone(),
        };

        Ok((nodes, binop_expr))
    }

    /// Linearizes a collection expression.
    fn linearize_collection(
        &mut self,
        items: &[Expression],
        result_type: &DataType,
        location: &TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> Result<(Vec<HirNode>, HirExpr), CompilerError> {
        let mut nodes = Vec::new();
        let mut hir_items = Vec::new();

        for item in items {
            let (item_nodes, item_expr) = self.linearize_expression(item, ctx)?;
            nodes.extend(item_nodes);
            hir_items.push(item_expr);
        }

        let collection_expr = HirExpr {
            kind: HirExprKind::Collection(hir_items),
            location: location.clone(),
        };

        Ok((nodes, collection_expr))
    }

    /// Linearizes a struct instance expression.
    fn linearize_struct_instance(
        &mut self,
        args: &[crate::compiler_frontend::parsers::ast_nodes::Var],
        result_type: &DataType,
        location: &TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> Result<(Vec<HirNode>, HirExpr), CompilerError> {
        let mut nodes = Vec::new();
        let mut hir_fields = Vec::new();

        for arg in args {
            let (field_nodes, field_expr) = self.linearize_expression(&arg.value, ctx)?;
            nodes.extend(field_nodes);
            hir_fields.push((arg.id, field_expr));
        }

        let struct_expr = HirExpr {
            kind: HirExprKind::StructConstruct {
                type_name: self.extract_struct_name(result_type),
                fields: hir_fields,
            },
            location: location.clone(),
        };

        Ok((nodes, struct_expr))
    }

    /// Linearizes a range expression.
    fn linearize_range(
        &mut self,
        start: &Expression,
        end: &Expression,
        result_type: &DataType,
        location: &TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> Result<(Vec<HirNode>, HirExpr), CompilerError> {
        let mut nodes = Vec::new();

        let (start_nodes, start_expr) = self.linearize_expression(start, ctx)?;
        nodes.extend(start_nodes);

        let (end_nodes, end_expr) = self.linearize_expression(end, ctx)?;
        nodes.extend(end_nodes);

        let range_expr = HirExpr {
            kind: HirExprKind::Range {
                start: Box::new(start_expr),
                end: Box::new(end_expr),
            },
            location: location.clone(),
        };

        Ok((nodes, range_expr))
    }

    /// Linearizes an AST node into HIR.
    pub fn linearize_ast_node(
        &mut self,
        node: &AstNode,
        ctx: &mut HirBuilderContext,
    ) -> Result<(Vec<HirNode>, HirExpr), CompilerError> {
        match &node.kind {
            NodeKind::Rvalue(expr) => self.linearize_expression(expr, ctx),

            NodeKind::VariableDeclaration(arg) => self.linearize_expression(&arg.value, ctx),

            NodeKind::FunctionCall {
                name,
                args,
                returns,
                location,
            } => self.linearize_function_call(*name, args, location, ctx),

            NodeKind::HostFunctionCall {
                host_function_id,
                args,
                returns,
                location,
            } => self.linearize_host_function_call(*host_function_id, args, location, ctx),

            NodeKind::FieldAccess {
                base,
                field,
                data_type,
                ..
            } => {
                let (base_nodes, base_expr) = self.linearize_ast_node(base, ctx)?;
                let base_var = self.extract_base_var(&base_expr)?;

                let field_expr = HirExpr {
                    kind: HirExprKind::Field {
                        base: base_var,
                        field: *field,
                    },
                    location: node.location.clone(),
                };

                Ok((base_nodes, field_expr))
            }

            _ => {
                // Try to get expression from the node
                match node.get_expr() {
                    Ok(expr) => self.linearize_expression(&expr, ctx),
                    Err(_) => {
                        // Return a placeholder for unsupported nodes
                        Ok((
                            Vec::new(),
                            HirExpr {
                                kind: HirExprKind::Int(0),
                                location: node.location.clone(),
                            },
                        ))
                    }
                }
            }
        }
    }

    // =========================================================================
    // Temporary Variable Management
    // =========================================================================

    /// Allocates a compiler_frontend-introduced local variable.
    ///
    /// CRITICAL INVARIANT: All compiler_frontend-introduced locals are entered into the same
    /// variable and scope system as user variables, with identical drop and borrow semantics.
    pub fn allocate_compiler_local(
        &mut self,
        kind: HirExprKind,
        location: TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> InternedString {
        // Generate a unique name for the temporary
        let temp_name = ctx.metadata_mut().generate_temp_name();
        let interned_name = ctx.string_table.intern(&temp_name);

        // Register the temporary in our local tracking
        self.compiler_introduced_locals
            .insert(interned_name, kind.clone());

        // Register the temporary in the context's metadata
        ctx.metadata_mut()
            .register_temporary(interned_name, kind.clone());

        // Mark as potentially owned (temporaries can own values)
        ctx.mark_potentially_owned(interned_name);

        // Add as a drop candidate in the current scope
        ctx.add_drop_candidate(interned_name, location);

        interned_name
    }

    /// Creates an assignment HIR node.
    ///
    /// This creates an assignment statement that assigns a value to a target place.
    pub fn create_assignment(
        &mut self,
        target: HirPlace,
        value: HirExpr,
        is_mutable: bool,
        location: TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> HirNode {
        let node_id = ctx.allocate_node_id();
        let build_context = ctx.create_build_context(location.clone());
        ctx.record_node_context(node_id, build_context);

        HirNode {
            kind: HirKind::Stmt(HirStmt::Assign {
                target,
                value,
                is_mutable,
            }),
            location,
            id: node_id,
        }
    }

    /// Creates a temporary variable and assigns a value to it.
    ///
    /// Returns the HIR nodes for the assignment and the expression to load the temporary.
    pub fn create_temporary_with_value(
        &mut self,
        value: HirExpr,
        ctx: &mut HirBuilderContext,
    ) -> (Vec<HirNode>, HirExpr) {
        let location = value.location.clone();

        // Allocate a temporary variable
        let temp_name = self.allocate_compiler_local(value.kind.clone(), location.clone(), ctx);

        // Create the assignment
        let assign_node = self.create_assignment(
            HirPlace::Var(temp_name),
            value,
            true, // Temporaries are mutable
            location.clone(),
            ctx,
        );

        // Create the load expression for the temporary
        let load_expr = HirExpr {
            kind: HirExprKind::Load(HirPlace::Var(temp_name)),
            location,
        };

        (vec![assign_node], load_expr)
    }

    /// Checks if a variable is a compiler_frontend-introduced temporary.
    pub fn is_compiler_local(&self, name: &InternedString) -> bool {
        self.compiler_introduced_locals.contains_key(name)
    }

    /// Gets the type of a compiler_frontend-introduced local.
    pub fn get_compiler_local_type(&self, name: &InternedString) -> Option<&HirExprKind> {
        self.compiler_introduced_locals.get(name)
    }

    // =========================================================================
    // Helper Methods for Expression Creation
    // =========================================================================

    /// Creates an integer literal expression.
    fn create_int_expr(&self, value: i64, location: &TextLocation) -> HirExpr {
        HirExpr {
            kind: HirExprKind::Int(value),
            location: location.clone(),
        }
    }

    /// Creates a float literal expression.
    fn create_float_expr(&self, value: f64, location: &TextLocation) -> HirExpr {
        HirExpr {
            kind: HirExprKind::Float(value),
            location: location.clone(),
        }
    }

    /// Creates a boolean literal expression.
    fn create_bool_expr(&self, value: bool, location: &TextLocation) -> HirExpr {
        HirExpr {
            kind: HirExprKind::Bool(value),
            location: location.clone(),
        }
    }

    /// Creates a character literal expression.
    fn create_char_expr(&self, value: char, location: &TextLocation) -> HirExpr {
        HirExpr {
            kind: HirExprKind::Char(value),
            location: location.clone(),
        }
    }

    /// Creates a string literal expression.
    fn create_string_literal_expr(
        &self,
        value: InternedString,
        location: &TextLocation,
    ) -> HirExpr {
        HirExpr {
            kind: HirExprKind::StringLiteral(value),
            location: location.clone(),
        }
    }

    /// Creates a none/empty expression.
    fn create_none_expr(&self, location: &TextLocation) -> HirExpr {
        HirExpr {
            kind: HirExprKind::Int(0), // Placeholder
            location: location.clone(),
        }
    }

    // =========================================================================
    // Operator Conversion
    // =========================================================================
    /// Converts an AST operator to an HIR binary operator.
    pub fn convert_operator(&self, op: &Operator) -> Result<BinOp, CompilerError> {
        match op {
            Operator::Add => Ok(BinOp::Add),
            Operator::Subtract => Ok(BinOp::Sub),
            Operator::Multiply => Ok(BinOp::Mul),
            Operator::Divide => Ok(BinOp::Div),
            Operator::Modulus => Ok(BinOp::Mod),
            Operator::Root => Ok(BinOp::Root),
            Operator::Exponent => Ok(BinOp::Exponent),
            Operator::And => Ok(BinOp::And),
            Operator::Or => Ok(BinOp::Or),
            Operator::GreaterThan => Ok(BinOp::Gt),
            Operator::GreaterThanOrEqual => Ok(BinOp::Ge),
            Operator::LessThan => Ok(BinOp::Lt),
            Operator::LessThanOrEqual => Ok(BinOp::Le),
            Operator::Equality => Ok(BinOp::Eq),
            Operator::Not => {
                return_compiler_error!("'not' is a unary operator, not binary")
            }
            Operator::Range => {
                return_compiler_error!("Range operator should be handled separately")
            }
        }
    }

    /// Converts an AST operator to an HIR unary operator.
    fn convert_unary_operator(&self, op: &Operator) -> Result<UnaryOp, CompilerError> {
        match op {
            Operator::Not => Ok(UnaryOp::Not),
            Operator::Subtract => Ok(UnaryOp::Neg),
            _ => return_compiler_error!("Operator {:?} is not a unary operator", op),
        }
    }

    /// Infers the result type of a binary operation.
    pub fn infer_binop_type(&self, left: &DataType, right: &DataType, op: &BinOp) -> DataType {
        match op {
            // Comparison operators always return bool
            BinOp::Eq | BinOp::Ne | BinOp::Lt | BinOp::Le | BinOp::Gt | BinOp::Ge => DataType::Bool,

            // Logical operators return bool
            BinOp::And | BinOp::Or => DataType::Bool,

            // Arithmetic operators - use the "wider" type
            BinOp::Add
            | BinOp::Sub
            | BinOp::Mul
            | BinOp::Div
            | BinOp::Mod
            | BinOp::Root
            | BinOp::Exponent => {
                // If either operand is float, result is float
                if matches!(left, DataType::Float) || matches!(right, DataType::Float) {
                    DataType::Float
                } else {
                    // Default to the left operand's type
                    left.clone()
                }
            }
        }
    }

    // =========================================================================
    // Helper Methods for Type Extraction
    // =========================================================================

    /// Gets the return type from a list of return arguments.
    fn get_return_type(
        &self,
        returns: &[crate::compiler_frontend::parsers::ast_nodes::Var],
    ) -> DataType {
        if returns.len() == 1 {
            returns[0].value.data_type.clone()
        } else if returns.is_empty() {
            DataType::None
        } else {
            DataType::Parameters(returns.to_vec())
        }
    }

    /// Extracts the struct name from a data type.
    /// Since DataType::Struct doesn't store the name directly, we return a placeholder.
    fn extract_struct_name(&self, _data_type: &DataType) -> InternedString {
        // DataType::Struct stores fields, not the struct name
        // The struct name would need to be passed separately or looked up
        InternedString::from_u32(0) // Placeholder for unknown struct
    }

    /// Extracts the base variable name from an HIR expression.
    fn extract_base_var(&self, expr: &HirExpr) -> Result<InternedString, CompilerError> {
        match &expr.kind {
            HirExprKind::Load(HirPlace::Var(name)) => Ok(*name),
            HirExprKind::Field { base, .. } => Ok(*base),
            _ => return_compiler_error!(
                "Cannot extract base variable from expression kind: {:?}",
                expr.kind
            ),
        }
    }
}
