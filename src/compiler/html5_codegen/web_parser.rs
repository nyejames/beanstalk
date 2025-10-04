use crate::compiler::compiler_errors::ErrorType;
use crate::compiler::parsers::tokens::TextLocation;


use super::js_parser::{expression_to_js, expressions_to_js};
use crate::compiler::compiler_errors::CompileError;
use crate::compiler::html5_codegen::js_parser::combine_vec_to_js;
use crate::compiler::parsers::ast_nodes::{Arg, AstNode};
use crate::compiler::parsers::expressions::expression::ExpressionKind;
use crate::compiler::parsers::template::{StyleFormat, TemplateIngredients, parse_template};
use crate::{
    compiler::datatypes::DataType, compiler::parsers::ast_nodes::NodeKind, return_compiler_error,
    return_type_error, settings::BS_VAR_PREFIX,
};

pub const JS_INDENT: &str = "    ";

pub struct ParserOutput {
    // For web this would be what's written the HTML file
    pub content_output: String,

    // This is JS atm, but will be Wasm in the future
    pub code_module: String,
    // Eventually will have a separate "bindings" string
    // This will be glue code specific to the target output environment
    // For web this will just be the JS that will glue the Wasm modules together
    // pub bindings: String,

    // May not ever use.
    // Might just stick with inlined styles.
    // Classes can be used for more complex built-in styling that will go in the standard HTML project BS CSS reset file.
    // pub css: String,
}

#[derive(PartialEq)]
#[allow(dead_code)]
pub enum Target {
    Web,
    Wasm,
    JS,
    Raw,
}

// Parse ast into valid WAT, JS, HTML and CSS
pub fn parse(
    ast: &[AstNode],
    indentation: &str,
    target: &Target,
) -> Result<ParserOutput, CompileError> {
    let mut code_module = String::new();
    // let mut types = TypeSection::new();
    // let mut wasm_export_section = ExportSection::new();
    // let mut type_index = 0;

    let mut content_output = String::new();
    let mut css = String::new();
    // let mut page_title = String::new();

    // Keeps track of whether a reference has already been used
    // This is to prevent duplicate JS code from updating the same element
    let mut module_references: Vec<Arg> = Vec::new();

    // Parse HTML
    for node in ast {
        // code_module.push('\n');

        match &node.kind {
            // Scenes at the top level of a block
            // MAY NOT DO THIS ANY MORE.
            // Possibly a new file type for scene-specific top-level stuff.
            // And a different way of injecting HTML into DOM from normal BS
            // that makes a lot more sense
            NodeKind::Reference(expr) => {
                //     code_module.push_str(&format!("\n{indentation}"));
                //     let top_level_scene_format = match target {
                //         Target::Web => StyleFormat::Markdown,
                //         Target::JS => StyleFormat::JSString,
                //         Target::Raw => StyleFormat::Raw,
                //         Target::Wasm => StyleFormat::WasmString,
                //     };
                //
                //     let parsed_scene = parse_scene(
                //         SceneIngredients {
                //             scene_body,
                //             scene_style,
                //             inherited_style: &None,
                //             scene_id: scene_id.to_owned(),
                //             format_context: top_level_scene_format,
                //         },
                //         &mut code_module,
                //         &expr.location,
                //     )?;
                //
                //     match target {
                //         Target::Web => {
                //             content_output.push_str(&parsed_scene);
                //         }
                //         Target::JS => {
                //             code_module.push_str(&format!(
                //                 "const {BS_VAR_PREFIX}{} = `{}`;",
                //                 scene_id, parsed_scene
                //             ));
                //         }
                //         Target::Raw => {
                //             code_module.push_str(&format!(
                //                 "const {BS_VAR_PREFIX}{} = `{}`;",
                //                 scene_id, parsed_scene
                //             ));
                //         }
                //         Target::Wasm => {}
                //     }
            }

            // JAVASCRIPT / WASM
            NodeKind::Declaration(name, expr, ..) => {
                code_module.push_str(&format!("\n{indentation}"));

                match expr.data_type {
                    DataType::Float(mutable)
                    | DataType::Int(mutable)
                    | DataType::String(mutable) => {
                        let var_dec = if mutable {
                            &format!(
                                "let {BS_VAR_PREFIX}{name} = {};",
                                expression_to_js(expr, "")?
                            )
                        } else {
                            &format!(
                                "const {BS_VAR_PREFIX}{name} = {};",
                                expression_to_js(expr, "")?
                            )
                        };

                        code_module.push_str(var_dec);
                    }

                    DataType::Template(..) => {
                        match &expr.kind {
                            ExpressionKind::Template(scene_body, scene_styles, id) => {
                                let scene_to_js_string = parse_template(
                                    TemplateIngredients {
                                        template_body: scene_body,
                                        template_style: scene_styles,
                                        inherited_style: &None,
                                        template_id: id.to_owned(),
                                        format_context: StyleFormat::JSString,
                                    },
                                    &mut code_module,
                                    &expr.location,
                                )?;

                                let var_dec = format!(
                                    "const {BS_VAR_PREFIX}{name} = `{}`;",
                                    scene_to_js_string
                                );

                                code_module.push_str(&var_dec);
                            }

                            _ => {
                                return_type_error!(
                                    node.location.to_owned(),
                                    "Scene declaration must be a scene",
                                )
                            }
                        };
                    }

                    DataType::Args(_) => {
                        let var_dec = format!(
                            "const {BS_VAR_PREFIX}{name} = {};",
                            expression_to_js(expr, "")?
                        );

                        code_module.push_str(&var_dec);
                    }

                    DataType::Function(..) => {
                        match &expr.kind {
                            ExpressionKind::Function(args, body, ..) => {
                                let mut func = format!("function {BS_VAR_PREFIX}{}(", name);

                                for arg in args {
                                    func.push_str(&format!(
                                        "{BS_VAR_PREFIX}{} = {},",
                                        arg.name,
                                        expression_to_js(&arg.value, "")?
                                    ));
                                }

                                func.push_str(") {");

                                // let utf16_units: Vec<u16> = rust_string.encode_utf16().collect();
                                let func_body =
                                    parse(body, &format!("{indentation}{JS_INDENT}"), target)?;

                                func.push_str(&format!(
                                    "{}\n{indentation}}}\n",
                                    func_body.code_module
                                ));

                                code_module.push_str(&func);
                            }

                            _ => {
                                return_type_error!(
                                    node.location.to_owned(),
                                    "Function declaration must be a function",
                                )
                            }
                        }
                    }

                    _ => {
                        code_module.push_str(&format!(
                            "const {BS_VAR_PREFIX}{name} = {};",
                            expression_to_js(expr, "")?
                        ));
                    }
                };

                module_references.push(Arg {
                    name: name.to_owned(),
                    value: expr.to_owned(),
                });
            }

            NodeKind::FunctionCall(name, arguments, ..) => {
                code_module.push_str(indentation);
                code_module.push_str(&format!(
                    "{BS_VAR_PREFIX}{}({})",
                    name,
                    combine_vec_to_js(arguments)?
                ));
            }

            NodeKind::Return(expressions, ..) => {
                code_module.push_str(&format!("\n{indentation}"));
                code_module.push_str(&format!("return {};", expressions_to_js(expressions, "")?));
            }

            NodeKind::If(condition, if_block_body, ..) => {
                code_module.push_str(&format!("\n\n{indentation}"));

                code_module.push_str(&format!(
                    "if ({}) {{\n{}\n{indentation}}}\n{indentation}",
                    expression_to_js(condition, "")?,
                    parse(
                        &if_block_body.ast,
                        &format!("{indentation}{JS_INDENT}"),
                        target
                    )?
                    .code_module
                ));
            }

            // DIRECT INSERTION OF JS / CSS into the page
            NodeKind::JS(js_string, ..) => {
                code_module.push_str(js_string);
            }

            NodeKind::Css(css_string, ..) => {
                css.push_str(css_string);
            }

            NodeKind::ForLoop(index, iterated_item, loop_body, ..) => {
                code_module.push_str(&format!("\n\n{indentation}"));

                let length_access = if iterated_item.kind.is_iterable() {
                    ".length()"
                } else {
                    ""
                };

                code_module.push_str(&format!(
                    "for (let {BS_VAR_PREFIX}{} = 0; {BS_VAR_PREFIX}{} < {}{}; {BS_VAR_PREFIX}{}++) {{{}\n{indentation}}}\n{indentation}",
                    index.name,
                    index.name,
                    expression_to_js(iterated_item, "")?,
                    length_access,
                    index.name,
                    parse(&loop_body.ast, &format!("{indentation}{JS_INDENT}"), target)?.code_module
                ));
            }

            NodeKind::WhileLoop(condition, loop_body, ..) => {
                code_module.push_str(&format!("\n\n{indentation}"));

                code_module.push_str(&format!(
                    "while ({}) {{\n{}\n{indentation}}}\n{indentation}",
                    expression_to_js(condition, "")?,
                    parse(&loop_body.ast, &format!("{indentation}{JS_INDENT}"), target)?
                        .code_module
                ));
            }

            NodeKind::Expression(expr, ..) => {
                code_module.push_str(&expression_to_js(expr, "")?);
            }

            // Ignore Everything Else
            _ => {
                return_compiler_error!(
                    "Unexpected node type: {:?}. Compiler bug, should be caught at earlier stage",
                    node
                )
            }
        }
    }

    // if config.html_meta.auto_site_title {
    //     page_title += &(" | ".to_owned() + &config.html_meta.site_title.clone());
    // }

    Ok(ParserOutput {
        content_output,
        code_module,
    })
}
