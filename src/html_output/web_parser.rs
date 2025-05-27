use super::js_parser::{expression_to_js, expressions_to_js};
use crate::html_output::js_parser::combine_vec_to_js;
use crate::parsers::ast_nodes::{Arg, Expr};
use crate::parsers::scene::{SceneIngredients, StyleFormat, parse_scene};
use crate::tokens::VarVisibility;
use crate::{
    CompileError, ErrorType, bs_types::DataType, parsers::ast_nodes::AstNode,
    settings::BS_VAR_PREFIX,
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

        match node {
            // Scenes at the top level of a block
            AstNode::Reference(Expr::Scene(scene_body, scene_style, scene_id), _) => {
                code_module.push_str(&format!("\n{indentation}"));
                let top_level_scene_format = match target {
                    Target::Web => StyleFormat::Markdown,
                    Target::JS => StyleFormat::JSString,
                    Target::Raw => StyleFormat::Raw,
                    Target::Wasm => StyleFormat::WasmString,
                };

                let parsed_scene = parse_scene(
                    SceneIngredients {
                        scene_body,
                        scene_style,
                        inherited_style: &None,
                        scene_id: scene_id.to_owned(),
                        format_context: top_level_scene_format,
                    },
                    &mut code_module,
                )?;

                match target {
                    Target::Web => {
                        content_output.push_str(&parsed_scene);
                    }
                    Target::JS => {
                        code_module.push_str(&format!(
                            "const {BS_VAR_PREFIX}{} = `{}`;",
                            scene_id, parsed_scene
                        ));
                    }
                    Target::Raw => {
                        code_module.push_str(&format!(
                            "const {BS_VAR_PREFIX}{} = `{}`;",
                            scene_id, parsed_scene
                        ));
                    }
                    Target::Wasm => {}
                }
            }

            // JAVASCRIPT / WASM
            AstNode::Declaration(name, expr, public, data_type, start_pos) => {
                code_module.push_str(&format!("\n{indentation}"));

                match data_type {
                    DataType::Float(mutable)
                    | DataType::Int(mutable)
                    | DataType::String(mutable) => {
                        let var_dec = match public {
                            VarVisibility::Public | VarVisibility::Private => {
                                if *mutable {
                                    &format!(
                                        "this.{BS_VAR_PREFIX}{name} = {};",
                                        expression_to_js(expr, start_pos, "")?
                                    )
                                } else {
                                    &format!(
                                        "static {BS_VAR_PREFIX}{name} = {};",
                                        expression_to_js(expr, start_pos, "")?
                                    )
                                }
                            }

                            VarVisibility::Temporary => {
                                if *mutable {
                                    &format!(
                                        "let {BS_VAR_PREFIX}{name} = {};",
                                        expression_to_js(expr, start_pos, "")?
                                    )
                                } else {
                                    &format!(
                                        "const {BS_VAR_PREFIX}{name} = {};",
                                        expression_to_js(expr, start_pos, "")?
                                    )
                                }
                            }
                        };

                        code_module.push_str(var_dec);
                    }

                    DataType::Scene(..) => {
                        match expr {
                            Expr::Scene(scene_body, scene_styles, id) => {
                                let scene_to_js_string = parse_scene(
                                    SceneIngredients {
                                        scene_body,
                                        scene_style: scene_styles,
                                        inherited_style: &None,
                                        scene_id: id.to_owned(),
                                        format_context: StyleFormat::JSString,
                                    },
                                    &mut code_module,
                                )?;

                                let var_dec = format!(
                                    "const {BS_VAR_PREFIX}{name} = `{}`;",
                                    scene_to_js_string
                                );

                                code_module.push_str(&var_dec);
                            }

                            _ => {
                                return Err(CompileError {
                                    msg: "Scene declaration must be a scene".to_string(),
                                    start_pos: start_pos.to_owned(),
                                    end_pos: expr.dimensions(),
                                    error_type: ErrorType::Type,
                                });
                            }
                        };
                    }

                    DataType::Object(_) => {
                        let var_dec = format!(
                            "const {BS_VAR_PREFIX}{name} = {};",
                            expression_to_js(expr, start_pos, "")?
                        );

                        code_module.push_str(&var_dec);
                    }

                    DataType::Block(..) => {
                        match expr {
                            Expr::Block(args, body, ..) => {
                                let mut func = format!("function {BS_VAR_PREFIX}{}(", name);

                                for arg in args {
                                    func.push_str(&format!(
                                        "{BS_VAR_PREFIX}{} = {},",
                                        arg.name,
                                        expression_to_js(&arg.expr, start_pos, "")?
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
                                return Err(CompileError {
                                    msg: "Function declaration must be a function".to_string(),
                                    start_pos: start_pos.to_owned(),
                                    end_pos: expr.dimensions(),
                                    error_type: ErrorType::Type,
                                });
                            }
                        }
                    }

                    _ => {
                        code_module.push_str(&format!(
                            "const {BS_VAR_PREFIX}{name} = {};",
                            expression_to_js(expr, start_pos, "")?
                        ));
                    }
                };

                module_references.push(Arg {
                    name: name.to_owned(),
                    data_type: data_type.to_owned(),
                    expr: expr.to_owned(),
                });
            }

            AstNode::Mutation(name, assignment_op, expr, argument_accessed, start_pos) => {
                code_module.push_str(&format!("\n{indentation}"));

                code_module.push_str(&format!("{BS_VAR_PREFIX}{name}",));

                for index in argument_accessed {
                    code_module.push_str(&format!("[{}]", index));
                }

                code_module.push_str(&format!(
                    " {} {};",
                    assignment_op.to_js(),
                    expression_to_js(expr, start_pos, "")?
                ));
            }

            AstNode::FunctionCall(name, arguments, _, argument_accessed, start_pos) => {
                code_module.push_str(indentation);
                code_module.push_str(&format!(
                    "{BS_VAR_PREFIX}{}({});",
                    name,
                    combine_vec_to_js(arguments, start_pos)?
                ));
                for index in argument_accessed {
                    code_module.push_str(&format!("[{}]", index));
                }
            }

            AstNode::Return(expressions, start_pos) => {
                code_module.push_str(&format!("\n{indentation}"));
                code_module.push_str(&format!(
                    "return {};",
                    expressions_to_js(expressions, start_pos, "")?
                ));
            }

            AstNode::If(condition, if_true, start_pos) => {
                code_module.push_str(&format!("\n\n{indentation}"));

                let if_block_body = if_true.get_block_nodes();

                code_module.push_str(&format!(
                    "if ({}) {{\n{}\n{indentation}}}\n{indentation}",
                    expression_to_js(condition, start_pos, "")?,
                    parse(if_block_body, &format!("{indentation}{JS_INDENT}"), target)?.code_module
                ));
            }

            // DIRECT INSERTION OF JS / CSS into the page
            AstNode::JS(js_string, ..) => {
                code_module.push_str(js_string);
            }

            AstNode::Css(css_string, ..) => {
                css.push_str(css_string);
            }

            AstNode::ForLoop(index, iterated_item, loop_body, start_pos) => {
                code_module.push_str(&format!("\n\n{indentation}"));

                let length_access = if iterated_item.is_collection() {
                    ".length()"
                } else {
                    ""
                };

                code_module.push_str(&format!(
                    "for (let {BS_VAR_PREFIX}{} = 0; {BS_VAR_PREFIX}{} < {}{}; {BS_VAR_PREFIX}{}++) {{{}\n{indentation}}}\n{indentation}",
                    index.name,
                    index.name,
                    expression_to_js(iterated_item, start_pos, "")?,
                    length_access,
                    index.name,
                    parse(loop_body.get_block_nodes(), &format!("{indentation}{JS_INDENT}"), target)?.code_module
                ));
            }

            AstNode::WhileLoop(condition, loop_body, start_pos) => {
                code_module.push_str(&format!("\n\n{indentation}"));

                code_module.push_str(&format!(
                    "while ({}) {{\n{}\n{indentation}}}\n{indentation}",
                    expression_to_js(condition, start_pos, "")?,
                    parse(
                        loop_body.get_block_nodes(),
                        &format!("{indentation}{JS_INDENT}"),
                        target
                    )?
                    .code_module
                ));
            }

            AstNode::Expression(expr, ..) => {
                code_module.push_str(&expression_to_js(expr, &node.dimensions(), "")?);
            }

            // Ignore Everything Else
            _ => {
                return Err(CompileError {
                    msg: format!(
                        "Unexpected node type: {:?}. Compiler bug, should be caught at earlier stage",
                        node
                    ),
                    start_pos: node.dimensions(),
                    end_pos: node.dimensions(),
                    error_type: ErrorType::Compiler,
                });
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
