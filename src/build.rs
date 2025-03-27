use crate::bs_types::DataType;
use crate::html_output::dom_hooks::{generate_dom_update_js, DOMUpdate};
use crate::html_output::generate_html::create_html_boilerplate;
use crate::html_output::web_parser;
use crate::parsers::ast_nodes::{AstNode, Value};
use crate::settings::Config;
use crate::{settings, Error, ErrorType};
use crate::{tokenizer, Token};
use std::collections::HashMap;

use crate::parsers::build_ast::new_ast;
use crate::tokenizer::TokenPosition;
use colour::{blue_ln, blue_ln_bold, cyan_ln, dark_grey_ln, dark_yellow_ln, green_ln, grey_ln, print_bold, print_ln_bold, yellow_ln_bold};
use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use wasm_encoder::Module;
use wasmparser::validate;

pub struct OutputFile {
    source_code: String,
    output_path: PathBuf,
    source_path: PathBuf,
    html: String,
    wasm: Module,
    imports: Vec<PathBuf>,
    global: bool,
}

impl OutputFile {
    pub fn new(source_code: String, output_path: PathBuf, source_path: PathBuf) -> Self {
        OutputFile {
            source_code,
            output_path,
            source_path,
            html: String::new(),
            wasm: Module::new(),
            imports: Vec::new(),
            global: false,
        }
    }
}
pub struct CompiledExport {
    pub js: String,
    pub wasm: Module,
    pub css: String,
    pub data_type: DataType,
}

impl CompiledExport {
    pub fn new() -> Self {
        CompiledExport {
            js: String::new(),
            wasm: Module::new(),
            css: String::new(),
            data_type: DataType::Inferred,
        }
    }
}

pub fn build(
    entry_path: &PathBuf,
    release_build: bool,
    output_info_level: i32,
) -> Result<Config, Error> {
    // Hashmap for fast lookup of variable declarations
    // The key is the path to the export
    let mut project_exports: HashMap<PathBuf, CompiledExport> = HashMap::new();

    // Create a new PathBuf from the entry_path
    let entry_dir = match std::env::current_dir() {
        Ok(dir) => dir.join(entry_path),
        Err(e) => {
            return Err(Error {
                msg: format!("Error getting current directory: {:?}", e),
                start_pos: TokenPosition::default(),
                end_pos: TokenPosition::default(),
                file_path: entry_path.to_owned(),
                error_type: ErrorType::File,
            });
        }
    };

    // Read content from a test file
    print_ln_bold!("Project Directory: ");
    dark_yellow_ln!("{:?}", &entry_dir);

    let mut source_code_to_parse: Vec<OutputFile> = Vec::new();
    let mut project_config = Config::default();

    // check to see if there is a config.bs file in this directory
    // if there is, read it and set the config settings
    // and check where the project entry points are
    enum CompileType {
        SingleFile(PathBuf, String), // File Name, Source Code
        MultiFile(PathBuf, String),  // Config file content
    }

    // Single BS file
    let config = if entry_dir.extension() == Some("bs".as_ref()) {
        let source_code = fs::read_to_string(&entry_dir);
        match source_code {
            Ok(content) => CompileType::SingleFile(entry_dir.with_extension("html"), content),
            Err(e) => {
                return Err(Error {
                    msg: format!("Error reading file: {:?}", e),
                    start_pos: TokenPosition::default(),
                    end_pos: TokenPosition::default(),
                    file_path: entry_dir.to_owned(),
                    error_type: ErrorType::File,
                });
            }
        }

    // Full project with config file
    } else {
        let config_path = entry_dir.join(settings::CONFIG_FILE_NAME);
        let source_code = fs::read_to_string(&config_path);
        match source_code {
            Ok(content) => CompileType::MultiFile(entry_dir.to_owned(), content),
            Err(_) => {
                return Err(Error {
                    msg: "No config file found in directory".to_string(),
                    start_pos: TokenPosition::default(),
                    end_pos: TokenPosition::default(),
                    file_path: config_path,
                    error_type: ErrorType::File,
                });
            }
        }
    };

    match config {
        CompileType::SingleFile(file_path, code) => {
            source_code_to_parse.push(OutputFile::new(
                code, 
                file_path.with_extension("html"), 
                file_path
            ));
        }

        CompileType::MultiFile(entry_dir, config_source_code) => {
            // Parse the config file
            let mut token_context = match tokenizer::tokenize(&config_source_code, "config") {
                Ok(tokens) => tokens,
                Err(e) => {
                    return Err(e.to_error(entry_dir.join("#config.bs")));
                }
            };

            let (config_ast, _) = match new_ast(&mut token_context, &[], &Vec::new(), true) {
                Ok(ast) => ast,
                Err(e) => return Err(e.to_error(entry_dir.join("#config.bs"))),
            };

            // get all exported variables from the config file
            get_config_from_ast(&config_ast, &mut project_config)?;

            let src_dir = entry_dir.join(&project_config.src);
            let output_dir = match release_build {
                true => entry_dir.join(&project_config.release_folder),
                false => entry_dir.join(&project_config.dev_folder),
            };

            add_bs_files_to_parse(&mut source_code_to_parse, output_dir, src_dir.to_owned())?;

            project_config.src = src_dir;
        }
    }

    // Compile all output files
    // And collect all exported functions and variables from the module
    // After compiling, collect all imported modules and add them to the list of exported modules
    for file in &mut source_code_to_parse {
        let (compiled_html, compiled_wasm, import_requests) = compile(
            file,
            release_build,
            output_info_level,
            &mut project_config,
            &mut project_exports,
        )?;

        file.html = compiled_html;
        file.wasm = compiled_wasm;
        file.imports.extend(import_requests);
    }

    // Add imports to the compiled code of the files
    for file in &mut source_code_to_parse {
        let mut js_imports: String = String::new();

        // Add the imports to the files source code importing them after compiling all of them
        for import in &file.imports {
            let requested_module = match project_exports.get(import) {
                Some(export) => export,
                None => {
                    return Err(Error {
                        msg: format!(
                            "Could not find module to add import to. May not be exported. {:?}",
                            import
                        ),
                        start_pos: TokenPosition {
                            line_number: 0,
                            char_column: 0,
                        },
                        end_pos: TokenPosition {
                            line_number: 0,
                            char_column: 0,
                        },
                        file_path: import.to_owned(),
                        error_type: ErrorType::File,
                    })
                }
            };

            js_imports += &requested_module.js;
        }

        file.html = file.html.replace("//imports", &js_imports);

        // Write the file to the output directory
        write_output_file(file)?;
    }

    // Any HTML files in the output dir not on the list of files to compile should be deleted
    if entry_dir.is_dir() {
        let output_dir = match release_build {
            true => PathBuf::from(&entry_dir).join(&project_config.release_folder),
            false => PathBuf::from(&entry_dir).join(&project_config.dev_folder),
        };

        let dir_files = match fs::read_dir(&output_dir) {
            Ok(dir) => dir,
            Err(e) => {
                return Err(Error {
                    msg: format!(
                        "Error reading output_dir directory: {:?}. {:?}",
                        &output_dir, e
                    ),
                    start_pos: TokenPosition::default(),
                    end_pos: TokenPosition::default(),
                    file_path: output_dir,
                    error_type: ErrorType::File,
                });
            }
        };

        for file in dir_files {
            let file = match file {
                Ok(f) => f,
                Err(e) => {
                    return Err(Error {
                        msg: format!("Error reading file when deleting old files: {:?}", e),
                        start_pos: TokenPosition::default(),
                        end_pos: TokenPosition::default(),
                        file_path: output_dir,
                        error_type: ErrorType::File,
                    });
                }
            };

            let file_path = file.path();
            if (file_path.extension() == Some("html".as_ref())
                || file_path.extension() == Some("wasm".as_ref()))
                && !source_code_to_parse
                    .iter()
                    .any(|f| f.output_path.file_stem() == file_path.file_stem())
            {
                match fs::remove_file(&file_path) {
                    Ok(_) => {
                        blue_ln!("Deleted unused file: {:?}", file_path);
                    }
                    Err(e) => {
                        return Err(Error {
                            msg: format!("Error deleting file: {:?}", e),
                            start_pos: TokenPosition::default(),
                            end_pos: TokenPosition::default(),
                            file_path: file_path.to_owned(),
                            error_type: ErrorType::File,
                        });
                    }
                }
            }
        }
    }

    Ok(project_config)
}

// Look for every subdirectory inside of dir and add all .bs files to the source_code_to_parse
pub fn add_bs_files_to_parse(
    source_code_to_parse: &mut Vec<OutputFile>,
    output_dir: PathBuf,
    src_dir: PathBuf,
) -> Result<(), Error> {
    // Can't just use the src_dir from config, because this might be recursively called for new subdirectories

    // Read all files in the src directory
    let all_dir_entries: fs::ReadDir = match fs::read_dir(&src_dir) {
        Ok(dir) => dir,
        Err(e) => {
            return Err(Error {
                msg: format!("Error reading directory (add bs files to parse): {:?}", e),
                start_pos: TokenPosition::default(),
                end_pos: TokenPosition::default(),
                file_path: src_dir,
                error_type: ErrorType::File,
            });
        }
    };

    for file in all_dir_entries {
        match file {
            Ok(f) => {
                let file_path = f.path();

                // If it's a .bs file, add it to the list of files to compile
                if file_path.extension() == Some("bs".as_ref()) {
                    let code = match fs::read_to_string(&file_path) {
                        Ok(content) => content,
                        Err(e) => {
                            return Err(Error {
                                msg: format!("Error reading a file when reading all bs files in directory: {:?}", e),
                                start_pos: TokenPosition::default(),
                                end_pos: TokenPosition::default(),
                                file_path: src_dir,
                                error_type: ErrorType::File,
                            });
                        }
                    };

                    let mut global = false;

                    let file_name = match file_path.file_stem().unwrap().to_str() {
                        Some(stem_str) => {
                            if stem_str.contains(settings::GLOBAL_PAGE_KEYWORD) {
                                global = true;
                                settings::GLOBAL_PAGE_KEYWORD.to_string()
                            } else if stem_str.contains(settings::COMP_PAGE_KEYWORD) {
                                settings::INDEX_PAGE_KEYWORD.to_string()
                            } else {
                                stem_str.to_string()
                            }
                        }
                        None => {
                            return Err(Error {
                                msg: "Error getting file stem".to_string(),
                                start_pos: TokenPosition::default(),
                                end_pos: TokenPosition::default(),
                                file_path,
                                error_type: ErrorType::File,
                            });
                        }
                    };

                    let final_file = OutputFile::new(
                        code, 
                        PathBuf::from(&output_dir)
                            .join(file_name)
                            .with_extension("html"),
                        file_path
                    );

                    if global {
                        source_code_to_parse.insert(0, final_file);
                    } else {
                        source_code_to_parse.push(final_file);
                    }

                // If directory, recursively call add_bs_files_to_parse
                } else if file_path.is_dir() {
                    // Add the new direcory folder to the output directory
                    let new_output_dir = output_dir.join(file_path.file_stem().unwrap());

                    // Recursively call add_bs_files_to_parse on the new directory
                    add_bs_files_to_parse(source_code_to_parse, new_output_dir, file_path)?;

                // HANDLE USING JS / HTML / CSS MIXED INTO THE PROJECT
                } else if let Some(ext) = file_path.extension() {
                    // TEMPORARY: JUST PUT THEM DIRECTLY INTO THE OUTPUT DIRECTORY
                    if ext == "js" || ext == "html" || ext == "css" {
                        let file_name = file_path.file_name().unwrap().to_str().unwrap();
                        source_code_to_parse.push(OutputFile::new(
                            String::new(), 
                            output_dir.join(file_name), 
                            file_path
                        ));
                    }
                }
            }

            Err(e) => {
                return Err(Error {
                    msg: format!("Error reading file while adding bs files to parse: {:?}", e),
                    start_pos: TokenPosition::default(),
                    end_pos: TokenPosition::default(),
                    file_path: src_dir,
                    error_type: ErrorType::File,
                });
            }
        }
    }

    Ok(())
}

fn compile(
    output: &OutputFile,
    release_build: bool,
    output_info_level: i32,
    project_config: &mut Config,
    mut exports: &mut HashMap<PathBuf, CompiledExport>,
    // html, wasm, import requests
) -> Result<(String, Module, Vec<PathBuf>), Error> {
    print_bold!("\nCompiling: ");

    let file_stem = output.output_path.to_owned();
    let file_name = file_stem
        .file_stem()
        .unwrap_or(OsStr::new(""))
        .to_str()
        .unwrap_or("");

    if file_name.is_empty() {
        return Err(Error {
            msg: "File name is empty".to_string(),
            start_pos: TokenPosition::default(),
            end_pos: TokenPosition::default(),
            file_path: PathBuf::from(""),
            error_type: ErrorType::File,
        });
    }

    dark_yellow_ln!("{:?}", file_name);

    // For letting the user know how long compile times are taking
    let time = Instant::now();

    // TOKENIZER
    let mut token_context = match tokenizer::tokenize(&output.source_code, file_name) {
        Ok(tokens) => tokens,
        Err(e) => {
            return Err(e.to_error(PathBuf::from(&output.source_path)));
        }
    };

    if output_info_level > 5 {
        print_token_output(&token_context.tokens);
    }

    if output_info_level > 2 {
        print!("Tokenized in: ");
        green_ln!("{:?}", time.elapsed());
    }

    let time = Instant::now();

    // PARSER
    let (ast, imports) = match new_ast(&mut token_context, &[], &[], true) {
        Ok(ast) => ast,
        Err(e) => {
            return Err(e.to_error(PathBuf::from(&output.source_path)));
        }
    };

    if output_info_level > 4 {
        print_ast_output(&ast);
    }

    if output_info_level > 2 {
        print!("AST created in: ");
        green_ln!("{:?}", time.elapsed());
    }

    let time = Instant::now();

    // IMPORTS
    let mut import_requests = Vec::new();
    for import in imports {
        match import {
            AstNode::Use(module_path, _) => {
                import_requests.push(module_path);
            }
            _ => {
                return Err(Error {
                    msg: "Import must be a string literal. Caught in build. This should not get this far".to_string(),
                    start_pos: TokenPosition::default(),
                    end_pos: TokenPosition::default(),
                    file_path: PathBuf::from(&output.source_path),
                    error_type: ErrorType::Syntax,
                });
            }
        }
    }

    // For each subdirectory from the dist or dev folder of the output_dir, add a ../ to the dist_url
    // This is for linking to CSS/images/other pages etc. in the HTML
    let output_dir_name = if release_build {
        &project_config.release_folder
    } else {
        &project_config.dev_folder
    };

    for ancestor in output.output_path.ancestors().skip(1) {
        if let Some(stem) = ancestor.file_stem() {
            if *stem == **output_dir_name {
                break;
            }
        };
        project_config.html_meta.page_root_url.push_str("../");
    }

    // PARSING INTO HTML
    let parser_output = match web_parser::parse(
        ast,
        project_config,
        file_name,
        &mut Module::new(),
        &mut exports,
    ) {
        Ok(parser_output) => parser_output,
        Err(e) => {
            return Err(e.to_error(PathBuf::from(&output.source_path)));
        }
    };

    // Add the required minimum dom update JS to the start of the parser JS output
    // Any other dom update JS would be dynamically added to the page by the parser only if needed
    let all_js = format!(
        "{}\n{}",
        generate_dom_update_js(DOMUpdate::InnerHTML),
        parser_output.js
    );

    // Add the HTML boilerplate and then add the parser output to the page
    let html = match create_html_boilerplate(&project_config.html_meta, release_build) {
        Ok(module_output) => module_output
            .replace("page-template", &parser_output.html)
            .replace("@page-css", &parser_output.css)
            .replace("page-title", &parser_output.page_title)
            .replace("//js", &all_js)
            .replace("wasm-module-name", file_name),
        Err(e) => {
            return Err(e.to_error(PathBuf::from(&output.source_path)));
        }
    };

    if output_info_level > 2 {
        print!("HTML/CSS/WAT/JS generated in: ");
        green_ln!("{:?}", time.elapsed());
    }

    // WASM GENERATION
    // let all_parsed_wasm = &format!(
    //     "(module {}(func (export \"set_wasm_globals\"){}))",
    //     &parser_output.wat, parser_output.wat_globals
    // );

    exports.extend(parser_output.exported);

    Ok((html, parser_output.wasm, import_requests))
}

fn write_output_file(output: &OutputFile) -> Result<(), Error> {
    // If the output directory does not exist, create it
    let file_path = output.output_path.to_owned();
    let parent_dir = match output.output_path.parent() {
        Some(dir) => dir,
        None => {
            return Err(Error {
                msg: format!(
                    "Error getting parent directory of output file when writing: {:?}",
                    file_path
                ),
                start_pos: TokenPosition::default(),
                end_pos: TokenPosition::default(),
                file_path: output.output_path.to_owned(),
                error_type: ErrorType::File,
            });
        }
    };

    // Create the needed directory if it doesn't exist
    if fs::metadata(parent_dir).is_err() {
        match fs::create_dir_all(parent_dir) {
            Ok(_) => {}
            Err(e) => {
                return Err(Error {
                    msg: format!("Error creating directory: {:?}", e),
                    start_pos: TokenPosition::default(),
                    end_pos: TokenPosition::default(),
                    file_path: output.output_path.to_owned(),
                    error_type: ErrorType::File,
                });
            }
        }
    }

    match fs::write(&output.output_path, &output.html) {
        Ok(_) => {}
        Err(e) => {
            return Err(Error {
                msg: format!("Error writing file: {:?}", e),
                start_pos: TokenPosition::default(),
                end_pos: TokenPosition::default(),
                file_path,
                error_type: ErrorType::File,
            });
        }
    }

    let wasm = output.wasm.to_owned().finish();

    match validate(&wasm) {
        Ok(_) => {}
        Err(e) => {
            return Err(Error {
                msg: format!("Error validating WASM module: {:?}", e),
                start_pos: TokenPosition::default(),
                end_pos: TokenPosition::default(),
                file_path,
                error_type: ErrorType::File,
            });
        }
    };

    // Write the wasm file to the same directory
    match fs::write(output.output_path.with_extension("wasm"), wasm) {
        Ok(_) => {}
        Err(e) => {
            return Err(Error {
                msg: format!("Error writing WASM module file: {:?}", e),
                start_pos: TokenPosition::default(),
                end_pos: TokenPosition::default(),
                file_path,
                error_type: ErrorType::File,
            });
        }
    }

    Ok(())
}

fn get_config_from_ast(ast: &Vec<AstNode>, project_config: &mut Config) -> Result<(), Error> {
    for node in ast {
        if let AstNode::VarDeclaration(name, value, ..) = node {
            // Check if this is the "Config" variable
            match name.as_str() {
                "project" => {
                    project_config.project = match value {
                        Value::String(value) => value.to_owned(),
                        _ => {
                            return Err(Error {
                                msg: "Project name must be a string".to_string(),
                                start_pos: TokenPosition::default(),
                                end_pos: TokenPosition::default(),
                                file_path: PathBuf::from("#config.bs"),
                                error_type: ErrorType::TypeError,
                            })
                        }
                    };
                }

                "src" => {
                    project_config.src = match value {
                        Value::String(value) => PathBuf::from(value),
                        _ => {
                            return Err(Error {
                                msg: "Source folder must be a string".to_string(),
                                start_pos: TokenPosition::default(),
                                end_pos: TokenPosition::default(),
                                file_path: PathBuf::from("#config.bs"),
                                error_type: ErrorType::TypeError,
                            })
                        }
                    };
                }

                "dev" => {
                    project_config.dev_folder = match value {
                        Value::String(value) => PathBuf::from(value),
                        _ => {
                            return Err(Error {
                                msg: "Dev folder must be a string".to_string(),
                                start_pos: TokenPosition::default(),
                                end_pos: TokenPosition::default(),
                                file_path: PathBuf::from("#config.bs"),
                                error_type: ErrorType::TypeError,
                            })
                        }
                    };
                }

                "release" => {
                    project_config.release_folder = match value {
                        Value::String(value) => PathBuf::from(value),
                        _ => {
                            return Err(Error {
                                msg: "Release folder must be a string".to_string(),
                                start_pos: TokenPosition::default(),
                                end_pos: TokenPosition::default(),
                                file_path: PathBuf::from("#config.bs"),
                                error_type: ErrorType::TypeError,
                            })
                        }
                    };
                }

                "name" => {
                    project_config.name = match value {
                        Value::String(value) => value.to_owned(),
                        _ => {
                            return Err(Error {
                                msg: "Name must be a string".to_string(),
                                start_pos: TokenPosition::default(),
                                end_pos: TokenPosition::default(),
                                file_path: PathBuf::from("#config.bs"),
                                error_type: ErrorType::TypeError,
                            })
                        }
                    };
                }

                "version" => {
                    project_config.version = match value {
                        Value::String(value) => value.to_owned(),
                        _ => {
                            return Err(Error {
                                msg: "Version must be a string".to_string(),
                                start_pos: TokenPosition::default(),
                                end_pos: TokenPosition::default(),
                                file_path: PathBuf::from("#config.bs"),
                                error_type: ErrorType::TypeError,
                            })
                        }
                    };
                }

                "author" => {
                    project_config.author = match value {
                        Value::String(value) => value.to_owned(),
                        _ => {
                            return Err(Error {
                                msg: "Author must be a string".to_string(),
                                start_pos: TokenPosition::default(),
                                end_pos: TokenPosition::default(),
                                file_path: PathBuf::from("#config.bs"),
                                error_type: ErrorType::TypeError,
                            })
                        }
                    };
                }

                "license" => {
                    project_config.license = match value {
                        Value::String(value) => value.to_owned(),
                        _ => {
                            return Err(Error {
                                msg: "License must be a string".to_string(),
                                start_pos: TokenPosition::default(),
                                end_pos: TokenPosition::default(),
                                file_path: PathBuf::from("#config.bs"),
                                error_type: ErrorType::TypeError,
                            })
                        }
                    };
                }

                "html_settings" => {
                    return match value {
                        Value::Structure(args) => {
                            for arg in args {
                                match arg.name.as_str() {
                                    "site_title" => {
                                        project_config.html_meta.site_title = match &arg.value {
                                            Value::String(value) => value.to_owned(),
                                            _ => {
                                                return Err(Error {
                                                    msg: "Site title must be a string".to_string(),
                                                    start_pos: TokenPosition::default(),
                                                    end_pos: TokenPosition::default(),
                                                    file_path: PathBuf::from("#config.bs"),
                                                    error_type: ErrorType::TypeError,
                                                })
                                            }
                                        };
                                    }

                                    "page_description" => {
                                        project_config.html_meta.page_description = match &arg.value
                                        {
                                            Value::String(value) => value.to_owned(),
                                            _ => {
                                                return Err(Error {
                                                    msg: "Page description must be a string"
                                                        .to_string(),
                                                    start_pos: TokenPosition::default(),
                                                    end_pos: TokenPosition::default(),
                                                    file_path: PathBuf::from("#config.bs"),
                                                    error_type: ErrorType::TypeError,
                                                })
                                            }
                                        };
                                    }

                                    "site_url" => {
                                        project_config.html_meta.site_url = match &arg.value {
                                            Value::String(value) => value.to_owned(),
                                            _ => {
                                                return Err(Error {
                                                    msg: "Site url must be a string".to_string(),
                                                    start_pos: TokenPosition::default(),
                                                    end_pos: TokenPosition::default(),
                                                    file_path: PathBuf::from("#config.bs"),
                                                    error_type: ErrorType::TypeError,
                                                })
                                            }
                                        };
                                    }

                                    "page_url" => {
                                        project_config.html_meta.page_url = match &arg.value {
                                            Value::String(value) => value.to_owned(),
                                            _ => {
                                                return Err(Error {
                                                    msg: "Page url must be a string".to_string(),
                                                    start_pos: TokenPosition::default(),
                                                    end_pos: TokenPosition::default(),
                                                    file_path: PathBuf::from("#config.bs"),
                                                    error_type: ErrorType::TypeError,
                                                })
                                            }
                                        };
                                    }

                                    "page_og_title" => {
                                        project_config.html_meta.page_og_title = match &arg.value {
                                            Value::String(value) => value.to_owned(),
                                            _ => {
                                                return Err(Error {
                                                    msg: "Page og title must be a string"
                                                        .to_string(),
                                                    start_pos: TokenPosition::default(),
                                                    end_pos: TokenPosition::default(),
                                                    file_path: PathBuf::from("#config.bs"),
                                                    error_type: ErrorType::TypeError,
                                                })
                                            }
                                        };
                                    }

                                    "page_og_description" => {
                                        project_config.html_meta.page_og_description =
                                            match &arg.value {
                                                Value::String(value) => value.to_owned(),
                                                _ => {
                                                    return Err(Error {
                                                        msg: "Page og description must be a string"
                                                            .to_string(),
                                                        start_pos: TokenPosition::default(),
                                                        end_pos: TokenPosition::default(),
                                                        file_path: PathBuf::from("#config.bs"),
                                                        error_type: ErrorType::TypeError,
                                                    })
                                                }
                                            };
                                    }

                                    "page_image_url" => {
                                        project_config.html_meta.page_image_url = match &arg.value {
                                            Value::String(value) => value.to_owned(),
                                            _ => {
                                                return Err(Error {
                                                    msg: "Page image url must be a string"
                                                        .to_string(),
                                                    start_pos: TokenPosition::default(),
                                                    end_pos: TokenPosition::default(),
                                                    file_path: PathBuf::from("#config.bs"),
                                                    error_type: ErrorType::TypeError,
                                                })
                                            }
                                        };
                                    }

                                    "page_image_alt" => {
                                        project_config.html_meta.page_image_alt = match &arg.value {
                                            Value::String(value) => value.to_owned(),
                                            _ => {
                                                return Err(Error {
                                                    msg: "Page image alt must be a string"
                                                        .to_string(),
                                                    start_pos: TokenPosition::default(),
                                                    end_pos: TokenPosition::default(),
                                                    file_path: PathBuf::from("#config.bs"),
                                                    error_type: ErrorType::TypeError,
                                                })
                                            }
                                        };
                                    }

                                    "page_locale" => {
                                        project_config.html_meta.page_locale = match &arg.value {
                                            Value::String(value) => value.to_owned(),
                                            _ => {
                                                return Err(Error {
                                                    msg: "Page locale must be a string".to_string(),
                                                    start_pos: TokenPosition::default(),
                                                    end_pos: TokenPosition::default(),
                                                    file_path: PathBuf::from("#config.bs"),
                                                    error_type: ErrorType::TypeError,
                                                })
                                            }
                                        };
                                    }

                                    "page_type" => {
                                        project_config.html_meta.page_type = match &arg.value {
                                            Value::String(value) => value.to_owned(),
                                            _ => {
                                                return Err(Error {
                                                    msg: "Page type must be a string".to_string(),
                                                    start_pos: TokenPosition::default(),
                                                    end_pos: TokenPosition::default(),
                                                    file_path: PathBuf::from("#config.bs"),
                                                    error_type: ErrorType::TypeError,
                                                })
                                            }
                                        };
                                    }

                                    "page_twitter_large_image" => {
                                        project_config.html_meta.page_twitter_large_image =
                                            match &arg.value {
                                                Value::String(value) => value.to_owned(),
                                                _ => return Err(Error {
                                                    msg:
                                                        "Page twitter large image must be a string"
                                                            .to_string(),
                                                    start_pos: TokenPosition::default(),
                                                    end_pos: TokenPosition::default(),
                                                    file_path: PathBuf::from("#config.bs"),
                                                    error_type: ErrorType::TypeError,
                                                }),
                                            };
                                    }

                                    "page_canonical_url" => {
                                        project_config.html_meta.page_canonical_url =
                                            match &arg.value {
                                                Value::String(value) => value.to_owned(),
                                                _ => {
                                                    return Err(Error {
                                                        msg: "Page canonical url must be a string"
                                                            .to_string(),
                                                        start_pos: TokenPosition::default(),
                                                        end_pos: TokenPosition::default(),
                                                        file_path: PathBuf::from("#config.bs"),
                                                        error_type: ErrorType::TypeError,
                                                    })
                                                }
                                            };
                                    }

                                    "page_root_url" => {
                                        project_config.html_meta.page_root_url = match &arg.value {
                                            Value::String(value) => value.to_owned(),
                                            _ => {
                                                return Err(Error {
                                                    msg: "Page root url must be a string"
                                                        .to_string(),
                                                    start_pos: TokenPosition::default(),
                                                    end_pos: TokenPosition::default(),
                                                    file_path: PathBuf::from("#config.bs"),
                                                    error_type: ErrorType::TypeError,
                                                })
                                            }
                                        };
                                    }

                                    "image_folder_url" => {
                                        project_config.html_meta.image_folder_url = match &arg.value
                                        {
                                            Value::String(value) => value.to_owned(),
                                            _ => {
                                                return Err(Error {
                                                    msg: "Image folder url must be a string"
                                                        .to_string(),
                                                    start_pos: TokenPosition::default(),
                                                    end_pos: TokenPosition::default(),
                                                    file_path: PathBuf::from("#config.bs"),
                                                    error_type: ErrorType::TypeError,
                                                })
                                            }
                                        };
                                    }
                                    _ => {}
                                }
                            }
                            Ok(())
                        }
                        _ => Err(Error {
                            msg: "HTML settings must be a struct".to_string(),
                            start_pos: TokenPosition::default(),
                            end_pos: TokenPosition::default(),
                            file_path: PathBuf::from("#config.bs"),
                            error_type: ErrorType::TypeError,
                        }),
                    };
                }

                _ => {}
            }

            // if *is_exported {
            //     exported_variables.push(Arg {
            //         name: name.to_owned(),
            //         data_type: data_type.to_owned(),
            //         value: value.to_owned(),
            //     });
            // }
        }
    }

    Ok(())
}

fn print_token_output(tokens: &Vec<Token>) {
    yellow_ln_bold!("TOKENIZING FILE\n");

    for token in tokens {
        match token {
            Token::SceneHead | Token::SceneClose => {
                blue_ln!("{:?}", token);
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
}

fn print_ast_output(ast: &Vec<AstNode>) {
    yellow_ln_bold!("CREATING AST\n");

    for node in ast {
        match node {
            AstNode::Literal(value, _) => {
                match value.get_type() {
                    DataType::Scene => {
                        print_scene(value, 0);
                    }
                    DataType::Style => {
                        green_ln!("{:?}", value);
                    }
                    _ => {
                        cyan_ln!("{:?}", value);
                    }
                }
            }
            AstNode::Comment(..) => {
                grey_ln!("{:?}", node);
            }
            _ => {
                println!("{:?}", node);
            }
        }
    }

    fn print_scene(scene: &Value, scene_nesting_level: u32) {
        // Indent the scene by how nested it is
        let mut indentation = String::new();
        for _ in 0..scene_nesting_level {
            indentation.push('\t');
        }

        if let Value::Scene(nodes, styles, ..) = scene {
            blue_ln_bold!("\n{}Scene Styles: ", indentation);
            for style in styles {
                green_ln!("{}  {:?}", indentation, style);
            }
            blue_ln_bold!("{}Scene Body:", indentation);

            for scene_node in nodes {
                match scene_node {
                    AstNode::Literal(value, _) => {
                        match value.get_type() {
                            DataType::Scene => {
                                print_scene(value, scene_nesting_level + 1);
                            }
                            DataType::Style => {
                                green_ln!("{}  {:?}", indentation, value);
                            }
                            _ => {
                                cyan_ln!("\n{}  {:?}", indentation, value);
                            }
                        }
                    }
                    AstNode::Comment(comment) => {
                        dark_grey_ln!("{}  {}", indentation, comment);
                    }
                    _ => {
                        println!("{}  {:?}", indentation, scene_node.get_value());
                    }
                }
            }
        }
        println!("\n");
    }
}
