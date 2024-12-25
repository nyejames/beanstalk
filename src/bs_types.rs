use crate::{
    parsers::ast_nodes::AstNode,
    Token,
};
use crate::parsers::ast_nodes::Arg;

#[derive(Debug, Clone, PartialEq)]
pub enum DataType {
    Inferred, // Type is inferred, this only gets to the emitter stage if it will definitely be JS rather than WASM
    Bool,
    True,
    False,
    String, // UTF-8 (will probably just be utf 16 because js for now)

    Float,
    Int,

    // Any type can be used in the expression and will be coerced to a string (for scenes only)
    // Mathematical operations will still work and take priority, but strings can be used in these expressions
    // And all types will finally be coerced to strings after everything is evaluated
    CoerseToString,

    Collection(Box<DataType>), // Collection of a single type, dynamically sized
    Struct,
    Scene,
    Choice,
    Type,

    Style,

    // Mixed types (fixed size), can be named so must be args, not just types
    Tuple(Vec<Arg>),

    // Functions have named arguments
    // Effectively identical to tuples
    // We don't use a Datatypes here (to put two tuples there) as it just adds an extra unwrapping step
    // And we want to be able to have optional names / default values for even single arguments
    Function(Vec<Arg>, Vec<Arg>), // Arguments, Return type

    Union(Vec<DataType>), // Union of types

    Error,
    None, // The None result of an option, or empty argument
}

pub fn return_datatype(node: &AstNode) -> DataType {
    match node {
        AstNode::RuntimeExpression(_, datatype, _) => datatype.clone(),
        AstNode::Literal(token, _) => match token {
            Token::FloatLiteral(_) => DataType::Float,
            Token::IntLiteral(_) => DataType::Int,
            Token::StringLiteral(_) => DataType::String,
            Token::BoolLiteral(value) => {
                if *value {
                    DataType::True
                } else {
                    DataType::False
                }
            }
            _ => DataType::Inferred,
        },
        AstNode::VarReference(_, datatype, _) | AstNode::ConstReference(_, datatype, _) => {
            datatype.clone()
        }
        AstNode::Tuple(arguments, _) => {
            DataType::Tuple(arguments.to_owned())
        }
        AstNode::Empty(_) => DataType::None,
        _ => DataType::Inferred,
    }
}
