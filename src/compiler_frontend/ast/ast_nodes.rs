use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::host_functions::registry::{HostAbiType, HostFunctionId};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, Operator,
};
use crate::compiler_frontend::ast::statements::branching::MatchArm;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::parsers::tokenizer::tokens::TextLocation;
use crate::compiler_frontend::string_interning::{InternedString, StringId, StringTable};
use crate::{return_compiler_error, return_type_error};
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct Var {
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
    Config(Vec<Var>), // Settings,

    // Imports a function from the host environment
    // This could be another Wasm file or a native function provided by the runtime
    Import(PathBuf),

    // Generic import path to another Beanstalk file
    // Effectively means the file is included in the current file as a struct that can be accessed
    Include(InternedString, PathBuf), // Name of file import, Imported file object

    // Control Flow
    Return(Vec<Expression>),                            // Return value,
    If(Expression, Vec<AstNode>, Option<Vec<AstNode>>), // Condition, If true, Else

    Match(
        Expression,           // Subject (condition)
        Vec<MatchArm>,        // Arms
        Option<Vec<AstNode>>, // for the wildcard/else case
    ),

    ForLoop(Box<Var>, Expression, Vec<AstNode>), // Item, Collection, Body,
    WhileLoop(Expression, Vec<AstNode>),         // Condition, Body,

    // Basics
    VariableDeclaration(Var), // Variable name, Value, Visibility,

    // For simple field access: obj.field
    FieldAccess {
        base: Box<AstNode>,   // The expression being accessed
        field: StringId,      // The field name
        data_type: DataType,  // Resolved type of this field access
        ownership: Ownership, // Ownership of the resolved field
    },

    // For method calls: obj.method(args)
    MethodCall {
        base: Box<AstNode>,
        method: StringId,
        args: Vec<AstNode>,
        signature: FunctionSignature,
    },

    FunctionCall {
        name: InternedString,
        args: Vec<Expression>,
        returns: Vec<DataType>,
        location: TextLocation,
        // bool, // Function is pure
    },

    // Host function call (functions provided by the runtime)
    HostFunctionCall {
        host_function_id: HostFunctionId,
        args: Vec<Expression>,
        returns: Vec<DataType>,
        location: TextLocation,
    },

    // example: new_struct_instance = MyStructDefinition(arg1, arg2)
    //          new_struct_instance(arg) -- Calls the main function of the struct
    StructDefinition(
        InternedString, // Name
        Vec<Var>,       // Fields
    ),

    Function(InternedString, FunctionSignature, Vec<AstNode>),

    // Mutation of existing mutable variables
    Assignment {
        target: Box<AstNode>, // Variable, FieldAccess, Deref, etc.
        value: Expression,
    },

    // An actual r-value
    Rvalue(Expression),

    // Built-in, always expected host Functions.
    // Print node - deprecated in favor of host_io_functions() host function
    // The host_io_functions() function is now the standard way to output to stdout
    // It accepts any type through CoerceToString and automatically adds newlines
    Print(Expression),

    Template(Expression),
    TopLevelTemplate(Expression),
    Slot,
    Empty, //

    // Operators
    // Operator, Precedence
    Operator(Operator), // Operator,

    Newline,
    Spaces(u32),
}

impl AstNode {
    pub fn get_expr(&self) -> Result<Expression, CompilerError> {
        match &self.kind {
            NodeKind::VariableDeclaration(arg) => Ok(arg.value.to_owned()),
            NodeKind::Rvalue(value, ..) => Ok(value.to_owned()),
            // NodeKind::Assignment(_, value) => Ok(value.to_owned()),
            NodeKind::FunctionCall {
                name,
                args: arguments,
                returns,
                location,
            } => Ok(Expression::function_call(
                *name,
                arguments.to_owned(),
                returns.to_owned(),
                location.to_owned(),
            )),

            NodeKind::HostFunctionCall {
                host_function_id,
                args: arguments,
                returns,
                location,
            } => Ok(Expression::function_call(
                InternedString::from_u32(0),
                arguments.to_owned(),
                returns.to_owned(),
                location.to_owned(),
            )),

            NodeKind::FieldAccess {
                data_type,
                ownership,
                ..
            } => Ok(Expression::runtime(
                vec![self.to_owned()],
                data_type.to_owned(),
                self.location.to_owned(),
                ownership.to_owned(),
            )),
            // Compiler tried to get the expression of a node that cannot contain expressions
            _ => {
                println!("{:?}", self.kind);
                return_compiler_error!(
                    "Compiler tried to get the expression of a node that cannot contain expressions in src/compiler_frontend/parsers/ast_nodes.rs"
                );
            }
        }
    }

    // If this is a boolean value, flip it to the opposite value
    pub fn flip(&mut self, string_table: &StringTable) -> Result<bool, CompilerError> {
        if let NodeKind::Rvalue(value) = &mut self.kind {
            match value.kind {
                ExpressionKind::Bool(val) => {
                    value.kind = ExpressionKind::Bool(!val);
                    return Ok(true);
                }
                ExpressionKind::Runtime(_) => {
                    if !value.ownership.is_mutable() {
                        return_type_error!(
                            "Tried to use the 'not' operator on a non-mutable value",
                            self.location.to_owned().to_error_location(&string_table), {
                                ExpectedType => "Boolean",
                                BorrowKind => "Shared",
                                LifetimeHint => "This value is borrowed",
                            }
                        )
                    } else {
                        return_type_error!(
                            format!(
                                "Tried to use the 'not' operator on value of type {:?}",
                                value.data_type
                            ),
                            self.location.to_owned().to_error_location(&string_table), {
                                ExpectedType => "Boolean",
                                BorrowKind => "Shared",
                                LifetimeHint => "This value is borrowed",
                            }
                        )
                    }
                }
                _ => {}
            }
        }

        return_type_error!(
            format!("Tried to use the 'not' operator on a non-boolean: {:?}", self.kind),
            self.location.to_owned().to_error_location(&string_table), {
                ExpectedType => "Boolean",
            }
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
