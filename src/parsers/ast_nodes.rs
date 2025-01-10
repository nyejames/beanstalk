use super::styles::{Action, Style, Tag};
use crate::bs_types::get_reference_data_type;
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
    // Config settings
    Settings(Vec<AstNode>, u32), // Settings, Line number

    // Named import path for the module
    Import(String, u32), // Path, Line number

    // Path to a module that will automatically import all styles and scenes
    // into the scope of the current module. Doesn't automatically import variables or functions into the scope
    Use(PathBuf, u32), // Path, Line number

    // Control Flow
    Return(Value, u32), // Return value, Line number

    // Basics
    Function(String, Vec<Arg>, Vec<AstNode>, bool, Vec<Arg>, u32), // Function name, Args (named), Body, Public, return types (named), Line number
    FunctionCall(String, Vec<Value>, Vec<Arg>, Vec<usize>, u32), // Function name, arguments (has been sorted into correct order), return args, argument accessed, Line number

    Comment(String),
    VarDeclaration(String, Value, bool, DataType, bool, u32), // Variable name, Value, Public, Type, is_const, Line number

    // Built-in Functions (Would probably be standard lib in other languages)
    // Print can accept multiple arguments and will coerce them to strings
    Print(Value, u32), // Value, Line number

    // Not even sure if this is needed
    JSStringReference(String, u32), // Variable name, Line number

    // Other language code blocks
    JS(String, u32),   // Code, Line number
    CSS(String, u32),  // Code, Line number
    WASM(String, u32), // Code, Line number

    // Literals
    Literal(Value, u32), // Token, Accessed args, Line number

    SceneTemplate,
    Empty(u32), // Line number

    // Operators
    // Operator, Precedence
    LogicalOperator(Token, u32),     // Operator, Line number
    BinaryOperator(Token, u32),      // Operator, Line number
    UnaryOperator(Token, bool, u32), // Operator, is_postfix, Line number

    // SCENES ONLY
    // Todo - separate from main AST
    Id(Vec<Arg>, u32), // ID, Line number

    Span(String, u32),              // ID, Line number
    P(String, u32),                 // ID, Line number
    Pre(String, u32),               // Code, Line number
    CodeBlock(String, String, u32), // Code, Language, Line number
    Newline,

    Heading(u8),
    BulletPoint(u8),
    Em(u8, String, u32),      // Strength, Content, Line number
    Superscript(String, u32), // Content, Line number
    Space(u32),               // Add a space at front of element (line number)

    // SCENE META DATA
    Title(String, u32), // Content, Line number
    Date(String, u32),  // Content, Line number
}

pub trait NodeInfo {
    fn get_type(&self) -> DataType;
    fn get_value(&self) -> Value;
    fn get_precedence(&self) -> u8;
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
}
