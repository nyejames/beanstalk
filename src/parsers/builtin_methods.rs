use crate::bs_types::DataType;
use crate::parsers::ast_nodes::{Arg, Expr};

pub fn get_builtin_methods(data_type: &DataType) -> Vec<Arg> {
    match data_type {
        DataType::Collection(inner_type) => {
            Vec::from([
                Arg {
                    name: "get".to_string(),
                    data_type: DataType::Block(
                        Vec::from([Arg {
                            name: "".to_string(),
                            data_type: DataType::Int(false),

                            // Should be no default behaviour. Index is required.
                            // get_last() and get_first() should be implemented later.
                            default_value: Expr::None,
                        }]),
                        Vec::from([*inner_type.to_owned()]),
                    ),
                    default_value: Expr::Int(0),
                },
                Arg {
                    name: "push".to_string(),
                    data_type: DataType::Block(
                        Vec::from([Arg {
                            name: "item".to_string(),
                            data_type: *inner_type.to_owned(),
                            default_value: Expr::None,
                        }]),
                        Vec::new(),
                    ),
                    default_value: Expr::Int(0),
                },
            ])
        }

        _ => Vec::new(),
    }
}
