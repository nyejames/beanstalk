use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::return_hir_transformation_error;

use super::RuntimeRpnTree;

impl<'a> HirBuilder<'a> {
    pub(super) fn build_runtime_rpn_tree(
        &self,
        nodes: &[AstNode],
        location: &SourceLocation,
    ) -> Result<RuntimeRpnTree, CompilerError> {
        let mut stack: Vec<RuntimeRpnTree> = Vec::with_capacity(nodes.len());

        for node in nodes {
            match &node.kind {
                NodeKind::Operator(op) => match op.required_values() {
                    1 => {
                        let Some(operand) = stack.pop() else {
                            return_hir_transformation_error!(
                                format!("RPN stack underflow for unary operator {:?}", op),
                                self.hir_error_location(&node.location)
                            );
                        };

                        stack.push(RuntimeRpnTree::Unary {
                            op: op.to_owned(),
                            operand: Box::new(operand),
                            location: node.location.clone(),
                        });
                    }
                    2 => {
                        let Some(right) = stack.pop() else {
                            return_hir_transformation_error!(
                                format!("RPN stack underflow for operator {:?} (missing rhs)", op),
                                self.hir_error_location(&node.location)
                            );
                        };
                        let Some(left) = stack.pop() else {
                            return_hir_transformation_error!(
                                format!("RPN stack underflow for operator {:?} (missing lhs)", op),
                                self.hir_error_location(&node.location)
                            );
                        };

                        stack.push(RuntimeRpnTree::Binary {
                            left: Box::new(left),
                            op: op.to_owned(),
                            right: Box::new(right),
                            location: node.location.clone(),
                        });
                    }
                    _ => {
                        return_hir_transformation_error!(
                            format!("Unsupported operator arity for {:?}", op),
                            self.hir_error_location(&node.location)
                        );
                    }
                },
                _ => stack.push(RuntimeRpnTree::Leaf(Box::new(node.to_owned()))),
            }
        }

        if stack.len() != 1 {
            return_hir_transformation_error!(
                format!(
                    "Malformed runtime RPN expression: expected one value on stack, got {}",
                    stack.len()
                ),
                self.hir_error_location(location)
            );
        }

        Ok(stack
            .pop()
            .expect("validated runtime RPN expression should leave exactly one tree node"))
    }
}
