use crate::compiler_frontend::ast::statements::declaration_syntax::{
    DeclarationSyntax, parse_declaration_syntax,
};
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::{CompilerWarning, WarningKind};
use crate::compiler_frontend::host_functions::HostRegistry;
use crate::compiler_frontend::identity::FileId;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::paths::parse_import_clause_tokens;
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

struct HeaderParseContext<'a> {
    host_function_registry: &'a HostRegistry,
    warnings: &'a mut Vec<CompilerWarning>,
    is_entry_file: bool,
    string_table: &'a mut StringTable,
    const_template_number: &'a mut usize,
    top_level_template_order: &'a mut usize,
    top_level_template_items: &'a mut Vec<TopLevelTemplateItem>,
}

struct HeaderBuildContext<'a> {
    source_file: &'a InternedPath,
    file_imports: &'a HashSet<InternedPath>,
    file_import_entries: &'a [FileImport],
    file_constant_order: &'a mut usize,
    string_table: &'a mut StringTable,
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

#[allow(dead_code)] // todo
#[derive(Clone, Debug)]
pub enum HeaderKind {
    Function { signature: FunctionSignature },

    Constant { metadata: ConstantHeaderMetadata },
    Struct { metadata: StructHeaderMetadata },
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

#[allow(dead_code)] // todo
#[derive(Clone, Debug)]
pub struct ConstantHeaderMetadata {
    pub declaration_syntax: DeclarationSyntax,
    pub file_constant_order: usize,
    pub import_dependencies: HashSet<InternedPath>,
    pub symbol_dependencies: HashSet<InternedPath>,
}

#[allow(dead_code)] // todo
#[derive(Clone, Debug)]
pub struct StructHeaderMetadata {
    pub default_value_dependencies: HashSet<InternedPath>,
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
    parse_headers_with_path_resolver(
        tokenized_files,
        host_registry,
        warnings,
        entry_file_path,
        None,
        string_table,
    )
}

pub fn parse_headers_with_path_resolver(
    tokenized_files: Vec<FileTokens>,
    host_registry: &HostRegistry,
    warnings: &mut Vec<CompilerWarning>,
    entry_file_path: &Path,
    entry_file_id: Option<FileId>,
    string_table: &mut StringTable,
) -> Result<Headers, Vec<CompilerError>> {
    let mut headers: Vec<Header> = Vec::new();
    let mut errors: Vec<CompilerError> = Vec::new();
    let mut const_template_count = 0;
    let mut top_level_template_items = Vec::new();
    let mut top_level_template_order = 0usize;

    for mut file in tokenized_files {
        let is_entry_file = match (entry_file_id, file.file_id) {
            (Some(expected_id), Some(current_id)) => expected_id == current_id,
            _ => file.src_path.to_path_buf(string_table) == entry_file_path,
        };

        let mut parse_context = HeaderParseContext {
            host_function_registry: host_registry,
            warnings,
            is_entry_file,
            string_table,
            const_template_number: &mut const_template_count,
            top_level_template_order: &mut top_level_template_order,
            top_level_template_items: &mut top_level_template_items,
        };

        let headers_from_file = parse_headers_in_file(&mut file, &mut parse_context);

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
fn parse_headers_in_file(
    token_stream: &mut FileTokens,
    context: &mut HeaderParseContext<'_>,
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
                if context
                    .host_function_registry
                    .get_function(context.string_table.resolve(name_id))
                    .is_none()
                {
                    // Reference to an existing symbol
                    if encountered_symbols.contains(&name_id) {
                        // If there was a hash before this, then error out as this is shadowing a constant
                        if next_statement_exported {
                            return_rule_error!(
                                "There is already a constant, function or struct using this name. You can't shadow these. Choose a unique name",
                                token_stream.current_location().to_error_location(context.string_table), {
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
                        let mut build_context = HeaderBuildContext {
                            source_file: &source_file,
                            file_imports: &file_import_paths,
                            file_import_entries: &file_imports,
                            file_constant_order: &mut file_constant_order,
                            string_table: context.string_table,
                        };
                        let header = create_header(
                            token_stream.src_path.append(name_id),
                            next_statement_exported,
                            token_stream,
                            current_location,
                            &mut build_context,
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
                        context.warnings.push(CompilerWarning::new(
                            "You can't export a reference to a host function, only new declarations.",
                            token_stream
                                .current_location()
                                .to_error_location(context.string_table),
                            WarningKind::PointlessExport,
                            token_stream.src_path.to_path_buf(context.string_table),
                        ))
                    }
                }
            }

            TokenKind::Import => {
                let import_index = token_stream.index.saturating_sub(1);
                let (paths, next_index) = parse_import_clause_tokens(
                    &token_stream.tokens,
                    import_index,
                    context.string_table,
                )?;

                for path in paths {
                    let normalized_path = normalize_import_dependency_path(
                        &path,
                        &token_stream.src_path,
                        context.string_table,
                    )?;

                    if let Some(name) = normalized_path.name() {
                        encountered_symbols.insert(name);
                    }

                    if file_import_paths.insert(normalized_path.to_owned()) {
                        file_imports.push(FileImport {
                            header_path: normalized_path,
                            location: current_location.clone(),
                        });
                    }
                }

                token_stream.index = next_index;
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
                    if !context.is_entry_file {
                        return_rule_error!(
                            "Top-level const templates are currently only supported in the module entry file.",
                            current_location.to_error_location(context.string_table), {
                                CompilationStage => "Header Parsing",
                                PrimarySuggestion => "Move this '#[...]' template to the entry file or remove the export marker",
                            }
                        );
                    }
                    // Top-level const template
                    // An 'exported' top-level template that must be evaluated at compile time
                    let source_file = token_stream.src_path.to_owned();
                    let mut build_context = HeaderBuildContext {
                        source_file: &source_file,
                        file_imports: &file_import_paths,
                        file_import_entries: &file_imports,
                        file_constant_order: &mut file_constant_order,
                        string_table: context.string_table,
                    };
                    let header = create_top_level_const_template(
                        token_stream.src_path.to_owned(),
                        current_token,
                        *context.const_template_number,
                        token_stream,
                        &mut build_context,
                    )?;

                    *context.const_template_number += 1;
                    if context.is_entry_file {
                        context.top_level_template_items.push(TopLevelTemplateItem {
                            file_order: *context.top_level_template_order,
                            location: header.name_location.clone(),
                            kind: TopLevelTemplateKind::ConstTemplate {
                                header_path: header.tokens.src_path.clone(),
                            },
                        });
                        *context.top_level_template_order += 1;
                    }
                    headers.push(header);
                    next_statement_exported = false;
                } else {
                    // Regular top-level templates just go into the start function
                    if context.is_entry_file {
                        context.top_level_template_items.push(TopLevelTemplateItem {
                            file_order: *context.top_level_template_order,
                            location: current_location.clone(),
                            kind: TopLevelTemplateKind::RuntimeTemplate,
                        });
                        *context.top_level_template_order += 1;
                    }
                    push_runtime_template_tokens_to_start_function(
                        current_token,
                        token_stream,
                        &file_import_paths,
                        &mut main_function_dependencies,
                        &mut main_function_body,
                        context.string_table,
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

    let mut start_tokens = FileTokens::new_with_file_id(
        token_stream.src_path.to_owned(),
        token_stream.file_id,
        main_function_body,
    );
    start_tokens.canonical_os_path = token_stream.canonical_os_path.clone();

    headers.push(Header {
        kind: HeaderKind::StartFunction,
        exported: next_statement_exported,
        dependencies: main_function_dependencies,
        name_location: TextLocation::default(),
        tokens: start_tokens,
        source_file: token_stream.src_path.to_owned(),
        file_imports,
    });

    Ok(headers)
}

fn normalize_import_dependency_path(
    import_path: &InternedPath,
    source_file: &InternedPath,
    string_table: &mut StringTable,
) -> Result<InternedPath, CompilerError> {
    let mut import_components = import_path.as_components().iter().copied();
    let Some(first) = import_components.next() else {
        return Ok(import_path.to_owned());
    };

    let first_segment = string_table.resolve(first);
    if first_segment != "." && first_segment != ".." {
        return Ok(import_path.to_owned());
    }

    let mut resolved_components = source_file.as_components().to_vec();
    resolved_components.pop();

    for component in import_path.as_components() {
        match string_table.resolve(*component) {
            "." => {}
            ".." => {
                resolved_components.pop();
            }
            _ => resolved_components.push(*component),
        }
    }

    Ok(InternedPath::from_components(resolved_components))
}

// Split a top-level declaration into a concrete header payload.
fn create_header(
    full_name: InternedPath,
    exported: bool,
    token_stream: &mut FileTokens,
    name_location: TextLocation,
    context: &mut HeaderBuildContext<'_>,
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
            let signature = FunctionSignature::new(token_stream, context.string_table, &full_name)?;

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
                            token_stream.current_location().to_error_location(context.string_table),
                            {
                                PrimarySuggestion => "Close the function body with ';'",
                                SuggestedInsertion => ";",
                            }
                        )
                    }

                    TokenKind::Symbol(name_id) => {
                        if let Some(path) = context
                            .file_imports
                            .iter()
                            .find(|f| f.name() == Some(*name_id))
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
                                token_stream.current_location().to_error_location(context.string_table),
                                {
                                    PrimarySuggestion => "Close the struct fields with a final '|'",
                                    SuggestedInsertion => "|",
                                }
                            )
                        }

                        TokenKind::Symbol(name_id) => {
                            body.push(token_stream.current_token());

                            if let Some(path) = context
                                .file_imports
                                .iter()
                                .find(|f| f.name() == Some(*name_id))
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

                let default_value_dependencies =
                    collect_struct_default_dependencies(&body, context);
                kind = HeaderKind::Struct {
                    metadata: StructHeaderMetadata {
                        default_value_dependencies,
                    },
                };
            } else if exported {
                let constant_header = create_constant_header_payload(
                    &full_name,
                    token_stream,
                    context,
                    &mut dependencies,
                )?;
                body = constant_header.body;
                kind = HeaderKind::Constant {
                    metadata: constant_header.metadata,
                };
            }

            // Anything else just goes into the start function
        }

        // Explicit declaration forms that are not immediate '='.
        // Example: '# page String = ...' and '# item ~ Foo = ...'
        TokenKind::Mutable
        | TokenKind::DatatypeInt
        | TokenKind::DatatypeFloat
        | TokenKind::DatatypeBool
        | TokenKind::DatatypeString
        | TokenKind::OpenCurly
        | TokenKind::Symbol(_) => {
            if exported {
                let constant_header = create_constant_header_payload(
                    &full_name,
                    token_stream,
                    context,
                    &mut dependencies,
                )?;
                body = constant_header.body;
                kind = HeaderKind::Constant {
                    metadata: constant_header.metadata,
                };
            }
        }

        // Should be a choice declaration
        // Choice :: Option1, Option2, Option3;
        TokenKind::DoubleColon => {
            return_rule_error!(
                "Choice declarations are not yet implemented in the language.",
                token_stream.current_location().to_error_location(context.string_table),
                {
                    CompilationStage => "Header Parsing",
                    PrimarySuggestion => "Remove the '::' declaration for now or rewrite this as supported syntax",
                }
            )
        }

        // Ignored, going into the start function
        _ => {}
    }

    let mut header_tokens = FileTokens::new_with_file_id(full_name, token_stream.file_id, body);
    header_tokens.canonical_os_path = token_stream.canonical_os_path.clone();

    Ok(Header {
        kind,
        exported,
        dependencies,
        name_location,
        tokens: header_tokens,
        source_file: context.source_file.to_owned(),
        file_imports: context.file_import_entries.to_vec(),
    })
}

struct ConstantHeaderPayload {
    body: Vec<Token>,
    metadata: ConstantHeaderMetadata,
}

fn create_constant_header_payload(
    full_name: &InternedPath,
    token_stream: &mut FileTokens,
    context: &mut HeaderBuildContext<'_>,
    dependencies: &mut HashSet<InternedPath>,
) -> Result<ConstantHeaderPayload, CompilerError> {
    let Some(declaration_name) = full_name.name() else {
        return Err(CompilerError::compiler_error(
            "Constant header path is missing its declaration name.",
        ));
    };
    let declaration_syntax =
        parse_declaration_syntax(token_stream, declaration_name, context.string_table)?;
    let declaration_tokens = declaration_syntax.to_tokens();

    for token in &declaration_tokens {
        if let TokenKind::Symbol(name_id) = token.kind
            && let Some(path) = context
                .file_imports
                .iter()
                .find(|import| import.name() == Some(name_id))
        {
            dependencies.insert(path.to_owned());
        }
    }

    let import_dependencies = dependencies.clone();
    let symbol_dependencies = collect_constant_symbol_dependencies(&declaration_syntax, context);
    let metadata = ConstantHeaderMetadata {
        declaration_syntax,
        file_constant_order: *context.file_constant_order,
        import_dependencies,
        symbol_dependencies,
    };
    *context.file_constant_order += 1;

    Ok(ConstantHeaderPayload {
        body: declaration_tokens,
        metadata,
    })
}

fn collect_constant_symbol_dependencies(
    declaration_syntax: &DeclarationSyntax,
    context: &HeaderBuildContext<'_>,
) -> HashSet<InternedPath> {
    let mut dependencies = HashSet::new();
    let mut previous_token_was_dot = false;

    if let Some(type_name) = declaration_syntax.explicit_named_type {
        if let Some(import_path) = context
            .file_imports
            .iter()
            .find(|import_path| import_path.name() == Some(type_name))
        {
            dependencies.insert(import_path.to_owned());
        } else {
            dependencies.insert(context.source_file.append(type_name));
        }
    }

    for token in &declaration_syntax.initializer_tokens {
        let token_kind = &token.kind;

        if let TokenKind::Symbol(symbol_id) = token_kind {
            if previous_token_was_dot {
                previous_token_was_dot = false;
                continue;
            }

            if let Some(import_path) = context
                .file_imports
                .iter()
                .find(|import_path| import_path.name() == Some(*symbol_id))
            {
                dependencies.insert(import_path.to_owned());
            } else {
                dependencies.insert(context.source_file.append(*symbol_id));
            }
        }

        previous_token_was_dot = matches!(token_kind, TokenKind::Dot);
    }

    dependencies
}

fn collect_struct_default_dependencies(
    tokens: &[Token],
    context: &HeaderBuildContext<'_>,
) -> HashSet<InternedPath> {
    let mut dependencies = HashSet::new();
    let mut saw_opening_bracket = false;
    let mut inside_default_expression = false;
    let mut paren_depth = 0usize;
    let mut curly_depth = 0usize;
    let mut template_depth = 0usize;
    let mut previous_token_was_dot = false;

    for token in tokens {
        let token_kind = &token.kind;

        if !saw_opening_bracket {
            if matches!(token_kind, TokenKind::TypeParameterBracket) {
                saw_opening_bracket = true;
            }
            previous_token_was_dot = matches!(token_kind, TokenKind::Dot);
            continue;
        }

        if !inside_default_expression {
            if matches!(token_kind, TokenKind::Assign) {
                inside_default_expression = true;
                paren_depth = 0;
                curly_depth = 0;
                template_depth = 0;
            }
            previous_token_was_dot = matches!(token_kind, TokenKind::Dot);
            continue;
        }

        if matches!(
            token_kind,
            TokenKind::Comma | TokenKind::TypeParameterBracket
        ) && paren_depth == 0
            && curly_depth == 0
            && template_depth == 0
        {
            inside_default_expression = false;
            previous_token_was_dot = matches!(token_kind, TokenKind::Dot);
            continue;
        }

        match token_kind {
            TokenKind::OpenParenthesis => {
                paren_depth += 1;
            }
            TokenKind::CloseParenthesis => {
                paren_depth = paren_depth.saturating_sub(1);
            }
            TokenKind::OpenCurly => {
                curly_depth += 1;
            }
            TokenKind::CloseCurly => {
                curly_depth = curly_depth.saturating_sub(1);
            }
            TokenKind::TemplateHead => {
                template_depth += 1;
            }
            TokenKind::TemplateClose => {
                template_depth = template_depth.saturating_sub(1);
            }
            TokenKind::Symbol(symbol_id) => {
                if previous_token_was_dot {
                    previous_token_was_dot = false;
                    continue;
                }

                if let Some(import_path) = context
                    .file_imports
                    .iter()
                    .find(|import_path| import_path.name() == Some(*symbol_id))
                {
                    dependencies.insert(import_path.to_owned());
                } else {
                    dependencies.insert(context.source_file.append(*symbol_id));
                }
            }
            _ => {}
        }

        previous_token_was_dot = matches!(token_kind, TokenKind::Dot);
    }

    dependencies
}

fn create_top_level_const_template(
    scope: InternedPath,
    opening_template_token: Token,
    const_template_number: usize,
    token_stream: &mut FileTokens,
    context: &mut HeaderBuildContext<'_>,
) -> Result<Header, CompilerError> {
    let const_template_name = context.string_table.intern(&format!(
        "{TOP_LEVEL_CONST_TEMPLATE_NAME}{const_template_number}"
    ));
    let mut dependencies: HashSet<InternedPath> = HashSet::new();

    // Keep the full template token stream (including open/close) so AST template parsing
    // can treat const templates exactly like regular templates.
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
                // Preserve the closing token for the outermost template too.
                // Template parsing relies on seeing a close/eof boundary token.
                body.push(token_stream.current_token());
            }

            TokenKind::Eof => {
                return_rule_error!(
                    "Unexpected end of file while parsing top-level const template. Missing ']' to close the template.",
                    token_stream.current_location().to_error_location(context.string_table),
                    {
                        PrimarySuggestion => "Close the template with ']'",
                        SuggestedInsertion => "]",
                    }
                )
            }

            TokenKind::Symbol(name_id) => {
                if let Some(path) = context
                    .file_imports
                    .iter()
                    .find(|f| f.name() == Some(*name_id))
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

    // Add an EOF sentinel so downstream parsers can safely terminate even if
    // expression parsing consumed to the end of this synthetic token stream.
    body.push(Token {
        kind: TokenKind::Eof,
        location: token_stream.current_location(),
    });

    let full_name = scope.append(const_template_name);
    let name_location = TextLocation {
        scope,
        start_pos: start_location.start_pos,
        end_pos: token_stream.current_location().end_pos,
    };

    let mut template_tokens = FileTokens::new_with_file_id(full_name, token_stream.file_id, body);
    template_tokens.canonical_os_path = token_stream.canonical_os_path.clone();

    Ok(Header {
        kind: HeaderKind::ConstTemplate {
            file_order: const_template_number,
        },
        exported: true,
        dependencies,
        name_location,
        tokens: template_tokens,
        source_file: context.source_file.to_owned(),
        file_imports: context.file_import_entries.to_vec(),
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
