//! Expression evaluation and AST-side constant folding.
//!
//! WHAT: resolves parsed infix expression fragments into typed AST expressions.
//! WHY: AST is the semantic boundary that owns operator typing, result handling checks, and the
//! final decision about whether an expression can collapse at compile time or must survive to HIR.

mod evaluator;
mod operator_policy;
mod ordering;
mod result_type;

#[cfg(test)]
pub use evaluator::concat_template;
pub use evaluator::evaluate_expression;

#[cfg(test)]
pub(crate) use crate::compiler_frontend::ast::expressions::expression::Expression;
#[cfg(test)]
pub(crate) use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
#[cfg(test)]
pub(crate) use crate::compiler_frontend::value_mode::ValueMode;

#[cfg(test)]
#[path = "../tests/eval_expression_tests.rs"]
mod eval_expression_tests;
