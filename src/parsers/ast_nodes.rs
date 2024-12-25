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
    pub value: AstNode, // Optional Value of the argument - AstNode::Empty if no value
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
    Return(Box<AstNode>, u32), // Return value, Line number

    // Basics
    Function(String, Vec<Arg>, Vec<AstNode>, bool, Vec<Arg>, u32), // Function name, Args (named), Body, Public, return types (named), Line number
    Expression(Vec<AstNode>, u32), // Expression that can contain mixed types, line number
    RuntimeExpression(Vec<AstNode>, DataType, u32), //Expression, Result type, Line number

    Error(String, u32), // Message, line number
    Comment(String),
    VarDeclaration(String, Box<AstNode>, bool, DataType, bool, u32), // Variable name, Value, Public, Type, is_const, Line number

    // Built-in Functions (Would probably be standard lib in other languages)

    // Print can accept multiple arguments and will coerse them to strings
    Print(Vec<Arg>, u32), // Value, Line number

    // References to existing variables
    VarReference(String, DataType, u32), // Variable name, Type, Line number
    ConstReference(String, DataType, u32), // Variable name, Type, Line number
    JSStringReference(String, u32), // Variable name, Line number
    FunctionCall(String, Vec<AstNode>, Vec<Arg>, u32), // Function name, arguments (can be a tuple of arguments), return args, Line number

    // Accessing fields
    CollectionAccess(String, usize, DataType, u32), // Name, Index, Type, Line number
    TupleAccess(String, usize, DataType, u32),      // Name, Index, Type, Line number

    // Other language code blocks
    JS(String, u32), // Code, Line number
    CSS(String, u32), // Code, Line number

    // Literals
    Literal(Token, u32), // Token, Line number
    Collection(Vec<AstNode>, DataType, u32), // Collection, Type, Line number
    Struct(String, Box<AstNode>, bool, u32), // Name, Fields, Public, Line number
    Tuple(Vec<Arg>, u32),           // Tuple, line number
    Scene(Vec<AstNode>, Vec<Tag>, Vec<Style>, Vec<Action>, u32), // Scene, Tags, Styles, Actions, Line number
    SceneTemplate,
    Empty(u32), // Line number

    // Operators
    // Operator, Precedence
    LogicalOperator(Token, u8, u32), // Negative, Not, Exponent, Line number
    BinaryOperator(Token, u8, u32),  // Operator, Precedence, Line number
    UnaryOperator(Token, bool, u32), // Operator, is_postfix, Line number

    // HTML
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

pub trait Node {
    fn get_type(&self) -> DataType;
}

impl Node for AstNode {
    fn get_type(&self) -> DataType {
        return_datatype(self)
    }
}
