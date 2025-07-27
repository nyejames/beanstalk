use crate::compiler::compiler_errors::CompileError;
use crate::compiler::compiler_errors::ErrorType;
use crate::compiler::datatypes::DataType;
use crate::compiler::parsers::build_ast::AstBlock;
use crate::compiler::parsers::expressions::expression::{Expression, Operator};
use crate::compiler::parsers::tokens::{TextLocation, VarVisibility};
use crate::return_compiler_error;
use std::path::PathBuf;

#[derive(Debug, PartialEq, Clone)]
pub struct Arg {
    pub name: String,      // Optional Name of the argument (empty string if unnamed)
    pub value: Expression, // Optional Value of the argument - 'None' if no value
}

#[derive(Debug, Clone, PartialEq)]
pub struct AstNode {
    pub kind: NodeKind,
    pub location: TextLocation,
    pub scope: PathBuf,
}

#[derive(Debug, Clone, PartialEq)]
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

    // Control Flow
    Access,
    Return(Vec<Expression>),  // Return value,
    If(Expression, AstBlock), // Condition, If true
    Else(AstBlock),           // Body,

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

    Comment(String),

    // Variable names should be the full namespace (module path + variable name)
    Declaration(String, Expression, VarVisibility), // Variable name, Value, Visibility, Type,

    // Built-in Functions (Would probably be standard lib in other languages)
    // Print can accept multiple arguments and will coerce them to strings
    Print(Expression), // Value,

    // Not even sure if this is needed
    JSStringReference(String), // Variable name,

    // Other language code blocks
    JS(String),  // Code,
    Css(String), // Code,
    // Wasm(String, TokenPosition), // Code,

    // Literals
    Reference(Expression),  // Token,
    Expression(Expression), // Token,

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
    pub fn get_type(&self) -> DataType {
        match &self.kind {
            NodeKind::Reference(value) => value.data_type.to_owned(),
            NodeKind::Empty => DataType::None,
            NodeKind::Declaration(_, expr, ..) => expr.data_type.to_owned(),

            _ => {
                debug_assert!(
                    true,
                    "Shouldn't be here. get_type should only be called on valid nodes. Datatype return not implemented for: {:?}",
                    self.kind
                );

                DataType::Inferred(false)
            }
        }
    }

    pub fn get_expr(&self) -> Result<Expression, CompileError> {
        match &self.kind {
            NodeKind::Reference(value, ..) | 
            NodeKind::Declaration(_, value, ..) | 
            NodeKind::Expression(value, ..) => {
                Ok(value.to_owned())
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
