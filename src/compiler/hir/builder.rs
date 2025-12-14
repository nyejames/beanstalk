//! HIR builder
//!
//! Converts AST into a structured HIR representation suitable for borrow checking.
//!
//! Key responsibilities:
//! - Linearize expressions into statements operating on places
//! - Eliminate nested expressions by introducing temporary locals
//! - Convert borrow intent (not ownership outcome)
//! - Preserve structured control flow for CFG analysis
//! - Maintain place-based memory model

use crate::compiler::compiler_errors::CompilerError;
use crate::compiler::compiler_messages::compiler_errors::CompilerMessages;
use crate::compiler::datatypes::DataType;
use crate::compiler::hir::nodes::{
    BinOp, HirExpr, HirExprKind, HirKind, HirMatchArm, HirNode, HirNodeId,
};
use crate::compiler::hir::place::Place;
use crate::compiler::interned_path::InternedPath;
use crate::compiler::parsers::ast_nodes::{AstNode, NodeKind};
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind, Operator};
use crate::compiler::parsers::statements::branching::MatchArm;
use crate::compiler::string_interning::{InternedString, StringTable};
use crate::return_compiler_error;
use std::collections::HashMap;

/// Build a HIR module from the AST
///
/// The HIR builder performs linearization of expressions and creates
/// a place-based representation suitable for borrow checking analysis.
pub struct HirBuilder<'a> {
    /// Current module scope for name resolution
    current_scope: InternedPath,

    /// Sequential ID generator for HIR nodes (used by borrow checker for CFG)
    next_node_id: usize,

    /// Track local variable bindings and their types
    local_bindings: HashMap<InternedString, DataType>,

    /// Counter for generating unique temporary variable names
    temp_counter: usize,

    /// Counter for generating unique runtime template function names
    template_counter: usize,

    /// Accumulated errors and warnings during lowering
    messages: CompilerMessages,

    /// String interning table
    string_table: &'a mut StringTable,
}

impl<'a> HirBuilder<'a> {
    pub fn new(scope: InternedPath, string_table: &'a mut StringTable) -> Self {
        Self {
            current_scope: scope,
            next_node_id: 0,
            local_bindings: HashMap::new(),
            temp_counter: 0,
            template_counter: 0,
            messages: CompilerMessages::new(),
            string_table,
        }
    }

    /// Main entry point: lower the entire AST to HIR
    pub fn lower_ast(
        ast: Vec<AstNode>,
        scope: InternedPath,
        string_table: &'a mut StringTable,
    ) -> Result<Vec<HirNode>, CompilerMessages> {
        let mut builder = Self::new(scope, string_table);

        let mut hir_nodes = Vec::new();
        for node in ast {
            match builder.lower_node(node) {
                Ok(mut node_hir) => hir_nodes.append(&mut node_hir),
                Err(e) => builder.messages.errors.push(e),
            }
        }

        if !builder.messages.errors.is_empty() {
            return Err(CompilerMessages {
                errors: builder.messages.errors,
                warnings: builder.messages.warnings,
            });
        }

        Ok(hir_nodes)
    }

    fn next_id(&mut self) -> HirNodeId {
        let id = self.next_node_id;
        self.next_node_id += 1;
        id
    }

    /// Generate a unique temporary variable name
    fn next_temp(&mut self) -> InternedString {
        let name = format!("_temp_{}", self.temp_counter);
        self.temp_counter += 1;
        self.string_table.intern(&name)
    }

    /// Lower a single AST node to HIR
    ///
    /// This method linearizes expressions by introducing temporary variables
    /// and converts all operations to work on places rather than nested expressions.
    pub(crate) fn lower_node(&mut self, node: AstNode) -> Result<Vec<HirNode>, CompilerError> {
        match node.kind {
            // === Variable Declaration ===
            NodeKind::VariableDeclaration(arg) => {
                self.local_bindings
                    .insert(arg.id, arg.value.data_type.clone());

                let place = Place::local(arg.id);
                let (value_nodes, value_place) = self.lower_expr_to_place(arg.value)?;

                let mut nodes = value_nodes;
                nodes.push(self.create_assign_node(place, value_place, node.location, node.scope));
                Ok(nodes)
            }

            // Mutating an existing variable or field on that variable.
            // This reference is already enforced to be mutable by the parser
            NodeKind::Assignment {
                target,
                value: value_ast,
            } => {
                // Convert the target AST node to a proper Place
                let target_place = self.lower_ast_node_to_place(*target)?;

                let (value_nodes, value_place) = self.lower_expr_to_place(value_ast)?;
                let mut nodes = value_nodes;

                // For type inference, we'll use the root variable's type
                let target_type = match &target_place.root {
                    crate::compiler::hir::place::PlaceRoot::Local(name) => self
                        .local_bindings
                        .get(name)
                        .cloned()
                        .unwrap_or(DataType::Inferred),
                    _ => DataType::Inferred,
                };

                // Create a candidate move
                let value_expr = HirExpr {
                    kind: HirExprKind::CandidateMove(value_place),
                    data_type: target_type,
                    location: node.location.clone(),
                };

                let assignment_node = self.create_assign_node_with_expr(
                    target_place,
                    value_expr,
                    node.location,
                    node.scope,
                );
                nodes.push(assignment_node);

                Ok(nodes)
            }

            // Control Flow
            NodeKind::If(cond, then_block, else_block) => {
                let (cond_nodes, cond_place) = self.lower_expr_to_place(cond)?;
                let then_block = self.lower_block(then_block)?;
                let else_block = else_block.map(|b| self.lower_block(b)).transpose()?;

                let mut nodes = cond_nodes;
                nodes.push(HirNode {
                    kind: HirKind::If {
                        condition: cond_place,
                        then_block,
                        else_block,
                    },
                    location: node.location,
                    scope: node.scope,
                    id: self.next_id(),
                });
                Ok(nodes)
            }

            NodeKind::Match(subject, arms, default) => {
                let (subject_nodes, subject_place) = self.lower_expr_to_place(subject)?;
                let arms = arms
                    .into_iter()
                    .map(|arm| self.lower_match_arm(arm))
                    .collect::<Result<Vec<_>, _>>()?;
                let default = default.map(|b| self.lower_block(b)).transpose()?;

                let mut nodes = subject_nodes;
                nodes.push(HirNode {
                    kind: HirKind::Match {
                        scrutinee: subject_place,
                        arms,
                        default,
                    },
                    location: node.location,
                    scope: node.scope,
                    id: self.next_id(),
                });
                Ok(nodes)
            }

            NodeKind::ForLoop(item_arg, collection, body) => {
                let (collection_nodes, collection_place) = self.lower_expr_to_place(collection)?;
                let binding = Some((item_arg.id, item_arg.value.data_type));
                let body = self.lower_block(body)?;

                let mut nodes = collection_nodes;
                nodes.push(HirNode {
                    kind: HirKind::Loop {
                        binding,
                        iterator: collection_place,
                        body,
                        index_binding: None, // TODO: handle index binding
                    },
                    location: node.location,
                    scope: node.scope,
                    id: self.next_id(),
                });
                Ok(nodes)
            }

            // === Function Calls ===
            NodeKind::FunctionCall(name, args, returns, _location) => {
                let mut nodes = Vec::new();
                let mut arg_places = Vec::new();

                // Lower all arguments to places
                for arg in args {
                    let (arg_nodes, arg_place) = self.lower_expr_to_place(arg)?;
                    nodes.extend(arg_nodes);
                    arg_places.push(arg_place);
                }

                // Create return places
                let return_places: Vec<Place> = returns
                    .iter()
                    .enumerate()
                    .map(|(i, _)| {
                        let temp_name = format!("_ret_{}", i);
                        Place::local(self.string_table.intern(&temp_name))
                    })
                    .collect();

                nodes.push(HirNode {
                    kind: HirKind::Call {
                        target: name,
                        args: arg_places,
                        returns: return_places,
                    },
                    location: node.location,
                    scope: node.scope,
                    id: self.next_id(),
                });
                Ok(nodes)
            }

            NodeKind::HostFunctionCall(name, args, returns, module, import, _location) => {
                let mut nodes = Vec::new();
                let mut arg_places = Vec::new();

                // Lower all arguments to places
                for arg in args {
                    let (arg_nodes, arg_place) = self.lower_expr_to_place(arg)?;
                    nodes.extend(arg_nodes);
                    arg_places.push(arg_place);
                }

                // Create return places
                let return_places: Vec<Place> = returns
                    .iter()
                    .enumerate()
                    .map(|(i, _)| {
                        let temp_name = format!("_ret_{}", i);
                        Place::local(self.string_table.intern(&temp_name))
                    })
                    .collect();

                nodes.push(HirNode {
                    kind: HirKind::HostCall {
                        target: name,
                        module,
                        import,
                        args: arg_places,
                        returns: return_places,
                    },
                    location: node.location,
                    scope: node.scope,
                    id: self.next_id(),
                });
                Ok(nodes)
            }

            // === Returns ===
            NodeKind::Return(exprs) => {
                let mut nodes = Vec::new();
                let mut return_places = Vec::new();

                // Lower all return expressions to places
                for expr in exprs {
                    let (expr_nodes, expr_place) = self.lower_expr_to_place(expr)?;
                    nodes.extend(expr_nodes);
                    return_places.push(expr_place);
                }

                nodes.push(HirNode {
                    kind: HirKind::Return(return_places),
                    location: node.location,
                    scope: node.scope,
                    id: self.next_id(),
                });
                Ok(nodes)
            }

            // === Function Definitions ===
            NodeKind::Function(name, signature, body) => {
                let body = self.lower_block(body)?;

                Ok(vec![HirNode {
                    kind: HirKind::FunctionDef {
                        name,
                        signature,
                        body,
                    },
                    location: node.location,
                    scope: node.scope,
                    id: self.next_id(),
                }])
            }

            // === Struct Definitions ===
            NodeKind::StructDefinition(name, fields) => Ok(vec![HirNode {
                kind: HirKind::StructDef { name, fields },
                location: node.location,
                scope: node.scope,
                id: self.next_id(),
            }]),

            // === Expression as Statement ===
            NodeKind::Rvalue(expr) => {
                let (expr_nodes, expr_place) = self.lower_expr_to_place(expr)?;
                let mut nodes = expr_nodes;
                nodes.push(HirNode {
                    kind: HirKind::ExprStmt(expr_place),
                    location: node.location,
                    scope: node.scope,
                    id: self.next_id(),
                });
                Ok(nodes)
            }

            // === Other nodes ===
            _ => {
                return_compiler_error!(
                    "Unsupported AST node in HIR lowering: {:?}",
                    node.kind; {
                        CompilationStage => "HIR Generation",
                        PrimarySuggestion => "This is a compiler bug"
                    }
                )
            }
        }
    }

    /// Helper: lower a block of nodes
    fn lower_block(&mut self, nodes: Vec<AstNode>) -> Result<Vec<HirNode>, CompilerError> {
        let mut hir_nodes = Vec::new();
        for node in nodes {
            let mut node_hir = self.lower_node(node)?;
            hir_nodes.append(&mut node_hir);
        }
        Ok(hir_nodes)
    }

    /// Lower an expression to a place, introducing temporaries as needed
    ///
    /// Returns a list of HIR nodes that compute the expression and the final place
    /// where the result is stored. This linearizes nested expressions.
    fn lower_expr_to_place(
        &mut self,
        expr: Expression,
    ) -> Result<(Vec<HirNode>, Place), CompilerError> {
        match expr.kind {
            // === Literals ===
            ExpressionKind::Int(n) => {
                let temp = self.next_temp();
                let temp_place = Place::local(temp);
                let literal_expr = HirExpr {
                    kind: HirExprKind::Int(n),
                    data_type: expr.data_type,
                    location: expr.location.clone(),
                };
                let assign_node = self.create_assign_node_with_expr(
                    temp_place.clone(),
                    literal_expr,
                    expr.location,
                    self.current_scope.clone(),
                );
                Ok((vec![assign_node], temp_place))
            }

            ExpressionKind::Float(f) => {
                let temp = self.next_temp();
                let temp_place = Place::local(temp);
                let literal_expr = HirExpr {
                    kind: HirExprKind::Float(f),
                    data_type: expr.data_type,
                    location: expr.location.clone(),
                };
                let assign_node = self.create_assign_node_with_expr(
                    temp_place.clone(),
                    literal_expr,
                    expr.location,
                    self.current_scope.clone(),
                );
                Ok((vec![assign_node], temp_place))
            }

            ExpressionKind::Bool(b) => {
                let temp = self.next_temp();
                let temp_place = Place::local(temp);
                let literal_expr = HirExpr {
                    kind: HirExprKind::Bool(b),
                    data_type: expr.data_type,
                    location: expr.location.clone(),
                };
                let assign_node = self.create_assign_node_with_expr(
                    temp_place.clone(),
                    literal_expr,
                    expr.location,
                    self.current_scope.clone(),
                );
                Ok((vec![assign_node], temp_place))
            }

            ExpressionKind::StringSlice(s) => {
                let temp = self.next_temp();
                let temp_place = Place::local(temp);
                let literal_expr = HirExpr {
                    kind: HirExprKind::StringLiteral(s),
                    data_type: expr.data_type,
                    location: expr.location.clone(),
                };
                let assign_node = self.create_assign_node_with_expr(
                    temp_place.clone(),
                    literal_expr,
                    expr.location,
                    self.current_scope.clone(),
                );
                Ok((vec![assign_node], temp_place))
            }

            // === Variable References ===
            ExpressionKind::Reference(name) => {
                // Direct reference to existing place
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

            _ => {
                return_compiler_error!(
                    "Unsupported expression kind in HIR lowering: {:?}",
                    expr.kind; {
                        CompilationStage => "HIR Generation"
                    }
                )
            }
        }
    }

    /// Lower RPN expression sequence to a place
    fn lower_rpn_to_place(
        &mut self,
        rpn: Vec<AstNode>,
        result_type: DataType,
        location: crate::compiler::parsers::tokenizer::tokens::TextLocation,
    ) -> Result<(Vec<HirNode>, Place), CompilerError> {
        let mut nodes = Vec::new();
        let mut stack: Vec<Place> = Vec::new();

        for node in rpn {
            match node.kind {
                // Push operands (expressions) onto stack
                NodeKind::Rvalue(expr) => {
                    let (expr_nodes, expr_place) = self.lower_expr_to_place(expr)?;
                    nodes.extend(expr_nodes);
                    stack.push(expr_place);
                }

                // Pop operands, apply operator, push result
                NodeKind::Operator(op) => {
                    let right = stack.pop().ok_or_else(|| {
                        use crate::compiler::compiler_messages::compiler_errors::{
                            ErrorLocation, ErrorType,
                        };
                        use crate::compiler::parsers::tokenizer::tokens::CharPosition;
                        use std::path::PathBuf;

                        let error_location = ErrorLocation::new(
                            PathBuf::new(),
                            CharPosition {
                                line_number: 0,
                                char_column: 0,
                            },
                            CharPosition {
                                line_number: 0,
                                char_column: 0,
                            },
                        );
                        CompilerError::new(
                            "RPN stack underflow (right operand)",
                            error_location,
                            ErrorType::Compiler,
                        )
                    })?;

                    let left = stack.pop().ok_or_else(|| {
                        use crate::compiler::compiler_messages::compiler_errors::{
                            ErrorLocation, ErrorType,
                        };
                        use crate::compiler::parsers::tokenizer::tokens::CharPosition;
                        use std::path::PathBuf;

                        let error_location = ErrorLocation::new(
                            PathBuf::new(),
                            CharPosition {
                                line_number: 0,
                                char_column: 0,
                            },
                            CharPosition {
                                line_number: 0,
                                char_column: 0,
                            },
                        );
                        CompilerError::new(
                            "RPN stack underflow (left operand)",
                            error_location,
                            ErrorType::Compiler,
                        )
                    })?;

                    let bin_op = self.convert_operator(op)?;
                    let temp = self.next_temp();
                    let temp_place = Place::local(temp);

                    // Determine result type based on operator
                    let op_result_type = match bin_op {
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

                    let binop_expr = HirExpr {
                        kind: HirExprKind::BinOp {
                            left,
                            op: bin_op,
                            right,
                        },
                        data_type: op_result_type,
                        location: node.location,
                    };

                    let assign_node = self.create_assign_node_with_expr(
                        temp_place.clone(),
                        binop_expr,
                        location.clone(),
                        self.current_scope.clone(),
                    );
                    nodes.push(assign_node);
                    stack.push(temp_place);
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
    fn convert_operator(&self, op: Operator) -> Result<BinOp, CompilerError> {
        let bin_op = match op {
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
        Ok(bin_op)
    }

    /// Create an assignment node from place to place
    fn create_assign_node(
        &mut self,
        target: Place,
        source: Place,
        location: crate::compiler::parsers::tokenizer::tokens::TextLocation,
        scope: InternedPath,
    ) -> HirNode {
        let load_expr = HirExpr {
            kind: HirExprKind::Load(source),
            data_type: DataType::Inferred, // Type will be inferred
            location: location.clone(),
        };

        HirNode {
            kind: HirKind::Assign {
                place: target,
                value: load_expr,
            },
            location,
            scope,
            id: self.next_id(),
        }
    }

    /// Create an assignment node from expression
    fn create_assign_node_with_expr(
        &mut self,
        target: Place,
        expr: HirExpr,
        location: crate::compiler::parsers::tokenizer::tokens::TextLocation,
        scope: InternedPath,
    ) -> HirNode {
        HirNode {
            kind: HirKind::Assign {
                place: target,
                value: expr,
            },
            location,
            scope,
            id: self.next_id(),
        }
    }

    /// Helper: lower match arm
    fn lower_match_arm(&mut self, arm: MatchArm) -> Result<HirMatchArm, CompilerError> {
        // Lower the condition expression to create a pattern
        let pattern = self.lower_expr_to_pattern(arm.condition)?;

        // Lower the body
        let body = self.lower_block(arm.body)?;

        Ok(HirMatchArm {
            pattern,
            guard: None, // TODO: Add guard support when needed
            body,
        })
    }

    /// Helper: convert expression to HIR pattern
    fn lower_expr_to_pattern(
        &mut self,
        expr: Expression,
    ) -> Result<crate::compiler::hir::nodes::HirPattern, CompilerError> {
        use crate::compiler::hir::nodes::HirPattern;

        match expr.kind {
            // Literal patterns
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

            // Range patterns
            ExpressionKind::Range(start_expr, end_expr) => {
                let start_hir = self.lower_expr_to_hir_expr(*start_expr)?;
                let end_hir = self.lower_expr_to_hir_expr(*end_expr)?;
                Ok(HirPattern::Range {
                    start: start_hir,
                    end: end_hir,
                })
            }

            _ => {
                // For now, treat everything else as wildcard
                // TODO: Add more pattern types as needed
                Ok(HirPattern::Wildcard)
            }
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

            _ => {
                return_compiler_error!(
                    "Unsupported expression in pattern context: {:?}",
                    expr.kind; {
                        CompilationStage => "HIR Generation"
                    }
                )
            }
        }
    }

    /// Helper: convert AST node to Place (for assignment targets)
    fn lower_ast_node_to_place(&mut self, node: AstNode) -> Result<Place, CompilerError> {
        match node.kind {
            NodeKind::Rvalue(expr) => {
                Ok(self.lower_expr_to_place(expr)?.1)
            },

            NodeKind::FieldAccess { base, field, .. } => {
                let base_place = self.lower_ast_node_to_place(*base)?;
                Ok(base_place.field(field))
            }

            // TODO: Add support for index access when it's implemented in the AST
            // For now, indexing might be handled through method calls like .get() and .set()
            _ => {
                return_compiler_error!(
                    "Invalid assignment target: {:?}",
                    node.kind; {
                        CompilationStage => "HIR Generation",
                        PrimarySuggestion => "Only variables and fields can be assigned to"
                    }
                )
            }
        }
    }
}
