use crate::backends::function_registry::HostRegistry;
use crate::compiler_frontend::ast::ast_nodes::Var;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::{CompilerWarning, WarningKind};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TextLocation, Token, TokenKind};
use std::collections::HashSet;
use std::fmt::Display;
use std::path::Path;

#[derive(Clone, Debug)]
pub enum HeaderKind {
    Function {
        signature: FunctionSignature,
        body: Vec<Token>,
    },

    Struct(Vec<Var>),
    Choice(Vec<Var>), // Tagged unions. Not yet implemented in the language
    Constant(Var),

    ConstTemplate {
        content: Vec<Token>,
        file_order: usize,
    },

    // The top-level scope of regular files.
    // Any other logic in the top level scope implicitly becomes a "start" function.
    // This only runs when explicitly called from an import.
    // Each .bst file can see and use these like normal functions.
    // Start functions have no arguments or return values
    // and are not visible to the host from the final wasm module.
    // The build system will know which start function is the main function based on which file is the entry point of the module.
    StartFunction(Vec<Token>),
}

#[derive(Clone, Debug)]
pub struct Header {
    // The last part of the path is the name of the header
    // It will also (MAYBE) have a special extension to indicate it's a header and not a file or directory
    // Might not bother with this idea tho
    pub full_name: InternedPath,
    pub kind: HeaderKind,
    pub exported: bool,
    // Which headers should be parsed before this one?
    // And what does this header name this import? (last part of the path)
    pub dependencies: HashSet<InternedPath>,
    pub name_location: TextLocation,
}

impl Display for Header {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Header kind: {:#?}", self.kind)
    }
}

pub struct FileImport {
    pub alias: Option<StringId>,
    pub header_path: InternedPath,
}

// This takes all the files in the module
// and parses them into headers, with entry file detection.
pub fn parse_headers(
    tokenized_files: Vec<FileTokens>,
    host_registry: &HostRegistry,
    warnings: &mut Vec<CompilerWarning>,
    entry_file_path: &Path,
    string_table: &mut StringTable,
) -> Result<Vec<Header>, Vec<CompilerError>> {
    let mut headers: Vec<Header> = Vec::new();
    let mut errors: Vec<CompilerError> = Vec::new();

    for mut file in tokenized_files {
        let is_entry_file = file.src_path.to_path_buf(string_table) == entry_file_path;

        //println!("\n\nfile src path: {:?}", file.src_path.to_path_buf(string_table));
        //println!("entry_file_path: {:?}\n\n", entry_file_path);

        let headers_from_file = parse_headers_in_file(
            &mut file,
            host_registry,
            warnings,
            is_entry_file,
            string_table,
        );

        match headers_from_file {
            Ok(file_headers) => {
                headers.extend(file_headers);
            }
            Err(e) => {
                errors.push(e);
            }
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    Ok(headers)
}

// Everything at the top level of a file is visible to the whole module.
// This function splits up the file into each of its headers with entry point detection.
// Each header is a function, struct, choice, constant declaration or part of the implicit main function (anything else in the top level scope).
pub fn parse_headers_in_file(
    token_stream: &mut FileTokens,
    host_function_registry: &HostRegistry,
    warnings: &mut Vec<CompilerWarning>,
    is_entry_file: bool,
    string_table: &mut StringTable,
) -> Result<Vec<Header>, CompilerError> {
    let mut headers = Vec::new();
    let mut encountered_symbols: HashSet<StringId> = HashSet::new();

    // We only need to know IF a header is exported,
    // So later on it can be added to the modules export section
    let mut next_statement_exported = false;
    let mut main_function_body = Vec::new();

    let mut main_function_dependencies: HashSet<InternedPath> = HashSet::new();

    // We parse and track imports as we go,
    // so we can check if the headers depend on those imports.
    let mut file_imports: HashSet<InternedPath> = HashSet::new();

    loop {
        let current_token = token_stream.current_token();
        // ast_log!("Parsing Header Token: {:?}", current_token);
        let current_location = token_stream.current_location();
        token_stream.advance();

        match current_token.kind.to_owned() {
            // New Function, Struct, Choice, or Constant declaration
            TokenKind::Symbol(name_id) => {
                if host_function_registry
                    .get_function(&string_table.resolve(name_id))
                    .is_none()
                {
                    // Reference to an existing symbol
                    if encountered_symbols.contains(&name_id) {
                        // This is a reference, so it goes into the implicit main function
                        main_function_body.push(current_token);

                        // We also store the path in dependencies and check if it's a header in scope already.
                        // Conflicts of naming between variables in the implicit main and other headers must be caught at this stage for the implicit main
                        // Create a path from the current file plus the symbol name
                        main_function_dependencies.insert(token_stream.src_path.append(name_id));

                        if next_statement_exported {
                            next_statement_exported = false;
                            warnings.push(CompilerWarning::new(
                                "You can't export a reference to a variable, only new declarations.",
                                token_stream
                                    .current_location()
                                    .to_error_location(string_table),
                                WarningKind::PointlessExport,
                                token_stream.src_path.to_path_buf(string_table),
                            ))
                        }

                    // New symbol declaration
                    } else {
                        // Every time we encounter a new symbol,
                        // we check if it fits into one of the Header categories.
                        // If not, it goes into the implicit main function.
                        let header = create_header(
                            token_stream.src_path.append(name_id),
                            next_statement_exported,
                            token_stream,
                            current_location,
                            // Since this is a new scope,
                            // We don't want to add any imports from the header's scope to the global imports.
                            // We also don't use encountered_symbols since headers don't capture variables from the surrounding scope
                            &file_imports,
                            host_function_registry,
                            string_table,
                        )?;

                        match header.kind {
                            HeaderKind::StartFunction(..) => {
                                main_function_body.push(current_token);
                                if let Some(path) =
                                    file_imports.iter().find(|f| f.name() == Some(name_id))
                                {
                                    main_function_dependencies.insert(path.to_owned());
                                }
                            }
                            _ => {
                                headers.push(header);
                            }
                        }

                        next_statement_exported = false;
                        encountered_symbols.insert(name_id);
                    };

                // Host function reference
                } else {
                    // This is a reference to a host function, so it goes into the implicit main function
                    // Does not need to be added as a dependency since host functions are globally available
                    main_function_body.push(current_token);
                    if next_statement_exported {
                        next_statement_exported = false;
                        warnings.push(CompilerWarning::new(
                            "You can't export a reference to a host function, only new declarations.",
                            token_stream
                                .current_location()
                                .to_error_location(string_table),
                            WarningKind::PointlessExport,
                            token_stream.src_path.to_path_buf(string_table),
                        ))
                    }
                }
            }

            // @(libraries/math/{round, sqrt})
            TokenKind::Path(paths) => {
                encountered_symbols.extend(paths.iter().map(|p| p.name().unwrap().clone()));
                file_imports.extend(paths);
            }

            TokenKind::Hash => {
                // TODO: Constants
            }

            TokenKind::Eof => {
                main_function_body.push(current_token);
                break;
            }

            _ => {
                // Everything else is shoved into the main function body
                main_function_body.push(current_token);
            }
        }
    }

    // The implicit main function also depends on other headers in this file.
    // So it can use and call any functions or structs defined in this file.
    for header in headers.iter() {
        println!("{:?}", header.full_name);

        main_function_dependencies.insert(header.full_name.to_owned());
    }

    headers.push(Header {
        full_name: token_stream.src_path.to_owned(),
        kind: HeaderKind::StartFunction(main_function_body),
        exported: next_statement_exported,
        dependencies: main_function_dependencies,
        name_location: TextLocation::default(),
    });

    Ok(headers)
}

fn create_header(
    full_name: InternedPath,
    exported: bool,
    token_stream: &mut FileTokens,
    name_location: TextLocation,
    file_imports: &HashSet<InternedPath>,
    _host_registry: &HostRegistry,
    string_table: &mut StringTable,
) -> Result<Header, CompilerError> {
    // We only need to know what imports this header is actually using.
    // So only track symbols matching this file's imports to add to the dependencies.
    let mut dependencies: HashSet<InternedPath> = HashSet::new();
    let mut kind: HeaderKind = HeaderKind::StartFunction(Vec::new());

    // Starts at the first token after the declaration symbol
    let current_token = token_stream.current_token_kind().to_owned();

    match current_token {
        // FUNCTIONS
        TokenKind::TypeParameterBracket => {
            let signature = FunctionSignature::new(token_stream, string_table, &full_name)?;

            let mut scopes_opened = 1;
            let mut scopes_closed = 0;
            let mut function_body = Vec::new();

            // FunctionSignature::new leaves us at the first token of the function body
            // Don't advance before the first iteration
            while scopes_opened > scopes_closed {
                match token_stream.current_token_kind() {
                    TokenKind::End => {
                        scopes_closed += 1;
                        if scopes_opened > scopes_closed {
                            function_body.push(token_stream.tokens[token_stream.index].to_owned());
                        }
                    }

                    // Colons used in templates parse into a different token (EndTemplateHead),
                    // so there isn't any issue with templates creating a colon imbalance.
                    // But all features in the language MUST otherwise follow the rule that all colons are closed with semicolons.
                    // The only violations of this rule have to be parsed differently in the tokenizer,
                    // but it's better from a language design POV for colons to only mean one thing as much as possible anyway.
                    TokenKind::Colon => {
                        scopes_opened += 1;
                        function_body.push(token_stream.tokens[token_stream.index].to_owned());
                    }

                    // Double colons need to be closed with semicolons also
                    TokenKind::DoubleColon => {
                        scopes_opened += 1;
                        function_body.push(token_stream.tokens[token_stream.index].to_owned());
                    }

                    TokenKind::Symbol(name_id) => {
                        if let Some(path) = file_imports.iter().find(|f| f.name() == Some(*name_id))
                        {
                            dependencies.insert(path.to_owned());
                        }
                        function_body.push(token_stream.tokens[token_stream.index].to_owned());
                    }
                    _ => {
                        function_body.push(token_stream.tokens[token_stream.index].to_owned());
                    }
                }

                token_stream.advance();
            }

            kind = HeaderKind::Function {
                signature,
                body: function_body,
            };
        }

        // Could be a struct
        TokenKind::Assign => {
            // Type parameter bracket is a new struct
            if let Some(TokenKind::TypeParameterBracket) = token_stream.peek_next_token() {
                // TODO: Struct headers
                // This needs to skip until the end of the type parameter bracket

                // Struct fields must prefix their name with their parent's name to make sure they are unique
            }

            // Anything else just goes into the start function
        }

        // Should be a choice declaration
        // Choice :: Option1, Option2, Option3;
        TokenKind::DoubleColon => {
            // Make sure to skip the semicolon at the end of the choice declaration
            token_stream.advance();
        }

        // Ignored, going into the start function
        _ => {}
    }

    Ok(Header {
        full_name,
        kind,
        exported,
        dependencies,
        name_location,
    })
}
