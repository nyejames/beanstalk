#[allow(unused_imports)]
use colour::{blue_ln, green_ln, red_ln};

use crate::html_output::web_parser::{JS_INDENT, Target, parse};
use crate::parsers::ast_nodes::{Expr, Operator};
use crate::parsers::scene::{SceneIngredients, StyleFormat, parse_scene};
use crate::tokenizer::TokenPosition;
use crate::{
    CompileError, ErrorType, bs_types::DataType, parsers::ast_nodes::AstNode,
    settings::BS_VAR_PREFIX,
};

// If there are multiple values, it gets wrapped in an array
pub fn expressions_to_js(expressions: &[Expr], indentation: &str) -> Result<String, CompileError> {
    let mut js = String::new();
    for expr in expressions {
        js.push_str(&expression_to_js(expr, indentation)?);
    }

    if expressions.len() > 0 {
        return Ok(format!("[{}]", js));
    }

    Ok(js)
}

// Create everything necessary in JS
pub fn expression_to_js(expr: &Expr, indentation: &str) -> Result<String, CompileError> {
    let mut js = String::new(); // Open the template string

    match expr {
        Expr::Runtime(nodes, expression_type) => {
            for node in nodes {
                match node {
                    AstNode::Reference(value, _) => match value {
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

                        Expr::Reference(id, ..) => {
                            // All just JS for now
                            js.push_str(&format!("{BS_VAR_PREFIX}{id}"));
                        }

                        _ => {
                            return Err(CompileError {
                                msg: format!(
                                    "Compiler Bug (Not Implemented yet): Invalid argument type for function call (js_parser): {:?}",
                                    value
                                ),
                                start_pos: TokenPosition::default(),
                                end_pos: expr.dimensions(),
                                error_type: ErrorType::Compiler,
                            });
                        }
                    },

                    AstNode::Operator(op, ..) => match op {
                        Operator::Add => js.push_str(" + "),
                        Operator::Subtract => js.push_str(" - "),
                        Operator::Multiply => js.push_str(" * "),
                        Operator::Divide => js.push_str(" / "),
                        Operator::Exponent => js.push_str(" ** "),
                        Operator::Modulus => js.push_str(" % "),
                        // Operator::Remainder => js.push_str(" % "),
                        Operator::Root => js.push_str(" ** (1/2)"),

                        // Logical
                        Operator::Equality => js.push_str(" === "),
                        Operator::NotEqual => js.push_str(" !== "),
                        Operator::GreaterThan => js.push_str(" > "),
                        Operator::GreaterThanOrEqual => js.push_str(" >= "),
                        Operator::LessThan => js.push_str(" < "),
                        Operator::LessThanOrEqual => js.push_str(" <= "),
                        Operator::And => js.push_str(" && "),
                        Operator::Or => js.push_str(" || "),
                    },

                    AstNode::FunctionCall(name, args, ..) => {
                        js.push_str(&format!(
                            "{BS_VAR_PREFIX}{}({})",
                            name,
                            combine_vec_to_js(args)?,
                        ));
                    }

                    _ => {
                        return Err(CompileError {
                            msg: format!(
                                "unknown AST node found in expression when parsing an expression into JS: {:?}",
                                node
                            ),
                            start_pos: TokenPosition::default(),
                            end_pos: expr.dimensions(),
                            error_type: ErrorType::Compiler,
                        });
                    }
                }

                js.push(' '); // Formatting
            }

            match expression_type {
                DataType::String(_) | DataType::Float(_) | DataType::Int(_) | DataType::Bool(_) => {
                }
                DataType::CoerceToString(_) => {
                    js.insert_str(0, " String(");
                    js.push(')');
                }
                _ => {
                    return Err(CompileError {
                        msg: format!(
                            "Compiler Bug: Haven't implemented this type yet in expression_to_js: {:?}",
                            expression_type
                        ),
                        start_pos: TokenPosition::default(),
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

        Expr::Reference(name, ..) => {
            js.push_str(&format!("{BS_VAR_PREFIX}{name}"));
        }

        // If the expression is just a tuple,
        // then it should automatically destructure into multiple arguments like this
        Expr::Args(args) => {
            let mut structure = String::from("{{\n");
            for (index, arg) in args.iter().enumerate() {
                let arg_name = if arg.name.is_empty() {
                    index.to_string()
                } else {
                    arg.name.to_owned()
                };

                let arg_value = expression_to_js(&arg.default_value, indentation)?;

                structure.push_str(&format!(
                    "{indentation}{JS_INDENT}{arg_name}: {arg_value}{}",
                    if index < args.len() - 1 { ",\n" } else { "" }
                ));
            }

            structure.push_str("\n}}\n");
            js.push_str(&structure);
        }

        Expr::Collection(items, _) => {
            js.push_str(&combine_vec_to_js(items)?);
        }

        Expr::Bool(value) => {
            js.push_str(&value.to_string());
        }

        // Scenes are basically just template strings when used entirely in JS
        Expr::Scene(scene_body, scene_style, scene_id) => {
            let parsed_scene = parse_scene(
                SceneIngredients {
                    scene_body,
                    scene_style,
                    scene_id: scene_id.to_owned(),
                    inherited_style: &None,
                    format_context: StyleFormat::JSString,
                },
                &mut js,
            )?;

            js.push_str(&format!("`{parsed_scene}`"));
        }

        // None pretty much only exists at compile time
        Expr::None => {}

        Expr::Block(args, body, ..) => {
            let mut func = "(".to_string();

            for arg in args {
                func.push_str(&format!(
                    "{BS_VAR_PREFIX}{} = {},",
                    arg.name,
                    expression_to_js(&arg.default_value, "")?
                ));
            }

            // Lambda
            func.push_str(") => {\n");

            // let utf16_units: Vec<u16> = rust_string.encode_utf16().collect();
            let func_body = parse(body, indentation, &Target::JS)?;

            func.push_str(&format!("{}\n}}\n", func_body.code_module));

            js.push_str(&func);
        }
    }

    Ok(js)
}

pub fn create_reactive_reference(name: &str, data_type: &DataType) -> String {
    match data_type {
        // DataType::Float | DataType::Int => {
        //     format!("uInnerHTML(\"{name}\", wsx.get_{BS_VAR_PREFIX}{name}());")
        // }
        DataType::Object(_) | DataType::Collection(_) => {
            format!("\nuInnerHTML(\"{name}\",{name});\n")
        }
        _ => {
            format!("\nuInnerHTML(\"{name}\",{name});\n")
        }
    }
}

pub fn combine_vec_to_js(collection: &[Expr]) -> Result<String, CompileError> {
    let mut js = String::new();

    for (i, node) in collection.iter().enumerate() {
        // Make sure correct commas at the end of each element but not the last one
        js.push_str(&format!(
            "{}{}",
            expression_to_js(node, "")?,
            if i < collection.len() - 1 { "," } else { "" }
        ));
    }

    Ok(js)
}

// pub fn access_args_to_js(accessed_members: &[Arg]) -> String {
//     let mut js = String::new();
//
//     for member in accessed_members {
//         match member.data_type {
//             DataType::Block(..) => {
//                 let arguments_js = expressions_to_js(arguments, "").unwrap_or_else(|_| "".to_owned());
//                 js.push_str(
//                     &format!(".{}({})", name, arguments_js)
//                 )
//                 match data_type {
//                     DataType::Block(..) => {
//
//                     }
//                     _ => {
//
//                     }
//                 }
//
//             }
//             _ => {
//                 js.push_str(&format!(".{}", member.name));
//             }
//         }
//     }
//
//     js
// }
