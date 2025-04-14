use crate::bs_types::DataType;
use crate::html_output::dom_hooks::{generate_dom_update_js, DOMUpdate};
use crate::html_output::generate_html::create_html_boilerplate;
use crate::html_output::web_parser;
use crate::parsers::ast_nodes::{AstNode, Value};
use crate::settings::Config;
use crate::{settings, Error, ErrorType};
use crate::{tokenizer, Token};
use std::collections::HashMap;
use crate::parsers::build_ast::{new_ast, TokenContext};
use crate::tokenizer::TokenPosition;
use colour::{blue_ln, blue_ln_bold, cyan_ln, dark_yellow_ln, green_bold, green_ln, green_ln_bold, grey_ln, print_bold, print_ln_bold, yellow_ln_bold};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use wasm_encoder::Module;
use wasmparser::validate;

pub struct OutputModule {
    source_code: String,
    output_path: PathBuf,
    source_path: PathBuf,
    html: String,
    js: String,
    wasm: Module,
}

impl OutputModule {
    pub fn new(source_code: String, output_path: PathBuf, source_path: PathBuf) -> Self {
        OutputModule {
            source_code,
            output_path,
            source_path,
            html: String::new(),
            js: String::new(),
            wasm: Module::new(),
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
    let mut public_exports: HashMap<String, DataType> = HashMap::new();

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

    let mut source_code_to_parse: Vec<OutputModule> = Vec::new();
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
            source_code_to_parse.push(OutputModule::new(
                code, 
                file_path.with_extension("html"), 
                file_path
            ));
        }

        CompileType::MultiFile(entry_dir, config_source_code) => {

            let config_path = entry_dir.join(settings::CONFIG_FILE_NAME);

            // Parse the config file
            let (mut token_context, exports) = match tokenizer::tokenize(&config_source_code, &config_path) {
                Ok(tokens) => tokens,
                Err(e) => {
                    return Err(e.to_error(config_path));
                }
            };

            public_exports.extend(exports);

            let config_ast = match new_ast(
                &mut token_context,
                &[],
                &mut DataType::None,
                &config_path,
                &mut true)
            {
                Ok(ast) => ast,
                Err(e) => return Err(e.to_error(config_path)),
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

    // First tokenize all files
    for file in &mut source_code_to_parse {
        dark_yellow_ln!("{:?}", file.output_path.file_name());
        
        // For letting the user know how long compile times are taking
        let time = Instant::now();

        // TOKENIZER
        let (mut token_context, exports) = match tokenizer::tokenize(&file.source_code, &file.output_path) {
            Ok(tokens) => tokens,
            Err(e) => {
                return Err(e.to_error(PathBuf::from(&file.source_path)));
            }
        };

        // Exports are type checked (will be enforced to have an explicit type declaration)
        // Imports currently are just for anything public in a module, there is no specifying imports atm
        public_exports.extend(exports);

        if output_info_level > 5 {
            print_token_output(&token_context.tokens);
        }

        if output_info_level > 2 {
            print!("Tokenized in: ");
            green_ln!("{:?}", time.elapsed());
        }

        let compile_result = compile(
            &mut token_context,
            file,
            release_build,
            output_info_level,
            &mut project_config,
        )?;

        let mut js_imports: String = format!("<script type=\"module\" src=\"./{}\"></script>", &file.output_path.with_extension("js").file_name().unwrap().to_string_lossy());

        for import in &compile_result.import_requests {
            js_imports += format!("<script type=\"module\" src=\"{}.js\"></script>", import).as_str();
        }

        file.html = compile_result.html.replace("<!--//js-modules-->", &js_imports);
        file.wasm = compile_result.wasm;
        file.js = compile_result.js;
        
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

            if (
                // These checks are mostly here for safety to avoid accidentally deleting files
                (  file_path.extension() == Some("html".as_ref())
                || file_path.extension() == Some("wasm".as_ref()))
                || file_path.extension() == Some("js".as_ref())  )

                // If the file is not in the source code to parse, it's not needed
                && !source_code_to_parse.iter().any(|f| f.output_path.with_extension("") == file_path.with_extension(""))
            
            {
                match fs::remove_file(&file_path) {
                    Ok(_) => {
                        blue_ln!("Deleted unused file: {:?}", file_path.file_name());
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
    source_code_to_parse: &mut Vec<OutputModule>,
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
                                settings::INDEX_PAGE_NAME.to_string()
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

                    let final_output_file = OutputModule::new(
                        code, 
                        PathBuf::from(&output_dir)
                            .join(file_name)
                            .with_extension("html"),
                        file_path
                    );

                    if global {
                        source_code_to_parse.insert(0, final_output_file);
                    } else {
                        source_code_to_parse.push(final_output_file);
                    }

                // If directory, recursively call add_bs_files_to_parse
                } else if file_path.is_dir() {
                    // Add the new directory folder to the output directory
                    let new_output_dir = output_dir.join(file_path.file_stem().unwrap());

                    // Recursively call add_bs_files_to_parse on the new directory
                    add_bs_files_to_parse(source_code_to_parse, new_output_dir, file_path)?;

                // HANDLE USING JS / HTML / CSS MIXED INTO THE PROJECT
                } else if let Some(ext) = file_path.extension() {
                    // TEMPORARY: JUST PUT THEM DIRECTLY INTO THE OUTPUT DIRECTORY
                    if ext == "js" || ext == "html" || ext == "css" {
                        let file_name = file_path.file_name().unwrap().to_str().unwrap();
                        
                        source_code_to_parse.push(OutputModule::new(
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

struct CompileResult {
    html: String,
    js: String,
    wasm: Module,
    import_requests: Vec<String>,
}

fn compile(
    token_context: &mut TokenContext,
    output: &OutputModule,
    release_build: bool,
    output_info_level: i32,
    project_config: &mut Config,
) -> Result<CompileResult, Error> {
    print_bold!("\nCompiling: ");

    let time = Instant::now();

    // AST PARSER
    let ast = match new_ast(
        token_context,
        &[], 
        &mut DataType::None,
        &output.source_path,
        &mut true) {
        Ok(ast) => ast,
        Err(e) => {
            return Err(e.to_error(PathBuf::from(&output.source_path)));
        }
    };

    if output_info_level > 4 {
        yellow_ln_bold!("CREATING AST\n");
        print_ast_output(&ast);
    }

    if output_info_level > 2 {
        print!("AST created in: ");
        green_ln!("{:?}", time.elapsed());
    }

    let time = Instant::now();

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
        &output.source_path.to_string_lossy(),
        &mut Module::new(),
    ) {
        Ok(parser_output) => parser_output,
        Err(e) => {
            return Err(e.to_error(PathBuf::from(&output.source_path)));
        }
    };

    // Add the required minimum dom update JS to the start of the parser JS output
    // Any other dom update JS would be dynamically added to the page by the parser only if needed
    let js = format!(
        "{}\n{}",
        generate_dom_update_js(DOMUpdate::InnerHTML),
        parser_output.js
    );

    // Add the HTML boilerplate and then add the parser output to the page
    let html = match create_html_boilerplate(&project_config.html_meta, release_build) {
        Ok(module_output) => module_output
            .replace("<!--page-template-->", &parser_output.html)
            .replace("page-title", &parser_output.page_title)
            .replace("wasm-module-name", &output.source_path.to_string_lossy()),
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

    Ok(CompileResult {
        html,
        js,
        wasm: parser_output.wasm,
        import_requests: parser_output.import_requests,
    })
}

fn write_output_file(output: &OutputModule) -> Result<(), Error> {
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

    // Write the JS file to the same directory
    match fs::write(output.output_path.with_extension("js"), &output.js) {
        Ok(_) => {}
        Err(e) => {
            return Err(Error {
                msg: format!("Error writing JS module file: {:?}", e),
                start_pos: TokenPosition::default(),
                end_pos: TokenPosition::default(),
                file_path,
                error_type: ErrorType::File,
            });
        }
    };

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
                        Value::StructLiteral(args) => {
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
    for node in ast {
        match node {
            AstNode::Literal(value, _) => {
                match value.get_type() {
                    DataType::Scene(_) => {
                        print_scene(value, 0);
                    }
                    _ => {
                        cyan_ln!("{:?}", value);
                    }
                }
            }
            AstNode::Comment(..) => {
                grey_ln!("{:?}", node);
            }
            AstNode::Function(name, args, body, ..) => {
                blue_ln!("Function: {:?}", name);
                for (i, arg) in args.iter().enumerate() {
                    green_ln_bold!("    {}: {} = {:?}", i, arg.name, arg.value);
                }
                print_ast_output(body);
            }
            AstNode::FunctionCall(name, args, ..) => {
                blue_ln!("Function Call: {:?}", name);
                green_bold!("Arguments: ");
                for (i, arg) in args.iter().enumerate() {
                    green_ln_bold!("    {}: {:?}", i, arg);
                }
            }
            _ => {
                println!("{:?}", node);
            }
        }
        println!("\n");
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
                println!("{}  {:?}", indentation, scene_node);
            }
        }
        println!("\n");
    }
}
