use colour::red_ln;

use crate::{bs_types::DataType, parsers::ast_nodes::AstNode, settings::BS_VAR_PREFIX, CompileError, Token};

// Create everything necessary in JS
// Break out pieces in WASM calls
pub fn expression_to_js(expr: &AstNode) -> Result<String, CompileError> {
    let mut js = String::new(); //Open the template string

    match expr {
        AstNode::RuntimeExpression(nodes, expression_type, line_number) => {
            for node in nodes {
                match node {
                    AstNode::Literal(token, _) => match token {
                        Token::FloatLiteral(value) => {
                            js.push_str(&value.to_string());
                        }
                        Token::IntLiteral(value) => {
                            js.push_str(&value.to_string());
                        }
                        Token::StringLiteral(value) => {
                            js.push_str(&format!("\"{}\"", value));
                        }
                        _ => {
                            return Err(CompileError {
                                msg: "unknown literal found in expression".to_string(),
                                line_number: line_number.to_owned(),
                            });
                        }
                    },

                    AstNode::VarReference(name, data_type, _)
                    | AstNode::ConstReference(name, data_type, _) => {
                        // If it's a string, it will just be pure JS, no WASM
                        match data_type {
                            DataType::String | DataType::Scene => {
                                js.push_str(&format!(" {BS_VAR_PREFIX}{name}"))
                            }
                            _ => js.push_str(&format!(" wsx.get_{BS_VAR_PREFIX}{name}()")),
                        }
                    }

                    AstNode::CollectionAccess(name, index, ..)
                    | AstNode::TupleAccess(name, index, ..) => {
                        js.push_str(&format!("{BS_VAR_PREFIX}{name}[{index}]"));
                    }

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

                    AstNode::Tuple(_, _) => {
                        js.push_str(&format!("[{}]", collection_to_js(node)?));
                    }

                    _ => {
                        return Err(CompileError {
                            msg: "unknown AST node found in expression when parsing an expression into JS".to_string(),
                            line_number: line_number.to_owned(),
                        });
                    }
                }
            }

            match expression_type {
                DataType::String | DataType::Float | DataType::Int => {}
                DataType::CoerseToString => {
                    js.insert_str(0, "String(");
                    js.push_str(")");
                }
                _ => {
                    red_ln!(
                        "Have not implimented this type yet in expression_to_js: {:?}",
                        expression_type
                    );
                }
            }
        }

        AstNode::Literal(token, line_number) => match token {
            Token::FloatLiteral(value) => {
                js.push_str(&value.to_string());
            }
            Token::IntLiteral(value) => {
                js.push_str(&value.to_string());
            }
            Token::StringLiteral(value) => {
                js.push_str(&format!("\"{}\"", value));
            }
            _ => {
                return Err(CompileError {
                    msg: "unknown literal found in expression".to_string(),
                    line_number: line_number.to_owned(),
                });
            }
        },

        AstNode::VarReference(name, data_type, _) | AstNode::ConstReference(name, data_type, _) => {
            match data_type {
                DataType::String | DataType::Scene => {
                    js.push_str(&format!("`${{{BS_VAR_PREFIX}{name}}}`"))
                }
                _ => js.push_str(&format!("`${{wsx.get_{BS_VAR_PREFIX}{name}()}}`")),
            }
        }

        // If the expression is just a tuple,
        // then it should automatically destructure into multiple arguments like this
        AstNode::Tuple(references, _) => {
            let mut values = Vec::new();
            for reference in references {
                values.push(reference.value.to_owned());
            }
            js.push_str(&format!("[{}]", combine_vec_to_js(&values)?));
        }

        AstNode::FunctionCall(name, arguments, ..) => {
            js.push_str(&function_call_to_js(name, arguments.to_owned())?);
        }

        _ => {
            return Err(CompileError {
                msg: "Compiler Bug: Invalid AST node given to expression_to_js".to_string(),
                line_number: 0,
            });
        }
    }

    Ok(js)
}

pub fn create_reference_in_js(name: &String, data_type: &DataType) -> String {
    match data_type {
        DataType::String | DataType::Scene | DataType::Inferred | DataType::CoerseToString => {
            format!("uInnerHTML(\"{name}\", {BS_VAR_PREFIX}{name});")
        }
        _ => {
            format!("uInnerHTML(\"{name}\", wsx.get_{BS_VAR_PREFIX}{name}());")
        }
    }
}

pub fn function_call_to_js(name: &String, arguments: Vec<AstNode>) -> Result<String, CompileError> {
    let mut js = format!("{BS_VAR_PREFIX}{name}(");

    for argument in arguments {
        match argument {
            AstNode::Literal(token, _) => match token {
                Token::StringLiteral(value) => {
                    js.push_str(&format!("\"{}\",", value));
                }
                Token::FloatLiteral(value) => {
                    js.push_str(&format!("{},", value));
                }
                Token::IntLiteral(value) => {
                    js.push_str(&format!("{},", value));
                }
                Token::BoolLiteral(value) => {
                    js.push_str(&format!("{},", value));
                }
                _ => {}
            }

            AstNode::CollectionAccess(collection_name, index_accessed, ..)
            | AstNode::TupleAccess(collection_name, index_accessed, ..) => {
                js.push_str(&format!("{collection_name}[{index_accessed}],"));
            }

            AstNode::RuntimeExpression(expr, data_type, line_number) => {
                js.push_str(&format!(
                    "{},",
                    expression_to_js(&AstNode::RuntimeExpression(
                        expr.clone(),
                        data_type.to_owned(),
                        line_number.to_owned(),
                    ))?
                ));
            }

            AstNode::VarReference(name, ..) | AstNode::ConstReference(name, ..) => {
                js.push_str(&format!("{},", name));
            }

            AstNode::FunctionCall(function_name, args, ..) => {
                js.push_str(&function_call_to_js(&function_name, args)?);
            }

            AstNode::Empty(..) => {}

            _ => {
                return Err(CompileError {
                    msg: "Compiler Bug: Invalid argument type for function call (js_parser)".to_string(),
                    line_number: 0,
                });
            }
        }
    }

    js.push_str(") ");

    Ok(js)
}

pub fn combine_vec_to_js(collection: &Vec<AstNode>) -> Result<String, CompileError> {
    let mut js = String::new();

    let mut i: usize = 0;
    for node in collection {
        // Make sure correct commas at end of each element but not last one
        js.push_str(&format!(
            "{}{}",
            expression_to_js(&node)?,
            if i < collection.len() - 1 { "," } else { "" }
        ));
        i += 1;
    }

    Ok(js)
}

pub fn collection_to_js(collection: &AstNode) -> Result<String, CompileError> {
    match collection {
        AstNode::Tuple(args, _) => {
            let mut nodes = Vec::new();
            for arg in args {
                nodes.push(arg.value.to_owned());
            }
            combine_vec_to_js(&nodes)
        }
        _ => {
            Err(CompileError {
                msg: "Non-tuple AST node given to collection_to_js".to_string(),
                line_number: 0,
            })
        }
    }
}

// pub fn _collection_to_vec_of_js(collection: &AstNode) -> Vec<String> {
//     let mut js = Vec::new();

//     match collection {
//         AstNode::Tuple(nodes, _) => {
//             for node in nodes {
//                 js.push(expression_to_js(node));
//             }
//         }
//         _ => {
//             red_ln!("Non-tuple AST node given to collection_to_vec_of_js");
//         }
//     }

//     js
// }
