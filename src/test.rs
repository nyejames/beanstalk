use colour::{blue_ln, cyan_ln, green_ln, grey_ln};
use colour::{blue_ln_bold, dark_grey_ln, dark_yellow_ln, green_ln_bold, yellow_ln_bold};

use crate::bs_types::DataType;
use crate::html_output::web_parser;
use crate::parsers::ast_nodes::{AstNode, NodeInfo, Value};
use crate::settings::Config;
use crate::{tokenizer, ToError};
use crate::tokenizer::TokenPosition;
use crate::{dev_server, parsers};
use crate::{Error, ErrorType, Token};
use std::fs;
use std::path::PathBuf;

pub fn test_build(path: &PathBuf) -> Result<(), Error> {
    // Read content from a test file
    yellow_ln_bold!("\nREADING TEST FILE\n");

    let file_name = path.file_stem().unwrap().to_str().unwrap();
    let compiler_test_path = path.join("src/#page.bs");
    let content = match fs::read_to_string(&compiler_test_path) {
        Ok(content) => content,
        Err(e) => {
            return Err(Error {
                msg: format!("Error reading file in test build: {:?}", e),
                start_pos: TokenPosition {
                    line_number: 0,
                    char_column: 0,
                },
                end_pos: TokenPosition {
                    line_number: 0,
                    char_column: 0,
                },
                file_path: PathBuf::from(""),
                error_type: ErrorType::File,
            });
        }
    };

    // Tokenize File
    yellow_ln_bold!("TOKENIZING FILE\n");
    let (tokens, token_positions) = match tokenizer::tokenize(&content, file_name) {
        Ok(tokens) => tokens,
        Err(e) => {
            return Err(e.to_error(compiler_test_path));
        }
    };

    for token in &tokens {
        match token {
            Token::SceneHead | Token::SceneClose(_) => {
                blue_ln!("{:?}", token);
            }
            Token::P(_)
            | Token::HeadingStart(_)
            | Token::BulletPointStart(_)
            | Token::Em(_, _)
            | Token::Superscript(_) => {
                green_ln!("{:?}", token);
            }
            Token::Empty | Token::Newline => {
                grey_ln!("{:?}", token);
            }

            // Ignore whitespace in test output
            // Token::Whitespace => {}
            _ => {
                println!("{:?}", token);
            }
        }
    }
    println!("\n");

    // Create AST
    yellow_ln_bold!("CREATING AST\n");
    let (ast, _var_declarations) = match parsers::build_ast::new_ast(
        tokens,
        &mut 0,
        &token_positions,
        &mut Vec::new(),
        &Vec::new(),
        true,
    ) {
        Ok(ast) => ast,
        Err(e) => {
            return Err(e.to_error(compiler_test_path));
        }
    };

    for node in &ast {
        match node {
            AstNode::P(..) | AstNode::Span(..) => {
                green_ln!("{:?}", node);
            }
            AstNode::Literal(value, _) => {
                if value.get_type() == DataType::Scene {
                    print_scene(&value, 0);
                }
                cyan_ln!("{:?}", node);
            }
            AstNode::Comment(..) => {
                grey_ln!("{:?}", node);
            }
            _ => {
                println!("{:?}", node);
            }
        }
    }

    yellow_ln_bold!("\nCREATING HTML OUTPUT\n");
    let parser_output = match web_parser::parse(
        ast,
        &Config::default(),
        false,
        "test",
        false,
        &String::new(),
    ) {
        Ok(parser_output) => parser_output,
        Err(e) => {
            return Err(e.to_error(compiler_test_path));
        }
    };

    for export in parser_output.exported_js {
        println!("JS EXPORTS:");
        println!("{:?}", export.path);
    }
    println!("CSS EXPORTS: {}", parser_output.exported_css);

    let all_parsed_wasm = &format!(
        "(module {}(func (export \"set_wasm_globals\"){}))",
        &parser_output.wat, parser_output.wat_globals
    );
    println!("WAT: {}", all_parsed_wasm);

    /*

        // Print the HTML output
        // Create a regex to match the content between the <main> and </main> tags
        let re = Regex::new(r"(?s)<body>(.*?)</body>").unwrap();

        // Extract the content between the <main> and </main> tags
        let main_content = re
            .captures(&html_output)
            .and_then(|cap| cap.get(1))
            .map_or("", |m| m.as_str());

        // Create a regex to match HTML tags
        let re_tags = Regex::new(r"(</?\w+[^>]*>)").unwrap();

        // Insert a newline before each HTML tag
        let formatted_content = re_tags.replace_all(main_content, "\n$1");

        // Print the formatted content
        println!("\n\n{}", formatted_content);

    */

    if path.is_dir() {
        dev_server::start_dev_server(path)?;
    }

    green_ln_bold!("Test complete!");
    Ok(())
}

fn print_scene(scene: &Value, scene_nesting_level: u32) {
    // Indent the scene by how nested it is
    let mut indentation = String::new();
    for _ in 0..scene_nesting_level {
        indentation.push_str("\t");
    }

    match scene {
        Value::Scene(nodes, tags, styles, actions, ..) => {
            blue_ln_bold!("\n{}Scene Head: ", indentation);
            for tag in tags {
                dark_yellow_ln!("{}  {:?}", indentation, tag);
            }
            for style in styles {
                cyan_ln!("{}  {:?}", indentation, style);
            }
            for action in actions {
                dark_yellow_ln!("{}  {:?}", indentation, action);
            }

            blue_ln_bold!("{}Scene Body:", indentation);

            for scene_node in nodes {
                match scene_node {
                    AstNode::Heading(..)
                    | AstNode::BulletPoint(..)
                    | AstNode::Em(..)
                    | AstNode::Superscript(..) => {
                        green_ln_bold!("{}  {:?}", indentation, scene_node);
                    }
                    AstNode::Literal(value, _) => {
                        if value.get_type() == DataType::Scene {
                            print_scene(&value, scene_nesting_level + 1);
                        }
                        cyan_ln!("{}  {:?}", indentation, scene_node);
                    }
                    AstNode::Space(..) | AstNode::Comment(..) => {
                        dark_grey_ln!("{}  {:?}", indentation, scene_node);
                    }
                    _ => {
                        println!("{}  {:?}", indentation, scene_node);
                    }
                }
            }
        }
        _ => {}
    }
    println!("\n");
}
