use std::path::PathBuf;

use super::styles::{Action, Style, Tag};
use crate::{
    bs_types::{return_datatype, DataType}, Token
};

#[derive(Debug, PartialEq, Clone)]
// Args are abstractions on top of Datatypes
// They are used to store the name, data type and optional value of an argument
// These are used for tuples and functions
// Args should basically disappear once the AST is parsed. Everything will be converted into just indexes
pub struct Arg {
    pub name: String, // Optional Name of the argument (empty string if unnamed)
    pub data_type: DataType,
    pub value: Value, // Optional Value of the argument - None if no value
}

// The possible values of any type
// Returns 'Runtime' if the value is not known at compile time
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    None,
    Reference(String, DataType),
    Runtime(Vec<AstNode>, DataType),

    Int(i64),
    Float(f64),
    String(String),
    Bool(bool),

    Scene(Vec<AstNode>, Vec<Tag>, Vec<Style>, Vec<Action>),

    Tuple(Vec<Arg>),
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
    Expression(Vec<AstNode>, u32), // Expression that can contain mixed types, line number
    RuntimeExpression(Vec<AstNode>, DataType, u32), // Expression, Result type, Line number

    Error(String, u32), // Message, line number
    Comment(String),
    VarDeclaration(String, Value, bool, DataType, bool, u32), // Variable name, Value, Public, Type, is_const, Line number

    // Built-in Functions (Would probably be standard lib in other languages)
    // Print can accept multiple arguments and will coerce them to strings
    Print(Vec<Value>, u32), // Value, Line number

    JSStringReference(String, u32), // Variable name, Line number
    FunctionCall(String, Vec<Value>, Vec<Arg>, u32), // Function name, arguments (can be a tuple of arguments), return args, Line number

    // Need to remove for just being literals
    CollectionAccess(String, usize, DataType, u32), // Name, Index, Type, Line number
    TupleAccess(String, usize, DataType, u32),      // Name, Index, Type, Line number

    // Other language code blocks
    JS(String, u32), // Code, Line number
    CSS(String, u32), // Code, Line number
    WASM(String, u32), // Code, Line number

    // Literals
    Literal(Value, u32), // Token, Line number
    Collection(Vec<AstNode>, DataType, u32), // Collection, Type, Line number
    Tuple(Vec<Arg>, u32),           // Tuple, line number

    SceneTemplate,
    Empty(u32), // Line number

    // Operators
    // Operator, Precedence
    LogicalOperator(Token, u8, u32), // Operator, Precedence, Line number
    BinaryOperator(Token, u8, u32),  // Operator, Precedence, Line number
    UnaryOperator(Token, bool, u32), // Operator, is_postfix, Line number

    // SCENES ONLY
    Id(Vec<Arg>, u32), // ID, Line number

    Span(String, u32), // ID, Line number
    P(String, u32), // ID, Line number
    Pre(String, u32), // Code, Line number
    CodeBlock(String, String, u32), // Code, Language, Line number
    Newline,

    Heading(u8),
    BulletPoint(u8),
    Em(u8, String, u32), // Strength, Content, Line number
    Superscript(String, u32), // Content, Line number
    Space(u32), // Add a space at front of element (line number)

    // SCENE META DATA
    Title(String, u32), // Content, Line number
    Date(String, u32), // Content, Line number
}

pub trait NodeInfo {
    fn get_type(&self) -> DataType;
    fn get_value(&self) -> Value;
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
            AstNode::Literal(value, _) => value.to_owned(),

            // Turns tuple into a vec of values
            AstNode::Tuple(args, _) => {
                let values: Vec<Value> = args.iter().map(|arg| arg.value.to_owned()).collect();

                // Automatically convert tuples of one item into that item
                if values.len() == 1 {
                    return values[0].to_owned()
                }

                // An empty tuple is None in this language
                if values.len() < 1 {
                    return Value::None
                }

                Value::Tuple(args.to_owned())
            }

            AstNode::Collection(nodes, data_type, _) => Value::Collection(nodes.iter().map(|arg| arg.get_value()).collect(), data_type.to_owned()),

            // Grab the value inside the variable declaration
            AstNode::VarDeclaration(_, node, ..) => {
                node.to_owned()
            },

            AstNode::RuntimeExpression(nodes, data_type, _) => Value::Runtime(nodes.to_owned(), data_type.to_owned()),

            _ => Value::None,
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
            Value::Scene(_, _, _, _) => DataType::Scene,
            Value::Collection(_, data_type) => data_type.to_owned(),
            Value::Tuple(args) => DataType::Tuple(args.to_owned()),
            Value::Reference(_, data_type) => data_type.to_owned(),
        }
    }

    fn get_value(&self) -> Value {
        self.to_owned()
    }
}
