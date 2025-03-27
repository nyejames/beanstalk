use crate::bs_types::get_reference_data_type;
use crate::parsers::scene::Style;
use crate::tokenizer::TokenPosition;
use crate::{bs_types::DataType, Token};
use colour::red_ln;
use std::path::PathBuf;
use wasm_encoder::ValType;
use crate::parsers::util::string_dimensions;

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

impl Arg {
    pub fn to_wasm_type(&self) -> Vec<ValType> {
        match &self.data_type {
            DataType::Float => vec![ValType::F64],
            DataType::Int | DataType::Bool | DataType::None | DataType::True | DataType::False => {
                vec![ValType::I32]
            }

            // TODO
            DataType::Decimal => vec![ValType::F64],

            DataType::String => vec![ValType::I32, ValType::I32],
            DataType::CoerceToString => vec![ValType::I32, ValType::I32],

            DataType::Type => vec![ValType::I32],

            DataType::Structure(args) => args
                .iter()
                .flat_map(|arg| arg.to_wasm_type())
                .collect::<Vec<ValType>>(),

            DataType::Collection(_) => vec![ValType::I32, ValType::I32],

            _ => vec![ValType::I32],
        }
    }
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

    Int(i32),
    Float(f64),
    String(String),
    Bool(bool),

    // Because functions can be values
    Function(
        String,
        Vec<Arg>,
        Vec<AstNode>,
        bool,
        Vec<Arg>,
        TokenPosition,
    ), // Function name, Args (named), Body, Public, return types (named), Line number

    Scene(Vec<AstNode>, Vec<Style>, String), // Content Nodes, Styles, ID
    Style(Style),

    Structure(Vec<Arg>),
    Collection(Vec<Value>, DataType),
}

#[derive(Debug, Clone, PartialEq)]
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

    // Variable names should be the full namespace (module path + variable name)
    VarDeclaration(String, Value, bool, DataType, bool, TokenPosition), // Variable name, Value, Public, Type, is_const, Line number

    // Built-in Functions (Would probably be standard lib in other languages)
    // Print can accept multiple arguments and will coerce them to strings
    Print(Value, TokenPosition), // Value, Line number

    // Not even sure if this is needed
    JSStringReference(String, TokenPosition), // Variable name, Line number

    // Other language code blocks
    JS(String, TokenPosition),   // Code, Line number
    Css(String, TokenPosition),  // Code, Line number
    Wasm(String, TokenPosition), // Code, Line number

    // Literals
    Literal(Value, TokenPosition), // Token, Accessed args, Line number

    SceneTemplate,
    Empty(TokenPosition), // Line number

    // Operators
    // Operator, Precedence
    LogicalOperator(Token, TokenPosition), // Operator, Line number
    BinaryOperator(Token, TokenPosition),  // Operator, Line number
    UnaryOperator(Token, bool, TokenPosition), // Operator, is_postfix, Line number
    
    Newline,
    Spaces(u32),
}

impl Value {
    pub fn as_string(&self) -> String {
        match self {
            Value::String(string) => string.to_owned(),
            Value::Int(int) => int.to_string(),
            Value::Float(float) => float.to_string(),
            Value::Bool(bool) => bool.to_string(),
            Value::Scene(..) => String::new(),
            Value::Style(..) => String::new(),
            Value::Collection(items, ..) => {
                let mut all_items = String::new();
                for item in items {
                    all_items.push_str(&item.as_string());
                }
                all_items
            }
            Value::Structure(args) => {
                let mut all_items = String::new();
                for arg in args {
                    all_items.push_str(&arg.value.as_string());
                }
                all_items
            }
            Value::Function(..) => String::new(),
            Value::Reference(..) => String::new(),
            Value::Runtime(..) => String::new(),
            Value::None => String::new(),
        }
    }
}

impl AstNode {
    pub fn get_type(&self) -> DataType {
        match self {
            AstNode::Literal(value, _) => match value {
                Value::Float(_) => DataType::Float,
                Value::Int(_) => DataType::Int,
                Value::String(_) => DataType::String,
                Value::Bool(value) => {
                    if *value {
                        DataType::True
                    } else {
                        DataType::False
                    }
                }

                Value::Scene(..) => DataType::Scene,
                Value::Style(..) => DataType::Style,
                Value::Collection(_, data_type) => data_type.to_owned(),
                Value::Structure(args) => DataType::Structure(args.to_owned()),
                Value::Reference(_, data_type, argument_accessed) => {
                    get_reference_data_type(data_type, argument_accessed)
                }
                Value::Function(_, args, _, _, return_args, ..) => {
                    DataType::Function(args.to_owned(), return_args.to_owned())
                }

                Value::Runtime(_, data_type) => data_type.to_owned(),
                Value::None => DataType::None,
            },

            AstNode::Empty(_) => DataType::None,
            AstNode::VarDeclaration(_, _, _, data_type, ..) => data_type.to_owned(),

            _ => {
                red_ln!(
                    "Probably compiler issue?: Datatype return not implemented for: {:?}",
                    self
                );

                DataType::Inferred
            }
        }
    }

    // Gets the compile time value of the node
    // This is pretty much just for literals
    // Returns 'None' if it's not a literal value
    // Returns 'Runtime' if it can't be evaluated at compile time
    pub(crate) fn get_value(&self) -> Value {
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

    pub fn get_precedence(&self) -> u8 {
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

    pub fn dimensions(&self) -> TokenPosition {
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

impl Value {
    pub fn get_type(&self) -> DataType {
        match self {
            Value::None => DataType::None,
            Value::Runtime(_, data_type) => data_type.to_owned(),
            Value::Int(_) => DataType::Int,
            Value::Float(_) => DataType::Float,
            Value::String(_) => DataType::String,
            Value::Bool(_) => DataType::Bool,
            Value::Scene(..) => DataType::Scene,
            Value::Style(..) => DataType::Style,
            Value::Collection(_, data_type) => data_type.to_owned(),
            Value::Structure(args) => DataType::Structure(args.to_owned()),
            Value::Function(_, args, _, _, return_args, ..) => {
                DataType::Function(args.to_owned(), return_args.to_owned())
            }
            // Need to check accessed args
            Value::Reference(_, data_type, argument_accessed) => {
                get_reference_data_type(data_type, argument_accessed)
            }
        }
    }

    pub fn dimensions(&self) -> TokenPosition {
        match self {
            Value::None => TokenPosition {
                line_number: 0,
                char_column: 0,
            },

            Value::Int(val) => TokenPosition {
                line_number: 0,
                char_column: val.to_string().len() as u32,
            },

            Value::Float(val) => TokenPosition {
                line_number: 0,
                char_column: val.to_string().len() as u32,
            },

            Value::String(val) => string_dimensions(val),

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
                line_number: 0,
                char_column: name.len() as u32,
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

            Value::Function(_, _, nodes, ..) => {
                let mut combined_dimensions = TokenPosition::default();
                
                // Get the largest dimensions of all the nodes
                for node in nodes {
                    if node.dimensions().line_number > combined_dimensions.line_number {
                        combined_dimensions.line_number = node.dimensions().line_number;
                    }

                    if node.dimensions().char_column > combined_dimensions.char_column {
                        combined_dimensions.char_column = node.dimensions().char_column;
                    }
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

            // TODO: style syntax
            Value::Style(_) => TokenPosition {
                line_number: 0,
                char_column: 0,
            },

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
