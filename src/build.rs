use crate::bs_types::DataType;
use crate::html_output::dom_hooks::{DOMUpdate, generate_dom_update_js};
use crate::html_output::generate_html::create_html_boilerplate;
use crate::html_output::web_parser;
use crate::html_output::web_parser::Target;
use crate::parsers::ast_nodes::{Arg, AstNode, Expr};
use crate::parsers::build_ast::{TokenContext, new_ast};
use crate::settings::{BS_VAR_PREFIX, Config, get_config_from_ast};
use crate::tokenizer::TokenPosition;
use crate::{Error, ErrorType, settings, Flag};
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
// use wasmparser::validate;
use crate::file_output::write_output_file;
use crate::module_dependencies::resolve_module_dependencies;

pub struct TemplateModule {
    pub source_path: PathBuf,
    pub output_path: PathBuf,
    source_code: String,
    pub tokens: TokenContext,
    pub import_requests: Vec<String>,
    pub exports: HashMap<String, Vec<Token>>,
}

pub struct OutputModule {
    tokens: TokenContext,
    imports: HashMap<String, Vec<Token>>,
    pub output_path: PathBuf,
    pub source_path: PathBuf,
    pub html: String,
    pub js: String,
    pub wasm: Module,
}

impl TemplateModule {
    pub fn new(source_code: impl Into<String>, source_path: &Path, output_path: &Path) -> Self {
        TemplateModule {
            source_code: source_code.into(),
            output_path: output_path.to_path_buf(),
            source_path: source_path.to_path_buf(),
            tokens: TokenContext::default(),
            import_requests: Vec::new(),
            exports: HashMap::new(),
        }
    }
}

impl OutputModule {
    pub fn new(
        output_path: PathBuf,
        tokens: TokenContext,
        imports: HashMap<String, Vec<Token>>,
        source_path: PathBuf,
    ) -> Self {
        OutputModule {
            output_path,
            tokens,
            imports,
            source_path,
            html: String::new(),
            js: String::new(),
            wasm: Module::new(),
        }
    }
}

pub fn build(
    entry_path: &Path,
    release_build: bool,
    flags: &[Flag],
) -> Result<Config, Error> {
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

    let mut modules_to_parse: Vec<TemplateModule> = Vec::new();
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
            modules_to_parse.push(TemplateModule::new(code, entry_path, &entry_path.with_extension("html")));
        }

        CompileType::MultiFile(config_source_code) => {
            let config_path = entry_dir.join(settings::CONFIG_FILE_NAME);

            // Parse the config file
            let mut tokenizer_output =
                match tokenizer::tokenize(&config_source_code, settings::CONFIG_FILE_NAME) {
                    Ok(tokens) => tokens,
                    Err(e) => {
                        return Err(e.to_error(config_path));
                    }
                };

            // Anything imported into the config file becomes an import of every module in the project
            global_imports.extend(tokenizer_output.imports);

            let config_block = match new_ast(&mut tokenizer_output.token_context, &[], &[], true) {
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

    // ----------------------------------
    // BUILD REST OF PROJECT AFTER CONFIG
    // ----------------------------------

    // TOKENIZE MODULES
    for module in &mut modules_to_parse {
        // TODO: Yuck, will this always unwrap ok?
        let file_name = module.source_path.file_stem().unwrap().to_str().unwrap();

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

        module.import_requests = global_imports.to_owned();
        module.import_requests.extend(tokenizer_output.imports);

        module
            .exports
            .insert(export_path.to_owned(), tokenizer_output.exports);

        if flags.contains(&Flag::ShowTokens) {
            print_token_output(&tokenizer_output.token_context.tokens);
        }

        module.tokens = tokenizer_output.token_context;

        if !flags.contains(&Flag::DisableTimers) {
            print!("Tokenized in: ");
            green_ln!("{:?}", time.elapsed());
        }
    }

    // RESOLVING MODULE DEPENDENCIES
    let (mut tokenised_modules, project_exports) = resolve_module_dependencies(&modules_to_parse)?;

    // PARSING MODULES INTO AN AST
    // And creating the output files

    for module in &mut tokenised_modules {
        let compile_result = compile_module(
            module,
            release_build,
            flags,
            &mut project_config,
            &project_exports,
        )?;

        let mut js_imports: String = format!(
            "<script type=\"module\" src=\"./{}\"></script>",
            &module
                .output_path
                .with_extension("js")
                .file_name()
                .unwrap()
                .to_string_lossy()
        );

        for import in &mut module.imports {
            // Stripping the src folder from the import path,
            // As this directory is removed in the output directory
            let trimmed_import = import.0.strip_prefix("src/").unwrap_or(import.0);

            js_imports += &format!(
                "<script type=\"module\" src=\"{}.js\"></script>",
                trimmed_import
            );
        }

        module.html = compile_result
            .html
            .replace("<!--//js-modules-->", &js_imports);
        module.js = compile_result.js;

        // Write the file to the output directory
        write_output_file(module)?;
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

                // If the file is not in the source code to parse, it's unnecessary
                && !tokenised_modules.iter().any(|f| f.output_path.with_extension("") == file_path.with_extension(""))
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
    source_code_to_parse: &mut Vec<TemplateModule>,
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

                    let final_output_file = TemplateModule::new(
                        code,
                        &file_path,
                        &PathBuf::from(&output_dir)
                            .join(file_name)
                            .with_extension("html"),
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

                        source_code_to_parse
                            .push(TemplateModule::new("", &file_path, &output_dir.join(file_name)));
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
}

fn compile_module(
    module: &mut OutputModule,
    release_build: bool,
    flags: &[Flag],
    project_config: &mut Config,
    project_exports: &[Arg],
) -> Result<CompileResult, Error> {
    print_bold!("\nCompiling: ");

    let time = Instant::now();
    let mut js = String::new();

    // These are just to insert the HTML module imports into the HTML file.
    let mut import_requests: Vec<&Token> = Vec::new();
    for import in &module.imports {
        // Import the other JS module at the top of this module
        // Import string will be in this format: "path/module:export"
        // Or if we are importing everything: "path/module"

        // Then strip the src/ from the path if it starts there
        // This is because the output folder gets rid of the src directory
        let import_path = import.0.strip_prefix("src/").unwrap_or(import.0);

        // Importing everything from the module
        js += &format!("import {{");
        for import in import.1 {
            import_requests.push(import);
            let import_name = import.get_name();
            js += &format!("{BS_VAR_PREFIX}{import_name}, ");
        }
        js += &format!("}} from \"./{import_path}.js\";\n");
    }

    // AST PARSER
    let block = match new_ast(&mut module.tokens, project_exports, &[], true) {
        Ok(block) => block,
        Err(e) => {
            return Err(e.to_error(PathBuf::from(&module.source_path)));
        }
    };

    let ast = block.get_block_nodes();

    if flags.contains(&Flag::ShowAst) {
        yellow_ln_bold!("CREATING AST\n");
        print_ast_output(ast);
    }

    if !flags.contains(&Flag::DisableTimers) {
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

    let parser_output = match web_parser::parse(ast, "", target) {
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

    if !flags.contains(&Flag::DisableTimers) {
        print!("HTML/CSS/WAT/JS generated in: ");
        green_ln!("{:?}", time.elapsed());
    }

    // WASM GENERATION
    // let all_parsed_wasm = &format!(
    //     "(module {}(func (export \"set_wasm_globals\"){}))",
    //     &parser_output.wat, parser_output.wat_globals
    // );

    Ok(CompileResult { html, js })
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
                green_ln_bold!("Expr: {:#?}", expr);
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
