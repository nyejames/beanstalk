//! HIR builder (scaffold)
//!
//! Converts AST into a structured HIR representation. For now, this is a
//! minimal placeholder that returns an empty module, so the pipeline can be
//! wired up incrementally.

use crate::compiler::compiler_errors::CompilerError;
use crate::compiler::compiler_messages::compiler_errors::CompilerMessages;
use crate::compiler::datatypes::DataType;
use crate::compiler::hir::nodes::{
    BinOp, HirExpr, HirExprKind, HirKind, HirMatchArm, HirModule, HirNode, HirNodeId,
};
use crate::compiler::hir::place::Place;
use crate::compiler::interned_path::InternedPath;
use crate::compiler::parsers::ast_nodes::{AstNode, NodeKind};
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind, Operator};
use crate::compiler::parsers::statements::branching::MatchArm;
use crate::compiler::parsers::statements::create_template_node::Template;
use crate::compiler::string_interning::{InternedString, StringTable};
use crate::{hir_log, return_compiler_error};
use std::collections::HashMap;

/// Build a HIR module from the AST.
pub struct HirBuilder<'a> {
    // State tracking during lowering
    current_scope: InternedPath,
    next_node_id: usize,

    // Track local bindings to build Place references
    local_bindings: HashMap<InternedString, DataType>,

    // For generating unique names for runtime template functions
    template_counter: usize,

    // Error collection
    messages: CompilerMessages,

    string_table: &'a mut StringTable,
}

impl<'a> HirBuilder<'a> {
    pub fn new(scope: InternedPath, string_table: &'a mut StringTable) -> Self {
        Self {
            current_scope: scope,
            next_node_id: 0,
            local_bindings: HashMap::new(),
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
                Ok(hir) => hir_nodes.push(hir),
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

    /// Lower a single AST node to HIR
    fn lower_node(&mut self, node: AstNode) -> Result<HirNode, CompilerError> {
        let id = self.next_id();
        let location = node.location.clone();
        let scope = node.scope.clone();

        let kind = match node.kind {
            // === Variable Declaration ===
            NodeKind::VariableDeclaration(arg) => {
                self.local_bindings
                    .insert(arg.id.clone(), arg.value.data_type.clone());

                let place = Place::Local(arg.id);
                let value = self.lower_expr(arg.value)?;

                HirKind::Let { place, value }
            }

            // === Mutation ===
            NodeKind::Mutation(name, expr, is_mutable) => {
                let place = Place::Local(name);
                let value = if is_mutable {
                    // Mutable assignment: could be move or mutable borrow
                    self.lower_expr_as_candidate_move(expr)?
                } else {
                    // Regular assignment: immutable borrow
                    self.lower_expr(expr)?
                };

                HirKind::Store { place, value }
            }

            // === Control Flow ===
            NodeKind::If(cond, then_block, else_block) => {
                let condition = self.lower_expr(cond)?;
                let then_block = self.lower_block(then_block)?;
                let else_block = else_block.map(|b| self.lower_block(b)).transpose()?;

                HirKind::If {
                    condition,
                    then_block,
                    else_block,
                }
            }

            NodeKind::Match(subject, arms, default) => {
                let scrutinee = self.lower_expr(subject)?;
                let arms = arms
                    .into_iter()
                    .map(|arm| self.lower_match_arm(arm))
                    .collect::<Result<Vec<_>, _>>()?;
                let default = default.map(|b| self.lower_block(b)).transpose()?;

                HirKind::Match {
                    scrutinee,
                    arms,
                    default,
                }
            }

            NodeKind::ForLoop(item_arg, collection, body) => {
                let binding = Some((item_arg.id, item_arg.value.data_type));
                let iterator = self.lower_expr(collection)?;
                let body = self.lower_block(body)?;

                HirKind::Loop {
                    binding,
                    iterator,
                    body,
                    index_binding: None, // TODO: handle index binding
                }
            }

            // === Function Calls ===
            NodeKind::FunctionCall(name, args, returns, location) => {
                let args = args
                    .into_iter()
                    .map(|e| self.lower_expr(e))
                    .collect::<Result<Vec<_>, _>>()?;

                let returns = returns.into_iter().map(|arg| arg.value.data_type).collect();

                HirKind::Call {
                    target: name,
                    args,
                    returns,
                }
            }

            NodeKind::HostFunctionCall(name, args, returns, module, import, location) => {
                let args = args
                    .into_iter()
                    .map(|e| self.lower_expr(e))
                    .collect::<Result<Vec<_>, _>>()?;

                HirKind::HostCall {
                    target: name,
                    module,
                    import,
                    args,
                    returns,
                }
            }

            // === Returns ===
            NodeKind::Return(exprs) => {
                let exprs = exprs
                    .into_iter()
                    .map(|e| self.lower_expr(e))
                    .collect::<Result<Vec<_>, _>>()?;

                HirKind::Return(exprs)
            }

            // === Function Definitions ===
            NodeKind::Function(name, signature, body) => {
                let body = self.lower_block(body)?;

                HirKind::FunctionDef {
                    name,
                    signature,
                    body,
                }
            }

            // === Struct Definitions ===
            NodeKind::StructDefinition(name, fields) => HirKind::StructDef { name, fields },

            // === Expression as Statement ===
            NodeKind::Expression(expr) => {
                let hir_expr = self.lower_expr(expr)?;
                HirKind::Expr(hir_expr)
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
        };

        Ok(HirNode {
            kind,
            location,
            scope,
            id,
        })
    }

    /// Lower an expression to HIR
    fn lower_expr(&mut self, expr: Expression) -> Result<HirExpr, CompilerError> {
        let location = expr.location.clone();
        let data_type = expr.data_type.clone();

        let kind = match expr.kind {
            // === Literals (already folded at AST stage) ===
            ExpressionKind::Int(n) => HirExprKind::Int(n),
            ExpressionKind::Float(f) => HirExprKind::Float(f),
            ExpressionKind::Bool(b) => HirExprKind::Bool(b),
            ExpressionKind::StringSlice(s) => HirExprKind::StringLiteral(s),

            // === Variable References ===
            ExpressionKind::Reference(name) => {
                // Default: immutable load
                let place = Place::Local(name);
                HirExprKind::Load(place)
            }

            // === Runtime Expressions (RPN from AST) ===
            ExpressionKind::Runtime(rpn_nodes) => {
                // Convert RPN sequence to expression tree
                self.lower_rpn_to_expr(rpn_nodes)?
            }

            // === Function Calls ===
            ExpressionKind::FunctionCall(name, args) => {
                let args = args
                    .into_iter()
                    .map(|e| self.lower_expr(e))
                    .collect::<Result<Vec<_>, _>>()?;

                HirExprKind::Call { target: name, args }
            }

            // === Templates ===
            ExpressionKind::Template(template) => {
                // If the template can be folded, it's already a string
                // If not, create runtime template call
                self.lower_template(*template)?
            }

            // === Collections ===
            ExpressionKind::Collection(items) => {
                let items = items
                    .into_iter()
                    .map(|e| self.lower_expr(e))
                    .collect::<Result<Vec<_>, _>>()?;

                HirExprKind::Collection(items)
            }

            // === Struct Construction ===
            ExpressionKind::StructInstance(fields) => {
                let fields = fields
                    .into_iter()
                    .map(|arg| {
                        let value = self.lower_expr(arg.value)?;
                        Ok((arg.id, value))
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                HirExprKind::StructConstruct {
                    type_name: self.string_table.intern(""), // TODO: Get from context
                    fields,
                }
            }

            // === Range ===
            ExpressionKind::Range(start, end) => {
                let start = Box::new(self.lower_expr(*start)?);
                let end = Box::new(self.lower_expr(*end)?);

                HirExprKind::Range { start, end }
            }

            _ => {
                return_compiler_error!(
                    "Unsupported expression kind in HIR lowering: {:?}",
                    expr.kind; {
                        CompilationStage => "HIR Generation"
                    }
                )
            }
        };

        Ok(HirExpr {
            kind,
            data_type,
            location,
        })
    }

    /// Lower expression as candidate move (for mutable assignments)
    fn lower_expr_as_candidate_move(&mut self, expr: Expression) -> Result<HirExpr, CompilerError> {
        let location = expr.location.clone();
        let data_type = expr.data_type.clone();

        // If the expression is a simple reference, mark as candidate move
        if let ExpressionKind::Reference(name) = expr.kind {
            let place = Place::Local(name);
            return Ok(HirExpr {
                kind: HirExprKind::CandidateMove(place),
                data_type,
                location,
            });
        }

        // Otherwise, it's a mutable borrow of the expression result
        self.lower_expr(expr)
    }

    /// Convert RPN sequence to expression tree
    fn lower_rpn_to_expr(&mut self, rpn: Vec<AstNode>) -> Result<HirExprKind, CompilerError> {
        let mut stack: Vec<HirExpr> = Vec::new();

        for node in rpn {
            match node.kind {
                // Push operands onto stack
                NodeKind::Expression(expr) => {
                    stack.push(self.lower_expr(expr)?);
                }

                // Pop operands, apply operator, push result
                NodeKind::Operator(op) => {
                    let right: HirExpr = match stack.pop() {
                        Some(right) => right,
                        None => {
                            return_compiler_error!("RPN stack underflow (right operand)")
                        }
                    };

                    let left: HirExpr = match stack.pop() {
                        Some(left) => left,
                        None => {
                            return_compiler_error!("RPN stack underflow (left operand)")
                        }
                    };

                    let bin_op = self.convert_operator(op)?;
                    let result_type = self.infer_binop_type(&left, &right, bin_op)?;

                    stack.push(HirExpr {
                        kind: HirExprKind::BinOp {
                            left: Box::new(left),
                            op: bin_op,
                            right: Box::new(right),
                        },
                        data_type: result_type,
                        location: node.location,
                    });
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

        // Should have exactly one expression left
        if stack.len() != 1 {
            return_compiler_error!(
                "Invalid RPN sequence: stack size = {}",
                stack.len(); {
                    CompilationStage => "HIR Generation"
                }
            )
        }

        Ok(stack.pop().unwrap().kind)
    }

    /// Helper: lower a block of nodes
    fn lower_block(&mut self, nodes: Vec<AstNode>) -> Result<Vec<HirNode>, CompilerError> {
        nodes.into_iter().map(|n| self.lower_node(n)).collect()
    }

    /// Helper: lower match arm
    fn lower_match_arm(&mut self, arm: MatchArm) -> Result<HirMatchArm, CompilerError> {
        // TODO: Implement based on your MatchArm structure
        unimplemented!("Match arm lowering")
    }

    /// Helper: lower template (handle runtime templates)
    fn lower_template(&mut self, template: Template) -> Result<HirExprKind, CompilerError> {
        // If the template has runtime interpolations, create a runtime template call
        // Otherwise, it should already be folded to a string literal at the AST stage

        // TODO: Implement based on your Template structure
        unimplemented!("Template lowering")
    }

    /// Helper: convert AST operator to HIR BinOp
    fn convert_operator(&self, op: Operator) -> Result<BinOp, CompilerError> {
        // TODO: Map your Operator enum to BinOp
        unimplemented!("Operator conversion")
    }

    /// Helper: infer a result type of binary operation
    fn infer_binop_type(
        &self,
        left: &HirExpr,
        right: &HirExpr,
        op: BinOp,
    ) -> Result<DataType, CompilerError> {
        // Type inference logic
        unimplemented!("Type inference for binop")
    }
}
