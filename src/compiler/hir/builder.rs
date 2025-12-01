//! HIR builder
//!
//! Converts AST into a structured HIR representation.
//! Expression lowering is handled in the lower_expression module.

use crate::compiler::compiler_errors::CompilerError;
use crate::compiler::compiler_messages::compiler_errors::CompilerMessages;
use crate::compiler::datatypes::DataType;
use crate::compiler::hir::lower_expression::{lower_expr, lower_expr_as_candidate_move};
use crate::compiler::hir::nodes::{HirKind, HirMatchArm, HirNode, HirNodeId};
use crate::compiler::hir::place::Place;
use crate::compiler::interned_path::InternedPath;
use crate::compiler::parsers::ast_nodes::{AstNode, NodeKind};
use crate::compiler::parsers::statements::branching::MatchArm;
use crate::compiler::string_interning::{InternedString, StringTable};
use crate::return_compiler_error;
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
    pub(crate) fn lower_node(&mut self, node: AstNode) -> Result<HirNode, CompilerError> {
        let id = self.next_id();
        let location = node.location.clone();
        let scope = node.scope.clone();

        let kind = match node.kind {
            // === Variable Declaration ===
            NodeKind::VariableDeclaration(arg) => {
                self.local_bindings
                    .insert(arg.id.clone(), arg.value.data_type.clone());

                let place = Place::Local(arg.id);
                let value = lower_expr(arg.value, self.string_table)?;

                HirKind::Let { place, value }
            }

            // === Mutation ===
            NodeKind::Mutation(name, expr, is_mutable) => {
                let place = Place::Local(name);
                let value = if is_mutable {
                    // Mutable assignment: could be move or mutable borrow
                    lower_expr_as_candidate_move(expr, self.string_table)?
                } else {
                    // Regular assignment: immutable borrow
                    lower_expr(expr, self.string_table)?
                };

                HirKind::Store { place, value }
            }

            // === Control Flow ===
            NodeKind::If(cond, then_block, else_block) => {
                let condition = lower_expr(cond, self.string_table)?;
                let then_block = self.lower_block(then_block)?;
                let else_block = else_block.map(|b| self.lower_block(b)).transpose()?;

                HirKind::If {
                    condition,
                    then_block,
                    else_block,
                }
            }

            NodeKind::Match(subject, arms, default) => {
                let scrutinee = lower_expr(subject, self.string_table)?;
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
                let iterator = lower_expr(collection, self.string_table)?;
                let body = self.lower_block(body)?;

                HirKind::Loop {
                    binding,
                    iterator,
                    body,
                    index_binding: None, // TODO: handle index binding
                }
            }

            // === Function Calls ===
            NodeKind::FunctionCall(name, args, returns, _location) => {
                let args = args
                    .into_iter()
                    .map(|e| lower_expr(e, self.string_table))
                    .collect::<Result<Vec<_>, _>>()?;

                let returns = returns.into_iter().map(|arg| arg.value.data_type).collect();

                HirKind::Call {
                    target: name,
                    args,
                    returns,
                }
            }

            NodeKind::HostFunctionCall(name, args, returns, module, import, _location) => {
                let args = args
                    .into_iter()
                    .map(|e| lower_expr(e, self.string_table))
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
                    .map(|e| lower_expr(e, self.string_table))
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
                let hir_expr = lower_expr(expr, self.string_table)?;
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

    /// Helper: lower a block of nodes
    fn lower_block(&mut self, nodes: Vec<AstNode>) -> Result<Vec<HirNode>, CompilerError> {
        nodes.into_iter().map(|n| self.lower_node(n)).collect()
    }

    /// Helper: lower match arm
    fn lower_match_arm(&mut self, _arm: MatchArm) -> Result<HirMatchArm, CompilerError> {
        // TODO: Implement based on your MatchArm structure
        unimplemented!("Match arm lowering")
    }
}
