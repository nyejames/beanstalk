use crate::parsers::ast_nodes::Value;
use crate::{
    bs_types::DataType, parsers::ast_nodes::AstNode, settings::BS_VAR_PREFIX, CompileError, Token,
};

// Create everything necessary in JS
// Break out pieces in WASM calls
pub fn expression_to_js(expr: &Value, line_number: u32) -> Result<String, CompileError> {
    let mut js = String::new(); //Open the template string

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
                            if let Some(index) = argument_accessed {
                                js.push_str(&format!("[{}]", index));
                            }
                            /*
                                js.push_str(&format!("wsx.get_{BS_VAR_PREFIX}{id}()"));
                            */
                        }

                        _ => {
                            return Err(CompileError {
                                msg: format!("Compiler Bug (Not Implemented yet): Invalid argument type for function call (js_parser): {:?}", value),
                                line_number,
                            });
                        }
                    },

                    AstNode::BinaryOperator(op, _, line_number) => match op {
                        Token::Add => js.push_str(" + "),
                        Token::Subtract => js.push_str(" - "),
                        Token::Multiply => js.push_str(" * "),
                        Token::Divide => js.push_str(" / "),
                        _ => {
                            return Err(CompileError {
                                msg: "Unsupported operator found in operator stack when parsing an expression into JS".to_string(),
                                line_number: line_number.to_owned(),
                            });
                        }
                    },

                    AstNode::FunctionCall(name, args, _, argument_accessed, _) => {
                        js.push_str(&format!(
                            " {}({})",
                            name,
                            combine_vec_to_js(&args, line_number)?
                        ));
                        if let Some(index) = argument_accessed {
                            js.push_str(&format!("[{}]", index));
                        }
                    }

                    _ => {
                        return Err(CompileError {
                            msg: format!(
                                "unknown AST node found in expression when parsing an expression into JS: {:?}",
                                node
                            ),
                            line_number: line_number.to_owned(),
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
                        line_number: line_number.to_owned(),
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

        Value::Reference(name, _, argument_accessed) => {
            js.push_str(&format!(" {BS_VAR_PREFIX}{name}"));
            if let Some(index) = argument_accessed {
                js.push_str(&format!("[{}]", index));
            }
        }

        // If the expression is just a tuple,
        // then it should automatically destructure into multiple arguments like this
        Value::Tuple(args) => {
            let mut values = Vec::new();
            for arg in args {
                values.push(arg.value.to_owned());
            }
            js.push_str(&format!("[{}]", combine_vec_to_js(&values, line_number)?));
        }

        _ => {
            return Err(CompileError {
                msg: format!(
                    "Compiler Bug: Invalid AST node given to expression_to_js: {:?}",
                    expr
                ),
                line_number: line_number.to_owned(),
            });
        }
    }

    Ok(js)
}

pub fn create_reference_in_js(name: &String, data_type: &DataType) -> String {
    match data_type {
        DataType::String | DataType::Scene | DataType::Inferred | DataType::CoerceToString => {
            format!("uInnerHTML(\"{name}\", {BS_VAR_PREFIX}{name});")
        }
        _ => {
            format!("uInnerHTML(\"{name}\", wsx.get_{BS_VAR_PREFIX}{name}());")
        }
    }
}

pub fn combine_vec_to_js(
    collection: &Vec<Value>,
    line_number: u32,
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

pub fn collection_to_js(collection: &Value, line_number: u32) -> Result<String, CompileError> {
    match collection {
        Value::Tuple(args) => {
            let mut nodes = Vec::new();
            for arg in args {
                nodes.push(arg.value.to_owned());
            }
            combine_vec_to_js(&nodes, line_number)
        }
        _ => Err(CompileError {
            msg: "Non-tuple AST node given to collection_to_js".to_string(),
            line_number: 0,
        }),
    }
}
