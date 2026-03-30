//! Tokenization and header parsing for Stage 0 project config files.
//!
//! WHAT: loads `#config.bst` into token and header form while collecting syntax-level config
//! issues.
//! WHY: config files intentionally reuse the normal frontend pipeline, so parser reuse keeps Stage
//! 0 behavior aligned with the rest of the language.

use crate::build_system::create_project_modules::extract_source_code;
use crate::compiler_frontend::compiler_errors::{
    CompilerError, CompilerMessages, ErrorMetaDataKey, ErrorType,
};
use crate::compiler_frontend::headers::parse_file_headers::{Header, parse_headers};
use crate::compiler_frontend::host_functions::HostRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::tokenizer::lexer::tokenize;
use crate::compiler_frontend::tokenizer::newline_handling::NewlineMode;
use crate::compiler_frontend::tokenizer::tokens::{Token, TokenKind, TokenizeMode};
use std::path::Path;

pub(super) struct ParsedConfigFile {
    pub(super) headers: Vec<Header>,
    pub(super) errors: Vec<CompilerError>,
}

/// Parse the raw config file into frontend headers plus any syntax-level config errors.
///
/// WHY: value validation happens later, but token/header parsing must still surface all structural
/// errors before Stage 0 tries to apply any settings.
pub(super) fn parse_config_file(
    config_path: &Path,
    style_directives: &StyleDirectiveRegistry,
    string_table: &mut StringTable,
) -> Result<ParsedConfigFile, CompilerMessages> {
    let mut errors = Vec::new();

    let source = extract_source_code(config_path, string_table)
        .map_err(|error| CompilerMessages::from_error(error, string_table.clone()))?;
    let interned_path = InternedPath::from_path_buf(config_path, string_table);

    // Tokenization errors are fatal because later parsing stages require a valid token stream.
    let token_stream = match tokenize(
        &source,
        &interned_path,
        TokenizeMode::Normal,
        NewlineMode::default(),
        style_directives,
        string_table,
        None,
    ) {
        Ok(tokens) => tokens,
        Err(error) => {
            errors.push(error);
            return Err(CompilerMessages {
                errors,
                warnings: Vec::new(),
                string_table: string_table.clone(),
            });
        }
    };

    // Collect legacy shorthand config syntax up front so users can fix all declarations together.
    errors.extend(validate_config_hash_assignments(
        &token_stream.tokens,
        string_table,
    ));

    let host_registry = HostRegistry::new();
    let mut warnings = Vec::new();

    // Duplicate-key parse failures need to be reclassified as config errors for Stage 0 callers.
    let parsed_headers = match parse_headers(
        vec![token_stream],
        &host_registry,
        &mut warnings,
        config_path,
        string_table,
    ) {
        Ok(headers) => headers,
        Err(header_errors) => {
            for error in header_errors {
                if is_duplicate_config_header_error(&error) {
                    let mut config_error = error.clone();
                    config_error.error_type = ErrorType::Config;
                    config_error.msg =
                        "Duplicate config key found. Each config key must be unique.".to_string();
                    errors.push(config_error);
                } else {
                    errors.push(error);
                }
            }
            return Err(CompilerMessages {
                errors,
                warnings: Vec::new(),
                string_table: string_table.clone(),
            });
        }
    };

    Ok(ParsedConfigFile {
        headers: parsed_headers.headers,
        errors,
    })
}

fn is_duplicate_config_header_error(error: &CompilerError) -> bool {
    matches!(error.error_type, ErrorType::Rule)
        && matches!(
            error.metadata.get(&ErrorMetaDataKey::CompilationStage),
            Some(stage) if stage == "Header Parsing"
        )
        && matches!(
            error.metadata.get(&ErrorMetaDataKey::ConflictType),
            Some(kind) if kind == "DuplicateTopLevelDeclaration"
        )
}

/// Validate that all config declarations use standard constant syntax (`#key = value`).
///
/// WHY: the old shorthand `#key value` was removed. Collecting all violations at once lets users
/// fix them in a single iteration rather than one error at a time.
fn validate_config_hash_assignments(
    tokens: &[Token],
    string_table: &StringTable,
) -> Vec<CompilerError> {
    let mut errors = Vec::new();
    let mut index = 0usize;

    while index < tokens.len() {
        if !matches!(tokens[index].kind, TokenKind::Hash) {
            index += 1;
            continue;
        }

        index += 1;
        skip_newlines(tokens, &mut index);

        let Some(name_token) = tokens.get(index) else {
            break;
        };
        let TokenKind::Symbol(name_id) = name_token.kind else {
            continue;
        };

        index += 1;
        skip_newlines(tokens, &mut index);

        let Some(next_token) = tokens.get(index) else {
            break;
        };

        // Standard constant syntax: `#name =`, `#name |`, `#name ::`, `#name[...]`.
        if matches!(
            next_token.kind,
            TokenKind::Assign | TokenKind::DoubleColon | TokenKind::TypeParameterBracket
        ) {
            continue;
        }

        // Scalar-like tokens immediately after `#name` indicate the removed shorthand form.
        if matches!(
            next_token.kind,
            TokenKind::StringSliceLiteral(_)
                | TokenKind::RawStringLiteral(_)
                | TokenKind::Symbol(_)
                | TokenKind::IntLiteral(_)
                | TokenKind::FloatLiteral(_)
                | TokenKind::BoolLiteral(_)
                | TokenKind::Path(_)
                | TokenKind::OpenCurly
        ) {
            let name = string_table.resolve(name_id);
            let mut error = CompilerError::new(
                format!(
                    "Invalid config declaration '#{name} ...'. Use standard constant syntax: '#{name} = value'."
                ),
                next_token.location.clone(),
                ErrorType::Config,
            );
            error.metadata.insert(
                ErrorMetaDataKey::PrimarySuggestion,
                format!("Add '=' between '#{name}' and the value"),
            );
            errors.push(error);
        }
    }

    errors
}

fn skip_newlines(tokens: &[Token], index: &mut usize) {
    while let Some(token) = tokens.get(*index) {
        if !matches!(token.kind, TokenKind::Newline) {
            break;
        }
        *index += 1;
    }
}
