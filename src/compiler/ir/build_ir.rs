use crate::compiler::compiler_errors::CompileError;
use crate::compiler::compiler_errors::ErrorType;
use crate::compiler::ir::ir_nodes::{IR, IRNode};
use crate::compiler::parsers::ast_nodes::{AstNode, NodeKind};
use crate::compiler::parsers::build_ast::AstBlock;
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
use crate::compiler::parsers::tokens::TextLocation;
use crate::return_compiler_error;

pub fn ast_to_ir(ast: AstBlock) -> IR {
    IR::new()
}

impl Expression {
    pub fn expr_to_ir(&self) -> Vec<IRNode> {
        match &self.kind {
            // Constants
            ExpressionKind::Int(value) => vec![IRNode::IntConst(*value)],
            ExpressionKind::Bool(value) => vec![IRNode::BoolConst(*value as i32)],
            ExpressionKind::Float(value) => vec![IRNode::FloatConst(*value)],

            // Runtime
            ExpressionKind::Runtime(nodes) => nodes.to_owned(),
            _ => vec![],
        }
    }
}
impl AstNode {
    pub fn to_ir(&self) -> Result<Vec<IRNode>, CompileError> {
        match &self.kind {
            NodeKind::Reference(value, ..)
            | NodeKind::Declaration(_, value, ..)
            | NodeKind::Expression(value, ..) => Ok(value.expr_to_ir()),
            _ => {
                return_compiler_error!("Compiler can't turn this node into IR yet: {:?}", self.kind)
            }
        }
    }
}
