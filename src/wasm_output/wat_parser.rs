use crate::parsers::ast_nodes::{NodeInfo, Value};
use crate::tokenizer::TokenPosition;
use crate::{
    bs_types::DataType, parsers::ast_nodes::AstNode, settings::BS_VAR_PREFIX, CompileError,
    ErrorType, Token,
};

pub fn new_wat_var(
    id: &String,
    expr: &Value,
    datatype: &DataType,
    wat_global_initialisation: &mut String,
    start_pos: &TokenPosition,
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

        _ => Err(CompileError {
            msg: format!(
                "Unsupported datatype found in WAT var creation: {:?}",
                datatype
            ),
            start_pos: TokenPosition {
                line_number: start_pos.line_number,
                char_column: start_pos.char_column,
            },
            end_pos: TokenPosition {
                line_number: start_pos.line_number,
                char_column: start_pos.char_column + expr.dimensions().char_column,
            },
            error_type: ErrorType::TypeError,
        }),
    }
}

pub fn expression_to_wat(expr: &Value) -> Result<String, CompileError> {
    let mut wat = String::new();

    match expr {
        Value::Runtime(nodes, data_type) => {
            if data_type == &DataType::Float {
                wat.push_str(&float_expr_to_wat(nodes)?);
            }
        }
        Value::Float(value) => wat.push_str(&format!(" f64.const {}", value.to_string())),
        Value::Int(value) => wat.push_str(&format!(" i64.const {}", value.to_string())),
        Value::Bool(value) => wat.push_str(&format!(" i64.const {}", value.to_string())),

        _ => {
            let dimensions = expr.dimensions();
            return Err(CompileError {
                msg: format!(
                    "Invalid AST node given to expression_to_wat (wat parser): {:?}",
                    expr
                ),
                start_pos: TokenPosition {
                    line_number: dimensions.line_number,
                    char_column: dimensions.char_column,
                },
                end_pos: TokenPosition {
                    line_number: dimensions.line_number,
                    char_column: dimensions.char_column + 1,
                },
                error_type: ErrorType::Compiler,
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
            AstNode::Literal(token, _) => match token {
                Value::Float(value) => {
                    wat.push_str(&format!(" f64.const {}", value));
                }
                _ => {
                    let first_node_dimensions = nodes[0].dimensions();
                    return Err(CompileError {
                        msg: format!(
                            "Wrong literal type found in expression sent to WAT parser: {:?}",
                            token
                        ),
                        start_pos: TokenPosition {
                            line_number: first_node_dimensions.line_number,
                            char_column: first_node_dimensions.char_column,
                        },
                        end_pos: TokenPosition {
                            line_number: nodes[nodes.len() - 1].dimensions().line_number,
                            char_column: nodes[nodes.len() - 1].dimensions().char_column,
                        },
                        error_type: ErrorType::Compiler,
                    });
                }
            },

            AstNode::BinaryOperator(op, pos) => {
                let wat_op = match op {
                    Token::Add => " f64.add",
                    Token::Subtract => " f64.sub",
                    Token::Multiply => " f64.mul",
                    Token::Divide => " f64.div",
                    _ => {
                        return Err(CompileError {
                            msg: format!("Unsupported operator found in operator stack when parsing an expression into WAT: {:?}", op),
                            start_pos: TokenPosition {
                                line_number: pos.line_number,
                                char_column: pos.char_column,
                            },
                            end_pos: TokenPosition {
                                line_number: pos.line_number,
                                char_column: pos.char_column + 1,
                            },
                            error_type: ErrorType::Syntax
                        });
                    }
                };

                wat.push_str(wat_op);
            }

            _ => {
                let first_node_dimensions = nodes[0].dimensions();
                return Err(CompileError {
                    msg: format!("Unknown AST node found in expression when parsing float expression into WAT: {:?}", node),
                    start_pos: TokenPosition {
                        line_number: first_node_dimensions.line_number,
                        char_column: first_node_dimensions.char_column,
                    },
                    end_pos: TokenPosition {
                        line_number: nodes[nodes.len() - 1].dimensions().line_number,
                        char_column: nodes[nodes.len() - 1].dimensions().char_column,
                    },
                    error_type: ErrorType::Compiler,
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
