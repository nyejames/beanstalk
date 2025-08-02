use crate::compiler::datatypes::DataType;
use crate::compiler::parsers::ast_nodes::Arg;

// TODO
pub fn get_builtin_methods(data_type: &DataType) -> Vec<Arg> {
    let mut methods: Vec<Arg> = Vec::new();

    match data_type {
        _ => Vec::new(),
    }
}
