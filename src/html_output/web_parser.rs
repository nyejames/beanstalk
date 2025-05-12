use super::js_parser::{expression_to_js, expressions_to_js};
use crate::html_output::js_parser::combine_vec_to_js;
use crate::parsers::ast_nodes::{Arg, Expr};
use crate::parsers::scene::{SceneIngredients, StyleFormat, parse_scene};
use crate::settings::Config;
use crate::{
    CompileError, ErrorType, bs_types::DataType, parsers::ast_nodes::AstNode,
    settings::BS_VAR_PREFIX,
};
use wasm_encoder::Module;
use crate::tokens::VarVisibility;

pub struct ParserOutput {
    pub html: String,
    pub js: String,
    pub css: String,
    pub wasm: Module,
    pub page_title: String,
}

// Parse ast into valid WAT, JS, HTML and CSS
pub fn parse(
    ast: &[AstNode],
    config: &Config,
    wasm_module: &mut Module,
) -> Result<ParserOutput, CompileError> {
    let mut js = String::new();
    // let mut types = TypeSection::new();
    // let mut wasm_export_section = ExportSection::new();
    // let mut type_index = 0;

    let mut html = String::new();
    let mut css = String::new();
    let mut exp_id: usize = 0;
    let mut page_title = String::new();

    // Keeps track of whether a reference has already been used
    // This is to prevent duplicate JS code for updating the same element
    let mut module_references: Vec<Arg> = Vec::new();

    let mut class_id: usize = 0;

    // Parse HTML
    for node in ast {
        match node {
            // SCENES (HTML)
            AstNode::Literal(Expr::Scene(scene_body, scene_style, scene_head, scene_id), _) => {
                let parsed_scene = parse_scene(
                    SceneIngredients {
                        scene_body: &scene_body,
                        scene_style: &scene_style,
                        scene_head: &scene_head,
                        inherited_style: &None,
                        scene_id: scene_id.to_owned(),
                        format_context: StyleFormat::None,
                    },
                    &mut js,
                    &mut css,
                    &mut module_references,
                    &mut class_id,
                    &mut exp_id,
                    &config.html_meta,
                )?;

                html.push_str(&parsed_scene);
            }

            // JAVASCRIPT / WASM
            AstNode::VarDeclaration(name, expr, public, data_type, start_pos) => {
                match data_type {
                    DataType::Float(mutable)
                    | DataType::Int(mutable)
                    | DataType::String(mutable) => {

                        let var_dec = match public {
                            VarVisibility::Public | VarVisibility::Private => {
                                if *mutable {
                                    &format!(
                                        "this.{BS_VAR_PREFIX}{name}={};",
                                        expression_to_js(expr, start_pos)?
                                    )
                                } else {
                                    &format!(
                                        "static {BS_VAR_PREFIX}{name}={};",
                                        expression_to_js(expr, start_pos)?
                                    )
                                }
                            }

                            VarVisibility::Temporary => {
                                if *mutable {
                                    &format!(
                                        "let {BS_VAR_PREFIX}{name}={};",
                                        expression_to_js(expr, start_pos)?
                                    )
                                } else {
                                    &format!(
                                        "const {BS_VAR_PREFIX}{name}={};",
                                        expression_to_js(expr, start_pos)?
                                    )
                                }
                            }
                        };

                        js.push_str(var_dec);
                    }

                    DataType::Scene(..) => {
                        match expr {
                            Expr::Scene(scene_body, scene_styles, scene_head, id) => {
                                let mut created_css = String::new();

                                let scene_to_js_string = parse_scene(
                                    SceneIngredients {
                                        scene_body,
                                        scene_style: scene_styles,
                                        scene_head,
                                        inherited_style: &None,
                                        scene_id: id.to_owned(),
                                        format_context: StyleFormat::None,
                                    },
                                    &mut js,
                                    &mut created_css,
                                    &mut module_references,
                                    &mut class_id,
                                    &mut exp_id,
                                    &config.html_meta,
                                )?;

                                let var_dec = format!(
                                    "const {BS_VAR_PREFIX}{name} = `{}`;",
                                    scene_to_js_string
                                );

                                css.push_str(&created_css);
                                js.push_str(&var_dec);
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

                    DataType::Arguments(_) => {
                        let var_dec = format!(
                            "const {BS_VAR_PREFIX}{name}={};",
                            expression_to_js(expr, start_pos)?
                        );

                        js.push_str(&var_dec);
                    }
                    
                    DataType::Block(..) => {
                        match expr {
                            Expr::Block(args, body, ..) => {
                                let mut func = format!("function {}(", name);

                                for arg in args {
                                    func.push_str(&format!(
                                        "{BS_VAR_PREFIX}{} = {},",
                                        arg.name,
                                        expression_to_js(&arg.expr, start_pos)?
                                    ));
                                }

                                func.push_str("){");

                                // let utf16_units: Vec<u16> = rust_string.encode_utf16().collect();
                                let func_body = parse(body, config, wasm_module)?;

                                func.push_str(&format!("{}}}", func_body.js));

                                js.push_str(&func);
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
                        js.push_str(&format!(
                            "const {BS_VAR_PREFIX}{name}={};",
                            expression_to_js(expr, start_pos)?
                        ));
                    }
                };

                module_references.push(Arg {
                    name: name.to_owned(),
                    data_type: data_type.to_owned(),
                    expr: expr.to_owned(),
                });
            }

            AstNode::FunctionCall(name, arguments, _, argument_accessed, start_pos, _) => {
                js.push_str(&format!(
                    " {}({})",
                    name,
                    combine_vec_to_js(&arguments, &start_pos)?
                ));
                for index in argument_accessed {
                    js.push_str(&format!("[{}]", index));
                }
            }

            AstNode::Return(expressions, start_pos) => {
                js.push_str(&format!("return {};", expressions_to_js(expressions, &start_pos)?));
            }

            AstNode::If(condition, if_true, start_pos) => {
                let if_block_body = if_true.get_block_nodes();
                
                js.push_str(&format!(
                    "if ({}) {{\n{}\n}}",
                    expression_to_js(&condition, &start_pos)?,
                    parse(if_block_body, config, wasm_module)?.js
                ));
            }

            // DIRECT INSERTION OF JS / CSS into the page
            AstNode::JS(js_string, ..) => {
                js.push_str(&js_string);
            }

            AstNode::Css(css_string, ..) => {
                css.push_str(&css_string);
            }

            // Ignore Everything Else
            _ => {},

        }
    }

    if config.html_meta.auto_site_title {
        page_title += &(" | ".to_owned() + &config.html_meta.site_title.clone());
    }

    Ok(ParserOutput {
        html,
        js,
        css,
        wasm: wasm_module.to_owned(),
        page_title,
    })
}

// fn collect_closing_tags(closing_tags: &mut Vec<String>) -> String {
//     let mut tags = String::new();
//
//     closing_tags.reverse();
//     while let Some(tag) = closing_tags.pop() {
//         tags.push_str(&tag);
//     }
//
//     tags
// }
//
// fn get_src(
//     value: &Value,
//     config: &HTMLMeta,
//     start_pos: &TokenPosition,
// ) -> Result<String, CompileError> {
//     let src: String = match value {
//         Value::String(value) => value.clone(),
//         Value::Runtime(_, data_type) => {
//             if *data_type == DataType::String || *data_type == DataType::CoerceToString {
//                 expression_to_js(value, start_pos)?
//             } else {
//                 return Err(CompileError {
//                     msg: format!("src attribute must be a string literal (Webparser - get src - runtime value). Got: {:?}", data_type),
//                     start_pos: start_pos.to_owned(),
//                     end_pos: value.dimensions(),
//                     error_type: ErrorType::TypeError
//                 });
//             }
//         }
//         _ => {
//             return Err(CompileError {
//                 msg: format!(
//                     "src attribute must be a string literal (web_parser - get src). Got: {:?}",
//                     value.get_type()
//                 ),
//                 start_pos: start_pos.to_owned(),
//                 end_pos: value.dimensions(),
//                 error_type: ErrorType::TypeError,
//             })
//         }
//     };
//
//     if src.starts_with("http") || src.starts_with('/') {
//         Ok(src)
//     } else {
//         Ok(format!(
//             "{}{}/{}",
//             config.page_root_url, config.image_folder_url, src
//         ))
//     }
// }

// Returns the index it inserted the html at
// fn insert_into_table(
//     inserted_html: &str,
//     ele_count: &mut u32,
//     columns: u32,
//     html: &mut String,
// ) -> usize {
//     *ele_count += 1;
//
//     let heading = *ele_count <= columns || columns < 2;
//     let ele_mod = *ele_count % columns;
//
//     if ele_mod == 1 {
//         // if this is the first element for this row
//         html.push_str("<tr>");
//     }
//
//     if heading {
//         html.push_str("<th scope='col'>");
//     } else {
//         html.push_str("<td>");
//     }
//
//     // Should check if we need to close some tags before the end of this scene
//     html.push_str(inserted_html);
//     let idx = html.len();
//
//     if heading {
//         html.push_str("</th>");
//     } else {
//         html.push_str("</td>");
//     }
//
//     // If this is the last element for this row
//     if ele_mod == 0 {
//         html.push_str("</tr>");
//
//         if *ele_count == columns {
//             html.push_str("</thead><tbody>");
//         }
//     }
//
//     idx
// }
//
// // Also make sure to escape reserved HTML characters and remove any empty lines
// fn sanitise_content(content: &str) -> String {
//     content
//         .replace('<', "&lt;")
//         .replace('>', "&gt;")
//         .trim_start()
//         .to_string()
// }
