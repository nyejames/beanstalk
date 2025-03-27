use super::js_parser::expression_to_js;
use crate::html_output::js_parser::combine_vec_to_js;
use crate::parsers::ast_nodes::Value;
use crate::parsers::scene::{parse_scene, PrecedenceStyle, SceneIngredients, StyleFormat};
use crate::settings::Config;
use crate::tokenizer::TokenPosition;
use crate::{
    bs_types::DataType,
    build::CompiledExport,
    parsers::ast_nodes::AstNode,
    settings::{HTMLMeta, BS_VAR_PREFIX},
    CompileError, ErrorType,
};
use std::collections::HashMap;
use std::path::PathBuf;
use wasm_encoder::Module;

pub struct ParserOutput {
    pub html: String,
    pub js: String,
    pub css: String,
    pub wasm: Module,
    pub exported: HashMap<PathBuf, CompiledExport>,
    pub page_title: String,
}

// Parse ast into valid WAT, JS, HTML and CSS
pub fn parse<'a>(
    ast: Vec<AstNode>,
    config: &'a Config,
    module_path: &'a str,
    wasm_module: &mut Module,
    exports: &mut HashMap<PathBuf, CompiledExport>,
) -> Result<ParserOutput, CompileError> {
    let mut js = String::new();
    // let mut types = TypeSection::new();
    // let mut wasm_export_section = ExportSection::new();
    // let mut type_index = 0;

    let mut html = String::new();
    let mut css = String::new();
    let mut exp_id: usize = 0;
    let mut page_title = String::new();

    let mut exported: HashMap<PathBuf, CompiledExport> = HashMap::new();

    // Keeps track of whether a reference has already been used
    // This is to prevent duplicate JS code for updating the same element
    let mut module_references: Vec<AstNode> = Vec::new();

    let mut class_id: usize = 0;

    // Parse HTML
    for node in ast {
        match node {
            // SCENES (HTML)
            AstNode::Literal(Value::Scene(scene_body, scene_styles, scene_id), _) => {
                let parsed_scene = parse_scene(
                    SceneIngredients {
                        scene_body: &scene_body,
                        scene_styles: &scene_styles,
                        inherited_style: PrecedenceStyle::new(),
                        scene_id,
                        format_context: &StyleFormat::None,
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
            AstNode::VarDeclaration(
                ref id,
                ref expr,
                is_exported,
                ref data_type,
                is_const,
                ref start_pos,
            ) => {
                let assignment_keyword = if is_const { "const" } else { "let" };

                match data_type {
                    DataType::Float | DataType::Int | DataType::String => {
                        let var_dec = format!(
                            "{} {BS_VAR_PREFIX}{id}={};",
                            assignment_keyword,
                            expression_to_js(expr, start_pos)?
                        );

                        js.push_str(&var_dec);
                        if is_exported {
                            exported.insert(
                                PathBuf::from(module_path).join(id),
                                CompiledExport {
                                    js: var_dec,
                                    css: String::new(),
                                    data_type: data_type.to_owned(),
                                    wasm: Module::new(),
                                },
                            );
                        }
                    }

                    DataType::Scene => {
                        match expr {
                            Value::Scene(scene_body, scene_styles, id) => {
                                let mut created_css = String::new();

                                let scene_to_js_string = parse_scene(
                                    SceneIngredients {
                                        scene_body,
                                        scene_styles,
                                        inherited_style: PrecedenceStyle::new(),
                                        scene_id: id.to_owned(),
                                        format_context: &StyleFormat::None,
                                    },
                                    &mut js,
                                    &mut created_css,
                                    &mut module_references,
                                    &mut class_id,
                                    &mut exp_id,
                                    &config.html_meta,
                                )?;

                                let var_dec = format!(
                                    "{} {BS_VAR_PREFIX}{id} = `{}`;",
                                    assignment_keyword, scene_to_js_string
                                );

                                css.push_str(&created_css);
                                js.push_str(&var_dec);

                                // If this scene is exported, add the CSS it created to the exported CSS
                                if is_exported {
                                    exported.insert(
                                        PathBuf::from(module_path).join(id),
                                        CompiledExport {
                                            js: var_dec,
                                            css: created_css,
                                            data_type: data_type.to_owned(),
                                            wasm: Module::new(),
                                        },
                                    );
                                }
                            }

                            _ => {
                                return Err(CompileError {
                                    msg: "Scene declaration must be a scene".to_string(),
                                    start_pos: start_pos.to_owned(),
                                    end_pos: expr.dimensions(),
                                    error_type: ErrorType::TypeError,
                                });
                            }
                        };
                    }

                    DataType::Structure(_) => {
                        let var_dec = format!(
                            "{} {BS_VAR_PREFIX}{id}={};",
                            assignment_keyword,
                            expression_to_js(expr, start_pos)?
                        );

                        js.push_str(&var_dec);
                        if is_exported {
                            exported.insert(
                                PathBuf::from(module_path).join(id),
                                CompiledExport {
                                    js: var_dec,
                                    css: String::new(),
                                    data_type: data_type.to_owned(),
                                    wasm: Module::new(),
                                },
                            );
                        }
                    }
                    _ => {
                        js.push_str(&format!(
                            "{} {BS_VAR_PREFIX}{id}={};",
                            assignment_keyword,
                            expression_to_js(expr, start_pos)?
                        ));
                    }
                };

                module_references.push(node);
            }

            AstNode::Function(name, args, body, is_exported, return_type, ref start_pos) => {
                let mut func = format!("function {}(", name);

                for arg in &args {
                    func.push_str(&format!(
                        "{BS_VAR_PREFIX}{} = {},",
                        arg.name,
                        expression_to_js(&arg.value, start_pos)?
                    ));
                }

                func.push_str("){");

                // let utf16_units: Vec<u16> = rust_string.encode_utf16().collect();
                let func_body = parse(body, config, module_path, wasm_module, exports)?;

                func.push_str(&format!("{}}}", func_body.js));

                js.push_str(&func);
                if is_exported {
                    exported.insert(
                        PathBuf::from(module_path).join(name),
                        CompiledExport {
                            js: func,
                            css: String::new(),
                            data_type: DataType::Function(args.to_owned(), return_type.to_owned()),
                            wasm: Module::new(),
                        },
                    );
                }
            }

            AstNode::FunctionCall(name, arguments, _, argument_accessed, start_pos) => {
                js.push_str(&format!(
                    " {}({})",
                    name,
                    combine_vec_to_js(&arguments, &start_pos)?
                ));
                for index in argument_accessed {
                    js.push_str(&format!("[{}]", index));
                }
            }

            AstNode::Return(ref expr, start_pos) => {
                js.push_str(&format!("return {};", expression_to_js(expr, &start_pos)?));
            }

            AstNode::Print(ref value, start_pos) => {
                // automatically unpack a tuple into one string
                let mut final_string = String::new();

                match value {
                    Value::Structure(args) => {
                        for arg in args {
                            final_string.push_str(&format!(
                                "{} ",
                                expression_to_js(&arg.value, &start_pos)?
                            ));
                        }
                    }
                    _ => {
                        final_string.push_str(&expression_to_js(value, &start_pos)?.to_string());
                    }
                };

                js.push_str(&format!("console.log({final_string});"));
            }

            // DIRECT INSERTION OF JS / CSS / HTML into page
            AstNode::JS(js_string, ..) => {
                js.push_str(&js_string);
            }

            AstNode::Css(css_string, ..) => {
                css.push_str(&css_string);
            }

            // Ignored
            AstNode::Comment(_) => {}

            _ => {
                return Err(CompileError {
                    msg: format!(
                        "Unknown AST node found when parsing AST in web parser: {:?}",
                        node
                    ),
                    start_pos: TokenPosition {
                        line_number: 0,
                        char_column: 0,
                    },
                    end_pos: TokenPosition {
                        line_number: 0,
                        char_column: 0,
                    },
                    error_type: ErrorType::Compiler,
                });
            }
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
        exported,
    })
}

fn collect_closing_tags(closing_tags: &mut Vec<String>) -> String {
    let mut tags = String::new();

    closing_tags.reverse();
    while let Some(tag) = closing_tags.pop() {
        tags.push_str(&tag);
    }

    tags
}

fn get_src(
    value: &Value,
    config: &HTMLMeta,
    start_pos: &TokenPosition,
) -> Result<String, CompileError> {
    let src: String = match value {
        Value::String(value) => value.clone(),
        Value::Runtime(_, data_type) => {
            if *data_type == DataType::String || *data_type == DataType::CoerceToString {
                expression_to_js(value, start_pos)?
            } else {
                return Err(CompileError {
                    msg: format!("src attribute must be a string literal (Webparser - get src - runtime value). Got: {:?}", data_type),
                    start_pos: start_pos.to_owned(),
                    end_pos: value.dimensions(),
                    error_type: ErrorType::TypeError
                });
            }
        }
        _ => {
            return Err(CompileError {
                msg: format!(
                    "src attribute must be a string literal (web_parser - get src). Got: {:?}",
                    value.get_type()
                ),
                start_pos: start_pos.to_owned(),
                end_pos: value.dimensions(),
                error_type: ErrorType::TypeError,
            })
        }
    };

    if src.starts_with("http") || src.starts_with('/') {
        Ok(src)
    } else {
        Ok(format!(
            "{}{}/{}",
            config.page_root_url, config.image_folder_url, src
        ))
    }
}

// Returns the index it inserted the html at
fn insert_into_table(
    inserted_html: &str,
    ele_count: &mut u32,
    columns: u32,
    html: &mut String,
) -> usize {
    *ele_count += 1;

    let heading = *ele_count <= columns || columns < 2;
    let ele_mod = *ele_count % columns;

    if ele_mod == 1 {
        // if this is the first element for this row
        html.push_str("<tr>");
    }

    if heading {
        html.push_str("<th scope='col'>");
    } else {
        html.push_str("<td>");
    }

    // Should check if we need to close some tags before the end of this scene
    html.push_str(inserted_html);
    let idx = html.len();

    if heading {
        html.push_str("</th>");
    } else {
        html.push_str("</td>");
    }

    // If this is the last element for this row
    if ele_mod == 0 {
        html.push_str("</tr>");

        if *ele_count == columns {
            html.push_str("</thead><tbody>");
        }
    }

    idx
}

// Also make sure to escape reserved HTML characters and remove any empty lines
fn sanitise_content(content: &str) -> String {
    content
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .trim_start()
        .to_string()
}
