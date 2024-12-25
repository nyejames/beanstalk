use crate::{bs_types::DataType, parsers::ast_nodes::AstNode, settings::BS_VAR_PREFIX, CompileError, Token};

pub fn new_wat_var(
    id: &String,
    expr: &AstNode,
    datatype: &DataType,
    wat_global_initialisation: &mut String,
    line_number: u32,
) -> Result<String, CompileError> {
    match datatype {
        DataType::Float => {
            wat_global_initialisation.push_str(&format!(
                "(global.set ${BS_VAR_PREFIX}{id} {})",
                expression_to_wat(&expr)?
            ));

            Ok(format!(
                "
                \n(global ${BS_VAR_PREFIX}{id} (export \"{BS_VAR_PREFIX}{id}\") (mut f64) (f64.const 0))
                \n(func (export \"get_{BS_VAR_PREFIX}{id}\") (result f64) (global.get ${BS_VAR_PREFIX}{id}))",
            ))
        }

        DataType::Int => {
            wat_global_initialisation.push_str(&format!(
                "(global.set ${BS_VAR_PREFIX}{id} {})",
                expression_to_wat(&expr)?
            ));

            Ok(format!(
                "
                \n(global ${BS_VAR_PREFIX}{id} (export \"{BS_VAR_PREFIX}{id}\") (mut i64) (i64.const 0))
                \n(func (export \"get_{BS_VAR_PREFIX}{id}\") (result i64) (global.get ${BS_VAR_PREFIX}{id}))",
            ))
        }

        _ => {
            Err(CompileError {
                msg: "Unsupported datatype found in WAT var creation".to_string(),
                line_number,
            })
        }
    }
}

pub fn expression_to_wat(expr: &AstNode) -> Result<String, CompileError> {
    let mut wat = String::new();

    match expr {
        AstNode::RuntimeExpression(nodes, datatype, line_number) => match datatype {
            &DataType::Float => {
                return float_expr_to_wat(nodes);
            }
            _ => {
                return Err(CompileError {
                    msg: "Unsupported datatype found in expression sent to WAT parser".to_string(),
                    line_number: line_number.to_owned(),
                });
            }
        },

        AstNode::Literal(token, line_number) => match token {
            Token::FloatLiteral(value) => {
                wat.push_str(&format!("\n(f64.const {})", value.to_string()));
            }
            Token::IntLiteral(value) => {
                wat.push_str(&format!("\n(i64.const {})", value.to_string()));
            }
            _ => {
                return Err(CompileError {
                    msg: "Compiler error: Wrong literal type found in expression sent to WAT parser".to_string(),
                    line_number: line_number.to_owned(),
                });
            }
        },

        _ => {
            return Err(CompileError {
                msg: "Compiler Bug: Invalid AST node given to expression_to_wat (wat parser): None".to_string(),
                line_number: 0,
            });
        }
    }

    Ok(wat)
}

pub fn _new_wat_function() {}

fn float_expr_to_wat(nodes: &Vec<AstNode>) -> Result<String, CompileError> {
    let mut wat: String = String::new();

    for node in nodes {
        match node {
            AstNode::Literal(token, line_number) => {
                match token {
                    Token::FloatLiteral(value) => {
                        wat.push_str(&format!(" f64.const {}", value));
                    }
                    _ => {
                        return Err(CompileError {
                            msg: "Compiler error: Wrong literal type found in expression sent to WAT parser".to_string(),
                            line_number: line_number.to_owned(),
                        });
                    }
                }
            }

            AstNode::VarReference(name, ..) | AstNode::ConstReference(name, ..) => {
                wat.push_str(&format!(" global.get ${BS_VAR_PREFIX}{name}"));
            }

            AstNode::BinaryOperator(op, _, line_number) => {
                let wat_op = match op {
                    Token::Add => " f64.add",
                    Token::Subtract => " f64.sub",
                    Token::Multiply => " f64.mul",
                    Token::Divide => " f64.div",
                    _ => {
                        return Err(CompileError {
                            msg: "Unsupported operator found in operator stack when parsing an expression into WAT".to_string(),
                            line_number: line_number.to_owned(),
                        });
                    }
                };

                wat.push_str(wat_op);
            }

            _ => {
                return Err(CompileError {
                    msg: "Compiler Bug: unknown AST node found in expression when parsing float expression into WAT".to_string(),
                    line_number: 0,
                });
            }
        }
    }

    Ok(wat)
}

// if operators_stack.len() > 0 && output_stack.len() > 0 {
//     let operator = match operators_stack.pop() {
//         Some(op) => match op {
//             Token::Add => "f64.add",
//             Token::Subtract => "f64.sub",
//             Token::Multiply => "f64.mul",
//             Token::Divide => "f64.div",
//             _ => {
//                 red_ln!("Unsupported operator found in operator stack when parsing an expression into WAT");
//                 return String::new();
//             }
//         }
//         None => {
//             red_ln!("No operator found in operator stack when parsing an expression into WAT");
//             return String::new();
//         }
//     };

//     // CURRENTLY DOES ZERO VALUE IF SOMETHING WENT WRONG HERE
//     let right_value = format!("f64.const {}", value);
//     wat.push_str(&format!("({} ({}) ({}))", operator, output_stack.pop().unwrap_or(String::from("0")), right_value));
// } else {
