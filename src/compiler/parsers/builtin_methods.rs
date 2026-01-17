use crate::compiler::datatypes::DataType;
use crate::compiler::parsers::ast_nodes::Var;
use crate::compiler::string_interning::StringTable;

// TODO: This function will be expanded when builtin methods are implemented
// For now, it returns an empty vector but is updated to use StringId for method names
pub fn get_builtin_methods(data_type: &DataType, _string_table: &mut StringTable) -> Vec<Var> {
    let _methods: Vec<Var> = Vec::new();

    match data_type {
        // Future builtin methods will use StringId for method names:
        // DataType::String => {
        //     // Example: length method
        //     let length_method_name = string_table.intern("length");
        //     // Create method signature with interned name...
        // }
        _ => Vec::new(),
    }
}
