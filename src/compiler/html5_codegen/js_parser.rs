use crate::compiler::compiler_errors::CompileError;
use crate::compiler::compiler_errors::ErrorType;
use crate::compiler::datatypes::DataType;
use crate::compiler::html5_codegen::web_parser::JS_INDENT;
use crate::compiler::html5_codegen::web_parser::{Target, parse_to_html5};
use crate::compiler::parsers::ast_nodes::NodeKind;
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind, Operator};
use crate::compiler::parsers::template::{StyleFormat, TemplateIngredients, parse_template};
use crate::compiler::parsers::tokens::TextLocation;
use crate::return_compiler_error;
use crate::settings::BS_VAR_PREFIX;

// If there are multiple values, it gets wrapped in an array
pub fn expressions_to_js(
    expressions: &[Expression],
    indentation: &str,
) -> Result<String, CompileError> {
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
pub fn expression_to_js(expr: &Expression, indentation: &str) -> Result<String, CompileError> {
    let mut js = String::new(); // Open the template string

    match &expr.kind {
        ExpressionKind::Runtime(nodes) => {
            for node in nodes {
                match &node.kind {
                    NodeKind::Reference(value) => match &value.kind {
                        ExpressionKind::Float(value) => {
                            js.push_str(&value.to_string());
                        }

                        ExpressionKind::Int(value) => {
                            js.push_str(&value.to_string());
                        }

                        ExpressionKind::String(value) => {
                            js.push_str(&format!("\"{}\"", value));
                        }

                        ExpressionKind::Bool(value) => {
                            js.push_str(&value.to_string());
                        }

                        ExpressionKind::Reference(id, ..) => {
                            // All just JS for now
                            js.push_str(&format!("{BS_VAR_PREFIX}{id}"));
                        }

                        _ => {
                            return_compiler_error!(
                                "Compiler Bug (Not Implemented yet): Invalid argument type for function call (js_parser): {:?}",
                                value
                            )
                        }
                    },

                    NodeKind::Operator(op, ..) => match op {
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

                    NodeKind::FunctionCall(name, args, ..) => {
                        js.push_str(&format!(
                            "{BS_VAR_PREFIX}{}({})",
                            name,
                            combine_vec_to_js(args)?,
                        ));
                    }

                    _ => {
                        return_compiler_error!(
                            "Compiler Bug (Not Implemented yet): Invalid AST node found in expression when parsing an expression into JS: {:?}",
                            node
                        )
                    }
                }

                js.push(' '); // Formatting
            }

            match expr.data_type {
                DataType::String(_) | DataType::Float(_) | DataType::Int(_) | DataType::Bool(_) => {
                }
                DataType::CoerceToString(_) => {
                    js.insert_str(0, " String(");
                    js.push(')');
                }
                _ => {
                    return_compiler_error!(
                        "Compiler Bug: Haven't implemented this type yet in expression_to_js: {:?}",
                        expr.data_type
                    )
                }
            }
        }

        ExpressionKind::Float(value) => {
            js.push_str(&value.to_string());
        }

        ExpressionKind::Int(value) => {
            let as_float = *value as f64;
            js.push_str(&as_float.to_string());
        }

        ExpressionKind::String(value) => {
            js.push_str(&format!("\"{}\"", value));
        }

        ExpressionKind::Reference(name, ..) => {
            js.push_str(&format!("{BS_VAR_PREFIX}{name}"));
        }

        // If the expression is just a tuple,
        // then it should automatically destructure into multiple arguments like this
        ExpressionKind::Struct(args) => {
            let mut structure = String::from("{{\n");
            for (index, arg) in args.iter().enumerate() {
                let arg_name = if arg.name.is_empty() {
                    index.to_string()
                } else {
                    arg.name.to_owned()
                };

                let arg_value = expression_to_js(&arg.value, indentation)?;

                structure.push_str(&format!(
                    "{indentation}{JS_INDENT}{arg_name}: {arg_value}{}",
                    if index < args.len() - 1 { ",\n" } else { "" }
                ));
            }

            structure.push_str("\n}}\n");
            js.push_str(&structure);
        }

        ExpressionKind::Collection(items) => {
            js.push_str(&combine_vec_to_js(items)?);
        }

        ExpressionKind::Bool(value) => {
            js.push_str(&value.to_string());
        }

        // Scenes are basically just template strings when used entirely in JS
        ExpressionKind::Template(template_body, template_style, template_id) => {
            let parsed_template = parse_template(
                TemplateIngredients {
                    template_body,
                    template_style,
                    template_id: template_id.to_owned(),
                    inherited_style: &None,
                    format_context: StyleFormat::JSString,
                },
                &mut js,
                &expr.location,
            )?;

            js.push_str(&format!("`{parsed_template}`"));
        }

        // None pretty much only exists at compile time
        ExpressionKind::None => {}

        ExpressionKind::Function(args, body, ..) => {
            let mut func = "(".to_string();

            for arg in args {
                func.push_str(&format!(
                    "{BS_VAR_PREFIX}{} = {},",
                    arg.name,
                    expression_to_js(&arg.value, "")?
                ));
            }

            // Lambda
            func.push_str(") => {\n");

            // let utf16_units: Vec<u16> = rust_string.encode_utf16().collect();
            let func_body = parse_to_html5(body, indentation, &Target::JS)?;

            func.push_str(&format!("{}\n}}\n", func_body.js));

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
        DataType::Args(_) | DataType::Collection(_) => {
            format!("\nuInnerHTML(\"{name}\",{name});\n")
        }
        _ => {
            format!("\nuInnerHTML(\"{name}\",{name});\n")
        }
    }
}

pub fn combine_vec_to_js(collection: &[Expression]) -> Result<String, CompileError> {
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

//         let mut js_imports: String = format!(
//             "<script type=\"module\" src=\"./{}\"></script>",
//             &module
//                 .output_path
//                 .with_extension("js")
//                 .file_name()
//                 .unwrap()
//                 .to_string_lossy()
//         );
//
//         for import in &mut module.imports {
//             // Stripping the src folder from the import path,
//             // As this directory is removed in the output directory
//             let trimmed_import = import.0.strip_prefix("src/").unwrap_or(import.0);
//
//             js_imports += &format!(
//                 "<script type=\"module\" src=\"{}.js\"></script>",
//                 trimmed_import
//             );
//         }
//
//         module.html = compile_result
//             .html
//             .replace("<!--//js-modules-->", &js_imports);
//         module.js = compile_result.js;
