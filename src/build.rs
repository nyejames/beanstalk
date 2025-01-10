use crate::bs_types::DataType;
use crate::html_output::dom_hooks::{generate_dom_update_js, DOMUpdate};
use crate::html_output::generate_html::create_html_boilerplate;
use crate::html_output::web_parser;
use crate::parsers::ast_nodes::{Arg, AstNode, Value};
use crate::settings::{get_html_config, Config};
use crate::tokenizer;
use crate::tokens::Token;
use crate::{parsers, settings, Error, ErrorType};

use colour::{blue_ln, dark_yellow_ln, green_ln, print_bold, print_ln_bold, red_ln};
use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use wat::parse_str;

pub struct OutputFile {
    source_code: String,
    output_path: PathBuf,
    source_path: PathBuf,
    compiled_code: String,
    wasm: Vec<u8>,
    imports: Vec<PathBuf>,
    global: bool,
}
pub struct ExportedJS {
    pub js: String,

    // Path to the output file exporting the module (for namespacing)
    // Includes the name of what is being exported
    pub path: PathBuf,
    pub global: bool,

    // Function types will contain the arguments and return types
    pub data_type: DataType,
}

pub fn build(
    entry_path: &PathBuf,
    release_build: bool,
    project_config: &mut Config,
) -> Result<(), Error> {
    // Create a new PathBuf from the entry_path
    let entry_dir = match std::env::current_dir() {
        Ok(dir) => dir.join(entry_path),
        Err(e) => {
            return Err(Error {
                msg: format!("Error getting current directory: {:?}", e),
                line_number: 0,
                file_path: entry_path.to_owned(),
                error_type: ErrorType::File,
            });
        }
    };

    // Read content from a test file
    print_ln_bold!("Project Directory: ");
    dark_yellow_ln!("{:?}", &entry_dir);

    let mut source_code_to_parse: Vec<OutputFile> = Vec::new();

    // check to see if there is a config.bs file in this directory
    // if there is, read it and set the config settings
    // and check where the project entry points are
    enum CompileType {
        SingleFile(PathBuf, String), // File Name, Source Code
        MultiFile(PathBuf, String),  // Config file content
    }

    let config = if entry_dir.extension() == Some("bs".as_ref()) {
        let source_code = fs::read_to_string(&entry_dir);
        match source_code {
            Ok(content) => CompileType::SingleFile(entry_dir.with_extension("html"), content),
            Err(e) => {
                return Err(Error {
                    msg: format!("Error reading file: {:?}", e),
                    line_number: 0,
                    file_path: entry_dir.to_owned(),
                    error_type: ErrorType::File,
                });
            }
        }
    } else {
        let config_path = entry_dir.join(settings::CONFIG_FILE_NAME);
        let source_code = fs::read_to_string(&config_path);
        match source_code {
            Ok(content) => CompileType::MultiFile(entry_dir.to_owned(), content),
            Err(_) => {
                return Err(Error {
                    msg: "No config file found in directory".to_string(),
                    line_number: 0,
                    file_path: config_path,
                    error_type: ErrorType::File,
                });
            }
        }
    };

    match config {
        CompileType::SingleFile(file_path, code) => {
            source_code_to_parse.push(OutputFile {
                source_code: code,
                output_path: file_path.with_extension("html"),
                source_path: file_path,
                compiled_code: String::new(),
                wasm: Vec::new(),
                imports: Vec::new(),
                global: false,
            });
        }

        CompileType::MultiFile(entry_dir, _) => {
            let src_dir = entry_dir.join(&project_config.src);
            let output_dir = match release_build {
                true => entry_dir.join(&project_config.release_folder),
                false => entry_dir.join(&project_config.dev_folder),
            };

            add_bs_files_to_parse(
                &mut source_code_to_parse,
                output_dir,
                src_dir.to_owned(),
                release_build,
                &project_config,
            )?;

            project_config.src = src_dir;
        }
    }

    let mut exported_js: Vec<ExportedJS> = Vec::new();
    let mut exported_css = String::new();

    // Compile all output files
    // And collect all exported functions and variables from the module
    // After compiling, collect all imported modules and add them to the list of exported modules
    for file in &mut source_code_to_parse {
        let (compiled_code, wasm, imports) = compile(
            &file,
            release_build,
            &project_config,
            &mut exported_js,
            &mut exported_css,
        )?;

        file.compiled_code = compiled_code;
        file.wasm = wasm;
        file.imports.extend(imports);
    }

    // Add imports and globals to the compiled code of the files
    for file in &mut source_code_to_parse {
        // Add the imports to the files source code importing them after compiling all of them
        let mut imports = exported_js
            .iter()
            .filter(|e| e.global)
            .map(|e| e.js.clone())
            .collect::<String>();
        for import in &file.imports {
            let requested_module = exported_js.iter().find(|e| e.path == *import);
            match requested_module {
                Some(export) => {
                    imports += &export.js;
                }
                None => {
                    red_ln!(
                        "Could not find module to add import to. May not be exported. {:?}",
                        import
                    );
                }
            }
        }
        file.compiled_code = file.compiled_code.replace("//imports", &imports);

        // Write the file to the output directory
        write_output_file(&file)?;
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
                    line_number: 0,
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
                        line_number: 0,
                        file_path: output_dir,
                        error_type: ErrorType::File,
                    });
                }
            };

            let file_path = file.path();
            if file_path.extension() == Some("html".as_ref())
                || file_path.extension() == Some("wasm".as_ref())
            {
                if !source_code_to_parse
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
                                line_number: 0,
                                file_path: file_path.to_owned(),
                                error_type: ErrorType::File,
                            });
                        }
                    }
                }
            }
        }
    }

    Ok(())
}

// Look for every subdirectory inside of dir and add all .bs files to the source_code_to_parse
pub fn add_bs_files_to_parse(
    source_code_to_parse: &mut Vec<OutputFile>,
    output_dir: PathBuf,
    src_dir: PathBuf,
    release_build: bool,
    config: &Config,
) -> Result<(), Error> {
    // Can't just use the src_dir from config, because this might be recursively called for new subdirectories

    // Read all files in the src directory
    let all_dir_entries: fs::ReadDir = match fs::read_dir(&src_dir) {
        Ok(dir) => dir,
        Err(e) => {
            return Err(Error {
                msg: format!("Error reading directory (add bs files to parse): {:?}", e),
                line_number: 0,
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
                                line_number: 0,
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
                                line_number: 0,
                                file_path,
                                error_type: ErrorType::File,
                            });
                        }
                    };

                    let final_file = OutputFile {
                        source_code: code,
                        output_path: PathBuf::from(&output_dir)
                            .join(file_name)
                            .with_extension("html"),
                        source_path: file_path,
                        compiled_code: String::new(),
                        wasm: Vec::new(),
                        imports: Vec::new(),
                        global,
                    };

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
                    add_bs_files_to_parse(
                        source_code_to_parse,
                        new_output_dir,
                        file_path,
                        release_build,
                        config,
                    )?;

                // HANDLE USING JS / HTML / CSS MIXED INTO THE PROJECT
                } else {
                    match file_path.extension() {
                        Some(ext) => {
                            // TEMPORARY: JUST PUT THEM DIRECTLY INTO THE OUTPUT DIRECTORY
                            if ext == "js" || ext == "html" || ext == "css" {
                                let file_name = file_path.file_name().unwrap().to_str().unwrap();
                                source_code_to_parse.push(OutputFile {
                                    source_code: String::new(),
                                    output_path: output_dir.join(file_name),
                                    source_path: file_path,
                                    compiled_code: String::new(),
                                    wasm: Vec::new(),
                                    imports: Vec::new(),
                                    global: false,
                                });
                            }
                        }
                        None => {}
                    }
                }
            }

            Err(e) => {
                return Err(Error {
                    msg: format!("Error reading file while adding bs files to parse: {:?}", e),
                    line_number: 0,
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
    config: &Config,
    exported_js: &mut Vec<ExportedJS>,
    exported_css: &mut String,
) -> Result<(String, Vec<u8>, Vec<PathBuf>), Error> {
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
            line_number: 0,
            file_path: PathBuf::from(""),
            error_type: ErrorType::File,
        });
    }

    dark_yellow_ln!("{:?}", file_name);

    // TODO - exports need to be sorted out
    // They are probably not working atm
    // This is a rough temporary implementation that won't work for circular imports
    // This also uses completely precompiled JS, but will eventually need to be WASM or just the AST
    let mut globals: Vec<Arg> = exported_js
        .iter()
        .filter(|e| e.global)
        .map(|e| Arg {
            name: e
                .path
                .file_name()
                .unwrap_or(OsStr::new(""))
                .to_str()
                .unwrap_or_else(|| "")
                .to_owned(),
            data_type: e.data_type.to_owned(),
            value: Value::None,
        })
        .collect();

    // For letting the user know how long compile times are taking
    let time = Instant::now();

    // TOKENIZER
    let (tokens, token_line_numbers): (Vec<Token>, Vec<u32>) =
        tokenizer::tokenize(&output.source_code, file_name);

    print!("Tokenized in: ");
    green_ln!("{:?}", time.elapsed());
    let time = Instant::now();

    // PARSER
    let (ast, imports) = match parsers::build_ast::new_ast(
        tokens,
        &mut 0,
        &token_line_numbers,
        &mut globals,
        &Vec::new(),
        true,
    ) {
        Ok(ast) => ast,
        Err(e) => {
            return Err(Error {
                msg: e.msg,
                line_number: e.line_number,
                file_path: PathBuf::from(&output.source_path),
                error_type: ErrorType::Syntax,
            });
        }
    };

    print!("AST created in: ");
    green_ln!("{:?}", time.elapsed());
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
                    msg: "Import must be a string literal".to_string(),
                    line_number: 0,
                    file_path: PathBuf::from(&output.source_path),
                    error_type: ErrorType::Syntax,
                });
            }
        }
    }

    // TODO - TO BE REPLACED WITH LOADING CONFIG.BS FILE (When all config tokens are in tokenizer)
    // Should be passed through in the Config struct
    let mut html_config = get_html_config();

    // For each subdirectory from the dist or dev folder of the output_dir, add a ../ to the dist_url
    // This is for linking to CSS/images/other pages etc. in the HTML
    let output_dir_name = if release_build {
        &config.release_folder
    } else {
        &config.dev_folder
    };

    for ancestor in output.output_path.ancestors().skip(1) {
        match ancestor.file_stem() {
            Some(stem) => {
                if *stem == **output_dir_name {
                    break;
                }
            }
            None => {}
        };
        html_config.page_root_url.push_str("../");
    }

    // PARSING INTO HTML
    let parser_output = match web_parser::parse(
        ast,
        &html_config,
        release_build,
        file_name,
        output.global,
        exported_css,
    ) {
        Ok(parser_output) => parser_output,
        Err(e) => {
            return Err(Error {
                msg: e.msg,
                line_number: e.line_number,
                file_path: PathBuf::from(&output.source_path),
                error_type: ErrorType::Syntax,
            });
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
    let module_output = match create_html_boilerplate(&html_config, release_build) {
        Ok(module_output) => module_output
            .replace("page-template", &parser_output.html)
            .replace("@page-css", &parser_output.css)
            .replace("page-title", &parser_output.page_title)
            .replace("//js", &all_js)
            .replace("wasm-module-name", file_name),
        Err(e) => {
            return Err(Error {
                msg: e.msg,
                line_number: e.line_number,
                file_path: PathBuf::from(&output.source_path),
                error_type: ErrorType::Compiler,
            });
        }
    };

    print!("HTML/CSS/WAT/JS generated in: ");
    green_ln!("{:?}", time.elapsed());
    let time = Instant::now();

    // WASM GENERATION
    let all_parsed_wasm = &format!(
        "(module {}(func (export \"set_wasm_globals\"){}))",
        &parser_output.wat, parser_output.wat_globals
    );
    let wasm = match parse_str(all_parsed_wasm) {
        Ok(wasm) => wasm,
        Err(e) => {
            return Err(Error {
                msg: format!("Error parsing wat to wasm: {:?}", e),
                line_number: 0,
                file_path: PathBuf::from(&output.source_path),
                error_type: ErrorType::Compiler,
            })
        }
    };

    print!("WAT parsed to WASM in: ");
    green_ln!("{:?}", time.elapsed());

    exported_js.extend(parser_output.exported_js);
    exported_css.push_str(&parser_output.exported_css);

    Ok((module_output, wasm, import_requests))
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
                line_number: 0,
                file_path: output.output_path.to_owned(),
                error_type: ErrorType::File,
            });
        }
    };

    // Create the needed directory if it doesn't exist
    if !fs::metadata(parent_dir).is_ok() {
        match fs::create_dir_all(parent_dir) {
            Ok(_) => {}
            Err(e) => {
                return Err(Error {
                    msg: format!("Error creating directory: {:?}", e),
                    line_number: 0,
                    file_path: output.output_path.to_owned(),
                    error_type: ErrorType::File,
                });
            }
        }
    }

    match fs::write(&output.output_path, &output.compiled_code) {
        Ok(_) => {}
        Err(e) => {
            return Err(Error {
                msg: format!("Error writing file: {:?}", e),
                line_number: 0,
                file_path,
                error_type: ErrorType::File,
            });
        }
    }

    // Write the wasm file to the same directory
    match fs::write(output.output_path.with_extension("wasm"), &output.wasm) {
        Ok(_) => {}
        Err(e) => {
            return Err(Error {
                msg: format!("Error writing WASM module file: {:?}", e),
                line_number: 0,
                file_path,
                error_type: ErrorType::File,
            });
        }
    }

    Ok(())
}
