use super::{
    code_block_highlighting::highlight_code_block,
    colors::get_color,
    js_parser::{collection_to_js, create_reference_in_js, expression_to_js},
};
use crate::html_output::js_parser::combine_vec_to_js;
use crate::parsers::ast_nodes::{NodeInfo, Value};
use crate::tokenizer::TokenPosition;
use crate::{
    bs_css::get_bs_css,
    bs_types::DataType,
    build::ExportedJS,
    parsers::{
        ast_nodes::AstNode,
        styles::{Action, Style, Tag},
        util::{count_newlines_at_end_of_string, count_newlines_at_start_of_string},
    },
    settings::{HTMLMeta, BS_VAR_PREFIX},
    CompileError, ErrorType, Token,
};
use std::path::Path;
use crate::settings::Config;
use crate::wasm_output::wat_parser::new_wat_var;

pub struct ParserOutput {
    pub html: String,
    pub js: String,
    pub css: String,
    pub page_title: String,
    pub exported_js: Vec<ExportedJS>,
    pub exported_css: String,
    pub wat: String,
    pub wat_globals: String,
}

// Parse ast into valid JS, HTML and CSS
pub fn parse<'a>(
    ast: Vec<AstNode>,
    config: &'a Config,
    release_build: bool,
    module_path: &'a str,
    is_global: bool,
    imported_css: &'a String,
) -> Result<ParserOutput, CompileError> {
    let mut js = String::new();
    let mut wat = String::new();
    let mut wat_global_initialisation = String::new();
    let mut html = String::new();
    let mut css = imported_css.to_owned();
    let mut page_title = String::new();
    let mut exp_id: usize = 0;

    let mut exported_js: Vec<ExportedJS> = Vec::new();
    let mut exported_css = String::new();
    let _exported_wat = String::new();

    // Keeps track of whether a reference has already been used
    // This is to prevent duplicate JS code for updating the same element
    let mut module_references: Vec<AstNode> = Vec::new();

    let mut class_id: usize = 0;

    // Parse HTML
    for node in ast {
        match node {
            // SCENES (HTML)
            AstNode::Literal(Value::Scene(scene, scene_tags, scene_styles, scene_actions), _) => {
                html.push_str(&parse_scene(
                    &scene,
                    &scene_tags,
                    &scene_styles,
                    &scene_actions,
                    &mut Tag::None,
                    &mut js,
                    &mut css,
                    &mut module_references,
                    &mut class_id,
                    &mut exp_id,
                    &mut Vec::new(),
                    &mut wat,
                    &config.html_meta,
                )?);
            }

            AstNode::Title(value, _) => {
                page_title = value;
            }
            AstNode::Date(_, _) => {
                // Eventually a way to get date information about the page
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
                    DataType::Float | DataType::Int => {
                        wat.push_str(&new_wat_var(
                            id,
                            expr,
                            data_type,
                            &mut wat_global_initialisation,
                            start_pos,
                        )?);
                    }

                    // Temporary just JS
                    DataType::String => {
                        let var_dec = format!(
                            "{} {BS_VAR_PREFIX}{id} = {};",
                            assignment_keyword,
                            expression_to_js(&expr, &start_pos)?
                        );

                        js.push_str(&var_dec);
                        if is_exported {
                            exported_js.push(ExportedJS {
                                js: var_dec,
                                path: Path::new(module_path).join(id),
                                global: is_global,
                                data_type: data_type.to_owned(),
                            });
                        }
                    }

                    DataType::Scene => {
                        match expr {
                            Value::Scene(scene, scene_tags, scene_styles, scene_actions) => {
                                let mut created_css = String::new();
                                let scene_to_js_string = parse_scene(
                                    scene,
                                    scene_tags,
                                    scene_styles,
                                    scene_actions,
                                    &mut Tag::None,
                                    &mut js,
                                    &mut created_css,
                                    &mut module_references,
                                    &mut class_id,
                                    &mut exp_id,
                                    &mut Vec::new(),
                                    &mut wat,
                                    &config.html_meta,
                                )?;
                                css.push_str(&created_css);

                                // If this scene is exported, add the CSS it created to the exported CSS
                                if is_exported {
                                    exported_css.push_str(&created_css);
                                }

                                let var_dec = format!(
                                    "{} {BS_VAR_PREFIX}{id} = `{}`;",
                                    assignment_keyword, scene_to_js_string
                                );
                                js.push_str(&var_dec);
                                if is_exported {
                                    exported_js.push(ExportedJS {
                                        js: var_dec,
                                        path: Path::new(module_path).join(id),
                                        global: is_global,
                                        data_type: data_type.to_owned(),
                                    });
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
                        // Create struct to represent a tuple in JS
                        let mut tuple_js = String::from("[");
                        let tuple = match &*expr {
                            Value::Structure(args) => args,
                            _ => {
                                return Err(CompileError {
                                    msg: "Tuple declaration must be a tuple".to_string(),
                                    start_pos: start_pos.to_owned(),
                                    end_pos: expr.dimensions(),
                                    error_type: ErrorType::TypeError,
                                });
                            }
                        };

                        // This currently is just another array
                        // The names are replaced with indexes
                        // Structs can't be accessed dynamically at runtime so this is fine
                        // Can just enumerate this if index is needed later
                        for arg in tuple {
                            tuple_js.push_str(&format!(
                                "{},",
                                expression_to_js(&arg.value, &start_pos)?
                            ));
                        }

                        tuple_js.push_str("]");
                        js.push_str(&format!(
                            "{} {BS_VAR_PREFIX}{id} = {};",
                            assignment_keyword, tuple_js
                        ));
                    }
                    _ => {
                        js.push_str(&format!(
                            "{} {BS_VAR_PREFIX}{id} = {};",
                            assignment_keyword,
                            expression_to_js(&expr, &start_pos)?
                        ));
                    }
                };

                module_references.push(node);
            }

            AstNode::Function(name, args, body, is_exported, return_type, start_pos) => {
                let mut arg_names = String::new();
                for arg in &args {
                    let default_arg = match &arg.value {
                        Value::None => "",
                        Value::String(value) => &format!("=\"{value}\""),
                        Value::Int(value) => &format!("={value}"),
                        Value::Float(value) => &format!("={value}"),
                        Value::Bool(value) => &format!("={value}"),

                        // TODO - default args for other types
                        Value::Scene(_, _, _, _) => "",
                        Value::Collection(_, _) => "",
                        Value::Structure(_) => "",

                        Value::Runtime(..) => return Err(CompileError {
                            msg: format!("Runtime value used as default argument for function: {name}. Default values must be constants"),
                            start_pos,
                            end_pos: arg.value.dimensions(),
                            error_type: ErrorType::TypeError
                        }),

                        Value::Reference(name, _, argument_accessed) => {
                            let mut accesses = String::new();
                            for index in argument_accessed {
                                accesses.push_str(&format!("[{}]", index));
                            }

                            &format!("={name}{accesses}")
                        }
                    };

                    arg_names.push_str(&format!("{BS_VAR_PREFIX}{}{default_arg},", arg.name));
                }

                let func_body = parse(
                    body,
                    config,
                    release_build,
                    module_path,
                    false,
                    imported_css,
                )?;

                let func = format!(
                    "{}function {BS_VAR_PREFIX}{name}({arg_names}){{{}}}",
                    if is_exported { "export " } else { "" },
                    func_body.js
                );

                if is_exported {
                    exported_js.push(ExportedJS {
                        js: func.to_owned(),
                        path: Path::new(module_path).join(name),
                        global: is_global,
                        data_type: DataType::Function(args.to_owned(), return_type.to_owned()),
                    });
                }

                js.push_str(&func);
                wat.push_str(&func_body.wat);
                wat_global_initialisation.push_str(&func_body.wat_globals);
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
                js.push_str(&format!("return {};", expression_to_js(&expr, &start_pos)?));
            }

            AstNode::Print(ref value, start_pos) => {
                // automatically unpack a tuple into one string
                let mut final_string = String::new();

                match value {
                    Value::Structure(ref args) => {
                        for arg in args {
                            final_string.push_str(&format!(
                                "{} ",
                                expression_to_js(&arg.value, &start_pos)?
                            ));
                        }
                    }
                    _ => {
                        final_string
                            .push_str(&format!("{}", expression_to_js(&value, &start_pos)?));
                    }
                };

                js.push_str(&format!("console.log({final_string});"));
            }

            // DIRECT INSERTION OF JS / CSS / HTML into page
            AstNode::JS(js_string, ..) => {
                js.push_str(&js_string);
            }

            AstNode::CSS(css_string, ..) => {
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
        page_title,
        exported_js,
        exported_css,
        wat,
        wat_globals: wat_global_initialisation,
    })
}

struct SceneTag {
    tag: Tag,
    outer_tag: Tag,
    properties: String,
    style: String,
    classes: String,
    child_styles: String,
}

// Returns a string of the HTML and the tag the scene is inside of
pub fn parse_scene(
    scene: &Vec<AstNode>,
    scene_tags: &Vec<Tag>,
    scene_styles: &Vec<Style>,
    scene_actions: &Vec<Action>,
    parent_tag: &mut Tag,
    js: &mut String,
    css: &mut String,
    module_references: &mut Vec<AstNode>,
    class_id: &mut usize,
    exp_id: &mut usize,
    positions: &mut Vec<f64>,
    wasm_module: &mut String,
    config: &HTMLMeta,
) -> Result<String, CompileError> {
    let mut html = String::new();
    let mut closing_tags = Vec::new();
    let mut codeblock_css_added = false;
    let mut content_size = 1.0;
    let mut text_is_size = true;

    let mut spaces_after_closing_tag = 0;

    // For tables
    let mut ele_count: u32 = 0;
    let mut columns: u32 = 1;

    struct Image<'a> {
        src: &'a Value,
        start_pos: &'a TokenPosition,
    }

    let mut images: Vec<Image> = Vec::new();

    let mut scene_wrap = SceneTag {
        tag: match parent_tag {
            Tag::List => Tag::List,
            _ => Tag::None,
        },
        outer_tag: match parent_tag {
            Tag::List => Tag::List,
            _ => Tag::None,
        },
        properties: String::new(),
        classes: String::new(),
        style: String::new(),
        child_styles: String::new(),
    };

    let mut style_assigned = false;
    for style in scene_styles {
        match style {
            Style::Padding(value, start_pos) => {
                // If literal, pass it straight in
                // If tuple, spread the values into the padding property
                match value {
                    Value::Float(value) => {
                        scene_wrap.style.push_str(&format!("padding:{}rem;", value));
                    }
                    Value::Int(value) => {
                        scene_wrap.style.push_str(&format!("padding:{}rem;", value));
                    }
                    Value::Structure(args) => {
                        let mut padding = String::new();
                        for arg in args {
                            match arg.value {
                                Value::Float(value) => {
                                    padding.push_str(&format!("{}rem ", value));
                                }
                                Value::Int(value) => {
                                    padding.push_str(&format!("{}rem ", value));
                                }
                                _ => {
                                    let dimension = arg.value.dimensions();
                                    return Err(CompileError {
                                        msg: "Padding must be a literal or a struct of literals"
                                            .to_string(),
                                        start_pos: start_pos.to_owned(),
                                        end_pos: dimension.to_owned(),
                                        error_type: ErrorType::TypeError,
                                    });
                                }
                            }
                        }
                        scene_wrap.style.push_str(&format!("padding:{};", padding));
                    }

                    _ => {
                        return Err(CompileError {
                            msg: "Padding must be a literal or a tuple of literals".to_string(),
                            start_pos: start_pos.to_owned(),
                            end_pos: value.dimensions(),
                            error_type: ErrorType::TypeError,
                        });
                    }
                }
                style_assigned = true;
            }

            Style::Margin(arg, start_pos) => {
                scene_wrap.style.push_str(&format!(
                    "margin:{}rem;",
                    expression_to_js(&arg, &start_pos)?
                ));
                // Only switch to span if there is no tag
                style_assigned = true;
            }

            Style::BackgroundColor(args, start_pos) => {
                scene_wrap.style.push_str(&format!(
                    "background-color:rgba({});",
                    collection_to_js(&args, &start_pos)?
                ));
                style_assigned = true;
            }

            Style::TextColor(value, type_of_color, start_pos) => {
                let color = match type_of_color {
                    Token::Rgb => format!("rgba({})", collection_to_js(&value, &start_pos)?),
                    Token::Hsv => format!("hsla({})", collection_to_js(&value, &start_pos)?),

                    Token::Red
                    | Token::Green
                    | Token::Blue
                    | Token::Yellow
                    | Token::Cyan
                    | Token::Magenta
                    | Token::White
                    | Token::Black
                    | Token::Orange
                    | Token::Pink
                    | Token::Purple
                    | Token::Grey => {
                        format!("hsla({})", get_color(&type_of_color, &value, start_pos)?)
                    }
                    _ => {
                        return Err(CompileError {
                            msg: "Invalid color type provided for text color".to_string(),
                            start_pos: start_pos.to_owned(),
                            end_pos: value.dimensions(),
                            error_type: ErrorType::TypeError,
                        });
                    }
                };

                scene_wrap.style.push_str(&format!("color:{};", color,));
                scene_wrap.child_styles.push_str("color:inherit;");
                style_assigned = true;
            }

            Style::Size(value, start_pos) => {
                content_size = match value {
                    Value::Float(value) => *value,
                    Value::Int(value) => *value as f64,
                    _ => {
                        return Err(CompileError {
                            msg: "Size argument was not numeric".to_string(),
                            start_pos: start_pos.to_owned(),
                            end_pos: value.dimensions(),
                            error_type: ErrorType::TypeError,
                        });
                    }
                };
            }

            Style::Alt(value, _) => {
                scene_wrap
                    .properties
                    .push_str(&format!(" alt=\"{}\"", value));
                style_assigned = true;
            }

            Style::Center(vertical, _) => {
                scene_wrap.style.push_str(
                    "display:flex;align-items:center;flex-direction:column;text-align:center;",
                );
                if *vertical {
                    scene_wrap.style.push_str("justify-content:center;");
                }
                scene_wrap.tag = Tag::Div;
            }

            // Must adapt its behaviour based on the parent tag and siblings
            Style::Order(value, start_pos) => {
                let order = match value {
                    Value::Float(value) => *value,
                    Value::Int(value) => *value as f64,
                    // TODO - runtime expressions
                    Value::Runtime(_, data_type) => {
                        return Err(CompileError {
                            msg: format!(
                                "Runtime expressions not supported yet in order declaration: {:?}",
                                data_type
                            ),
                            start_pos: start_pos.to_owned(),
                            end_pos: value.dimensions(),
                            error_type: ErrorType::Compiler,
                        });
                    }
                    _ => {
                        return Err(CompileError {
                            msg: "Incorrect type arguments passed into order declaration (must be an integer literal)".to_string(),
                            start_pos: start_pos.to_owned(),
                            end_pos: value.dimensions(),
                            error_type: ErrorType::Compiler
                        });
                    }
                };
                match parent_tag {
                    Tag::Nav(..) => {
                        positions.push(order);
                    }
                    _ => {
                        return Err(CompileError {
                            msg: "Order not implemented for this tag yet".to_string(),
                            start_pos: start_pos.to_owned(),
                            end_pos: value.dimensions(),
                            error_type: ErrorType::Compiler,
                        });
                    }
                }
                style_assigned = true;
            }

            Style::Blank => {
                scene_wrap.style.push_str("all:unset;");
                style_assigned = true;
            }

            Style::Hide(_) => {
                scene_wrap.style.push_str("display:none;");
                style_assigned = true;
            }
        }
    }

    let mut img_count = 0;

    // Scene tags usually override each other, only the last one will actually be used
    // There may be some exceptions
    for tag in scene_tags {
        match tag {
            Tag::Img(value, start_pos) => {
                text_is_size = false;
                images.push(Image {
                    src: value,
                    start_pos,
                });
                img_count += 1;
            }

            Tag::Table(c, start_pos) => {
                match scene_wrap.tag {
                    Tag::Img(..) | Tag::Video(..) | Tag::Audio(..) => {
                        // TO DO: Error handling that passes correctly into the AST for the end user
                    }
                    _ => {
                        scene_wrap.tag =
                            Tag::Table(Value::Int(columns as i64), start_pos.to_owned());
                        match c {
                            Value::Int(value) => {
                                columns = *value as u32;
                            }
                            _ => {
                                return Err(CompileError {
                                    msg: format!("Invalid column count in table tag: {:?}", c),
                                    start_pos: start_pos.to_owned(),
                                    end_pos: c.dimensions(),
                                    error_type: ErrorType::Compiler,
                                });
                            }
                        };
                    }
                }
            }

            Tag::Video(value, start_pos) => {
                text_is_size = false;
                scene_wrap.tag = Tag::Video(value.to_owned(), start_pos.to_owned());

                // TO DO, add poster after images are parsed
                // if img_count > 0 {
                //     let poster = format!("{}{}", img_default_dir, images[0]);
                //     scene_wrap
                //         .properties
                //         .push_str(&format!(" poster=\"{}\"", poster));
                // }

                continue;
            }

            Tag::Audio(src, start_pos) => {
                text_is_size = false;
                scene_wrap.tag = Tag::Audio(src.to_owned(), start_pos.to_owned());
            }

            Tag::A(node, start_pos) => {
                scene_wrap.tag = Tag::A(node.to_owned(), start_pos.to_owned());
            }

            Tag::Title(node, start_pos) => {
                scene_wrap.tag = Tag::Title(node.to_owned(), start_pos.to_owned());
            }

            Tag::Nav(style, start_pos) => {
                scene_wrap.tag = Tag::Nav(style.to_owned(), start_pos.to_owned());
            }

            // Interactive
            Tag::Button(node, start_pos) => {
                text_is_size = false;
                scene_wrap.tag = Tag::Button(node.to_owned(), start_pos.to_owned());
            }

            // Structure of the page
            Tag::Main => {
                if scene_wrap.outer_tag == Tag::None {
                    scene_wrap.outer_tag = Tag::Main;
                }
            }

            Tag::Header => {
                if scene_wrap.outer_tag == Tag::None {
                    scene_wrap.outer_tag = Tag::Header;
                }
            }

            Tag::Footer => {
                if scene_wrap.outer_tag == Tag::None {
                    scene_wrap.outer_tag = Tag::Footer;
                }
            }

            Tag::Section => {
                if scene_wrap.outer_tag == Tag::None {
                    scene_wrap.outer_tag = Tag::Section;
                }
            }

            // Scripts
            Tag::Redirect(value, start_pos) => {
                let src = get_src(value, config, start_pos)?;
                js.push_str(&format!("window.location.href='{}';", src));
            }
            _ => {}
        }
    }

    if img_count == 1 {
        match scene_wrap.tag {
            Tag::None => {
                // TODO No line number here?
                scene_wrap.tag = Tag::Img(images[0].src.to_owned(), images[0].start_pos.to_owned());
            }

            Tag::Video(_, ref start_pos) => {
                let poster = get_src(images[0].src, config, start_pos)?;
                scene_wrap
                    .properties
                    .push_str(&format!(" poster=\"{}\"", poster));
            }

            Tag::A(_, ref start_pos) => {
                let img_src = get_src(images[0].src, config, start_pos)?;
                html.push_str(&format!("<img src=\"{img_src}\" />"));
            }

            _ => {}
        }
    }

    if style_assigned && scene_wrap.tag == Tag::None {
        scene_wrap.tag = Tag::Span;
    }

    // If there are multiple images, turn it into a grid of images
    if img_count > 1 {
        scene_wrap.tag = Tag::Div;
        scene_wrap
            .style
            .push_str(&"display:flex;flex-wrap:wrap;justify-content:center;".to_string());
        let img_resize = (content_size * 100.0) / f64::sqrt(img_count as f64);

        for image in images {
            let img = get_src(image.src, config, image.start_pos)?;
            html.push_str(&format!(
                "<img src=\"{img}\" style=\"width:{img_resize}%;height:{img_resize}%;\"/>"
            ));
        }
    }

    // Add any actions that need to be added to the scene
    for action in scene_actions {
        match action {
            Action::Click(node, start_pos) => {
                // Should accept a function as an argument
                scene_wrap.properties.push_str(&format!(
                    " onclick=\"{}\"",
                    expression_to_js(&node, start_pos)?
                ));
            }
            Action::_Swap => {}
        }
    }

    if content_size != 1.0 {
        if text_is_size {
            scene_wrap
                .style
                .push_str(&format!("font-size:{}rem;", content_size));
        } else {
            let size = content_size * 100.0;
            scene_wrap.style.push_str(&format!("width:{size}%;"));
        }
    }

    struct SceneHeadLiteral {
        value: Value,
        html_location: usize,
        start_pos: TokenPosition,
    }
    let mut scenehead_literals: Vec<SceneHeadLiteral> = Vec::new();
    let mut scenehead_templates: Vec<usize> = Vec::new();

    for node in scene {
        match node {
            AstNode::Span(content, ..) => {
                let content = sanitise_content(&content);

                // Special tags
                match scene_wrap.tag {
                    Tag::Title(..) | Tag::List | Tag::A(..) | Tag::Button(..) => {
                        html.push_str(&content.to_owned());
                        continue;
                    }
                    _ => {}
                }

                match *parent_tag {
                    Tag::P => {
                        html.push_str(&format!("<span>{}</span>", &content));
                        if count_newlines_at_end_of_string(&content) > 0 {
                            *parent_tag = Tag::None;
                            html.push_str("</p>");

                            // Find the last p tag in closing tags and remove it
                            let mut i = closing_tags.len();
                            while i > 0 {
                                i -= 1;
                                if closing_tags[i] == "</p>" {
                                    closing_tags.remove(i);
                                    break;
                                }
                            }
                        }
                    }
                    Tag::Heading | Tag::BulletPoint => {
                        let newlines_at_start = count_newlines_at_start_of_string(&content);
                        if newlines_at_start > 0 {
                            // If newlines at start, break out of heading and add normal P tag instead
                            html.push_str(&format!("<p>{}", &content));
                            closing_tags.push("</p>".to_string());
                            *parent_tag = Tag::None;
                        } else {
                            html.push_str(&content);
                            if count_newlines_at_end_of_string(&content) > 0 {
                                html.push_str(&collect_closing_tags(&mut closing_tags));
                                *parent_tag = Tag::None;
                            }
                        }
                    }
                    Tag::Table(..) | Tag::List | Tag::A(..) | Tag::Button(..) => {
                        html.push_str(&content.to_owned());
                    }
                    _ => {
                        html.push_str(&format!("<span>{}</span>", content));
                    }
                }
            }

            AstNode::P(content, ..) => {
                let content = sanitise_content(&content);

                match scene_wrap.tag {
                    Tag::Img(..) | Tag::Video(..) => {
                        scene_wrap
                            .properties
                            .push_str(&format!(" alt=\"{}\"", &content));
                    }
                    Tag::Title(..) | Tag::List => {
                        html.push_str(&content.to_owned());
                        continue;
                    }
                    Tag::A(..) | Tag::Button(..) => {
                        html.push_str(&collect_closing_tags(&mut closing_tags));
                        html.push_str(&content.to_owned());
                    }
                    _ => {
                        html.push_str(&collect_closing_tags(&mut closing_tags));
                        match *parent_tag {
                            Tag::P => {
                                if count_newlines_at_start_of_string(&content) > 1 {
                                    html.push_str("</p>");
                                    html.push_str(&format!("<p>{}", &content));
                                } else {
                                    html.push_str(&format!("<span> {}</span>", &content));
                                }
                            }
                            Tag::Table(..) | Tag::Nav(..) | Tag::List | Tag::Button(..) => {
                                html.push_str(&content.to_owned());
                            }
                            Tag::Heading | Tag::BulletPoint => {
                                let newlines_at_start =
                                    count_newlines_at_start_of_string(content.as_str());
                                if newlines_at_start > 0 {
                                    for _ in 1..newlines_at_start {
                                        html.push_str("<br>");
                                    }
                                    html.push_str(&format!("<p>{}", content));
                                    closing_tags.push("</p>".to_string());
                                    *parent_tag = Tag::P;
                                } else {
                                    html.push_str(&content.to_owned());
                                    if count_newlines_at_end_of_string(content.as_str()) > 0 {
                                        html.push_str(&collect_closing_tags(&mut closing_tags));
                                        *parent_tag = Tag::None;
                                    }
                                }
                            }
                            _ => {
                                html.push_str(&format!("<p>{}", content));
                                if count_newlines_at_end_of_string(&content) > 1 {
                                    html.push_str("</p>");
                                    *parent_tag = Tag::None;
                                } else {
                                    // closing_tags.push("</p>".to_string());
                                    *parent_tag = Tag::P;
                                }
                            }
                        }
                    }
                }
            }

            AstNode::Pre(content, ..) => {
                html.push_str(&collect_closing_tags(&mut closing_tags));
                html.push_str(&format!("<pre>{}", content));
                closing_tags.push("</pre>".to_string());
            }

            AstNode::Newline => {
                match *parent_tag {
                    Tag::Table(..) | Tag::Nav(..) => {}
                    _ => {
                        html.push_str("<br><br>");
                    }
                };
            }

            AstNode::CodeBlock(content, language, ..) => {
                // Add the CSS for code highlighting
                if !codeblock_css_added {
                    css.push_str(get_bs_css("codeblock-0"));
                    codeblock_css_added = true;
                }

                html.push_str(&collect_closing_tags(&mut closing_tags));

                let highlighted_block = highlight_code_block(&content, &language);
                html.push_str(&format!("<pre><code>{}</code></pre>", highlighted_block));
            }

            // Special Markdown Syntax Elements
            AstNode::Heading(size) => {
                match *parent_tag {
                    Tag::Table(..) | Tag::Nav(..) => {}
                    _ => {
                        html.push_str(&collect_closing_tags(&mut closing_tags));
                        html.push_str(&format!("<h{}>", size));
                        closing_tags.push(format!("</h{}>", size));
                        *parent_tag = Tag::Heading;
                    }
                };
            }
            AstNode::BulletPoint(_strength) => {
                match *parent_tag {
                    Tag::Table(..) | Tag::Nav(..) => {}
                    _ => {
                        html.push_str(&collect_closing_tags(&mut closing_tags));
                        html.push_str(&"<li>".to_string());
                        closing_tags.push("</li>".to_string());
                        *parent_tag = Tag::None;
                    }
                };
            }
            AstNode::Em(strength, content, _) => {
                match *parent_tag {
                    Tag::Table(..) | Tag::Nav(..) | Tag::P => {}
                    _ => {
                        html.push_str(&collect_closing_tags(&mut closing_tags));
                        html.push_str("<p>");
                        closing_tags.push("</p>".to_string());
                        *parent_tag = Tag::P;
                    }
                };

                match strength {
                    1 => {
                        html.push_str(&format!("<em>{}</em>", content));
                    }
                    2 => {
                        html.push_str(&format!("<strong>{}</strong>", content));
                    }
                    3 => {
                        html.push_str(&format!("<strong><em>{}</em></strong>", content));
                    }
                    _ => {
                        html.push_str(&format!("<b><strong><em>{}</em></strong></b>", content));
                    }
                }
            }

            AstNode::Superscript(content, start_pos) => {
                html.push_str(&format!("<sup>{}</sup>", content));
                *parent_tag = Tag::None;
                // TODO
                return Err(CompileError {
                    msg: "Superscript not yet supported in HTML output".to_string(),
                    start_pos: start_pos.to_owned(),
                    end_pos: TokenPosition {
                        line_number: start_pos.line_number,
                        char_column: start_pos.char_column + 1,
                    },
                    error_type: ErrorType::Compiler,
                });
            }

            AstNode::Space(_) => {
                spaces_after_closing_tag += 1;
            }

            // STUFF THAT IS INSIDE SCENE HEAD THAT NEEDS TO BE PASSED INTO SCENE BODY
            AstNode::FunctionCall(ref name, ref arguments, _, arguments_accessed, start_pos) => {
                html.push_str(&format!("<span class=\"{name}\"></span>"));
                if !module_references.contains(&node) {
                    module_references.push(node.to_owned());

                    let mut accesses = String::new();
                    for index in arguments_accessed {
                        accesses.push_str(&format!("[{}]", index));
                    }

                    js.push_str(&format!(
                        "uInnerHTML(\"{name}\",{}",
                        format!(
                            "{name}({}){accesses}",
                            combine_vec_to_js(&arguments, start_pos)?
                        )
                    ));

                    js.push_str(");");
                }
            }

            // All literals passed directly into the scene head
            // This includes scenes
            AstNode::Literal(value, start_pos) => {
                match value {
                    Value::Scene(
                        new_scene_nodes,
                        new_scene_tags,
                        new_scene_styles,
                        new_scene_actions,
                    ) => {
                        // Switch scene tag for certain child scenes
                        let mut new_scene_tag = match scene_wrap.tag {
                            Tag::Nav(..) => Tag::List,
                            Tag::List => Tag::None,
                            _ => scene_wrap.tag.to_owned(),
                        };

                        let new_scene = parse_scene(
                            new_scene_nodes,
                            new_scene_tags,
                            new_scene_styles,
                            new_scene_actions,
                            &mut new_scene_tag,
                            js,
                            css,
                            module_references,
                            class_id,
                            exp_id,
                            &mut Vec::new(),
                            wasm_module,
                            config,
                        )?;

                        // If this is in a table, add correct table tags
                        // What happens if columns are 0?
                        match scene_wrap.tag {
                            Tag::Table(..) => {
                                insert_into_table(
                                    &new_scene,
                                    &mut ele_count,
                                    columns.to_owned(),
                                    &mut html,
                                );
                            }
                            Tag::Nav(..) => {
                                html.push_str(&format!("<ul>{}</ul>", new_scene));
                                ele_count += 1;
                            }
                            _ => {
                                html.push_str(&new_scene);
                            }
                        }
                    }

                    Value::Reference(name, data_type, argument_accessed) => {
                        // Create a span in the HTML with a class that can be referenced by JS
                        // TO DO: Should be reactive in future -> this can change at runtime
                        html.push_str(&format!("<span class=\"{name}\"></span>"));

                        if !module_references.contains(&node) {
                            module_references.push(node.to_owned());

                            match &data_type {
                                DataType::Structure(items) => {
                                    // Automatically unpack all items in the tuple into the scene
                                    // If no items accessed
                                    if argument_accessed.len() == 0 {
                                        let mut elements = String::new();
                                        let mut index = 0;
                                        for _ in &**items {
                                            elements.push_str(&format!(
                                                "{BS_VAR_PREFIX}{name}[{index}],"
                                            ));
                                            index += 1;
                                        }

                                        js.push_str(&format!(
                                            "uInnerHTML(\"{name}\",[{elements}]);"
                                        ));
                                    } else {
                                        js.push_str(&create_reference_in_js(
                                            name,
                                            data_type,
                                            argument_accessed,
                                        ));
                                    }
                                }
                                _ => {
                                    js.push_str(&create_reference_in_js(
                                        name,
                                        data_type,
                                        argument_accessed,
                                    ));
                                }
                            }
                        }
                    }

                    Value::Structure(items) => {
                        for item in items {
                            let value = item.value.to_owned();
                            scenehead_literals.push(SceneHeadLiteral {
                                value: value.to_owned(),
                                html_location: html.len(),
                                start_pos: start_pos.to_owned(),
                            });
                        }
                    }

                    Value::None => {
                        // Ignore this
                        // Currently 'ignored' or hidden scenes result in a None value being added to a scene,
                        // So it's not an error
                        // Hopefully the compiler will always catch unintended use of None in scenes
                    }

                    // TODO - add / test remaining types, some of them might need unpacking
                    _ => {
                        scenehead_literals.push(SceneHeadLiteral {
                            value: value.to_owned(),
                            html_location: html.len(),
                            start_pos: start_pos.to_owned(),
                        });
                    }
                }
            }

            AstNode::SceneTemplate => {
                if &columns > &0 {
                    scenehead_templates.push(insert_into_table(
                        &String::new(),
                        &mut ele_count,
                        columns,
                        &mut html,
                    ));
                } else {
                    scenehead_templates.push(html.len());
                }
            }

            _ => {
                return Err(CompileError {
                    msg: format!("Compiler Bug: unknown AST node found in scene: {:?}", node),
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

    for tag in closing_tags.iter().rev() {
        html.push_str(tag);
    }

    // Take all scenehead variables and add them into any templates inside the scene body
    // When there are no templates left, create a new span element to hold the literal
    for literal in scenehead_literals.into_iter().rev() {
        let js_string = match literal.value {
            Value::Runtime(..) => expression_to_js(&literal.value, &literal.start_pos)?,
            Value::String(value) => {
                format!("\"{}\"", value)
            }
            Value::Float(value) => value.to_string(),
            Value::Int(value) => value.to_string(),
            Value::Bool(value) => value.to_string(),
            _ => {
                return Err(CompileError {
                    msg: format!(
                        "Invalid literal type found in scene head: {:?}",
                        literal.value
                    ),
                    start_pos: literal.start_pos.to_owned(),
                    end_pos: TokenPosition {
                        line_number: literal.start_pos.line_number,
                        char_column: literal.start_pos.char_column + 1,
                    },
                    error_type: ErrorType::Compiler,
                });
            }
        };

        // If there are templates inside the scene, use that index.
        // Otherwise, just use the index of where the literal would be inserted.
        let html_index = scenehead_templates.pop().unwrap_or(literal.html_location);
        html.insert_str(html_index, &format!("<span id=\"exp{exp_id}\"></span>"));

        js.push_str(&format!(
            "document.getElementById('exp{exp_id}').innerHTML={js_string};"
        ));

        *exp_id += 1;
    }

    // Create class for all child elements
    if !scene_wrap.child_styles.is_empty() {
        scene_wrap.classes.push_str(&format!(" bs-{class_id}"));
        css.push_str(&format!(
            ".bs-{class_id} > * {{{}}}",
            scene_wrap.child_styles
        ));
        *class_id += 1;
    }

    match scene_wrap.tag {
        Tag::Span => {
            html.insert_str(
                0,
                &format!(
                    "<span style=\"{}\" {} class=\"{}\" >",
                    scene_wrap.style, scene_wrap.properties, scene_wrap.classes
                ),
            );
            html.push_str("</span>");
        }

        Tag::Div => {
            html.insert_str(
                0,
                &format!(
                    "<div style=\"{}\" {} class=\"{}\" >",
                    scene_wrap.style, scene_wrap.properties, scene_wrap.classes
                ),
            );
            if match *parent_tag {
                Tag::P => true,
                _ => false,
            } {
                html.insert_str(0, "</p>");
                *parent_tag = Tag::None;
            }
            html.push_str("</div>");
        }

        Tag::A(href, ref start_pos) => {
            html.insert_str(
                0,
                &format!(
                    "<a href={} style=\"{}\" class=\"{}\" {}>",
                    expression_to_js(&href, start_pos)?,
                    scene_wrap.style,
                    scene_wrap.classes,
                    scene_wrap.properties
                ),
            );
            html.push_str("</a>");
        }

        Tag::Button(button, ref start_pos) => {
            html.insert_str(
                0,
                &format!(
                    "<button onclick=\"{}\" style=\"{}\" class=\"{}\" {}>",
                    expression_to_js(&button, start_pos)?,
                    scene_wrap.style,
                    scene_wrap.classes,
                    scene_wrap.properties
                ),
            );
            html.push_str("</button>");
        }

        Tag::Img(src, ref start_pos) => {
            let img_src = get_src(&src, config, start_pos)?;
            html.insert_str(
                0,
                &format!(
                    "<img src={} style=\"{}\" class=\"{}\" {} />",
                    img_src, scene_wrap.style, scene_wrap.classes, scene_wrap.properties
                ),
            );
            if match *parent_tag {
                Tag::P => true,
                _ => false,
            } {
                html.insert_str(0, "</p>");
                *parent_tag = Tag::None;
            }
        }
        Tag::Table(..) => {
            // If not enough elements to fill the table, add empty cells
            let ele_mod = ele_count % columns;
            if ele_mod != 0 {
                for _ in 0..columns - ele_mod {
                    html.push_str("<td></td>");
                }
            }

            collect_closing_tags(&mut closing_tags);
            html.insert_str(
                0,
                &format!(
                    "<table style=\"{}\" {} class=\"{}\" ><thead>",
                    scene_wrap.style, scene_wrap.properties, scene_wrap.classes,
                ),
            );
            html.push_str("</tbody></table>");
        }

        Tag::Video(src, ref start_pos) => {
            html.insert_str(
                0,
                &format!(
                    "<video src=\"{}\" style=\"{}\" {} class=\"{}\" controls />",
                    expression_to_js(&src, start_pos)?,
                    scene_wrap.style,
                    scene_wrap.properties,
                    scene_wrap.classes
                ),
            );
            if match *parent_tag {
                Tag::P => true,
                _ => false,
            } {
                html.insert_str(0, "</p>");
                *parent_tag = Tag::None;
            }
        }

        Tag::Audio(src, ref start_pos) => {
            html.insert_str(
                0,
                &format!(
                    "<audio src=\"{}\" style=\"{}\" {} class=\"{}\" controls />",
                    expression_to_js(&src, start_pos)?,
                    scene_wrap.style,
                    scene_wrap.properties,
                    scene_wrap.classes
                ),
            );
        }

        Tag::Code(..) => {
            html.insert_str(
                0,
                &format!(
                    "<code style=\"{}\" {} class=\"{}\" >",
                    scene_wrap.style, scene_wrap.properties, scene_wrap.classes,
                ),
            );
            html.push_str("</code>");
        }

        Tag::Nav(nav_style, ref start_pos) => {
            html.insert_str(
                0,
                &format!(
                    "<nav style=\"{}\" class=\"bs-nav-{} {}\" {} >",
                    scene_wrap.style,
                    expression_to_js(&nav_style, start_pos)?,
                    scene_wrap.classes,
                    scene_wrap.properties,
                ),
            );

            css.push_str(get_bs_css(&format!("nav-{}", class_id)));
            html.push_str("</nav>");
        }

        Tag::Title(size, _) => {
            let class_id = match size {
                Value::Float(value) => value,
                Value::Int(value) => value as f64,
                _ => 1.0,
            };
            html.insert_str(
                0,
                &format!(
                    "<b class=\"bs-title-{} {}\" style=\"{}\" {} >",
                    class_id, scene_wrap.classes, scene_wrap.style, scene_wrap.properties,
                ),
            );
            css.push_str(get_bs_css(&format!("title-{}", class_id)));
            html.push_str("</b>");
        }

        Tag::None => {}

        _ => {
            return Err(CompileError {
                msg: format!("Tag not implemented yet (web parser): {:?}", scene_wrap.tag),
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
    };

    for _ in 0..spaces_after_closing_tag {
        html.push_str("&nbsp;");
    }

    match scene_wrap.outer_tag {
        Tag::Main => {
            html.insert_str(0, "<main class=\"container\">");
            html.push_str("</main>");
        }
        Tag::Header => {
            html.insert_str(0, "<header class=\"container\">");
            html.push_str("</header>");
        }
        Tag::Footer => {
            html.insert_str(0, "<footer class=\"container\">");
            html.push_str("</footer>");
        }
        Tag::Section => {
            html.insert_str(0, "<section>");
            html.push_str("</section>");
        }
        Tag::List => {
            html.insert_str(0, "<li>");
            html.push_str("</li>");
        }
        _ => {}
    };

    Ok(html)
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
                expression_to_js(&value, start_pos)?
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
    inserted_html: &String,
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
fn sanitise_content(content: &String) -> String {
    content
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .trim_start()
        .to_string()
}
