use crate::compiler::compiler_errors::{CompileError, CompilerMessages};
use crate::compiler::compiler_warnings::{CompilerWarning, WarningKind};
use crate::compiler::host_functions::registry::HostFunctionRegistry;
use crate::compiler::parsers::ast_nodes::Arg;
use crate::compiler::parsers::statements::functions::FunctionSignature;
use crate::compiler::parsers::tokens::{FileTokens, Token, TokenKind};
use crate::{ast_log, return_rule_error, timer_log};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

// Each header is one of these categories:
// - Functions
// - Structs
// - Choices (not yet implemented)
// - Constants
// - Implicit Main Function:
//      any other logic in the top level scope implicitly becomes a main function.
//      This only runs when explicitly called from the file importing this file,
//      or it will be called at the start of the program if this file is the entry point.
#[derive(Clone, Debug)]
pub enum HeaderKind {
    Function(FunctionSignature, Vec<Token>),
    Struct(Vec<Arg>),
    Choice,
    Constant(Arg),

    // The top-level scope of regular files.
    ImplicitMain(Vec<Token>),

    // The top-level scope of the entry file.
    // This will automatically run when the program starts.
    EntryPoint(Vec<Token>),
}

#[derive(Clone, Debug)]
pub struct Header {
    pub path: PathBuf,
    pub kind: HeaderKind,
    pub exported: bool,
    // Which headers should be parsed before this one?
    // And what does this header name this import?
    pub dependencies: HashMap<PathBuf, Arg>,
}

// This takes all the files in the module
// and parses them into headers.
pub fn parse_headers(
    tokenized_files: Vec<FileTokens>,
    host_registry: &HostFunctionRegistry,
) -> Result<Vec<Header>, CompilerMessages> {
    let mut messages = CompilerMessages::new();
    let mut headers: Vec<Header> = Vec::new();

    for mut file in tokenized_files {
        let headers_from_file =
            parse_headers_in_file(&mut file, host_registry, &mut messages.warnings);

        match headers_from_file {
            Ok(file_headers) => {
                headers.extend(file_headers);
            }
            Err(e) => {
                messages.errors.push(e);
            }
        }
    }

    Ok(headers)
}

// Everything at the top level of a file is visible to the whole module.
// This function splits up the file into each of its headers.
// Each header is a function, struct, choice, constant declaration or part of the implicit main function (anything else in the top level scope).
pub fn parse_headers_in_file(
    token_stream: &mut FileTokens,
    host_function_registry: &HostFunctionRegistry,
    warnings: &mut Vec<CompilerWarning>,
) -> Result<Vec<Header>, CompileError> {
    let mut headers = Vec::new();
    let mut encountered_symbols = HashSet::new();
    let mut next_statement_exported = false;
    let mut main_function_body = Vec::new();

    // We parse and track imports as we go,
    // so we can check if the headers depend on those imports.
    let mut file_imports: HashMap<PathBuf, Arg> = HashMap::new();

    loop {
        ast_log!("Parsing Header Token: {:?}", current_token);
        let current_token = token_stream.current_token_kind().to_owned();
        token_stream.advance();

        match current_token {
            // New Function, Struct, Choice, or Variable declaration
            TokenKind::Symbol(ref name) => {
                // If this is also not a host registry function,
                // Then it's a new symbol and should be parsed as a header
                if host_function_registry.get_function(name).is_none()
                    && encountered_symbols.get(name).is_none()
                {
                    // Every time we encounter a new symbol,
                    // we check if it fits into one of the Header categories.
                    // If not, it goes into the implicit main function.
                    headers.push(create_header(
                        token_stream.src_path.join(name),
                        next_statement_exported,
                        token_stream,
                        host_function_registry,
                        warnings,
                        // Since this is a new scope,
                        // We don't want to add any imports from the header's scope to the global imports.
                        &file_imports,
                    )?);

                    next_statement_exported = false;
                    encountered_symbols.insert(name.to_owned());
                } else {
                    // This is a reference, so it goes into the implicit main function
                    main_function_body.push(token_stream.tokens[token_stream.index].to_owned());
                }
            }

            // TODO:
            // Parse new imports and add them to the file imports
            TokenKind::Import => {}

            TokenKind::Export => {
                if let TokenKind::Symbol(name) = token_stream.current_token_kind() {
                    if encountered_symbols.get(name).is_none() {
                        next_statement_exported = true;
                    } else {
                        warnings.push(CompilerWarning::new(
                            "You can't export a reference to a variable, only new declarations.",
                            token_stream.current_location(),
                            WarningKind::PointlessExport,
                            token_stream.src_path.to_owned(),
                        ))
                    }
                } else {
                    warnings.push(CompilerWarning::new(
                        "Expected variable declaration after an export",
                        token_stream.current_location(),
                        WarningKind::PointlessExport,
                        token_stream.src_path.to_owned(),
                    ))
                }
            }

            TokenKind::Eof => {
                main_function_body.push(token_stream.tokens[token_stream.index].to_owned());
                break;
            }

            _ => {
                // Everything else is shoved into the main function body
                main_function_body.push(token_stream.tokens[token_stream.index].to_owned());
            }
        }
    }

    headers.push(Header {
        path: token_stream.src_path.to_owned(),
        kind: HeaderKind::ImplicitMain(main_function_body),
        exported: next_statement_exported,
        dependencies: file_imports,
    });

    Ok(headers)
}

fn create_header(
    path: PathBuf,
    exported: bool,
    token_stream: &mut FileTokens,
    host_function_registry: &HostFunctionRegistry,
    warnings: &mut Vec<CompilerWarning>,
    file_imports: &HashMap<PathBuf, Arg>,
) -> Result<Header, CompileError> {
    // We only need to know what imports this header is actually using.
    // So only track symbols matching this file's imports to add to the dependencies.
    let mut dependencies = Hash::new();
    let mut kind: HeaderKind;
    let mut imports = file_imports.clone();

    // Starts at the first token after the declaration symbol
    let current_token = token_stream.current_token_kind().to_owned();

    match current_token {
        // -----------------------------
        //      NEW FUNCTION HEADER
        // -----------------------------
        TokenKind::TypeParameterBracket => {
            let mut scopes_opened = 1;
            let mut scopes_closed = 0;

            while scopes_opened > scopes_closed {
                token_stream.advance();
                match token_stream.current_token_kind() {
                    TokenKind::End => scopes_closed += 1,
                    TokenKind::Colon => scopes_opened += 1,
                    TokenKind::Symbol(name) => {
                        if let Some(import) = imports.get(name) {
                            dependencies.insert(import);
                        }
                    }
                    _ => {}
                }
            }

            let signature = FunctionSignature::new(token_stream)?;
            kind = HeaderKind::Function(signature)
        }
    }

    Ok(Header {
        path,
        kind,
        exported,
        dependencies,
    })
}
