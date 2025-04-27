use crate::parsers::ast_nodes::{Expr, Operator};
use crate::tokenizer::TokenPosition;
use crate::{
    CompileError, ErrorType, Token, bs_types::DataType, parsers::ast_nodes::AstNode,
    settings::BS_VAR_PREFIX,
};

// Create everything necessary in JS
pub fn expression_to_js(expr: &Expr, start_pos: &TokenPosition) -> Result<String, CompileError> {
    let mut js = String::new(); // Open the template string

    match expr {
        Expr::Runtime(nodes, expression_type) => {
            for node in nodes {
                match node {
                    AstNode::Literal(value, _) => match value {
                        Expr::Float(value) => {
                            js.push_str(&value.to_string());
                        }

                        Expr::Int(value) => {
                            js.push_str(&value.to_string());
                        }

                        Expr::String(value) => {
                            js.push_str(&format!("\"{}\"", value));
                        }

                        Expr::Bool(value) => {
                            js.push_str(&value.to_string());
                        }

                        Expr::Reference(id, _, argument_accessed) => {
                            // All just JS for now
                            js.push_str(&format!(" {BS_VAR_PREFIX}{id} "));
                            for index in argument_accessed {
                                js.push_str(&format!("[{}]", index));
                            }
                            /*
                                js.push_str(&format!("wsx.get_{BS_VAR_PREFIX}{id}()"));
                            */
                        }

                        _ => {
                            return Err(CompileError {
                                msg: format!(
                                    "Compiler Bug (Not Implemented yet): Invalid argument type for function call (js_parser): {:?}",
                                    value
                                ),
                                start_pos: start_pos.to_owned(),
                                end_pos: expr.dimensions(),
                                error_type: ErrorType::Compiler,
                            });
                        }
                    },

                    AstNode::Operator(op, token_position) => match op {
                        Operator::Add => js.push_str(" + "),
                        Operator::Subtract => js.push_str(" - "),
                        Operator::Multiply => js.push_str(" * "),
                        Operator::Divide => js.push_str(" / "),
                        Operator::Exponent => js.push_str(" ** "),
                        Operator::Modulus => js.push_str(" % "),
                        // Operator::Remainder => js.push_str(" % "),
                        Operator::Root => js.push_str(" ** (1/2)"),

                        // Logical
                        Operator::Equal => js.push_str(" === "),
                        Operator::NotEqual => js.push_str(" !== "),
                        Operator::GreaterThan => js.push_str(" > "),
                        Operator::GreaterThanOrEqual => js.push_str(" >= "),
                        Operator::LessThan => js.push_str(" < "),
                        Operator::LessThanOrEqual => js.push_str(" <= "),
                        Operator::And => js.push_str(" && "),
                        Operator::Or => js.push_str(" || "),
                    },

                    AstNode::FunctionCall(name, args, _, arguments_accessed, ..) => {
                        js.push_str(&format!(
                            " {}({})",
                            name,
                            combine_vec_to_js(args, start_pos)?
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
                DataType::String(_) | DataType::Float(_) | DataType::Int(_) => {}
                DataType::CoerceToString(_) => {
                    js.insert_str(0, "String(");
                    js.push(')');
                }
                _ => {
                    return Err(CompileError {
                        msg: format!(
                            "Compiler Bug: Haven't implemented this type yet in expression_to_js: {:?}",
                            expression_type
                        ),
                        start_pos: start_pos.to_owned(),
                        end_pos: expr.dimensions(),
                        error_type: ErrorType::Compiler,
                    });
                }
            }
        }

        Expr::Float(value) => {
            js.push_str(&value.to_string());
        }

        Expr::Int(value) => {
            let as_float = *value as f64;
            js.push_str(&as_float.to_string());
        }

        Expr::String(value) => {
            js.push_str(&format!("\"{}\"", value));
        }

        Expr::Reference(name, _, arguments_accessed) => {
            js.push_str(&format!(" {BS_VAR_PREFIX}{name} "));
            for index in arguments_accessed {
                js.push_str(&format!("[{}]", index));
            }
        }

        // If the expression is just a tuple,
        // then it should automatically destructure into multiple arguments like this
        Expr::StructLiteral(args) => {
            let mut structure = String::from("{{");
            for (index, arg) in args.iter().enumerate() {
                let arg_name = if arg.name.is_empty() {
                    index.to_string()
                } else {
                    arg.name.to_owned()
                };

                let arg_value = expression_to_js(&arg.value, start_pos)?;

                structure.push_str(&format!(
                    "{arg_name}:{arg_value}{}",
                    if index < args.len() - 1 { "," } else { "" }
                ));
            }
        }

        Expr::Collection(items, _) => {
            js.push_str(&combine_vec_to_js(items, start_pos)?);
        }

        Expr::None => {
            js.push_str("null ");
        }

        Expr::Bool(value) => {
            js.push_str(&value.to_string());
        }

        _ => {
            return Err(CompileError {
                msg: format!("Invalid AST node given to expression_to_js: {:?}", expr),
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
    collection: &[Expr],
    line_number: &TokenPosition,
) -> Result<String, CompileError> {
    let mut js = String::new();

    for (i, node) in collection.iter().enumerate() {
        // Make sure correct commas at end of each element but not last one
        js.push_str(&format!(
            "{}{}",
            expression_to_js(node, line_number)?,
            if i < collection.len() - 1 { "," } else { "" }
        ));
    }

    Ok(js)
}
