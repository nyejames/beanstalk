use super::styles::{Action, Style, Tag};
use crate::bs_types::get_reference_data_type;
use crate::tokenizer::TokenPosition;
use crate::{
    bs_types::{return_datatype, DataType},
    Token,
};
use std::path::PathBuf;

#[derive(Debug, PartialEq, Clone)]
// Args are abstractions on top of Datatypes
// They are used to store the name, data type and optional value of an argument
// These are used for structs and functions
// Args should basically disappear once the AST is parsed. Everything will be converted into just indexes
pub struct Arg {
    pub name: String, // Optional Name of the argument (empty string if unnamed)
    pub data_type: DataType,
    pub value: Value, // Optional Value of the argument - 'None' if no value
}

// The possible values of any type
// Returns 'Runtime' if the value is not known at compile time
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    None,

    // For variables, function calls and structs / collection access
    // Name, DataType, Specific argument accessed
    // Arg accessed might be useful for built-in methods on any type
    Reference(String, DataType, Vec<usize>),

    Runtime(Vec<AstNode>, DataType),

    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),

    Scene(Vec<AstNode>, Vec<Tag>, Vec<Style>, Vec<Action>),

    Structure(Vec<Arg>),
    Collection(Vec<Value>, DataType),
}

#[derive(Debug, Clone, PartialEq)]
#[allow(dead_code)]
pub enum AstNode {
    // Warning Message
    // This could be stuff like unused variables, possible race conditions, etc
    Warning(String, TokenPosition), // Message, Line number, Start pos, End pos

    // Config settings
    Settings(Vec<AstNode>, TokenPosition), // Settings, Line number

    // Named import path for the module
    Import(String, TokenPosition), // Path, Line number

    // Path to a module that will automatically import all styles and scenes
    // into the scope of the current module. Doesn't automatically import variables or functions into the scope
    Use(PathBuf, TokenPosition), // Path, Line number

    // Control Flow
    Return(Value, TokenPosition), // Return value, Line number

    // Basics
    Function(
        String,
        Vec<Arg>,
        Vec<AstNode>,
        bool,
        Vec<Arg>,
        TokenPosition,
    ), // Function name, Args (named), Body, Public, return types (named), Line number
    FunctionCall(String, Vec<Value>, Vec<Arg>, Vec<usize>, TokenPosition), // Function name, arguments (has been sorted into correct order), return args, argument accessed, Line number

    Comment(String),
    VarDeclaration(String, Value, bool, DataType, bool, TokenPosition), // Variable name, Value, Public, Type, is_const, Line number

    // Built-in Functions (Would probably be standard lib in other languages)
    // Print can accept multiple arguments and will coerce them to strings
    Print(Value, TokenPosition), // Value, Line number

    // Not even sure if this is needed
    JSStringReference(String, TokenPosition), // Variable name, Line number

    // Other language code blocks
    JS(String, TokenPosition),   // Code, Line number
    CSS(String, TokenPosition),  // Code, Line number
    WASM(String, TokenPosition), // Code, Line number

    // Literals
    Literal(Value, TokenPosition), // Token, Accessed args, Line number

    SceneTemplate,
    Empty(TokenPosition), // Line number

    // Operators
    // Operator, Precedence
    LogicalOperator(Token, TokenPosition), // Operator, Line number
    BinaryOperator(Token, TokenPosition),  // Operator, Line number
    UnaryOperator(Token, bool, TokenPosition), // Operator, is_postfix, Line number

    // SCENES ONLY
    // Todo - separate from main AST
    Id(Vec<Arg>, TokenPosition), // ID, Line number

    Span(String, TokenPosition),              // ID, Line number
    P(String, TokenPosition),                 // ID, Line number
    Pre(String, TokenPosition),               // Code, Line number
    CodeBlock(String, String, TokenPosition), // Code, Language, Line number
    Newline,

    Heading(u8),
    BulletPoint(u8),
    Em(u8, String, TokenPosition), // Strength, Content, Line number
    Superscript(String, TokenPosition), // Content, Line number
    Space(u32),                    // Add a space at front of element (number of spaces)

    // SCENE META DATA
    Title(String, TokenPosition), // Content, Line number
    Date(String, TokenPosition),  // Content, Line number
}

pub trait NodeInfo {
    fn get_type(&self) -> DataType;
    fn get_value(&self) -> Value;
    fn get_precedence(&self) -> u8;
    fn dimensions(&self) -> TokenPosition;
}

impl NodeInfo for AstNode {
    fn get_type(&self) -> DataType {
        return_datatype(self)
    }

    // Gets the compile time value of the node
    // This is pretty much just for literals
    // Returns 'None' if it's not a literal value
    // Returns 'Runtime' if it can't be evaluated at compile time
    fn get_value(&self) -> Value {
        match self {
            AstNode::Literal(value, ..) => value.to_owned(),

            // Grab the value inside the variable declaration
            AstNode::VarDeclaration(_, node, ..) => node.to_owned(),

            // Note: Function calls can't be evaluated for their value at compile time (yet)
            // When the compiler gets more complex, some function calls may be possible to evaluate
            // Maybe even compile time only functions that do the work of macros
            _ => Value::None,
        }
    }

    fn get_precedence(&self) -> u8 {
        match self {
            AstNode::BinaryOperator(op, _) => match op {
                Token::Add => 2,
                Token::Subtract => 2,
                Token::Multiply => 3,
                Token::Divide => 3,
                Token::Modulus => 3,
                Token::Remainder => 3,
                Token::Root => 3,
                Token::Exponent => 4,
                _ => 0,
            },

            AstNode::LogicalOperator(op, _) => match op {
                Token::Equal => 5,
                Token::LessThan => 5,
                Token::LessThanOrEqual => 5,
                Token::GreaterThan => 5,
                Token::GreaterThanOrEqual => 5,
                Token::And => 6,
                Token::Or => 7,
                _ => 0,
            },
            _ => 0,
        }
    }

    fn dimensions(&self) -> TokenPosition {
        match self {
            AstNode::Literal(value, _) => value.dimensions(),

            AstNode::VarDeclaration(name, _, _, _, _, token_position) => TokenPosition {
                line_number: token_position.char_column + name.to_string().len() as u32,
                char_column: token_position.line_number,
            },

            _ => TokenPosition {
                line_number: 0,
                char_column: 0,
            },
        }
    }
}

impl NodeInfo for Value {
    fn get_type(&self) -> DataType {
        match self {
            Value::None => DataType::None,
            Value::Runtime(_, data_type) => data_type.to_owned(),
            Value::Int(_) => DataType::Int,
            Value::Float(_) => DataType::Float,
            Value::String(_) => DataType::String,
            Value::Bool(_) => DataType::Bool,
            Value::Scene(..) => DataType::Scene,
            Value::Collection(_, data_type) => data_type.to_owned(),
            Value::Structure(args) => DataType::Structure(args.to_owned()),

            // Need to check accessed args
            Value::Reference(_, data_type, argument_accessed) => {
                get_reference_data_type(data_type, argument_accessed)
            }
        }
    }

    fn get_value(&self) -> Value {
        self.to_owned()
    }

    fn get_precedence(&self) -> u8 {
        match self {
            Value::Runtime(..) => 1,
            _ => 0,
        }
    }

    fn dimensions(&self) -> TokenPosition {
        match self {
            Value::None => TokenPosition {
                line_number: 0,
                char_column: 0,
            },

            Value::Int(val) => TokenPosition {
                line_number: val.to_string().len() as u32,
                char_column: 0,
            },

            Value::Float(val) => TokenPosition {
                line_number: val.to_string().len() as u32,
                char_column: 0,
            },

            Value::String(val) => TokenPosition {
                line_number: val.len() as u32,
                char_column: val.chars().filter(|c| *c == '\n').count() as u32,
            },

            Value::Bool(val) => {
                if *val {
                    TokenPosition {
                        line_number: 4,
                        char_column: 0,
                    }
                } else {
                    TokenPosition {
                        line_number: 5,
                        char_column: 0,
                    }
                }
            }

            Value::Reference(name, ..) => TokenPosition {
                line_number: name.len() as u32,
                char_column: 0,
            },

            Value::Structure(args) => {
                let mut combined_dimensions = TokenPosition {
                    line_number: args[0].value.dimensions().line_number,
                    char_column: args[0].value.dimensions().char_column,
                };

                for arg in args {
                    combined_dimensions.char_column += arg.value.dimensions().char_column;
                }

                combined_dimensions
            }

            // Get the position of the first node, and the last node
            // And get the positions
            // This just gets the widest char line
            // So error formatting will need to clip the line to each length
            Value::Scene(nodes, ..) | Value::Runtime(nodes, ..) => {
                let first_node = &nodes[0];
                let last_node = &nodes[nodes.len() - 1];
                TokenPosition {
                    line_number: last_node.dimensions().line_number,
                    char_column: last_node.dimensions().char_column
                        - first_node.dimensions().char_column,
                }
            }

            Value::Collection(nodes, ..) => {
                let first_node = &nodes[0];
                let last_node = &nodes[nodes.len() - 1];
                TokenPosition {
                    line_number: last_node.dimensions().line_number,
                    char_column: last_node.dimensions().char_column
                        - first_node.dimensions().char_column,
                }
            }
        }
    }
}
