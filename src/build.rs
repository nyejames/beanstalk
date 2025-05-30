use crate::bs_types::DataType;
use crate::html_output::dom_hooks::{DOMUpdate, generate_dom_update_js};
use crate::html_output::generate_html::create_html_boilerplate;
use crate::html_output::web_parser;
use crate::html_output::web_parser::Target;
use crate::parsers::ast_nodes::{Arg, AstNode, Expr};
use crate::parsers::build_ast::{TokenContext, new_ast};
use crate::settings::{BS_VAR_PREFIX, Config};
use crate::tokenizer::TokenPosition;
use crate::{Error, ErrorType, settings};
use crate::{Token, tokenizer};
use colour::{
    blue_ln, blue_ln_bold, cyan_ln, dark_yellow_ln, green_bold, green_ln, green_ln_bold, grey_ln,
    print_bold, print_ln_bold, yellow_ln_bold,
};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;
use wasm_encoder::Module;
use wasmparser::validate;

pub struct OutputModule {
    source_code: String,
    tokens: TokenContext,
    imports: Vec<String>,
    exports: Vec<usize>,
    output_path: PathBuf,
    source_path: PathBuf,
    html: String,
    js: String,
    wasm: Module,
}

impl OutputModule {
    pub fn new(source_code: impl Into<String>, output_path: PathBuf, source_path: PathBuf) -> Self {
        OutputModule {
            source_code: source_code.into(),
            output_path,
            source_path,
            tokens: TokenContext::default(),
            imports: Vec::new(),
            exports: Vec::new(),
            html: String::new(),
            js: String::new(),
            wasm: Module::new(),
        }
    }
}

pub fn build(
    entry_path: &Path,
    release_build: bool,
    output_info_level: i32,
) -> Result<Config, Error> {
    let mut public_exports: HashMap<String, Vec<Token>> = HashMap::new();

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

    let mut modules_to_parse: Vec<OutputModule> = Vec::new();
    let mut project_config = Config::default();

    // check to see if there is a config.bs file in this directory
    // if there is, read it and set the config settings
    // and check where the project entry points are
    enum CompileType {
        SingleFile(String), // Source Code
        MultiFile(String),  // Config Source Code
    }

    // Single BS file
    let config = if entry_dir.extension() == Some("bs".as_ref()) {
        let source_code = fs::read_to_string(&entry_dir);
        match source_code {
            Ok(content) => CompileType::SingleFile(content),
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

    // Full project with a config file
    } else {
        let config_path = entry_dir.join(settings::CONFIG_FILE_NAME);
        let source_code = fs::read_to_string(&config_path);
        match source_code {
            Ok(content) => CompileType::MultiFile(content),
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

    let mut global_imports: Vec<String> = Vec::new();

    match config {
        CompileType::SingleFile(code) => {
            modules_to_parse.push(OutputModule::new(
                code,
                entry_path.with_extension("html"),
                entry_path.to_owned(),
            ));
        }

        CompileType::MultiFile(config_source_code) => {
            let config_path = entry_dir.join(settings::CONFIG_FILE_NAME);

            // Parse the config file
            let mut tokenizer_output =
                match tokenizer::tokenize(&config_source_code, &settings::CONFIG_FILE_NAME) {
                    Ok(tokens) => tokens,
                    Err(e) => {
                        return Err(e.to_error(config_path));
                    }
                };

            // Anything imported into the config file becomes an import of every module in the project
            global_imports.extend(tokenizer_output.imports);

            let config_block = match new_ast(&mut tokenizer_output.token_context, &[], &mut [], true) {
                Ok(expr) => expr,
                Err(e) => return Err(e.to_error(config_path)),
            };

            get_config_from_ast(config_block.get_block_nodes(), &mut project_config)?;

            let src_dir = entry_dir.join(&project_config.src);
            let output_dir = match release_build {
                true => entry_dir.join(&project_config.release_folder),
                false => entry_dir.join(&project_config.dev_folder),
            };

            add_bs_files_to_parse(&mut modules_to_parse, &output_dir, &src_dir)?;
        }
    }

    // First, tokenise all files
    for module in &mut modules_to_parse {
        // TODO: Yuck, will this always unwrap ok?
        let file_name = module.output_path.file_stem().unwrap().to_str().unwrap();

        dark_yellow_ln!("{:?}", file_name);

        // For letting the user know how long compile times are taking
        let time = Instant::now();

        // Make the imports and exports line up from the root of the project
        // So we can get the right key for the exports
        // Removes the full path and extension
        let relative_export_path = module
            .source_path
            .strip_prefix(&entry_dir)
            .map_err(|_| Error {
                msg: "Could not create relative path to this export from the project directory"
                    .to_string(),
                start_pos: TokenPosition::default(),
                end_pos: TokenPosition::default(),
                file_path: module.source_path.to_owned(),
                error_type: ErrorType::File,
            })?
            .with_extension("");

        // TOKENIZER
        let tokenizer_output = match tokenizer::tokenize(&module.source_code, file_name) {
            Ok(tokens) => tokens,
            Err(e) => {
                return Err(e.to_error(PathBuf::from(&module.source_path)));
            }
        };

        // Create a new export entry for this module
        let export_path = relative_export_path.to_string_lossy().to_string();
        public_exports.insert(export_path.to_owned(), tokenizer_output.exports);

        module.imports = global_imports.to_owned();
        module.imports.extend(tokenizer_output.imports);

        if output_info_level > 5 {
            print_token_output(&tokenizer_output.token_context.tokens);
        }

        module.tokens = tokenizer_output.token_context;

        if output_info_level > 2 {
            print!("Tokenized in: ");
            green_ln!("{:?}", time.elapsed());
        }
    }

    // Resolving exports / imports

    for file in &mut modules_to_parse {
        let compile_result = compile(
            file,
            release_build,
            output_info_level,
            &mut project_config,
            &public_exports,
        )?;

        let mut js_imports: String = format!(
            "<script type=\"module\" src=\"./{}\"></script>",
            &file
                .output_path
                .with_extension("js")
                .file_name()
                .unwrap()
                .to_string_lossy()
        );

        for import in &compile_result.import_requests {
            // Stripping the src folder from the import path,
            // As this directory is removed in the output directory
            let trimmed_import = import.strip_prefix("src/").unwrap_or(import);

            js_imports += &format!(
                "<script type=\"module\" src=\"{}.js\"></script>",
                trimmed_import
            );
        }

        file.html = compile_result
            .html
            .replace("<!--//js-modules-->", &js_imports);
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
                && !modules_to_parse.iter().any(|f| f.output_path.with_extension("") == file_path.with_extension(""))
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

// Look for every subdirectory inside the dir and add all .bs files to the source_code_to_parse
pub fn add_bs_files_to_parse(
    source_code_to_parse: &mut Vec<OutputModule>,
    output_dir: &Path,
    project_root_dir: &Path,
) -> Result<(), Error> {
    // Can't just use the src_dir from config, because this might be recursively called for new subdirectories

    // Read all files in the src directory
    let all_dir_entries: fs::ReadDir = match fs::read_dir(project_root_dir) {
        Ok(dir) => dir,
        Err(e) => {
            return Err(Error {
                msg: format!("Error reading directory (add bs files to parse): {:?}", e),
                start_pos: TokenPosition::default(),
                end_pos: TokenPosition::default(),
                file_path: PathBuf::from(project_root_dir),
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
                                msg: format!(
                                    "Error reading a file when reading all bs files in directory: {:?}",
                                    e
                                ),
                                start_pos: TokenPosition::default(),
                                end_pos: TokenPosition::default(),
                                file_path: PathBuf::from(project_root_dir),
                                error_type: ErrorType::File,
                            });
                        }
                    };

                    // If code is empty, skip compiling this module
                    if code.is_empty() {
                        continue;
                    }

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
                        file_path,
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
                    add_bs_files_to_parse(source_code_to_parse, &new_output_dir, &file_path)?;

                // HANDLE USING JS / HTML / CSS MIXED INTO THE PROJECT
                } else if let Some(ext) = file_path.extension() {
                    // TEMPORARY: PUT THEM DIRECTLY INTO THE OUTPUT DIRECTORY
                    if ext == "js" || ext == "html" || ext == "css" {
                        let file_name = file_path.file_name().unwrap().to_str().unwrap();

                        source_code_to_parse.push(OutputModule::new(
                            "",
                            output_dir.join(file_name),
                            file_path,
                        ));
                    }
                }
            }

            Err(e) => {
                return Err(Error {
                    msg: format!("Error reading file while adding bs files to parse: {:?}", e),
                    start_pos: TokenPosition::default(),
                    end_pos: TokenPosition::default(),
                    file_path: PathBuf::from(project_root_dir),
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
    import_requests: Vec<String>,
}

fn compile(
    module: &mut OutputModule,
    release_build: bool,
    output_info_level: i32,
    project_config: &mut Config,
    public_exports: &HashMap<String, Vec<Token>>,
) -> Result<CompileResult, Error> {
    print_bold!("\nCompiling: ");

    let time = Instant::now();

    let mut js = String::new();

    // Create declarations out of the imports
    // We don't know their Type, so we just use the pointer type for them
    let mut declarations: Vec<Arg> = Vec::new();

    // These are just to insert the HTML module imports into the HTML file.
    let mut import_requests: Vec<String> = Vec::new();
    for import in &mut module.imports {
        // Import the other JS module at the top of this module
        // Import string will be in this format: "path/module:export"
        // Or if we are importing everything: "path/module"
        let split_import_string = import.split(":").collect::<Vec<&str>>();

        // Then strip the src/ from the path if it starts there
        // This is because the output folder gets rid of the src directory
        let import_path = split_import_string[0]
            .strip_prefix("src/")
            .unwrap_or(split_import_string[0]);

        // Importing everything from the module
        if split_import_string.len() > 1 {
            // We need to only put the path into the import requests
            // This is where we remove the colon
            import_requests.push(split_import_string[0].to_string());

            js += &format!(
                "import {BS_VAR_PREFIX}{} from \"./{}.js\";\n",
                split_import_string[1], import_path
            );

            declarations.push(Arg {
                name: split_import_string[1].to_owned(),
                expr: Expr::Reference(
                    split_import_string[1].to_owned(),
                    DataType::Pointer(split_import_string[1].to_owned()),
                    Vec::new(),
                ),
                data_type: DataType::Pointer(split_import_string[1].to_owned()),
            })
        } else {
            import_requests.push(import.to_string());

            // We now need to get the names of all the exports from this module
            let import_names = match public_exports.get(split_import_string[0]) {
                Some(exported_names) => exported_names,
                None => {
                    return Err(Error {
                        msg: format!(
                            "Could not find any exports from module path: {}",
                            import_path
                        ),
                        start_pos: TokenPosition::default(),
                        end_pos: TokenPosition::default(),
                        file_path: module.source_path.to_owned(),
                        error_type: ErrorType::File,
                    });
                }
            };

            let formatted_variable_names = import_names
                .iter()
                .map(|export| format!("{}{}, ", BS_VAR_PREFIX, export.get_name()))
                .collect::<String>();
            js += &format!(
                "import {{{}}} from \"./{}.js\";\n",
                formatted_variable_names, import_path
            );

            for export in import_names {
                declarations.push(Arg {
                    name: export.get_name(),
                    expr: Expr::Reference(export.get_name(), DataType::Inferred(false), Vec::new()),
                    data_type: DataType::Inferred(false),
                })
            }
        }
    }

    // AST PARSER
    let ast = match new_ast(&mut module.tokens, &declarations, &[], true) {
        Ok(block) => block.get_block_nodes().to_owned(),
        Err(e) => {
            return Err(e.to_error(PathBuf::from(&module.source_path)));
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

    for ancestor in module.output_path.ancestors().skip(1) {
        if let Some(stem) = ancestor.file_stem() {
            if *stem == **output_dir_name {
                break;
            }
        };
        project_config.html_meta.page_root_url.push_str("../");
    }

    // PARSING INTO HTML

    // Temporarily always setting target to web
    // until different project outputs are available in the future
    let target = &Target::Web;

    let parser_output = match web_parser::parse(&ast, "", target) {
        Ok(parser_output) => parser_output,
        Err(e) => {
            return Err(e.to_error(PathBuf::from(&module.source_path)));
        }
    };

    // Add the required minimum dom update JS to the start of the parser JS output
    // Any other dom update JS would be dynamically added to the page by the parser only if needed
    js += &format!(
        "{}\n{}",
        generate_dom_update_js(DOMUpdate::InnerHTML),
        parser_output.code_module
    );

    // Add the HTML boilerplate and then add the parser output to the page
    let html = match create_html_boilerplate(&project_config.html_meta, release_build) {
        Ok(module_output) => module_output
            .replace("<!--page-template-->", &parser_output.content_output)
            .replace("wasm-module-name", &module.source_path.to_string_lossy()),
        Err(e) => {
            return Err(e.to_error(PathBuf::from(&module.source_path)));
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
        import_requests,
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

fn get_config_from_ast(ast: &[AstNode], project_config: &mut Config) -> Result<(), Error> {
    for node in ast {
        if let AstNode::Settings(args, ..) = node {
            for arg in args {
                match arg.name.as_str() {
                    "project" => {
                        project_config.project = match &arg.expr {
                            Expr::String(value) => value.to_owned(),
                            _ => {
                                return Err(Error {
                                    msg: "Project name must be a string".to_string(),
                                    start_pos: TokenPosition::default(),
                                    end_pos: TokenPosition::default(),
                                    file_path: PathBuf::from("#config.bs"),
                                    error_type: ErrorType::Type,
                                });
                            }
                        };
                    }

                    "src" => {
                        project_config.src = match &arg.expr {
                            Expr::String(value) => PathBuf::from(value),
                            _ => {
                                return Err(Error {
                                    msg: "Source folder must be a string".to_string(),
                                    start_pos: TokenPosition::default(),
                                    end_pos: TokenPosition::default(),
                                    file_path: PathBuf::from("#config.bs"),
                                    error_type: ErrorType::Type,
                                });
                            }
                        };
                    }

                    "dev" => {
                        project_config.dev_folder = match &arg.expr {
                            Expr::String(value) => PathBuf::from(value),
                            _ => {
                                return Err(Error {
                                    msg: "Dev folder must be a string".to_string(),
                                    start_pos: TokenPosition::default(),
                                    end_pos: TokenPosition::default(),
                                    file_path: PathBuf::from("#config.bs"),
                                    error_type: ErrorType::Type,
                                });
                            }
                        };
                    }

                    "release" => {
                        project_config.release_folder = match &arg.expr {
                            Expr::String(value) => PathBuf::from(value),
                            _ => {
                                return Err(Error {
                                    msg: "Release folder must be a string".to_string(),
                                    start_pos: TokenPosition::default(),
                                    end_pos: TokenPosition::default(),
                                    file_path: PathBuf::from("#config.bs"),
                                    error_type: ErrorType::Type,
                                });
                            }
                        };
                    }

                    "name" => {
                        project_config.name = match &arg.expr {
                            Expr::String(value) => value.to_owned(),
                            _ => {
                                return Err(Error {
                                    msg: "Name must be a string".to_string(),
                                    start_pos: TokenPosition::default(),
                                    end_pos: TokenPosition::default(),
                                    file_path: PathBuf::from("#config.bs"),
                                    error_type: ErrorType::Type,
                                });
                            }
                        };
                    }

                    "version" => {
                        project_config.version = match &arg.expr {
                            Expr::String(value) => value.to_owned(),
                            _ => {
                                return Err(Error {
                                    msg: "Version must be a string".to_string(),
                                    start_pos: TokenPosition::default(),
                                    end_pos: TokenPosition::default(),
                                    file_path: PathBuf::from("#config.bs"),
                                    error_type: ErrorType::Type,
                                });
                            }
                        };
                    }

                    "author" => {
                        project_config.author = match &arg.expr {
                            Expr::String(value) => value.to_owned(),
                            _ => {
                                return Err(Error {
                                    msg: "Author must be a string".to_string(),
                                    start_pos: TokenPosition::default(),
                                    end_pos: TokenPosition::default(),
                                    file_path: PathBuf::from("#config.bs"),
                                    error_type: ErrorType::Type,
                                });
                            }
                        };
                    }

                    "license" => {
                        project_config.license = match &arg.expr {
                            Expr::String(value) => value.to_owned(),
                            _ => {
                                return Err(Error {
                                    msg: "License must be a string".to_string(),
                                    start_pos: TokenPosition::default(),
                                    end_pos: TokenPosition::default(),
                                    file_path: PathBuf::from("#config.bs"),
                                    error_type: ErrorType::Type,
                                });
                            }
                        };
                    }

                    "html_settings" => {
                        return match &arg.expr {
                            Expr::Args(args) => {
                                for arg in args {
                                    match arg.name.as_str() {
                                        "site_title" => {
                                            project_config.html_meta.site_title = match &arg.expr {
                                                Expr::String(value) => value.to_owned(),
                                                _ => {
                                                    return Err(Error {
                                                        msg: "Site title must be a string"
                                                            .to_string(),
                                                        start_pos: TokenPosition::default(),
                                                        end_pos: TokenPosition::default(),
                                                        file_path: PathBuf::from("#config.bs"),
                                                        error_type: ErrorType::Type,
                                                    });
                                                }
                                            };
                                        }

                                        "page_description" => {
                                            project_config.html_meta.page_description = match &arg
                                                .expr
                                            {
                                                Expr::String(value) => value.to_owned(),
                                                _ => {
                                                    return Err(Error {
                                                        msg: "Page description must be a string"
                                                            .to_string(),
                                                        start_pos: TokenPosition::default(),
                                                        end_pos: TokenPosition::default(),
                                                        file_path: PathBuf::from("#config.bs"),
                                                        error_type: ErrorType::Type,
                                                    });
                                                }
                                            };
                                        }

                                        "site_url" => {
                                            project_config.html_meta.site_url = match &arg.expr {
                                                Expr::String(value) => value.to_owned(),
                                                _ => {
                                                    return Err(Error {
                                                        msg: "Site url must be a string"
                                                            .to_string(),
                                                        start_pos: TokenPosition::default(),
                                                        end_pos: TokenPosition::default(),
                                                        file_path: PathBuf::from("#config.bs"),
                                                        error_type: ErrorType::Type,
                                                    });
                                                }
                                            };
                                        }

                                        "page_url" => {
                                            project_config.html_meta.page_url = match &arg.expr {
                                                Expr::String(value) => value.to_owned(),
                                                _ => {
                                                    return Err(Error {
                                                        msg: "Page url must be a string"
                                                            .to_string(),
                                                        start_pos: TokenPosition::default(),
                                                        end_pos: TokenPosition::default(),
                                                        file_path: PathBuf::from("#config.bs"),
                                                        error_type: ErrorType::Type,
                                                    });
                                                }
                                            };
                                        }

                                        "page_og_title" => {
                                            project_config.html_meta.page_og_title = match &arg.expr
                                            {
                                                Expr::String(value) => value.to_owned(),
                                                _ => {
                                                    return Err(Error {
                                                        msg: "Page og title must be a string"
                                                            .to_string(),
                                                        start_pos: TokenPosition::default(),
                                                        end_pos: TokenPosition::default(),
                                                        file_path: PathBuf::from("#config.bs"),
                                                        error_type: ErrorType::Type,
                                                    });
                                                }
                                            };
                                        }

                                        "page_og_description" => {
                                            project_config.html_meta.page_og_description =
                                                match &arg.expr {
                                                    Expr::String(value) => value.to_owned(),
                                                    _ => {
                                                        return Err(Error {
                                                            msg: "Page og description must be a string"
                                                                .to_string(),
                                                            start_pos: TokenPosition::default(),
                                                            end_pos: TokenPosition::default(),
                                                            file_path: PathBuf::from("#config.bs"),
                                                            error_type: ErrorType::Type,
                                                        });
                                                    }
                                                };
                                        }

                                        "page_image_url" => {
                                            project_config.html_meta.page_image_url =
                                                match &arg.expr {
                                                    Expr::String(value) => value.to_owned(),
                                                    _ => {
                                                        return Err(Error {
                                                            msg: "Page image url must be a string"
                                                                .to_string(),
                                                            start_pos: TokenPosition::default(),
                                                            end_pos: TokenPosition::default(),
                                                            file_path: PathBuf::from("#config.bs"),
                                                            error_type: ErrorType::Type,
                                                        });
                                                    }
                                                };
                                        }

                                        "page_image_alt" => {
                                            project_config.html_meta.page_image_alt =
                                                match &arg.expr {
                                                    Expr::String(value) => value.to_owned(),
                                                    _ => {
                                                        return Err(Error {
                                                            msg: "Page image alt must be a string"
                                                                .to_string(),
                                                            start_pos: TokenPosition::default(),
                                                            end_pos: TokenPosition::default(),
                                                            file_path: PathBuf::from("#config.bs"),
                                                            error_type: ErrorType::Type,
                                                        });
                                                    }
                                                };
                                        }

                                        "page_locale" => {
                                            project_config.html_meta.page_locale = match &arg.expr {
                                                Expr::String(value) => value.to_owned(),
                                                _ => {
                                                    return Err(Error {
                                                        msg: "Page locale must be a string"
                                                            .to_string(),
                                                        start_pos: TokenPosition::default(),
                                                        end_pos: TokenPosition::default(),
                                                        file_path: PathBuf::from("#config.bs"),
                                                        error_type: ErrorType::Type,
                                                    });
                                                }
                                            };
                                        }

                                        "page_type" => {
                                            project_config.html_meta.page_type = match &arg.expr {
                                                Expr::String(value) => value.to_owned(),
                                                _ => {
                                                    return Err(Error {
                                                        msg: "Page type must be a string"
                                                            .to_string(),
                                                        start_pos: TokenPosition::default(),
                                                        end_pos: TokenPosition::default(),
                                                        file_path: PathBuf::from("#config.bs"),
                                                        error_type: ErrorType::Type,
                                                    });
                                                }
                                            };
                                        }

                                        "page_twitter_large_image" => {
                                            project_config.html_meta.page_twitter_large_image =
                                                match &arg.expr {
                                                    Expr::String(value) => value.to_owned(),
                                                    _ => return Err(Error {
                                                        msg:
                                                        "Page twitter large image must be a string"
                                                            .to_string(),
                                                        start_pos: TokenPosition::default(),
                                                        end_pos: TokenPosition::default(),
                                                        file_path: PathBuf::from("#config.bs"),
                                                        error_type: ErrorType::Type,
                                                    }),
                                                };
                                        }

                                        "page_canonical_url" => {
                                            project_config.html_meta.page_canonical_url =
                                                match &arg.expr {
                                                    Expr::String(value) => value.to_owned(),
                                                    _ => {
                                                        return Err(Error {
                                                        msg: "Page canonical url must be a string"
                                                            .to_string(),
                                                        start_pos: TokenPosition::default(),
                                                        end_pos: TokenPosition::default(),
                                                        file_path: PathBuf::from("#config.bs"),
                                                        error_type: ErrorType::Type,
                                                    });
                                                    }
                                                };
                                        }

                                        "page_root_url" => {
                                            project_config.html_meta.page_root_url = match &arg.expr
                                            {
                                                Expr::String(value) => value.to_owned(),
                                                _ => {
                                                    return Err(Error {
                                                        msg: "Page root url must be a string"
                                                            .to_string(),
                                                        start_pos: TokenPosition::default(),
                                                        end_pos: TokenPosition::default(),
                                                        file_path: PathBuf::from("#config.bs"),
                                                        error_type: ErrorType::Type,
                                                    });
                                                }
                                            };
                                        }

                                        "image_folder_url" => {
                                            project_config.html_meta.image_folder_url = match &arg
                                                .expr
                                            {
                                                Expr::String(value) => value.to_owned(),
                                                _ => {
                                                    return Err(Error {
                                                        msg: "Image folder url must be a string"
                                                            .to_string(),
                                                        start_pos: TokenPosition::default(),
                                                        end_pos: TokenPosition::default(),
                                                        file_path: PathBuf::from("#config.bs"),
                                                        error_type: ErrorType::Type,
                                                    });
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
                                error_type: ErrorType::Type,
                            }),
                        };
                    }

                    _ => {}
                }
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

fn print_token_output(tokens: &[Token]) {
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

fn print_ast_output(ast: &[AstNode]) {
    for node in ast {
        match node {
            AstNode::Reference(value, _) => match value.get_type(false) {
                DataType::Scene(_) => {
                    print_scene(value, 0);
                }
                _ => {
                    cyan_ln!("{:?}", value);
                }
            },
            AstNode::Comment(..) => {
                // grey_ln!("{:?}", node);
            }
            AstNode::Declaration(name, expr, ..) => {
                blue_ln!("Variable: {:?}", name);
                green_ln_bold!("Expr: {:?}", expr);
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

    fn print_scene(scene: &Expr, scene_nesting_level: u32) {
        // Indent the scene by how nested it is
        let mut indentation = String::new();
        for _ in 0..scene_nesting_level {
            indentation.push('\t');
        }

        if let Expr::Scene(nodes, style, ..) = scene {
            blue_ln_bold!("\n{}Scene Styles: ", indentation);

            green_ln!("{}  {:?}", indentation, style.format);
            green_ln!("{}  {:?}", indentation, style.child_default);
            green_ln!("{}  {:?}", indentation, style.unlocked_scenes);

            blue_ln_bold!("{}Scene Body:", indentation);

            for scene_node in nodes.flatten() {
                println!("{}  {:?}", indentation, scene_node);
            }
        }
    }
}
