use crate::compiler::compiler_errors::CompileError;
use crate::compiler::parsers::ast_nodes::{AstNode, NodeKind};
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
use crate::compiler::parsers::template::TemplateContent;
use std::collections::HashMap;

/// Use counter for tracking variable and field/index uses in AST
#[derive(Debug)]
pub struct UseCounter {
    /// Variable use counts (simple variable names)
    variable_counts: HashMap<String, usize>,
    /// Field access counts (variable.field)
    field_access_counts: HashMap<String, usize>,
    /// Index access counts (variable[index])
    index_access_counts: HashMap<String, usize>,
}

impl UseCounter {
    /// Create a new use counter
    pub(crate) fn new() -> Self {
        Self {
            variable_counts: HashMap::new(),
            field_access_counts: HashMap::new(),
            index_access_counts: HashMap::new(),
        }
    }

    /// Count uses in a single AST node
    pub(crate) fn count_node_uses(&mut self, node: &AstNode) -> Result<(), CompileError> {
        match &node.kind {
            NodeKind::Declaration(_, expression, _) => {
                self.count_expression_uses(expression)?;
            }
            NodeKind::Expression(expression) => {
                self.count_expression_uses(expression)?;
            }
            NodeKind::FunctionCall(_, args, _, _) => {
                for arg in args {
                    self.count_expression_uses(arg)?;
                }
            }
            NodeKind::Print(expression) => {
                self.count_expression_uses(expression)?;
            }
            NodeKind::Return(expressions) => {
                for expr in expressions {
                    self.count_expression_uses(expr)?;
                }
            }
            NodeKind::If(condition, then_block, else_block) => {
                self.count_expression_uses(condition)?;
                self.count_block_uses(&then_block.ast)?;
                if let Some(else_block) = else_block {
                    self.count_block_uses(&else_block.ast)?;
                }
            }
            NodeKind::Match(subject, arms, default_arm) => {
                self.count_expression_uses(subject)?;
                for (pattern, block) in arms {
                    self.count_expression_uses(pattern)?;
                    self.count_block_uses(&block.ast)?;
                }
                if let Some(default_block) = default_arm {
                    self.count_block_uses(&default_block.ast)?;
                }
            }
            NodeKind::ForLoop(arg, collection, body) => {
                self.count_expression_uses(&arg.value)?;
                self.count_expression_uses(collection)?;
                self.count_block_uses(&body.ast)?;
            }
            NodeKind::WhileLoop(condition, body) => {
                self.count_expression_uses(condition)?;
                self.count_block_uses(&body.ast)?;
            }
            _ => {
                // Other node types don't contain variable uses
            }
        }
        Ok(())
    }

    /// Count uses in a block of AST nodes
    fn count_block_uses(&mut self, nodes: &[AstNode]) -> Result<(), CompileError> {
        for node in nodes {
            self.count_node_uses(node)?;
        }
        Ok(())
    }

    /// Count uses in an expression
    fn count_expression_uses(&mut self, expression: &Expression) -> Result<(), CompileError> {
        match &expression.kind {
            ExpressionKind::Reference(var_name) => {
                // Simple variable reference
                *self.variable_counts.entry(var_name.clone()).or_insert(0) += 1;
            }
            ExpressionKind::Runtime(runtime_nodes) => {
                // Count uses in runtime expression nodes
                for runtime_node in runtime_nodes {
                    self.count_node_uses(runtime_node)?;
                }
            }
            ExpressionKind::Function(args, body, _) => {
                // Count uses in function arguments
                for arg in args {
                    self.count_expression_uses(&arg.value)?;
                }
                // Count uses in function body
                self.count_block_uses(body)?;
            }
            ExpressionKind::Collection(items) => {
                // Count uses in collection items
                for item in items {
                    self.count_expression_uses(item)?;
                }
            }
            ExpressionKind::Struct(args) => {
                // Count uses in struct field values
                for arg in args {
                    self.count_expression_uses(&arg.value)?;
                }
            }
            ExpressionKind::Template(content, _, _) => {
                // Count uses in template content
                self.count_template_uses(content)?;
            }
            _ => {
                // Other expression types (literals) don't contain variable references
            }
        }
        Ok(())
    }

    /// Count uses in template content
    fn count_template_uses(&mut self, _content: &TemplateContent) -> Result<(), CompileError> {
        // Template use counting would be implemented here
        // For now, we'll skip this as it's complex and not critical for the basic implementation
        Ok(())
    }

    /// Get all use counts combined
    pub(crate) fn get_use_counts(&self) -> HashMap<String, usize> {
        let mut combined = self.variable_counts.clone();

        // Add field access counts
        for (key, count) in &self.field_access_counts {
            *combined.entry(key.clone()).or_insert(0) += count;
        }

        // Add index access counts
        for (key, count) in &self.index_access_counts {
            *combined.entry(key.clone()).or_insert(0) += count;
        }

        combined
    }
}
