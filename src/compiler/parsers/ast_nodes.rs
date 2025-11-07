use crate::compiler::compiler_errors::CompileError;
use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::interned_path::InternedPath;
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind, Operator};
use crate::compiler::parsers::statements::branching::MatchArm;
use crate::compiler::parsers::statements::functions::FunctionSignature;
use crate::compiler::parsers::tokenizer::tokens::TextLocation;
use crate::compiler::string_interning::{InternedString, StringId};
use crate::{return_common_type_error, return_compiler_error, return_type_error};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Arg {
    pub id: InternedString,
    pub value: Expression,
}

#[derive(Debug, Clone)]
pub struct AstNode {
    pub kind: NodeKind,
    pub location: TextLocation,
    pub scope: InternedPath,
}

#[derive(Debug, Clone)]
pub enum NodeKind {
    // Warning Message
    // This could be stuff like unused variables, possible race conditions, etc
    Warning(String), // Message, Start pos, End pos

    // Config settings
    Config(Vec<Arg>), // Settings,

    // Imports a function from the host environment
    // This could be another Wasm file or a native function provided by the runtime
    Import(PathBuf),

    // Generic import path to another Beanstalk file
    // Effectively means the file is included in the current file as a struct that can be accessed
    Include(InternedString, PathBuf), // Name of file import, Imported file object

    // Control Flow
    Access,
    Return(Vec<Expression>),                            // Return value,
    If(Expression, Vec<AstNode>, Option<Vec<AstNode>>), // Condition, If true, Else

    Match(
        Expression,           // Subject (condition)
        Vec<MatchArm>,        // Arms
        Option<Vec<AstNode>>, // for the wildcard/else case
    ),

    ForLoop(Box<Arg>, Expression, Vec<AstNode>), // Item, Collection, Body,
    WhileLoop(Expression, Vec<AstNode>),         // Condition, Body,

    // Basics
    FunctionCall(
        InternedString,
        Vec<Expression>, // Arguments passed in (eventually will have a syntax for named arguments)
        Vec<Arg>,        // Returns (can be named so using an Arg rather than just the data type)
        TextLocation,
        // bool, // Function is pure
    ),

    // Host function call (functions provided by the runtime)
    // Uses the same structure as regular function calls but includes binding info
    HostFunctionCall(
        InternedString,  // Function name
        Vec<Expression>, // Arguments passed in - Can't be named for host functions
        Vec<DataType>,   // Return types
        InternedString,  // WASM module name (e.g., "beanstalk_io")
        InternedString,  // WASM import name (e.g., "print")
        TextLocation,    // Location for error reporting
    ),

    // Variable names should be the full namespace (module path + variable name)
    VariableDeclaration(Arg), // Variable name, Value, Visibility,

    // example: new_struct_instance = MyStructDefinition(arg1, arg2)
    //          new_struct_instance(arg) -- Calls the main function of the struct
    StructDefinition(
        InternedString, // Name
        Vec<Arg>,       // Fields
    ),

    Function(InternedString, FunctionSignature, Vec<AstNode>),

    // Mutation of existing mutable variables
    Mutation(InternedString, Expression, bool), // Variable name, New value, Is mutable assignment (~=)

    // An actual r-value
    Expression(Expression),

    // Built-in, always expected host Functions.
    // Print can accept multiple arguments and will coerce them to strings.
    // This will eventually change to a Beanstalk import called "io"
    // that will have a default init function that prints to the standard output
    Print(Expression),

    // Other language code blocks
    JS(InternedString),  // Code,
    Css(InternedString), // Code,

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
            NodeKind::VariableDeclaration(arg) => Ok(arg.value.to_owned()),
            NodeKind::Expression(value, ..) | NodeKind::Mutation(_, value, _) => {
                Ok(value.to_owned())
            }
            NodeKind::FunctionCall(name, arguments, returns, location) => {
                Ok(Expression::function_call(
                    *name,
                    arguments.to_owned(),
                    returns.to_owned(),
                    location.to_owned(),
                ))
            }
            NodeKind::HostFunctionCall(name, arguments, return_types, _, _, location) => {
                // We need a string table to create interned strings for indices
                // For now, we'll use a placeholder approach - this will need to be updated
                // when the AST construction is updated to use the string table
                let mut return_types_from_expr = Vec::new();
                for (idx, _return_type) in return_types.iter().enumerate() {
                    // This is a temporary solution - we'll need to pass string_table here
                    // when updating AST construction methods
                    let placeholder_name = InternedString::from_u32(idx as u32);
                    return_types_from_expr.push(Arg {
                        id: placeholder_name,
                        value: Expression::int(0, location.to_owned(), Ownership::default()),
                    })
                }

                Ok(Expression::function_call(
                    *name,
                    arguments.to_owned(),
                    return_types_from_expr,
                    location.to_owned(),
                ))
            }
            // Compiler tried to get the expression of a node that cannot contain expressions
            _ => return_compiler_error!(
                PathBuf::from("src/compiler/parsers/ast_nodes.rs"),
                154
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
                        return_common_type_error!(
                            self.location.to_owned(),
                            "Tried to use the 'not' operator on a non-mutable value"
                        )
                    } else {
                        return_type_error!(
                            string_table,
                            self.location.to_owned(),
                            "Tried to use the 'not' operator on value of type {:?}",
                            value.data_type
                        )
                    }
                }
                _ => {}
            }
        }

        return_type_error!(
            string_table,
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
