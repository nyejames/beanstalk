use crate::compiler::compiler_errors::CompileError;
use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::parsers::build_ast::AstBlock;
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind, Operator};
use crate::compiler::parsers::statements::branching::MatchArm;
use crate::compiler::parsers::tokens::{TextLocation, VarVisibility};
use crate::{return_compiler_error, return_type_error};
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
        Expression,       // Subject (condition)
        Vec<MatchArm>,    // Arms
        Option<AstBlock>, // for the wildcard/else case
    ),

    ForLoop(Box<Arg>, Expression, AstBlock), // Item, Collection, Body,
    WhileLoop(Expression, AstBlock),         // Condition, Body,

    // Basics
    FunctionCall(
        String,
        Vec<Expression>, // Arguments passed in (eventually will have a syntax for named arguments)
        Vec<Arg>,        // Returns (can be named so using an Arg rather than just the data type)
        TextLocation,
        // bool, // Function is pure
    ),

    // Host function call (functions provided by the runtime)
    // Uses the same structure as regular function calls but includes binding info
    HostFunctionCall(
        String,          // Function name
        Vec<Expression>, // Arguments passed in - Can't be named for host functions
        Vec<DataType>,   // Return types
        String,          // WASM module name (e.g., "beanstalk_io")
        String,          // WASM import name (e.g., "print")
        TextLocation,    // Location for error reporting
    ),

    // Variable names should be the full namespace (module path + variable name)
    VariableDeclaration(String, Expression, VarVisibility), // Variable name, Value, Visibility,

    StructDefinition(String, Vec<Arg>),

    // Mutation of existing mutable variables
    Mutation(String, Expression, bool), // Variable name, New value, Is mutable assignment (~=)

    // An actual r-value
    // Expression(Expression),

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
            NodeKind::VariableDeclaration(_, value, ..)
            // | NodeKind::Expression(value, ..)
            | NodeKind::Mutation(_, value, _) => Ok(value.to_owned()),
            NodeKind::FunctionCall(name, arguments, returns, location) => {
                Ok(Expression::function_call(
                    name.to_owned(),
                    arguments.to_owned(),
                    returns.to_owned(),
                    location.to_owned(),
                ))
            }
            NodeKind::HostFunctionCall(name, arguments, return_types, _, _, location) => {
                let mut return_types_from_expr = Vec::new();
                for (idx, _return_type) in return_types.iter().enumerate() {
                    return_types_from_expr.push(Arg {
                        name: idx.to_string(),
                        value: Expression::int(0, location.to_owned(), Ownership::default()),
                    })
                }

                Ok(Expression::function_call(
                    name.to_owned(),
                    arguments.to_owned(),
                    return_types_from_expr,
                    location.to_owned(),
                ))
            }
            _ => return_compiler_error!(
                "Compiler tried to get the expression of a node that cannot contain expressions: {:?}",
                self.kind
            ),
        }
    }

    // If this is a boolean value, flip it to the opposite value
    pub fn flip(&mut self) -> Result<bool, CompileError> {
        if let NodeKind::Expression(value) = &mut self.kind {
            match value.kind {
                ExpressionKind::Bool(val) => {
                    value.kind = ExpressionKind::Bool(!val);
                    return Ok(true);
                }
                ExpressionKind::Runtime(_) => {
                    if !value.ownership.is_mutable() {
                        return_type_error!(
                            self.location.to_owned(),
                            "Tried to use the 'not' operator on a non-mutable value"
                        )
                    } else {
                        return_type_error!(
                            self.location.to_owned(),
                            "Tried to use the 'not' operator on value of type {:?}",
                            value.data_type
                        )
                    }

                    return Ok(true);
                }
                _ => {}
            }
        }

        return_type_error!(
            self.location.to_owned(),
            "Tried to use the 'not' operator on a non-boolean node: {:?}",
            self.kind
        );
    }

    pub fn get_precedence(&self) -> u32 {
        match &self.kind {
            NodeKind::Operator(op) => match op {
                // Special Operators with the highest precedence
                Operator::Range => 6,
                Operator::Not => 6,

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
