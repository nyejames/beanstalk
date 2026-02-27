use crate::backends::function_registry::HostRegistry;
use crate::compiler_frontend::ast::statements::declaration_syntax::{
    DeclarationSyntax, parse_declaration_syntax,
};
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::{CompilerWarning, WarningKind};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TextLocation, Token, TokenKind};
use crate::projects::settings::{
    MINIMUM_LIKELY_DECLARATIONS, TOKEN_TO_DECLARATION_RATIO, TOKEN_TO_HEADER_RATIO,
    TOP_LEVEL_CONST_TEMPLATE_NAME,
};
use crate::{header_log, return_rule_error};
use std::collections::HashSet;
use std::fmt::Display;
use std::path::Path;

pub struct Headers {
    pub headers: Vec<Header>,
    pub top_level_template_items: Vec<TopLevelTemplateItem>,
}

#[derive(Clone, Debug)]
pub struct TopLevelTemplateItem {
    pub file_order: usize,
    pub location: TextLocation,
    pub kind: TopLevelTemplateKind,
}

#[derive(Clone, Debug)]
pub enum TopLevelTemplateKind {
    ConstTemplate { header_path: InternedPath },
    RuntimeTemplate,
}

#[derive(Clone, Debug)]
pub enum HeaderKind {
    Function { signature: FunctionSignature },

    Constant { metadata: ConstantHeaderMetadata },
    Struct,
    Choice, // Tagged unions. Not yet implemented in the language

    ConstTemplate { file_order: usize },

    // The top-level scope of regular files.
    // Any other logic in the top level scope implicitly becomes a "start" function.
    // This only runs when explicitly called from an import.
    // Each .bst file can see and use these like normal functions.
    // Start functions have no arguments or return values
    // and are not visible to the host from the final wasm module.
    // The build system will know which start function is the main function based on which file is the entry point of the module.
    StartFunction,
}

#[derive(Clone, Debug)]
pub struct ConstantHeaderMetadata {
    pub declaration_syntax: DeclarationSyntax,
    pub file_constant_order: usize,
    pub import_dependencies: HashSet<InternedPath>,
}

#[derive(Clone, Debug)]
pub struct Header {
    pub kind: HeaderKind,
    pub exported: bool,
    // Which headers should be parsed before this one?
    // And what does this header name this import? (last part of the path)
    pub dependencies: HashSet<InternedPath>,
    pub name_location: TextLocation,

    // The actual content of the header to be parsed at the AST stage.
    // And the full name / path
    // The last part of the path is the name of the header
    // It will also (MAYBE) have a special extension to indicate it's a header and not a file or directory
    // Might not bother with this idea tho
    pub tokens: FileTokens,
    pub source_file: InternedPath,
    pub file_imports: Vec<FileImport>,
}

impl Display for Header {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Header kind: {:#?}", self.kind)
    }
}

#[derive(Clone, Debug)]
pub struct FileImport {
    pub header_path: InternedPath,
    pub location: TextLocation,
}

// This takes all the files in the module
// and parses them into headers, with entry file detection.
pub fn parse_headers(
    tokenized_files: Vec<FileTokens>,
    host_registry: &HostRegistry,
    warnings: &mut Vec<CompilerWarning>,
    entry_file_path: &Path,
    string_table: &mut StringTable,
) -> Result<Headers, Vec<CompilerError>> {
    let mut headers: Vec<Header> = Vec::new();
    let mut errors: Vec<CompilerError> = Vec::new();
    let mut const_template_count = 0;
    let mut top_level_template_items = Vec::new();
    let mut top_level_template_order = 0usize;

    for mut file in tokenized_files {
        let is_entry_file = file.src_path.to_path_buf(string_table) == entry_file_path;

        let headers_from_file = parse_headers_in_file(
            &mut file,
            host_registry,
            warnings,
            is_entry_file,
            string_table,
            &mut const_template_count,
            &mut top_level_template_order,
            &mut top_level_template_items,
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

    Ok(Headers {
        headers,
        top_level_template_items,
    })
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
    const_template_number: &mut usize,
    top_level_template_order: &mut usize,
    top_level_template_items: &mut Vec<TopLevelTemplateItem>,
) -> Result<Vec<Header>, CompilerError> {
    let mut headers = Vec::with_capacity(token_stream.length / TOKEN_TO_HEADER_RATIO);
    let mut encountered_symbols: HashSet<StringId> = HashSet::with_capacity(
        MINIMUM_LIKELY_DECLARATIONS + (token_stream.tokens.len() / TOKEN_TO_DECLARATION_RATIO),
    );

    // We only need to know IF a header is exported,
    // So later on it can be added to the modules export section
    let mut next_statement_exported = false;
    let mut main_function_body = Vec::new();

    let mut main_function_dependencies: HashSet<InternedPath> = HashSet::new();

    // We parse and track imports as we go,
    // so we can check if the headers depend on those imports.
    let mut file_import_paths: HashSet<InternedPath> = HashSet::new();
    let mut file_imports: Vec<FileImport> = Vec::new();
    let mut file_constant_order = 0usize;

    loop {
        let current_token = token_stream.current_token();
        // ast_log!("Parsing Header Token: {:?}", current_token);
        let current_location = token_stream.current_location();
        token_stream.advance();

        match current_token.kind.to_owned() {
            // New Function, Struct, Choice, or Constant declaration
            TokenKind::Symbol(name_id) => {
                if host_function_registry
                    .get_function(string_table.resolve(name_id))
                    .is_none()
                {
                    // Reference to an existing symbol
                    if encountered_symbols.contains(&name_id) {
                        // If there was a hash before this, then error out as this is shadowing a constant
                        if next_statement_exported {
                            return_rule_error!(
                                "There is already a constant, function or struct using this name. You can't shadow these. Choose a unique name",
                                token_stream.current_location().to_error_location(string_table), {
                                    PrimarySuggestion => "Rename the constant to something unique"
                                }
                            )
                        }

                        // This is a reference, so it goes into the implicit main function
                        main_function_body.push(current_token);

                        // Only imported symbols create inter-header dependency edges here.
                        // Local variables declared in the start function are resolved in AST scope order
                        // and should never be treated as module-level import dependencies.
                        if let Some(path) =
                            file_import_paths.iter().find(|f| f.name() == Some(name_id))
                        {
                            main_function_dependencies.insert(path.to_owned());
                        }

                    // New symbol declaration
                    } else {
                        // Every time we encounter a new symbol,
                        // we check if it fits into one of the Header categories.
                        // If not, it goes into the implicit main function.
                        let source_file = token_stream.src_path.to_owned();
                        let header = create_header(
                            token_stream.src_path.append(name_id),
                            &source_file,
                            next_statement_exported,
                            token_stream,
                            current_location,
                            // Since this is a new scope,
                            // We don't want to add any imports from the header's scope to the global imports.
                            // We also don't use encountered_symbols since headers don't capture variables from the surrounding scope
                            &file_import_paths,
                            &file_imports,
                            &mut file_constant_order,
                            host_function_registry,
                            string_table,
                        )?;

                        match header.kind {
                            HeaderKind::StartFunction => {
                                main_function_body.push(current_token);
                                if let Some(path) =
                                    file_import_paths.iter().find(|f| f.name() == Some(name_id))
                                {
                                    main_function_dependencies.insert(path.to_owned());
                                }
                            }
                            _ => {
                                headers.push(header);
                            }
                        }

                        encountered_symbols.insert(name_id);
                        next_statement_exported = false;
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

            TokenKind::Import => {
                if let TokenKind::Path(paths) = token_stream.current_token_kind() {
                    for path in paths {
                        if let Some(name) = path.name() {
                            encountered_symbols.insert(name);
                        }

                        if file_import_paths.insert(path.to_owned()) {
                            file_imports.push(FileImport {
                                header_path: path.to_owned(),
                                location: token_stream.current_location(),
                            });
                        }
                    }
                    token_stream.advance();
                } else {
                    return_rule_error!(
                        "Expected a path after the 'import' keyword",
                        token_stream.current_location().to_error_location(string_table),
                        {
                            PrimarySuggestion => "Add a path after the 'import' keyword"
                        }
                    )
                }
            }

            TokenKind::Eof => {
                main_function_body.push(current_token);
                break;
            }

            TokenKind::Hash => {
                next_statement_exported = true;
            }

            TokenKind::TemplateHead => {
                if next_statement_exported {
                    if !is_entry_file {
                        return_rule_error!(
                            "Top-level const templates are currently only supported in the module entry file.",
                            current_location.to_error_location(string_table), {
                                CompilationStage => "Header Parsing",
                                PrimarySuggestion => "Move this '#[...]' template to the entry file or remove the export marker",
                            }
                        );
                    }
                    // Top-level const template
                    // An 'exported' top-level template that must be evaluated at compile time
                    let source_file = token_stream.src_path.to_owned();
                    let header = create_top_level_const_template(
                        token_stream.src_path.to_owned(),
                        current_token,
                        *const_template_number,
                        &source_file,
                        &file_import_paths,
                        &file_imports,
                        token_stream,
                        string_table,
                    )?;

                    *const_template_number += 1;
                    if is_entry_file {
                        top_level_template_items.push(TopLevelTemplateItem {
                            file_order: *top_level_template_order,
                            location: header.name_location.clone(),
                            kind: TopLevelTemplateKind::ConstTemplate {
                                header_path: header.tokens.src_path.clone(),
                            },
                        });
                        *top_level_template_order += 1;
                    }
                    headers.push(header);
                    next_statement_exported = false;
                } else {
                    // Regular top-level templates just go into the start function
                    if is_entry_file {
                        top_level_template_items.push(TopLevelTemplateItem {
                            file_order: *top_level_template_order,
                            location: current_location.clone(),
                            kind: TopLevelTemplateKind::RuntimeTemplate,
                        });
                        *top_level_template_order += 1;
                    }
                    push_runtime_template_tokens_to_start_function(
                        current_token,
                        token_stream,
                        &file_import_paths,
                        &mut main_function_dependencies,
                        &mut main_function_body,
                        string_table,
                    )?;
                }
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
        header_log!(#header.tokens.src_path);

        if !matches!(header.kind, HeaderKind::ConstTemplate { .. }) {
            main_function_dependencies.insert(header.tokens.src_path.to_owned());
        }
    }

    headers.push(Header {
        kind: HeaderKind::StartFunction,
        exported: next_statement_exported,
        dependencies: main_function_dependencies,
        name_location: TextLocation::default(),
        tokens: FileTokens::new(token_stream.src_path.to_owned(), main_function_body),
        source_file: token_stream.src_path.to_owned(),
        file_imports,
    });

    Ok(headers)
}

// This should probably be just creating a HeaderKind instead,
// Lots of stuff is just being passed straight through, but who cares tbh
fn create_header(
    full_name: InternedPath,
    source_file: &InternedPath,
    exported: bool,
    token_stream: &mut FileTokens,
    name_location: TextLocation,
    file_imports: &HashSet<InternedPath>,
    file_import_entries: &[FileImport],
    file_constant_order: &mut usize,
    _host_registry: &HostRegistry,
    string_table: &mut StringTable,
) -> Result<Header, CompilerError> {
    // We only need to know what imports this header is actually using.
    // So only track symbols matching this file's imports to add to the dependencies.
    let mut dependencies: HashSet<InternedPath> = HashSet::new();
    let mut kind: HeaderKind = HeaderKind::StartFunction;

    // This 10 comes straight out of my ass
    let mut body = Vec::with_capacity(10);

    // Starts at the first token after the declaration symbol
    let current_token = token_stream.current_token_kind().to_owned();

    match current_token {
        // FUNCTIONS
        TokenKind::TypeParameterBracket => {
            let signature = FunctionSignature::new(token_stream, string_table, &full_name)?;

            let mut scopes_opened = 1;
            let mut scopes_closed = 0;

            // FunctionSignature::new leaves us at the first token of the function body
            // Don't advance before the first iteration
            while scopes_opened > scopes_closed {
                match token_stream.current_token_kind() {
                    TokenKind::End => {
                        scopes_closed += 1;
                        if scopes_opened > scopes_closed {
                            body.push(token_stream.current_token());
                        }
                    }

                    // Colons used in templates parse into a different token (EndTemplateHead),
                    // so there isn't any issue with templates creating a colon imbalance.
                    // But all features in the language MUST otherwise follow the rule that all colons are closed with semicolons.
                    // The only violations of this rule have to be parsed differently in the tokenizer,
                    // but it's better from a language design POV for colons to only mean one thing as much as possible anyway.
                    TokenKind::Colon => {
                        scopes_opened += 1;
                        body.push(token_stream.current_token());
                    }

                    // Double colons need to be closed with semicolons also
                    TokenKind::DoubleColon => {
                        scopes_opened += 1;
                        body.push(token_stream.current_token());
                    }

                    TokenKind::Eof => {
                        return_rule_error!(
                            "Unexpected end of file while parsing function body. Missing ';' to close this scope.",
                            token_stream.current_location().to_error_location(string_table),
                            {
                                PrimarySuggestion => "Close the function body with ';'",
                                SuggestedInsertion => ";",
                            }
                        )
                    }

                    TokenKind::Symbol(name_id) => {
                        if let Some(path) = file_imports.iter().find(|f| f.name() == Some(*name_id))
                        {
                            dependencies.insert(path.to_owned());
                        }
                        body.push(token_stream.current_token());
                    }
                    _ => {
                        body.push(token_stream.current_token());
                    }
                }

                token_stream.advance();
            }

            kind = HeaderKind::Function { signature };
        }

        // Could be a struct
        TokenKind::Assign => {
            // Type parameter bracket is a new struct
            if let Some(TokenKind::TypeParameterBracket) = token_stream.peek_next_token() {
                token_stream.advance();
                let mut seen_opening_bracket = false;

                loop {
                    match token_stream.current_token_kind() {
                        TokenKind::TypeParameterBracket => {
                            body.push(token_stream.current_token());

                            if seen_opening_bracket {
                                token_stream.advance();
                                break;
                            }

                            seen_opening_bracket = true;
                        }

                        TokenKind::Eof => {
                            return_rule_error!(
                                "Unexpected end of file while parsing struct definition. Missing closing '|'.",
                                token_stream.current_location().to_error_location(string_table),
                                {
                                    PrimarySuggestion => "Close the struct fields with a final '|'",
                                    SuggestedInsertion => "|",
                                }
                            )
                        }

                        TokenKind::Symbol(name_id) => {
                            body.push(token_stream.current_token());

                            if let Some(path) =
                                file_imports.iter().find(|f| f.name() == Some(*name_id))
                            {
                                dependencies.insert(path.to_owned());
                            }
                        }

                        _ => {
                            body.push(token_stream.current_token());
                        }
                    }

                    token_stream.advance();
                }

                kind = HeaderKind::Struct;
            } else if exported {
                let declaration_syntax = parse_declaration_syntax(
                    token_stream,
                    full_name.name().unwrap(),
                    string_table,
                )?;

                let declaration_tokens = declaration_syntax.to_tokens();

                for token in &declaration_tokens {
                    if let TokenKind::Symbol(name_id) = token.kind
                        && let Some(path) = file_imports
                            .iter()
                            .find(|import| import.name() == Some(name_id))
                    {
                        dependencies.insert(path.to_owned());
                    }
                }

                let import_dependencies = dependencies.clone();
                let metadata = ConstantHeaderMetadata {
                    declaration_syntax,
                    file_constant_order: *file_constant_order,
                    import_dependencies,
                };
                *file_constant_order += 1;
                body = declaration_tokens;
                kind = HeaderKind::Constant { metadata };
            }

            // Anything else just goes into the start function
        }

        // Should be a choice declaration
        // Choice :: Option1, Option2, Option3;
        TokenKind::DoubleColon => {
            todo!("Choice declarations are not yet implemented in the language");
            // Make sure to skip the semicolon at the end of the choice declaration
        }

        // Ignored, going into the start function
        _ => {}
    }

    Ok(Header {
        kind,
        exported,
        dependencies,
        name_location,
        tokens: FileTokens::new(full_name, body),
        source_file: source_file.to_owned(),
        file_imports: file_import_entries.to_vec(),
    })
}

fn create_top_level_const_template(
    scope: InternedPath,
    opening_template_token: crate::compiler_frontend::tokenizer::tokens::Token,
    const_template_number: usize,
    source_file: &InternedPath,
    file_imports: &HashSet<InternedPath>,
    file_import_entries: &[FileImport],
    token_stream: &mut FileTokens,
    string_table: &mut StringTable,
) -> Result<Header, CompilerError> {
    let const_template_name = string_table.intern(&format!(
        "{TOP_LEVEL_CONST_TEMPLATE_NAME}{const_template_number}"
    ));
    let mut dependencies: HashSet<InternedPath> = HashSet::new();

    // Keep the full token stream (including the template opener) so AST template parsing
    // can treat const templates the same way as other templates.
    let mut body = Vec::with_capacity(10);
    body.push(opening_template_token);

    let start_location = token_stream.current_location();

    let mut scopes_opened = 1;
    let mut scopes_closed = 0;

    // The caller has already consumed the opening token.
    while scopes_opened > scopes_closed {
        match token_stream.current_token_kind() {
            TokenKind::TemplateHead => {
                scopes_opened += 1;
                body.push(token_stream.current_token());
            }

            TokenKind::TemplateClose => {
                scopes_closed += 1;
                if scopes_opened > scopes_closed {
                    body.push(token_stream.current_token());
                }
            }

            TokenKind::Eof => {
                return_rule_error!(
                    "Unexpected end of file while parsing top-level const template. Missing ']' to close the template.",
                    token_stream.current_location().to_error_location(string_table),
                    {
                        PrimarySuggestion => "Close the template with ']'",
                        SuggestedInsertion => "]",
                    }
                )
            }

            TokenKind::Symbol(name_id) => {
                if let Some(path) = file_imports.iter().find(|f| f.name() == Some(*name_id)) {
                    dependencies.insert(path.to_owned());
                }
                body.push(token_stream.current_token());
            }

            _ => {
                body.push(token_stream.current_token());
            }
        }

        token_stream.advance();
    }

    let full_name = scope.append(const_template_name);
    let name_location = TextLocation {
        scope,
        start_pos: start_location.start_pos,
        end_pos: token_stream.current_location().end_pos,
    };

    Ok(Header {
        kind: HeaderKind::ConstTemplate {
            file_order: const_template_number,
        },
        exported: true,
        dependencies,
        name_location,
        tokens: FileTokens::new(full_name, body),
        source_file: source_file.to_owned(),
        file_imports: file_import_entries.to_vec(),
    })
}

fn push_runtime_template_tokens_to_start_function(
    opening_template_token: Token,
    token_stream: &mut FileTokens,
    file_imports: &HashSet<InternedPath>,
    main_function_dependencies: &mut HashSet<InternedPath>,
    main_function_body: &mut Vec<Token>,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    main_function_body.push(opening_template_token);

    let mut scopes_opened = 1usize;
    let mut scopes_closed = 0usize;

    while scopes_opened > scopes_closed {
        match token_stream.current_token_kind() {
            TokenKind::TemplateHead => {
                scopes_opened += 1;
                main_function_body.push(token_stream.current_token());
            }

            TokenKind::TemplateClose => {
                scopes_closed += 1;
                main_function_body.push(token_stream.current_token());
            }

            TokenKind::Eof => {
                return_rule_error!(
                    "Unexpected end of file while parsing top-level runtime template. Missing ']' to close the template.",
                    token_stream.current_location().to_error_location(string_table),
                    {
                        PrimarySuggestion => "Close the template with ']'",
                        SuggestedInsertion => "]",
                    }
                )
            }

            TokenKind::Symbol(name_id) => {
                if let Some(path) = file_imports
                    .iter()
                    .find(|path| path.name() == Some(*name_id))
                {
                    main_function_dependencies.insert(path.to_owned());
                }
                main_function_body.push(token_stream.current_token());
            }

            _ => {
                main_function_body.push(token_stream.current_token());
            }
        }

        token_stream.advance();
    }

    Ok(())
}

#[cfg(test)]
#[path = "tests/parse_file_headers_tests.rs"]
mod parse_file_headers_tests;
