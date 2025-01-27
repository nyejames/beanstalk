use crate::parsers::ast_nodes::{NodeInfo, Value};
use crate::tokenizer::TokenPosition;
use crate::{
    bs_types::DataType, parsers::ast_nodes::AstNode, settings::BS_VAR_PREFIX, CompileError,
    ErrorType, Token,
};

// Create everything necessary in JS
// Break out pieces in WASM calls
pub fn expression_to_js(expr: &Value, start_pos: &TokenPosition) -> Result<String, CompileError> {
    let mut js = String::new(); // Open the template string

    match expr {
        Value::Runtime(nodes, expression_type) => {
            for node in nodes {
                match node {
                    AstNode::Literal(value, _) => match value {
                        Value::Float(value) => {
                            js.push_str(&value.to_string());
                        }

                        Value::Int(value) => {
                            js.push_str(&value.to_string());
                        }

                        Value::String(value) => {
                            js.push_str(&format!("\"{}\"", value));
                        }

                        Value::Bool(value) => {
                            js.push_str(&value.to_string());
                        }

                        Value::Reference(id, _, argument_accessed) => {
                            // All just JS for now
                            js.push_str(&format!(" {BS_VAR_PREFIX}{id}"));
                            for index in argument_accessed {
                                js.push_str(&format!("[{}]", index));
                            }
                            /*
                                js.push_str(&format!("wsx.get_{BS_VAR_PREFIX}{id}()"));
                            */
                        }

                        _ => {
                            return Err(CompileError {
                                msg: format!("Compiler Bug (Not Implemented yet): Invalid argument type for function call (js_parser): {:?}", value),
                                start_pos: start_pos.to_owned(),
                                end_pos: expr.dimensions(),
                                error_type: ErrorType::Compiler,
                            });
                        }
                    },

                    AstNode::BinaryOperator(op, token_position) => match op {
                        Token::Add => js.push_str(" + "),
                        Token::Subtract => js.push_str(" - "),
                        Token::Multiply => js.push_str(" * "),
                        Token::Divide => js.push_str(" / "),
                        _ => {
                            return Err(CompileError {
                                msg: "Unsupported operator found in operator stack when parsing an expression into JS".to_string(),
                                start_pos: token_position.to_owned(),
                                end_pos: TokenPosition {
                                    line_number: token_position.line_number,
                                    char_column: token_position.char_column + 1,
                                },
                                error_type: ErrorType::Compiler,
                            });
                        }
                    },

                    AstNode::FunctionCall(name, args, _, arguments_accessed, _) => {
                        js.push_str(&format!(
                            " {}({})",
                            name,
                            combine_vec_to_js(&args, start_pos)?
                        ));
                        for index in arguments_accessed {
                            js.push_str(&format!("[{}]", index));
                        }
                    }

                    _ => {
                        return Err(CompileError {
                            msg: format!(
                                "unknown AST node found in expression when parsing an expression into JS: {:?}",
                                node
                            ),
                            start_pos: start_pos.to_owned(),
                            end_pos: expr.dimensions(),
                            error_type: ErrorType::Compiler,
                        });
                    }
                }
            }

            match expression_type {
                DataType::String | DataType::Float | DataType::Int => {}
                DataType::CoerceToString => {
                    js.insert_str(0, "String(");
                    js.push_str(")");
                }
                _ => {
                    return Err(CompileError {
                        msg: format!("Compiler Bug: Haven't implemented this type yet in expression_to_js: {:?}", expression_type),
                        start_pos: start_pos.to_owned(),
                        end_pos: expr.dimensions(),
                        error_type: ErrorType::Compiler,
                    });
                }
            }
        }

        Value::Float(value) => {
            js.push_str(&value.to_string());
        }

        Value::Int(value) => {
            let as_float = *value as f64;
            js.push_str(&as_float.to_string());
        }

        Value::String(value) => {
            js.push_str(&format!("\"{}\"", value));
        }

        Value::Reference(name, _, arguments_accessed) => {
            js.push_str(&format!(" {BS_VAR_PREFIX}{name}"));
            for index in arguments_accessed {
                js.push_str(&format!("[{}]", index));
            }
        }

        // If the expression is just a tuple,
        // then it should automatically destructure into multiple arguments like this
        Value::Structure(args) => {
            let mut values = Vec::new();
            for arg in args {
                values.push(arg.value.to_owned());
            }
            js.push_str(&format!("[{}]", combine_vec_to_js(&values, start_pos)?));
        }

        _ => {
            return Err(CompileError {
                msg: format!(
                    "Compiler Bug: Invalid AST node given to expression_to_js: {:?}",
                    expr
                ),
                start_pos: start_pos.to_owned(),
                end_pos: expr.dimensions(),
                error_type: ErrorType::Compiler,
            });
        }
    }

    Ok(js)
}

pub fn create_reference_in_js(
    name: &String,
    data_type: &DataType,
    accessed_args: &Vec<usize>,
) -> String {
    match data_type {
        // DataType::Float | DataType::Int => {
        //     format!("uInnerHTML(\"{name}\", wsx.get_{BS_VAR_PREFIX}{name}());")
        // }
        DataType::Structure(_) | DataType::Collection(_) => {
            let mut accesses = String::new();
            for index in accessed_args {
                accesses.push_str(&format!("[{}]", index));
            }
            format!("uInnerHTML(\"{name}\",{BS_VAR_PREFIX}{name}{accesses});")
        }
        _ => {
            format!("uInnerHTML(\"{name}\",{BS_VAR_PREFIX}{name});")
        }
    }
}

pub fn combine_vec_to_js(
    collection: &Vec<Value>,
    line_number: &TokenPosition,
) -> Result<String, CompileError> {
    let mut js = String::new();

    let mut i: usize = 0;
    for node in collection {
        // Make sure correct commas at end of each element but not last one
        js.push_str(&format!(
            "{}{}",
            expression_to_js(&node, line_number)?,
            if i < collection.len() - 1 { "," } else { "" }
        ));
        i += 1;
    }

    Ok(js)
}

pub fn collection_to_js(
    collection: &Value,
    start_pos: &TokenPosition,
) -> Result<String, CompileError> {
    match collection {
        Value::Structure(args) => {
            let mut nodes = Vec::new();
            for arg in args {
                nodes.push(arg.value.to_owned());
            }
            combine_vec_to_js(&nodes, start_pos)
        }
        _ => Err(CompileError {
            msg: "Non-tuple AST node given to collection_to_js".to_string(),
            start_pos: start_pos.to_owned(),
            end_pos: collection.dimensions(),
            error_type: ErrorType::Compiler,
        }),
    }
}
