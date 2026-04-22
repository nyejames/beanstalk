//! Config value validation and application helpers for Stage 0 project config loading.
//!
//! WHAT: validates parsed config constants, converts supported value shapes, and applies the
//! results to [`Config`].
//! WHY: separating value semantics from token/header parsing keeps the Stage 0 pipeline easier to
//! audit and extend.

use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorMetaDataKey, ErrorType};
use crate::compiler_frontend::headers::parse_file_headers::{Header, HeaderKind};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{Token, TokenKind};
use crate::projects::settings::Config;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Validate parsed config headers and apply accepted values to the runtime config.
///
/// WHY: this keeps duplicate detection and value semantics in one place after parsing has produced
/// a clean header view of the config file.
pub(super) fn validate_and_apply_config_headers(
    config: &mut Config,
    headers: &[Header],
    string_table: &StringTable,
    config_path: &Path,
) -> Result<(), Vec<CompilerError>> {
    let mut errors = Vec::new();

    if let Some(duplicate_errors) = detect_duplicate_config_keys(headers, string_table) {
        errors.extend(duplicate_errors);
    }

    if let Err(mut config_errors) =
        apply_config_constants_from_headers(config, headers, string_table, config_path)
    {
        errors.append(&mut config_errors);
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

/// Extract config key-value pairs from parsed headers and apply them to `Config`.
///
/// WHY: location tracking is stored alongside each key so later validation/reporting can point at
/// the original config declaration rather than a derived setting record.
fn apply_config_constants_from_headers(
    config: &mut Config,
    headers: &[Header],
    string_table: &StringTable,
    config_path: &Path,
) -> Result<(), Vec<CompilerError>> {
    let mut errors = Vec::new();

    for header in headers {
        let HeaderKind::Constant { declaration } = &header.kind else {
            continue;
        };

        let Some(key_id) = header.tokens.src_path.name() else {
            errors.push(CompilerError::compiler_error(
                "Config constant header is missing a symbol name.",
            ));
            continue;
        };
        let key = string_table.resolve(key_id).to_string();

        let location = header.name_location.clone();
        config.setting_locations.insert(key.clone(), location);

        let mut initializer_tokens = declaration.initializer_tokens.clone();
        initializer_tokens.push(Token::new(TokenKind::Eof, header.name_location.to_owned()));
        let mut value_index = 0usize;
        skip_newlines(&initializer_tokens, &mut value_index);

        // Deprecated key: '#libraries' was renamed to '#root_folders'.
        if key == "libraries" {
            let mut error = CompilerError::new(
                "Config key '#libraries' has been replaced. Use '#root_folders' instead.",
                header.name_location.clone(),
                ErrorType::Config,
            );
            error.metadata.insert(
                ErrorMetaDataKey::PrimarySuggestion,
                "Rename '#libraries' to '#root_folders' in your config file".to_string(),
            );
            errors.push(error);
            continue;
        }

        // Special handling: '#root_folders' accepts a single path or a `{ ... }` block.
        if key == "root_folders" {
            match parse_root_folders_value(
                &initializer_tokens,
                &mut value_index,
                string_table,
                config_path,
            ) {
                Ok(root_folders) => config.root_folders = root_folders,
                Err(mut folder_errors) => errors.append(&mut folder_errors),
            }
            continue;
        }

        let Some(value_token) = initializer_tokens.get(value_index) else {
            let mut error = CompilerError::new(
                format!("Missing value for config constant '#{key}'."),
                header.name_location.clone(),
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
                value_token.location.clone(),
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
                header.name_location.clone(),
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
/// Accepts either a single path token or a `{ path, path, ... }` block. Validates each path entry
/// and deduplicates the resulting list.
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
                    ) {
                        Ok(validated_path) => root_folders.push(validated_path),
                        Err(error) => errors.push(error),
                    }
                }
                TokenKind::Symbol(value) => {
                    match validate_root_folder_path(
                        PathBuf::from(string_table.resolve(*value)),
                        token,
                    ) {
                        Ok(validated_path) => root_folders.push(validated_path),
                        Err(error) => errors.push(error),
                    }
                }
                TokenKind::Comma | TokenKind::Newline => {}
                TokenKind::Eof => {
                    let mut error = CompilerError::new(
                        "Unterminated '#root_folders' block. Missing closing '}'.",
                        token.location.clone(),
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
                        token.location.clone(),
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
            ) {
                Ok(validated_path) => root_folders.push(validated_path),
                Err(error) => errors.push(error),
            }
        }
        TokenKind::Symbol(value) => {
            match validate_root_folder_path(
                PathBuf::from(string_table.resolve(*value)),
                start_token,
            ) {
                Ok(validated_path) => root_folders.push(validated_path),
                Err(error) => errors.push(error),
            }
        }
        _ => {
            let mut error = CompilerError::new(
                "Unsupported '#root_folders' value. Use a path, string, or '{ ... }' block.",
                start_token.location.clone(),
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
        let mut error_string_table = string_table.clone();
        errors.push(CompilerError::file_error(
            config_path,
            "Expected at least one root folder in '#root_folders'.",
            &mut error_string_table,
        ));
    }

    *index += 1;
    dedupe_paths(&mut root_folders);

    if errors.is_empty() {
        Ok(root_folders)
    } else {
        Err(errors)
    }
}

/// Validate one `#root_folders` entry and normalize it to the stored path form.
///
/// WHY: only single top-level project folders are legal explicit import roots. Nested or absolute
/// paths would undermine the project-relative import model.
fn validate_root_folder_path(
    root_folder: PathBuf,
    token: &Token,
) -> Result<PathBuf, CompilerError> {
    if root_folder.as_os_str().is_empty() {
        let mut error = CompilerError::new(
            "Invalid '#root_folders' entry. Root folders cannot be empty.",
            token.location.clone(),
            ErrorType::Config,
        );
        error.metadata.insert(
            ErrorMetaDataKey::PrimarySuggestion,
            "Provide a folder name like '@lib' or '@utils'".to_string(),
        );
        return Err(error);
    }

    if root_folder.is_absolute() || root_folder.as_os_str().to_string_lossy().starts_with('/') {
        let mut error = CompilerError::new(
            format!(
                "Invalid '#root_folders' entry '{}'. Root folders must be relative to the project root.",
                root_folder.display()
            ),
            token.location.clone(),
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
            token.location.clone(),
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
            token.location.clone(),
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
/// WHY: header parsing can still leave structural duplicates behind, so config loading needs one
/// explicit pass that guarantees users see every duplicate key at once.
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

        if seen_keys.contains_key(key) {
            let mut metadata = HashMap::new();
            metadata.insert(
                ErrorMetaDataKey::PrimarySuggestion,
                String::from("Remove or rename one of the duplicate keys"),
            );

            errors.push(CompilerError {
                msg: format!(
                    "Duplicate config key '#{key}' found. Each config key must be unique."
                ),
                location: header.name_location.clone(),
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
