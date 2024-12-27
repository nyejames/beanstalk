use crate::bs_types::DataType;
use crate::html_output::dom_hooks::{generate_dom_update_js, DOMUpdate};
use crate::html_output::generate_html::create_html_boilerplate;
use crate::html_output::web_parser;
use crate::parsers::ast_nodes::{AstNode, Arg, Value};
use crate::settings::{get_default_config, get_html_config, Config};
use crate::{tokenizer, CompileError};
use crate::tokens::Token;
use crate::{parsers, settings};

use colour::{blue_ln, dark_cyan_ln, dark_yellow_ln, green_ln, print_bold, print_ln_bold, red_ln};
use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use wat::parse_str;

pub struct OutputFile {
    source_code: String,
    file: PathBuf,
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

pub fn build(entry_path: &PathBuf, release_build: bool) -> Result<(), CompileError> {
    // Change default output directory to dev if release_build is true
    let project_config = get_default_config();
    let output_dir_folder = if release_build {
        PathBuf::from(&project_config.release_folder)
    } else {
        PathBuf::from(&project_config.dev_folder)
    };

    // Create a new PathBuf from the entry_path
    let entry_dir = if entry_path.to_str().is_none() {
        match std::env::current_dir() {
            Ok(dir) => dir,
            Err(e) => {
                return Err(CompileError {
                    msg: format!("Error getting current directory: {:?}", e),
                    line_number: 0,
                });
            }
        }
    } else {
        // turn whitespace in file name to dashes
        match entry_path.to_str() {
            Some(path) => PathBuf::from(path.replace(|c: char| c.is_whitespace(), "-")),
            None => {
                return Err(CompileError {
                    msg: "Error getting entry path as a string".to_string(),
                    line_number: 0,
                });
            }
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
        Error(String),
    }

    let config = if entry_dir.extension() == Some("bs".as_ref()) {
        let source_code = fs::read_to_string(&entry_dir);
        match source_code {
            Ok(content) => CompileType::SingleFile(entry_dir.with_extension("html"), content),
            Err(_) => CompileType::Error("No file found".to_string()),
        }
    } else {
        let source_code = fs::read_to_string(entry_dir.join(settings::CONFIG_FILE_NAME));
        match source_code {
            Ok(content) => CompileType::MultiFile(entry_dir.clone(), content),
            Err(_) => CompileType::Error("No config file found in directory".to_string()),
        }
    };

    match config {
        CompileType::SingleFile(file_path, code) => {
            source_code_to_parse.push(OutputFile {
                source_code: code,
                file: file_path,
                compiled_code: String::new(),
                wasm: Vec::new(),
                imports: Vec::new(),
                global: false,
            });
        }

        CompileType::Error(e) => {
            return Err(CompileError {
                msg: e,
                line_number: 0,
            });
        }

        CompileType::MultiFile(entry_dir, _source_code) => {
            dark_cyan_ln!("Reading Config File ...");
            // Get config settings from config file
            // let project_config = get_config_data(&source_code)?;
            // Just get the default config for now until can parse the config settings
            let src_dir: fs::ReadDir = match fs::read_dir(entry_dir.join(&project_config.src)) {
                Ok(dir) => dir,
                Err(e) => {
                    return Err(CompileError {
                        msg: format!("Error reading directory: {:?}", e),
                        line_number: 0,
                    });
                }
            };
            match add_bs_files_to_parse(
                &mut source_code_to_parse,
                src_dir,
                entry_dir.join(&output_dir_folder),
            ) {
                Ok(_) => {}
                Err(e) => {
                    return Err(e);
                }
            }
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
                        "Error: Could not find module to add import to. May not be exported. {:?}",
                        import
                    );
                }
            }
        }
        file.compiled_code = file.compiled_code.replace("//imports", &imports);

        // Write the file to the output directory
        write_output_file(&file)?;
    }

    // Any HTML files in the output dir not on the list of files to compile should be deleted if this is a release build
    if release_build && entry_dir.is_dir() {
        let output_dir = PathBuf::from(&entry_dir).join(&project_config.release_folder);
        let dir_files = match fs::read_dir(&output_dir) {
            Ok(dir) => dir,
            Err(e) => {
                return Err(CompileError {
                    msg: format!("Error reading output_dir directory: {:?}. {:?}", &output_dir, e),
                    line_number: 0,
                });
            }
        };

        for file in dir_files {
            let file = match file {
                Ok(f) => f,
                Err(e) => {
                    red_ln!("Error reading file when building project in release mode: {:?}", e);
                    continue;
                }
            };
            let file_path = file.path();
            if file_path.extension() == Some("html".as_ref()) {
                if !source_code_to_parse
                    .iter()
                    .any(|f| f.file.file_stem() == file_path.file_stem())
                {
                    match fs::remove_file(&file_path) {
                        Ok(_) => {
                            blue_ln!("Deleted unused file: {:?}", file_path);
                        }
                        Err(e) => {
                            red_ln!("Error deleting file: {:?}", e);
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
    dir: fs::ReadDir,
    output_file_dir: PathBuf,
) -> Result<(), CompileError> {
    for file in dir {
        match file {
            Ok(f) => {
                let file_path = &f.path();
                if file_path.extension() == Some("bs".as_ref()) {
                    let code = match fs::read_to_string(file_path) {
                        Ok(content) => content,
                        Err(e) => {
                            red_ln!("Error reading file while adding bs files to parse: {:?}", e);
                            continue;
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
                            red_ln!("Error getting file stem");
                            continue;
                        }
                    };

                    let final_file = OutputFile {
                        source_code: code,
                        file: output_file_dir.join(file_name).with_extension("html"),
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
                    let new_dir = match fs::read_dir(file_path) {
                        Ok(new_path) => new_path,
                        Err(e) => {
                            red_ln!(
                                "Error reading directory while adding bs files to parse: {:?}",
                                e
                            );
                            continue;
                        }
                    };

                    let new_output_dir = output_file_dir.join(file_path.file_stem().unwrap());
                    add_bs_files_to_parse(source_code_to_parse, new_dir, new_output_dir)?;

                // HANDLE USING JS / HTML / CSS MIXED INTO THE PROJECT
                } else {
                    match file_path.extension() {
                        Some(ext) => {
                            // TEMPORARY: JUST PUT THEM DIRECTLY INTO THE OUTPUT DIRECTORY
                            if ext == "js" || ext == "html" || ext == "css" {
                                let file_name = file_path.file_name().unwrap().to_str().unwrap();
                                source_code_to_parse.push(OutputFile {
                                    source_code: String::new(),
                                    file: output_file_dir.join(file_name),
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
                red_ln!("Error reading file while adding bs files to parse: {:?}", e);
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
) -> Result<(String, Vec<u8>, Vec<PathBuf>), CompileError> {
    print_bold!("\nCompiling: ");

    let file_name = output
        .file
        .file_stem()
        .unwrap_or(OsStr::new(""))
        .to_str()
        .unwrap_or("");

    if file_name.is_empty() {
        red_ln!("Error: File name is empty");
        return Err(CompileError {
            msg: "Error: File name is empty".to_string(),
            line_number: 0,
        });
    }

    dark_yellow_ln!("{:?}", file_name);

    // TODO - exports need to be sorted out
    let mut globals: Vec<Arg> = exported_js
        .iter()
        .filter(|e| e.global)
        .map(|e| Arg {
            name: e.path.file_name().unwrap_or(OsStr::new("")).to_str().unwrap_or_else(|| "").to_owned(),
            data_type: e.data_type.to_owned(),
            value: Value::None,
        })
        .collect();

    let time = Instant::now();

    let (tokens, token_line_numbers): (Vec<Token>, Vec<u32>) =
        tokenizer::tokenize(&output.source_code, file_name);

    print!("Tokenized in: ");
    green_ln!("{:?}", time.elapsed());
    let time = Instant::now();

    let (ast, imports) = parsers::build_ast::new_ast(
        tokens,
        &mut 0,
        &token_line_numbers,
        &mut globals,
        &Vec::new(),
        true,
    )?;

    print!("AST created in: ");
    green_ln!("{:?}", time.elapsed());
    let time = Instant::now();

    // find the imports
    let mut import_requests = Vec::new();
    for import in imports {
        match import {
            AstNode::Use(module_path, _) => {
                import_requests.push(module_path);
            }
            _ => {
                red_ln!("Error: Import must be a string literal");
            }
        }
    }
    // TO BE REPLACED WITH LOADING CONFIG.BS FILE (When all config tokens are in tokenizer)
    let mut html_config = get_html_config();

    // For each subdirectory from the dist or dev folder of the output_dir, add a ../ to the dist_url
    let output_dir_name = if release_build {
        &config.release_folder
    } else {
        &config.dev_folder
    };

    for ancestor in output.file.ancestors().skip(1) {
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

    let parser_output = web_parser::parse(
        ast,
        &html_config,
        release_build,
        file_name,
        output.global,
        exported_css,
    )?;

    // Add HTML boilerplate
    let all_js = format!(
        "{}\n{}",
        generate_dom_update_js(DOMUpdate::InnerHTML),
        parser_output.js
    );
    
    let module_output = create_html_boilerplate(&html_config, release_build)?
        .replace("page-template", &parser_output.html)
        .replace("@page-css", &parser_output.css)
        .replace("page-title", &parser_output.page_title)
        .replace("//js", &all_js)
        .replace("wasm-module-name", file_name);

    print!("HTML/CSS/WAT/JS generated in: ");
    green_ln!("{:?}", time.elapsed());
    let time = Instant::now();

    let all_parsed_wasm = &format!(
        "(module {}(func (export \"set_wasm_globals\"){}))",
        &parser_output.wat, parser_output.wat_globals
    );
    let wasm = match parse_str(all_parsed_wasm) {
        Ok(wasm) => wasm,
        Err(e) => return Err(CompileError {
            msg: format!("Error parsing wat to wasm: {:?}", e),
            line_number: 0,
        }),
    };

    print!("WAT parsed to WASM in: ");
    green_ln!("{:?}", time.elapsed());

    exported_js.extend(parser_output.exported_js);
    exported_css.push_str(&parser_output.exported_css);

    Ok((module_output, wasm, import_requests))
}

fn write_output_file(output: &OutputFile) -> Result<(), CompileError> {
    // If the output directory does not exist, create it
    let parent_dir = match output.file.parent() {
        Some(dir) => dir,
        None => {
            red_ln!(
                "Error getting parent directory of output file when writing: {:?}",
                output.file
            );
            return Err(CompileError {
                msg: format!("Error getting parent directory of output file when writing: {:?}", output.file),
                line_number: 0,
            });
        }
    };

    if !fs::metadata(parent_dir).is_ok() {
        match fs::create_dir_all(parent_dir) {
            Ok(_) => {}
            Err(e) => {
                red_ln!("Error creating directory: {:?}", e);
            }
        }
    }

    match fs::write(&output.file, &output.compiled_code) {
        Ok(_) => {}
        Err(e) => {
            return Err(CompileError {
                msg: format!("Error writing file: {:?}", e),
                line_number: 0,
            });
        }
    }

    // Write the wasm file
    match fs::write(output.file.with_extension("wasm"), &output.wasm) {
        Ok(_) => {}
        Err(e) => {
            return Err(CompileError {
                msg: format!("Error writing WASM module file: {:?}", e),
                line_number: 0,
            });
        }
    }

    Ok(())
}

// fn get_config_data(config_source_code: &str) -> Result<Config, Box<dyn Error>> {
//     let config_ast = compile(
//         OutputFile {
//             source_code: config_source_code.to_string(),
//             output_dir: String::new(),
//             file_name: String::new(),
//         },
//         false,
//     );
//     let config = get_default_config();

//     match config_ast {
//         Ok(ast) => {
//             for node in ast {
//                 match node {
//                     AstNode::Error(e) => {
//                         return Err(e.into());
//                     }
//                     _ => {}
//                 }
//             }
//         }
//         Err(e) => {
//             return Err(e.into());
//         }
//     }

//     Ok(config)
// }
