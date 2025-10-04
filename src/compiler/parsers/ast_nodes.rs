use crate::compiler::compiler_errors::CompileError;
use crate::compiler::datatypes::DataType;
use crate::compiler::parsers::build_ast::AstBlock;
use crate::compiler::parsers::expressions::expression::{Expression, Operator};
use crate::compiler::parsers::tokens::{TextLocation, VarVisibility};
use crate::return_compiler_error;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Arg {
    pub name: String,      // Optional Name of the argument (empty string if unnamed)
    pub value: Expression, // Optional Value of the argument - 'None' if no value
}

#[derive(Debug, Clone)]
pub struct AstNode {
    pub kind: NodeKind,
    pub location: TextLocation,
    pub scope: PathBuf,
}

#[derive(Debug, Clone)]
pub enum NodeKind {
    // Warning Message
    // This could be stuff like unused variables, possible race conditions, etc
    Warning(String), // Message, Start pos, End pos

    // Config settings
    Config(Vec<Arg>), // Settings,

    // Named an import path for the module
    // Import(String, TokenPosition), // Path,

    // Path to a module that will automatically import all styles and templates
    // into the scope of the current module. Doesn't automatically import variables or functions into the scope
    Use(PathBuf), // Path,

    // Memory management (inserted by the compiler)
    Free(PathBuf),

    // Control Flow
    Access,
    Return(Vec<Expression>),                    // Return value,
    If(Expression, AstBlock, Option<AstBlock>), // Condition, If true, Else

    Match(
        Expression,                  // Subject (condition)
        Vec<(Expression, AstBlock)>, // Arms
        Option<AstBlock>,            // for the wildcard/else case
    ),

    ForLoop(Box<Arg>, Expression, AstBlock), // Item, Collection, Body,
    WhileLoop(Expression, AstBlock),         // Condition, Body,

    // Basics
    FunctionCall(
        String,
        Vec<Expression>, // Arguments passed in
        Vec<DataType>,
        TextLocation,
        // bool, // Function is pure
    ),

    // Variable names should be the full namespace (module path + variable name)
    Declaration(String, Expression, VarVisibility), // Variable name, Value, Visibility,

    // Mutation of existing mutable variables
    Mutation(String, Expression), // Variable name, New value

    // An actual value
    Expression(Expression),

    // Built-in Functions (Would probably be standard lib in other languages)
    // Print can accept multiple arguments and will coerce them to strings
    Print(Expression), // Value,

    // Other language code blocks
    JS(String),  // Code,
    Css(String), // Code,

    TemplateFormatter,
    Slot,
    Empty, //

    // Operators
    // Operator, Precedence
    Operator(Operator), // Operator,
    // UnaryOperator(Token, bool, TokenPosition), // Operator, is_postfix,
    Newline,
    Spaces(u32),
}

impl AstNode {
    pub fn get_expr(&self) -> Result<Expression, CompileError> {
        match &self.kind {
            NodeKind::Declaration(_, value, ..)
            | NodeKind::Expression(value, ..)
            | NodeKind::Mutation(_, value) => Ok(value.to_owned()),
            NodeKind::FunctionCall(_, _, return_types, location) => {
                let data_type = if return_types.len() == 1 {
                    return_types[0].to_owned()
                } else {
                    DataType::Choices(return_types.to_owned())
                };

                Ok(Expression::runtime(
                    vec![self.to_owned()],
                    data_type,
                    location.to_owned(),
                ))
            }
            _ => return_compiler_error!(
                "Compiler tried to get the expression of a node that cannot contain expressions: {:?}",
                self.kind
            ),
        }
    }

    pub fn get_precedence(&self) -> u32 {
        match &self.kind {
            NodeKind::Operator(op) => match op {
                // Special Operators with the highest precedence
                Operator::Range => 6,

                // Highest precedence: exponentiation
                Operator::Exponent => 5,
                Operator::Root => 5,

                // High precedence: multiplication, division, modulus
                Operator::Multiply => 4,
                Operator::Divide => 4,
                Operator::Modulus => 4,

                // Medium precedence: addition, subtraction
                Operator::Add => 3,
                Operator::Subtract => 3,

                // Comparisons
                Operator::LessThan => 2,
                Operator::LessThanOrEqual => 2,
                Operator::GreaterThan => 2,
                Operator::GreaterThanOrEqual => 2,
                Operator::Equality => 2,
                Operator::NotEqual => 2,

                // Logical AND
                Operator::And => 1,

                // Logical OR
                Operator::Or => 0,
            },
            _ => 0,
        }
    }

    pub fn is_left_associative(&self) -> bool {
        !matches!(self.kind, NodeKind::Operator(Operator::Exponent, ..))
    }
}
