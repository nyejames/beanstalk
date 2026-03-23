//! Stage 0 config loading, parsing, and validation for Beanstalk projects.
//!
//! This module owns the full lifecycle of `#config.bst`: locating it, tokenizing it, validating
//! syntax, detecting duplicate keys, handling deprecated keys, and applying parsed values to the
//! `Config` struct. It runs before module discovery and frontend compilation.

use crate::build_system::create_project_modules::extract_source_code;
use crate::compiler_frontend::compiler_errors::{
    CompilerError, CompilerMessages, ErrorMetaDataKey, ErrorType,
};
use crate::compiler_frontend::headers::parse_file_headers::{Header, HeaderKind, parse_headers};
use crate::compiler_frontend::host_functions::HostRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::tokenizer::tokenizer::tokenize;
use crate::compiler_frontend::tokenizer::tokens::{Token, TokenKind, TokenizeMode};
use crate::projects::settings::{self, Config};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Load and validate the project config from `#config.bst` before compilation begins (Stage 0).
///
/// Config files are optional — if none exists, this returns `Ok(())`. When present the file is
/// tokenized, parsed, and all constant declarations are applied to `config`.
pub fn load_project_config(config: &mut Config) -> Result<(), CompilerMessages> {
    let config_path = config.entry_dir.join(settings::CONFIG_FILE_NAME);

    if !config_path.exists() {
        return Ok(()); // Config file is optional
    }

    parse_project_config_file(config, &config_path)
}

/// Parse `#config.bst` and extract top-level constant declarations into the `Config` struct.
///
/// WHY: config follows regular Beanstalk constant syntax; Stage 0 reuses the tokenizer and header
/// parser so config validation stays consistent with the rest of the language.
///
/// Error policy: tokenization and header parsing errors are collected and returned together.
/// Value-level errors (`apply_config_constants_from_headers`) also collect all errors in one pass.
pub(crate) fn parse_project_config_file(
    config: &mut Config,
    config_path: &Path,
) -> Result<(), CompilerMessages> {
    let mut errors = Vec::new();

    let source = extract_source_code(config_path).map_err(CompilerMessages::from_error)?;
    let mut string_table = StringTable::new();
    let style_directives = StyleDirectiveRegistry::built_ins();
    let interned_path = InternedPath::from_path_buf(config_path, &mut string_table);

    // Tokenization errors are fatal — stop immediately so later passes have a clean token stream.
    let token_stream = match tokenize(
        &source,
        &interned_path,
        TokenizeMode::Normal,
        &style_directives,
        &mut string_table,
    ) {
        Ok(tokens) => tokens,
        Err(error) => {
            errors.push(error.with_file_path(config_path.to_path_buf()));
            return Err(CompilerMessages {
                errors,
                warnings: Vec::new(),
            });
        }
    };

    // Collect all legacy-syntax violations before header parsing so users see them together.
    let legacy_errors = validate_config_hash_assignments(&token_stream.tokens, &string_table);
    errors.extend(legacy_errors);

    let host_registry = HostRegistry::new(&mut string_table);
    let mut warnings = Vec::new();

    // WHY: duplicate-key detection runs separately rather than relying solely on parse_headers,
    // because parse_headers may succeed while structural duplicates remain uncaught.
    let parsed_headers = match parse_headers(
        vec![token_stream],
        &host_registry,
        &mut warnings,
        config_path,
        &mut string_table,
    ) {
        Ok(headers) => headers,
        Err(header_errors) => {
            for error in header_errors {
                if error.msg.contains("already a constant") || error.msg.contains("shadow") {
                    // Reclassify Rule errors as Config errors when they occur in the config file.
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
            });
        }
    };

    if let Some(duplicate_errors) =
        detect_duplicate_config_keys(&parsed_headers.headers, &string_table)
    {
        errors.extend(duplicate_errors);
    }

    if let Err(config_errors) = apply_config_constants_from_headers(
        config,
        &parsed_headers.headers,
        &string_table,
        config_path,
    ) {
        errors.extend(config_errors);
    }

    if !errors.is_empty() {
        return Err(CompilerMessages {
            errors,
            warnings: Vec::new(),
        });
    }

    Ok(())
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

        // Standard constant syntax: `#name =`, `#name |`, `#name ::`, `#name[...]`
        if matches!(
            next_token.kind,
            TokenKind::Assign | TokenKind::DoubleColon | TokenKind::TypeParameterBracket
        ) {
            continue;
        }

        // Scalar-like tokens immediately after `#name` indicate the old shorthand `#key value`.
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
                next_token.location.to_error_location(string_table),
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

/// Extract config key-value pairs from parsed headers and apply them to `Config`.
///
/// WHY: location tracking is stored alongside each key so that later validation passes can report
/// precise error locations. All errors are collected in one pass so users can fix them together.
fn apply_config_constants_from_headers(
    config: &mut Config,
    headers: &[Header],
    string_table: &StringTable,
    config_path: &Path,
) -> Result<(), Vec<CompilerError>> {
    let mut errors = Vec::new();

    for header in headers {
        let HeaderKind::Constant { metadata } = &header.kind else {
            continue;
        };

        let Some(key_id) = header.tokens.src_path.name() else {
            errors.push(CompilerError::compiler_error(
                "Config constant header is missing a symbol name.",
            ));
            continue;
        };
        let key = string_table.resolve(key_id).to_string();

        let location = header.name_location.to_error_location(string_table);
        config.setting_locations.insert(key.clone(), location);

        let mut initializer_tokens = metadata.declaration_syntax.initializer_tokens.clone();
        initializer_tokens.push(Token::new(TokenKind::Eof, header.name_location.to_owned()));
        let mut value_index = 0usize;
        skip_newlines(&initializer_tokens, &mut value_index);

        // Deprecated key: '#libraries' was renamed to '#root_folders'.
        if key == "libraries" {
            let mut error = CompilerError::new(
                "Config key '#libraries' has been replaced. Use '#root_folders' instead.",
                header.name_location.to_error_location(string_table),
                ErrorType::Config,
            );
            error.metadata.insert(
                ErrorMetaDataKey::PrimarySuggestion,
                "Rename '#libraries' to '#root_folders' in your config file".to_string(),
            );
            errors.push(error);
            continue;
        }

        // Special handling: '#root_folders' accepts a single path or a '{ ... }' block.
        if key == "root_folders" {
            match parse_root_folders_value(
                &initializer_tokens,
                &mut value_index,
                string_table,
                config_path,
            ) {
                Ok(root_folders) => config.root_folders = root_folders,
                Err(folder_errors) => errors.extend(folder_errors),
            }
            continue;
        }

        let Some(value_token) = initializer_tokens.get(value_index) else {
            let mut error = CompilerError::new(
                format!("Missing value for config constant '#{key}'."),
                header.name_location.to_error_location(string_table),
                ErrorType::Config,
            );
            error.metadata.insert(
                ErrorMetaDataKey::PrimarySuggestion,
                format!("Add a value after '#{key} =' (e.g., '#{key} = \"value\"')"),
            );
            errors.push(error);
            continue;
        };
        let Some(value) = parse_config_scalar_value(&value_token.kind, string_table) else {
            let mut error = CompilerError::new(
                format!("Unsupported value for config constant '#{key}'."),
                value_token.location.to_error_location(string_table),
                ErrorType::Config,
            );
            error.metadata.insert(
                ErrorMetaDataKey::PrimarySuggestion,
                "Config values must be strings, numbers, booleans, or paths".to_string(),
            );
            errors.push(error);
            continue;
        };

        // Deprecated key: '#src' was renamed to '#entry_root'.
        if key == "src" {
            let mut error = CompilerError::new(
                "Config key '#src' is deprecated. Use '#entry_root' instead.",
                header.name_location.to_error_location(string_table),
                ErrorType::Config,
            );
            error.metadata.insert(
                ErrorMetaDataKey::PrimarySuggestion,
                "Rename '#src' to '#entry_root' in your config file".to_string(),
            );
            errors.push(error);
            continue;
        }

        apply_config_entry(config, &key, &value);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Parse the value of a `#root_folders` declaration.
///
/// Accepts either a single path token or a `{ path, path, ... }` block. Validates each path
/// entry and deduplicates the resulting list.
fn parse_root_folders_value(
    tokens: &[Token],
    index: &mut usize,
    string_table: &StringTable,
    config_path: &Path,
) -> Result<Vec<PathBuf>, Vec<CompilerError>> {
    let mut root_folders = Vec::new();
    let mut errors = Vec::new();

    let Some(start_token) = tokens.get(*index) else {
        return Ok(root_folders);
    };

    if matches!(start_token.kind, TokenKind::OpenCurly) {
        *index += 1;
        while let Some(token) = tokens.get(*index) {
            match &token.kind {
                TokenKind::CloseCurly => {
                    *index += 1;
                    break;
                }
                TokenKind::Path(paths) => {
                    for path in paths {
                        match validate_root_folder_path(
                            PathBuf::from(path.to_string(string_table)),
                            token,
                            string_table,
                        ) {
                            Ok(validated_path) => root_folders.push(validated_path),
                            Err(error) => errors.push(error),
                        }
                    }
                }
                TokenKind::StringSliceLiteral(value) | TokenKind::RawStringLiteral(value) => {
                    match validate_root_folder_path(
                        PathBuf::from(string_table.resolve(*value)),
                        token,
                        string_table,
                    ) {
                        Ok(validated_path) => root_folders.push(validated_path),
                        Err(error) => errors.push(error),
                    }
                }
                TokenKind::Symbol(value) => {
                    match validate_root_folder_path(
                        PathBuf::from(string_table.resolve(*value)),
                        token,
                        string_table,
                    ) {
                        Ok(validated_path) => root_folders.push(validated_path),
                        Err(error) => errors.push(error),
                    }
                }
                TokenKind::Comma | TokenKind::Newline => {}
                TokenKind::Eof => {
                    let mut error = CompilerError::new(
                        "Unterminated '#root_folders' block. Missing closing '}'.",
                        token.location.to_error_location(string_table),
                        ErrorType::Config,
                    );
                    error.metadata.insert(
                        ErrorMetaDataKey::PrimarySuggestion,
                        "Add '}' to close the '#root_folders' block".to_string(),
                    );
                    errors.push(error);
                    break;
                }
                _ => {
                    let mut error = CompilerError::new(
                        "Unsupported value in '#root_folders' block.",
                        token.location.to_error_location(string_table),
                        ErrorType::Config,
                    );
                    error.metadata.insert(
                        ErrorMetaDataKey::PrimarySuggestion,
                        "Use folder names like '@lib' or strings like \"@lib\"".to_string(),
                    );
                    errors.push(error);
                }
            }
            *index += 1;
        }
        dedupe_paths(&mut root_folders);

        if !errors.is_empty() {
            return Err(errors);
        }
        return Ok(root_folders);
    }

    match &start_token.kind {
        TokenKind::Path(paths) => {
            for path in paths {
                match validate_root_folder_path(
                    PathBuf::from(path.to_string(string_table)),
                    start_token,
                    string_table,
                ) {
                    Ok(validated_path) => root_folders.push(validated_path),
                    Err(error) => errors.push(error),
                }
            }
        }
        TokenKind::StringSliceLiteral(value) | TokenKind::RawStringLiteral(value) => {
            match validate_root_folder_path(
                PathBuf::from(string_table.resolve(*value)),
                start_token,
                string_table,
            ) {
                Ok(validated_path) => root_folders.push(validated_path),
                Err(error) => errors.push(error),
            }
        }
        TokenKind::Symbol(value) => {
            match validate_root_folder_path(
                PathBuf::from(string_table.resolve(*value)),
                start_token,
                string_table,
            ) {
                Ok(validated_path) => root_folders.push(validated_path),
                Err(error) => errors.push(error),
            }
        }
        _ => {
            let mut error = CompilerError::new(
                "Unsupported '#root_folders' value. Use a path, string, or '{ ... }' block.",
                start_token.location.to_error_location(string_table),
                ErrorType::Config,
            );
            error.metadata.insert(
                ErrorMetaDataKey::PrimarySuggestion,
                "Use '#root_folders = @lib' or '#root_folders = { @lib, @utils }'".to_string(),
            );
            errors.push(error);
        }
    }

    if root_folders.is_empty() && errors.is_empty() {
        errors.push(CompilerError::file_error(
            config_path,
            "Expected at least one root folder in '#root_folders'.",
        ));
    }

    *index += 1;
    dedupe_paths(&mut root_folders);

    if !errors.is_empty() {
        Err(errors)
    } else {
        Ok(root_folders)
    }
}

/// Validate one `#root_folders` entry and normalize it to the stored path form.
///
/// WHY: only single top-level project folders are legal explicit import roots. Nested or absolute
/// paths would undermine the project-relative import model.
fn validate_root_folder_path(
    root_folder: PathBuf,
    token: &Token,
    string_table: &StringTable,
) -> Result<PathBuf, CompilerError> {
    if root_folder.as_os_str().is_empty() {
        let mut error = CompilerError::new(
            "Invalid '#root_folders' entry. Root folders cannot be empty.",
            token.location.to_error_location(string_table),
            ErrorType::Config,
        );
        error.metadata.insert(
            ErrorMetaDataKey::PrimarySuggestion,
            "Provide a folder name like '@lib' or '@utils'".to_string(),
        );
        return Err(error);
    }

    if root_folder.is_absolute() {
        let mut error = CompilerError::new(
            format!(
                "Invalid '#root_folders' entry '{}'. Root folders must be relative to the project root.",
                root_folder.display()
            ),
            token.location.to_error_location(string_table),
            ErrorType::Config,
        );
        error.metadata.insert(
            ErrorMetaDataKey::PrimarySuggestion,
            "Use a relative folder name like '@lib' instead of an absolute path".to_string(),
        );
        return Err(error);
    }

    let mut components = root_folder.components();
    let Some(first) = components.next() else {
        let mut error = CompilerError::new(
            "Invalid '#root_folders' entry. Root folders cannot be empty.",
            token.location.to_error_location(string_table),
            ErrorType::Config,
        );
        error.metadata.insert(
            ErrorMetaDataKey::PrimarySuggestion,
            "Provide a folder name like '@lib' or '@utils'".to_string(),
        );
        return Err(error);
    };

    if !matches!(first, std::path::Component::Normal(_)) || components.next().is_some() {
        let mut error = CompilerError::new(
            format!(
                "Invalid '#root_folders' entry '{}'. Root folders must be a single top-level folder name such as '@lib'.",
                root_folder.display()
            ),
            token.location.to_error_location(string_table),
            ErrorType::Config,
        );
        error.metadata.insert(
            ErrorMetaDataKey::PrimarySuggestion,
            "Use a single folder name like '@lib', not a nested path like '@lib/utils'".to_string(),
        );
        return Err(error);
    }

    Ok(root_folder)
}

fn parse_config_scalar_value(kind: &TokenKind, string_table: &StringTable) -> Option<String> {
    match kind {
        TokenKind::StringSliceLiteral(value)
        | TokenKind::RawStringLiteral(value)
        | TokenKind::Symbol(value) => Some(string_table.resolve(*value).to_string()),
        TokenKind::IntLiteral(value) => Some(value.to_string()),
        TokenKind::FloatLiteral(value) => Some(value.to_string()),
        TokenKind::BoolLiteral(value) => Some(value.to_string()),
        TokenKind::Path(paths) if paths.len() == 1 => Some(paths[0].to_string(string_table)),
        _ => None,
    }
}

fn apply_config_entry(config: &mut Config, key: &str, value: &str) {
    match key {
        "entry_root" => config.entry_root = PathBuf::from(value),
        "output_folder" => config.release_folder = PathBuf::from(value),
        "dev_folder" => config.dev_folder = PathBuf::from(value),
        "project" => {
            config
                .settings
                .insert("project".to_string(), value.to_string());
        }
        "project_name" | "name" => config.project_name = value.to_string(),
        "version" => config.version = value.to_string(),
        "author" => config.author = value.to_string(),
        "license" => config.license = value.to_string(),
        _ => {
            config.settings.insert(key.to_string(), value.to_string());
        }
    }
}

/// Detect duplicate config keys across all parsed headers.
///
/// WHY: `parse_headers` may allow structural duplicates through in some cases; this pass explicitly
/// catches all of them so users see every duplicate at once.
fn detect_duplicate_config_keys(
    headers: &[Header],
    string_table: &StringTable,
) -> Option<Vec<CompilerError>> {
    let mut seen_keys = HashMap::new();
    let mut errors = Vec::new();

    for header in headers {
        let HeaderKind::Constant { .. } = &header.kind else {
            continue;
        };

        let Some(key_id) = header.tokens.src_path.name() else {
            continue;
        };

        let key = string_table.resolve(key_id);

        if let Some(_first_location) = seen_keys.get(key) {
            let mut metadata = HashMap::new();
            metadata.insert(
                ErrorMetaDataKey::PrimarySuggestion,
                String::from("Remove or rename one of the duplicate keys"),
            );

            errors.push(CompilerError {
                msg: format!(
                    "Duplicate config key '#{key}' found. Each config key must be unique."
                ),
                location: header.name_location.to_error_location(string_table),
                error_type: ErrorType::Config,
                metadata,
            });
        } else {
            seen_keys.insert(key.to_string(), header.name_location.clone());
        }
    }

    if errors.is_empty() {
        None
    } else {
        Some(errors)
    }
}

fn skip_newlines(tokens: &[Token], index: &mut usize) {
    while let Some(token) = tokens.get(*index) {
        if !matches!(token.kind, TokenKind::Newline) {
            break;
        }
        *index += 1;
    }
}

fn dedupe_paths(paths: &mut Vec<PathBuf>) {
    let mut seen = HashSet::new();
    paths.retain(|path| seen.insert(path.clone()));
}
