use crate::compiler::datatypes::DataType;
use crate::compiler::parsers::ast_nodes::Arg;
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};

pub fn get_builtin_methods(data_type: &DataType) -> Vec<Arg> {
    let mut methods: Vec<Arg> = Vec::new();
    
    match data_type {
        DataType::Collection(inner_type) => Vec::from([]),

        _ => Vec::new(),
    }
}
