use crate::compiler::compiler_errors::CompilerError;
use crate::compiler::datatypes::DataType;
use crate::compiler::hir::builder::HirBuilder;
use crate::compiler::hir::nodes::{BinOp, HirExpr, HirExprKind, HirKind, HirNode};
use crate::compiler::hir::place::Place;
use crate::compiler::parsers::ast_nodes::{AstNode, NodeKind};
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind, Operator};
use crate::compiler::parsers::tokenizer::tokens::TextLocation;
use crate::return_compiler_error;

/// Lower an expression to a place, introducing temporaries as needed
///
/// Returns a list of HIR nodes that compute the expression and the final place
/// where the result is stored. This linearizes nested expressions.
impl<'a> HirBuilder<'a> {
    pub(crate) fn lower_expr_to_place(
        &mut self,
        expr: Expression,
    ) -> Result<(Vec<HirNode>, Place), CompilerError> {
        match expr.kind {
            // === Literals ===
            ExpressionKind::Int(n) => {
                let (nodes, place) = self.create_literal_assignment(
                    HirExprKind::Int(n),
                    expr.data_type,
                    expr.location,
                );
                Ok((nodes, place))
            }

            ExpressionKind::Float(f) => {
                let (nodes, place) = self.create_literal_assignment(
                    HirExprKind::Float(f),
                    expr.data_type,
                    expr.location,
                );
                Ok((nodes, place))
            }

            ExpressionKind::Bool(b) => {
                let (nodes, place) = self.create_literal_assignment(
                    HirExprKind::Bool(b),
                    expr.data_type,
                    expr.location,
                );
                Ok((nodes, place))
            }

            ExpressionKind::StringSlice(s) => {
                let (nodes, place) = self.create_literal_assignment(
                    HirExprKind::StringLiteral(s),
                    expr.data_type,
                    expr.location,
                );
                Ok((nodes, place))
            }

            ExpressionKind::Char(c) => {
                let (nodes, place) = self.create_literal_assignment(
                    HirExprKind::Char(c),
                    expr.data_type,
                    expr.location,
                );
                Ok((nodes, place))
            }

            // === Variable References ===
            ExpressionKind::Reference(name) => {
                // Direct reference to an existing place
                Ok((vec![], Place::local(name)))
            }

            // === Runtime Expressions (RPN) ===
            ExpressionKind::Runtime(rpn_nodes) => {
                self.lower_rpn_to_place(rpn_nodes, expr.data_type, expr.location)
            }

            // === Collections ===
            ExpressionKind::Collection(elements) => {
                let mut nodes = Vec::new();
                let mut element_places = Vec::new();

                // Lower all elements to places
                for element in elements {
                    let (element_nodes, element_place) = self.lower_expr_to_place(element)?;
                    nodes.extend(element_nodes);
                    element_places.push(element_place);
                }

                let temp = self.next_temp();
                let temp_place = Place::local(temp);
                let collection_expr = HirExpr {
                    kind: HirExprKind::Collection(element_places),
                    data_type: expr.data_type,
                    location: expr.location.clone(),
                };
                let assign_node = self.create_assign_node_with_expr(
                    temp_place.clone(),
                    collection_expr,
                    expr.location,
                    self.current_scope.clone(),
                );
                nodes.push(assign_node);
                Ok((nodes, temp_place))
            }

            // === Struct Construction ===
            ExpressionKind::StructInstance(fields) => {
                let mut nodes = Vec::new();
                let mut field_places = Vec::new();

                // Lower all field values to places
                for field in fields {
                    let (field_nodes, field_place) = self.lower_expr_to_place(field.value)?;
                    nodes.extend(field_nodes);
                    field_places.push((field.id, field_place));
                }

                let temp = self.next_temp();
                let temp_place = Place::local(temp);

                // Extract type name from the data type
                // For now, we'll use a generic name since DataType::Struct doesn't contain the type name
                let type_name = self.string_table.intern("StructInstance");

                let struct_expr = HirExpr {
                    kind: HirExprKind::StructConstruct {
                        type_name,
                        fields: field_places,
                    },
                    data_type: expr.data_type,
                    location: expr.location.clone(),
                };
                let assign_node = self.create_assign_node_with_expr(
                    temp_place.clone(),
                    struct_expr,
                    expr.location,
                    self.current_scope.clone(),
                );
                nodes.push(assign_node);
                Ok((nodes, temp_place))
            }

            // === Range Expressions ===
            ExpressionKind::Range(start_expr, end_expr) => {
                let mut nodes = Vec::new();

                // Lower start and end expressions to places
                let (start_nodes, start_place) = self.lower_expr_to_place(*start_expr)?;
                nodes.extend(start_nodes);

                let (end_nodes, end_place) = self.lower_expr_to_place(*end_expr)?;
                nodes.extend(end_nodes);

                let temp = self.next_temp();
                let temp_place = Place::local(temp);
                let range_expr = HirExpr {
                    kind: HirExprKind::Range {
                        start: start_place,
                        end: end_place,
                    },
                    data_type: expr.data_type,
                    location: expr.location.clone(),
                };
                let assign_node = self.create_assign_node_with_expr(
                    temp_place.clone(),
                    range_expr,
                    expr.location,
                    self.current_scope.clone(),
                );
                nodes.push(assign_node);
                Ok((nodes, temp_place))
            }

            // === Function Call Expressions ===
            ExpressionKind::FunctionCall(name, args) => {
                let mut nodes = Vec::new();
                let mut arg_places = Vec::new();

                // Lower all arguments to places
                for arg in args {
                    let (arg_nodes, arg_place) = self.lower_expr_to_place(arg)?;
                    nodes.extend(arg_nodes);
                    arg_places.push(arg_place);
                }

                let temp = self.next_temp();
                let temp_place = Place::local(temp);
                let call_expr = HirExpr {
                    kind: HirExprKind::Call {
                        target: name,
                        args: arg_places,
                    },
                    data_type: expr.data_type,
                    location: expr.location.clone(),
                };
                let assign_node = self.create_assign_node_with_expr(
                    temp_place.clone(),
                    call_expr,
                    expr.location,
                    self.current_scope.clone(),
                );
                nodes.push(assign_node);
                Ok((nodes, temp_place))
            }

            // === None Expression ===
            ExpressionKind::None => {
                // None expressions don't produce a value, so we create a placeholder
                let temp = self.next_temp();
                let temp_place = Place::local(temp);
                let none_expr = HirExpr {
                    kind: HirExprKind::Load(temp_place.clone()), // Load from self as placeholder
                    data_type: DataType::None,
                    location: expr.location.clone(),
                };
                let assign_node = self.create_assign_node_with_expr(
                    temp_place.clone(),
                    none_expr,
                    expr.location,
                    self.current_scope.clone(),
                );
                Ok((vec![assign_node], temp_place))
            }

            // === Function Expressions ===
            ExpressionKind::Function(signature, body) => {
                // Function expressions become function definitions
                // Generate a unique name for the anonymous function
                let anonymous_function_name = format!("_anon_func_{}", self.temp_counter);
                self.temp_counter += 1;
                let anonymous_function_name_interned =
                    self.string_table.intern(&anonymous_function_name);

                // Lower the function body
                let function_body_hir = self.lower_block(body)?;

                // Create a function definition node
                let function_definition_node = HirNode {
                    kind: HirKind::FunctionDef {
                        name: anonymous_function_name_interned,
                        signature,
                        body: function_body_hir,
                    },
                    location: expr.location.clone(),
                    scope: self.current_scope.clone(),
                    id: self.next_id(),
                };

                // Create a place that references this function
                let temp_place_name = self.next_temp();
                let temp_place = Place::local(temp_place_name);
                let function_reference_expr = HirExpr {
                    kind: HirExprKind::Load(Place::local(anonymous_function_name_interned)),
                    data_type: expr.data_type,
                    location: expr.location.clone(),
                };
                let assignment_node = self.create_assign_node_with_expr(
                    temp_place.clone(),
                    function_reference_expr,
                    expr.location,
                    self.current_scope.clone(),
                );

                Ok((vec![function_definition_node, assignment_node], temp_place))
            }

            // === Template Expressions ===
            ExpressionKind::Template(_template) => {
                // Templates become runtime template calls or string literals
                // For now, we'll treat them as string literals since template processing
                // is typically done at compile time
                let temp = self.next_temp();
                let temp_place = Place::local(temp);

                // Create a placeholder string literal for the template
                let template_string = self.string_table.intern("template_placeholder");
                let template_expr = HirExpr {
                    kind: HirExprKind::StringLiteral(template_string),
                    data_type: DataType::String,
                    location: expr.location.clone(),
                };
                let assign_node = self.create_assign_node_with_expr(
                    temp_place.clone(),
                    template_expr,
                    expr.location,
                    self.current_scope.clone(),
                );
                Ok((vec![assign_node], temp_place))
            }

            // === Struct Definition Expressions ===
            ExpressionKind::StructDefinition(_fields) => {
                // Struct definitions as expressions are not typical in HIR
                // We'll treat this as an error for now since struct definitions
                // should be handled at the statement level
                return_compiler_error!(
                    "Struct definitions as expressions are not supported in HIR"; {
                        CompilationStage => "HIR Generation",
                        PrimarySuggestion => "Move struct definition to statement level"
                    }
                )
            }
        }
    }

    /// Lower an RPN expression sequence to a place
    fn lower_rpn_to_place(
        &mut self,
        rpn: Vec<AstNode>,
        result_type: DataType,
        location: TextLocation,
    ) -> Result<(Vec<HirNode>, Place), CompilerError> {
        let mut nodes = Vec::new();
        let mut stack: Vec<Place> = Vec::new();

        for node in rpn {
            match node.kind {
                // Push operands (expressions) onto the stack
                NodeKind::Rvalue(expr) => {
                    let (expr_nodes, expr_place) = self.lower_expr_to_place(expr)?;
                    nodes.extend(expr_nodes);
                    stack.push(expr_place);
                }

                // Pop operands, apply operator, push the result
                NodeKind::Operator(op) => {
                    let right_operand = stack.pop().ok_or_else(|| {
                        use crate::compiler::compiler_messages::compiler_errors::{
                            ErrorLocation, ErrorType,
                        };
                        CompilerError::new(
                            "RPN stack underflow (right operand)",
                            ErrorLocation::default(),
                            ErrorType::Compiler,
                        )
                    })?;

                    let left_operand = stack.pop().ok_or_else(|| {
                        use crate::compiler::compiler_messages::compiler_errors::{
                            ErrorLocation, ErrorType,
                        };
                        CompilerError::new(
                            "RPN stack underflow (left operand)",
                            ErrorLocation::default(),
                            ErrorType::Compiler,
                        )
                    })?;

                    let binary_operator = self.convert_operator(op)?;
                    let result_temp_name = self.next_temp();
                    let result_temp_place = Place::local(result_temp_name);

                    // Determine the result type based on operator
                    let operation_result_type = match binary_operator {
                        BinOp::Eq
                        | BinOp::Ne
                        | BinOp::Lt
                        | BinOp::Le
                        | BinOp::Gt
                        | BinOp::Ge
                        | BinOp::And
                        | BinOp::Or => DataType::Bool,
                        _ => result_type.clone(),
                    };

                    let binary_operation_expr = HirExpr {
                        kind: HirExprKind::BinOp {
                            left: left_operand,
                            op: binary_operator,
                            right: right_operand,
                        },
                        data_type: operation_result_type,
                        location: node.location,
                    };

                    let assignment_node = self.create_assign_node_with_expr(
                        result_temp_place.clone(),
                        binary_operation_expr,
                        location.clone(),
                        self.current_scope.clone(),
                    );
                    nodes.push(assignment_node);
                    stack.push(result_temp_place);
                }

                _ => {
                    return_compiler_error!(
                        "Unexpected node in RPN sequence: {:?}",
                        node.kind; {
                            CompilationStage => "HIR Generation"
                        }
                    )
                }
            }
        }

        // Should have exactly one result
        if stack.len() != 1 {
            return_compiler_error!(
                "Invalid RPN sequence: stack size = {}",
                stack.len(); {
                    CompilationStage => "HIR Generation"
                }
            )
        }

        Ok((nodes, stack.pop().unwrap()))
    }

    /// Convert AST operator to HIR BinOp
    fn convert_operator(&self, ast_operator: Operator) -> Result<BinOp, CompilerError> {
        let hir_binary_operator = match ast_operator {
            Operator::Add => BinOp::Add,
            Operator::Subtract => BinOp::Sub,
            Operator::Multiply => BinOp::Mul,
            Operator::Divide => BinOp::Div,
            Operator::Modulus => BinOp::Mod,
            Operator::Root => BinOp::Root,
            Operator::Exponent => BinOp::Exponent,
            Operator::And => BinOp::And,
            Operator::Or => BinOp::Or,
            Operator::GreaterThan => BinOp::Gt,
            Operator::GreaterThanOrEqual => BinOp::Ge,
            Operator::LessThan => BinOp::Lt,
            Operator::LessThanOrEqual => BinOp::Le,
            Operator::Equality => BinOp::Eq,
            Operator::Not => {
                return_compiler_error!(
                    "Unary operator 'Not' found in binary operation context"; {
                        CompilationStage => "HIR Generation"
                    }
                )
            }
            Operator::Range => {
                return_compiler_error!(
                    "Range operator should be handled as Range expression, not binary operation"; {
                        CompilationStage => "HIR Generation"
                    }
                )
            }
        };
        Ok(hir_binary_operator)
    }

    /// Helper: convert expression to HIR pattern
    pub(crate) fn lower_expr_to_pattern(
        &mut self,
        expr: Expression,
    ) -> Result<crate::compiler::hir::nodes::HirPattern, CompilerError> {
        use crate::compiler::hir::nodes::HirPattern;

        match expr.kind {
            // Literal patterns - these match exact values
            ExpressionKind::Int(n) => {
                let literal_expr = HirExpr {
                    kind: HirExprKind::Int(n),
                    data_type: expr.data_type,
                    location: expr.location,
                };
                Ok(HirPattern::Literal(literal_expr))
            }

            ExpressionKind::Float(f) => {
                let literal_expr = HirExpr {
                    kind: HirExprKind::Float(f),
                    data_type: expr.data_type,
                    location: expr.location,
                };
                Ok(HirPattern::Literal(literal_expr))
            }

            ExpressionKind::Bool(b) => {
                let literal_expr = HirExpr {
                    kind: HirExprKind::Bool(b),
                    data_type: expr.data_type,
                    location: expr.location,
                };
                Ok(HirPattern::Literal(literal_expr))
            }

            ExpressionKind::StringSlice(s) => {
                let literal_expr = HirExpr {
                    kind: HirExprKind::StringLiteral(s),
                    data_type: expr.data_type,
                    location: expr.location,
                };
                Ok(HirPattern::Literal(literal_expr))
            }

            ExpressionKind::Char(c) => {
                let literal_expr = HirExpr {
                    kind: HirExprKind::Char(c),
                    data_type: expr.data_type,
                    location: expr.location,
                };
                Ok(HirPattern::Literal(literal_expr))
            }

            // Range patterns - match values within a range
            ExpressionKind::Range(start_expr, end_expr) => {
                let start_hir = self.lower_expr_to_hir_expr(*start_expr)?;
                let end_hir = self.lower_expr_to_hir_expr(*end_expr)?;
                Ok(HirPattern::Range {
                    start: start_hir,
                    end: end_hir,
                })
            }

            // Variable references in patterns - these would be binding patterns
            // For now, treat as wildcard since binding patterns aren't fully implemented
            ExpressionKind::Reference(_) => Ok(HirPattern::Wildcard),

            // Complex expressions that can't be patterns
            ExpressionKind::Runtime(_)
            | ExpressionKind::Collection(_)
            | ExpressionKind::StructInstance(_)
            | ExpressionKind::FunctionCall(_, _) => {
                return_compiler_error!(
                    "Complex expressions cannot be used as match patterns: {:?}",
                    expr.kind; {
                        CompilationStage => "HIR Generation",
                        PrimarySuggestion => "Use literal values, ranges, or variables in match patterns"
                    }
                )
            }

            // Unsupported pattern types - treat as wildcard for now
            _ => Ok(HirPattern::Wildcard),
        }
    }

    /// Helper: convert expression to HIR expression (for patterns)
    fn lower_expr_to_hir_expr(&mut self, expr: Expression) -> Result<HirExpr, CompilerError> {
        match expr.kind {
            ExpressionKind::Int(n) => Ok(HirExpr {
                kind: HirExprKind::Int(n),
                data_type: expr.data_type,
                location: expr.location,
            }),

            ExpressionKind::Float(f) => Ok(HirExpr {
                kind: HirExprKind::Float(f),
                data_type: expr.data_type,
                location: expr.location,
            }),

            ExpressionKind::Bool(b) => Ok(HirExpr {
                kind: HirExprKind::Bool(b),
                data_type: expr.data_type,
                location: expr.location,
            }),

            ExpressionKind::StringSlice(s) => Ok(HirExpr {
                kind: HirExprKind::StringLiteral(s),
                data_type: expr.data_type,
                location: expr.location,
            }),

            ExpressionKind::Char(c) => Ok(HirExpr {
                kind: HirExprKind::Char(c),
                data_type: expr.data_type,
                location: expr.location,
            }),

            // Variable references in pattern contexts
            ExpressionKind::Reference(name) => {
                // For range bounds, we need to look up the variable's current value
                // This creates a Load operation to get the variable's place
                let place = Place::local(name);
                Ok(HirExpr {
                    kind: HirExprKind::Load(place),
                    data_type: expr.data_type,
                    location: expr.location,
                })
            }

            _ => {
                return_compiler_error!(
                    "Unsupported expression in pattern context: {:?}",
                    expr.kind; {
                        CompilationStage => "HIR Generation",
                        PrimarySuggestion => "Only literal values and variables can be used in pattern expressions"
                    }
                )
            }
        }
    }
}
