use crate::bs_types::get_reference_data_type;
use crate::parsers::scene::{SceneContent, Style};
use crate::parsers::util::string_dimensions;
use crate::tokenizer::TokenPosition;
use crate::{Token, bs_types::DataType};
use colour::red_ln;
use std::path::PathBuf;
use wasm_encoder::ValType;

#[derive(Debug, PartialEq, Clone)]
// Args are abstractions on top of Datatypes
// They are used to store the name, data type and optional value of an argument
// These are used for structs and functions
// Args should basically disappear once the AST is parsed. Everything will be converted into just indexes
pub struct Arg {
    pub name: String, // Optional Name of the argument (empty string if unnamed)
    pub data_type: DataType,
    pub value: Expr, // Optional Value of the argument - 'None' if no value
}

impl Arg {
    pub fn to_wasm_type(&self) -> Vec<ValType> {
        match &self.data_type {
            DataType::Float(_) => vec![ValType::F64],
            DataType::Int(_)
            | DataType::Bool(_)
            | DataType::None
            | DataType::True
            | DataType::False => {
                vec![ValType::I32]
            }

            DataType::Decimal(_) => vec![ValType::F64],

            DataType::String(_) => vec![ValType::I32, ValType::I32],
            DataType::CoerceToString(_) => vec![ValType::I32, ValType::I32],

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

// The possible values of any type.
// Return 'Runtime' if the value is not known at compile time
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
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
        DataType,
        TokenPosition,
        bool,
    ),

    Scene(SceneContent, Style, SceneContent, String), // Scene Body, Styles, Scene head, ID

    Collection(Vec<Expr>, DataType),

    // Could behave just like a tuple if the type isn't specified
    // And the arguments are not named
    StructLiteral(Vec<Arg>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum AstNode {
    // Warning Message
    // This could be stuff like unused variables, possible race conditions, etc
    Warning(String, TokenPosition), // Message, Line number, Start pos, End pos

    // Config settings
    Settings(Vec<Arg>, TokenPosition), // Settings, Line number

    // Named import path for the module
    Import(String, TokenPosition), // Path, Line number

    // Path to a module that will automatically import all styles and scenes
    // into the scope of the current module. Doesn't automatically import variables or functions into the scope
    Use(PathBuf, TokenPosition), // Path, Line number

    // Control Flow
    Return(Expr, TokenPosition),           // Return value, Line number
    If(Expr, Vec<AstNode>, TokenPosition), // Condition, If true, Line number
    Else(Vec<AstNode>, TokenPosition),     // Body, Line number
    ForLoop(Expr, Expr, Vec<AstNode>, TokenPosition), // Item, Collection, Body, Line number
    WhileLoop(Expr, Vec<AstNode>, TokenPosition), // Condition, Body, Line number

    // Basics
    Function(
        String,       // Name
        Vec<Arg>,     // Args (named)
        Vec<AstNode>, // Body
        bool,         // Public
        DataType,     // return type
        TokenPosition,
        bool, // Pure
    ),

    FunctionCall(
        String,
        Vec<Expr>,  // Arguments passed in
        DataType,   // return type
        Vec<usize>, // Accessed args
        TokenPosition,
        bool, // Function is pure
    ),

    Comment(String),

    // Variable names should be the full namespace (module path + variable name)
    VarDeclaration(String, Expr, bool, DataType, TokenPosition), // Variable name, Value, Public, Type, Line number

    // Built-in Functions (Would probably be standard lib in other languages)
    // Print can accept multiple arguments and will coerce them to strings
    Print(Expr, TokenPosition), // Value, Line number

    // Not even sure if this is needed
    JSStringReference(String, TokenPosition), // Variable name, Line number

    // Other language code blocks
    JS(String, TokenPosition),   // Code, Line number
    Css(String, TokenPosition),  // Code, Line number
    Wasm(String, TokenPosition), // Code, Line number

    // Literals
    Literal(Expr, TokenPosition), // Token, Accessed args, Line number

    SceneTemplate,
    Slot,
    Empty(TokenPosition), // Line number

    // Operators
    // Operator, Precedence
    LogicalOperator(Token, TokenPosition), // Operator, Line number
    BinaryOperator(Token, TokenPosition),  // Operator, Line number
    UnaryOperator(Token, bool, TokenPosition), // Operator, is_postfix, Line number

    Newline,
    Spaces(u32),
}

impl AstNode {
    pub fn get_type(&self) -> DataType {
        match self {
            AstNode::Literal(value, _) => match value {
                Expr::Float(_) => DataType::Float(false),
                Expr::Int(_) => DataType::Int(false),
                Expr::String(_) => DataType::String(false),
                Expr::Bool(value) => {
                    if *value {
                        DataType::True
                    } else {
                        DataType::False
                    }
                }

                Expr::Scene(..) => DataType::Scene(false),
                Expr::Collection(_, data_type) => data_type.to_owned(),
                Expr::StructLiteral(args) => {
                    let mut data_type = DataType::Inferred(false);
                    for arg in args {
                        data_type = arg.data_type.to_owned();
                    }
                    data_type
                }
                Expr::Reference(_, data_type, argument_accessed) => {
                    get_reference_data_type(data_type, argument_accessed)
                }
                Expr::Function(_, args, _, _, return_type, ..) => {
                    DataType::Function(args.to_owned(), Box::new(return_type.to_owned()))
                }

                Expr::Runtime(_, data_type) => data_type.to_owned(),
                Expr::None => DataType::None,
            },

            AstNode::Empty(_) => DataType::None,
            AstNode::VarDeclaration(_, _, _, data_type, ..) => data_type.to_owned(),

            AstNode::FunctionCall(_, _, return_type, ..) => DataType::Structure(Vec::from([Arg {
                name: "".to_string(),
                data_type: return_type.to_owned(),
                value: Expr::None,
            }])),

            _ => {
                red_ln!(
                    "Probably compiler issue?: Datatype return not implemented for: {:?}",
                    self
                );

                DataType::Inferred(false)
            }
        }
    }

    // Gets the compile time value of the node
    // This is pretty much just for literals
    // Returns 'None' if it's not a literal value
    // Returns 'Runtime' if it can't be evaluated at compile time
    pub(crate) fn get_value(&self) -> Expr {
        match self {
            AstNode::Literal(value, ..) | AstNode::VarDeclaration(_, value, ..) => value.to_owned(),
            AstNode::Function(name, args, body, is_exported, return_type, _, is_pure) => {
                Expr::Function(
                    name.to_owned(),
                    args.to_owned(),
                    body.to_owned(),
                    *is_exported,
                    return_type.to_owned(),
                    TokenPosition::default(),
                    is_pure.to_owned(),
                )
            }
            _ => Expr::None,
        }
    }

    pub fn get_precedence(&self) -> u32 {
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

            AstNode::VarDeclaration(name, _, _, _, token_position) => TokenPosition {
                line_number: token_position.char_column + name.to_string().len() as i32,
                char_column: token_position.line_number,
            },

            _ => TokenPosition {
                line_number: 0,
                char_column: 0,
            },
        }
    }
}

impl Expr {
    pub fn get_type(&self) -> DataType {
        match self {
            Expr::None => DataType::None,
            Expr::Runtime(_, data_type) => data_type.to_owned(),
            Expr::Int(_) => DataType::Int(false),
            Expr::Float(_) => DataType::Float(false),
            Expr::String(_) => DataType::String(false),
            Expr::Bool(_) => DataType::Bool(false),
            Expr::Scene(..) => DataType::Scene(false),
            Expr::Collection(_, data_type) => data_type.to_owned(),
            Expr::StructLiteral(args) => DataType::Structure(args.to_owned()),
            Expr::Function(_, args, _, _, return_type, ..) => {
                DataType::Function(args.to_owned(), Box::new(return_type.to_owned()))
            }
            // Need to check accessed args
            Expr::Reference(_, data_type, argument_accessed) => {
                get_reference_data_type(data_type, argument_accessed)
            }
        }
    }

    pub fn as_string(&self) -> String {
        match self {
            Expr::String(string) => string.to_owned(),
            Expr::Int(int) => int.to_string(),
            Expr::Float(float) => float.to_string(),
            Expr::Bool(bool) => bool.to_string(),
            Expr::Scene(..) => String::new(),
            Expr::Collection(items, ..) => {
                let mut all_items = String::new();
                for item in items {
                    all_items.push_str(&item.as_string());
                }
                all_items
            }
            Expr::StructLiteral(args) => {
                let mut all_items = String::new();
                for arg in args {
                    all_items.push_str(&arg.value.as_string());
                }
                all_items
            }
            Expr::Function(..) => String::new(),
            Expr::Reference(..) => String::new(),
            Expr::Runtime(..) => String::new(),
            Expr::None => String::new(),
        }
    }

    pub fn is_pure(&self) -> bool {
        match self {
            Expr::Runtime(..) | Expr::Reference(..) => false,
            Expr::Function(_, _, _, _, _, _, is_pure) => {
                // red_ln!("is_pure: {:?}", is_pure);
                *is_pure
            }
            Expr::Collection(values, _) => {
                for value in values {
                    if !value.is_pure() {
                        return false;
                    }
                }
                true
            }
            Expr::StructLiteral(args) => {
                for arg in args {
                    if !arg.value.is_pure() {
                        return false;
                    }
                }
                true
            }

            // Not sure about how to handle this yet
            Expr::Scene(..) => false,
            _ => true,
        }
    }

    pub fn dimensions(&self) -> TokenPosition {
        match self {
            Expr::None => TokenPosition {
                line_number: 0,
                char_column: 0,
            },

            Expr::Int(val) => TokenPosition {
                line_number: 0,
                char_column: val.to_string().len() as i32,
            },

            Expr::Float(val) => TokenPosition {
                line_number: 0,
                char_column: val.to_string().len() as i32,
            },

            Expr::String(val) => string_dimensions(val),

            Expr::Bool(val) => {
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

            Expr::Reference(name, ..) => TokenPosition {
                line_number: 0,
                char_column: name.len() as i32,
            },

            Expr::Function(_, _, nodes, ..) => {
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
            Expr::Scene(nodes, ..) => {
                let first_node = &nodes.before[0];
                let last_node = &nodes.after[nodes.after.len() - 1];
                TokenPosition {
                    line_number: last_node.dimensions().line_number,
                    char_column: last_node.dimensions().char_column
                        - first_node.dimensions().char_column,
                }
            }
            Expr::Runtime(nodes, ..) => {
                let first_node = &nodes[0];
                let last_node = &nodes[nodes.len() - 1];
                TokenPosition {
                    line_number: last_node.dimensions().line_number,
                    char_column: last_node.dimensions().char_column
                        - first_node.dimensions().char_column,
                }
            }

            Expr::Collection(nodes, ..) => {
                let first_node = &nodes[0];
                let last_node = &nodes[nodes.len() - 1];
                TokenPosition {
                    line_number: last_node.dimensions().line_number,
                    char_column: last_node.dimensions().char_column
                        - first_node.dimensions().char_column,
                }
            }

            Expr::StructLiteral(args) => {
                let mut combined_dimensions = TokenPosition {
                    line_number: args[0].value.dimensions().line_number,
                    char_column: args[0].value.dimensions().char_column,
                };

                for arg in args {
                    combined_dimensions.char_column += arg.value.dimensions().char_column;
                }

                combined_dimensions
            }
        }
    }
}
