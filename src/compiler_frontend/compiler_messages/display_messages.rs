use crate::backends::error_types::BackendErrorType;
use crate::compiler_frontend::basic_utility_functions::normalize_path;
use crate::compiler_frontend::compiler_errors::{
    CompilerError, CompilerMessages, ErrorMetaDataKey, ErrorType,
};
use crate::compiler_frontend::compiler_warnings::{CompilerWarning, WarningKind};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use saying::say;
use std::path::{Path, PathBuf};
use std::{env, fs};

pub(crate) fn relative_display_path_from_root(scope: &Path, root: &Path) -> String {
    let normalized_scope = normalize_path(scope);
    let normalized_root = normalize_path(root);

    normalized_scope
        .strip_prefix(&normalized_root)
        .unwrap_or(&normalized_scope)
        .to_string_lossy()
        .to_string()
}

pub(crate) fn resolved_display_path(scope: &InternedPath, string_table: &StringTable) -> String {
    let source_file = resolve_source_file_path(scope, string_table);

    match env::current_dir() {
        Ok(dir) => relative_display_path_from_root(&source_file, &dir),
        Err(err) => {
            say!(Red
                "Compiler failed to determine the current directory for diagnostic display. ",
                err
            );
            source_file.to_string_lossy().to_string()
        }
    }
}

pub(crate) fn resolve_source_file_path(
    scope: &InternedPath,
    string_table: &StringTable,
) -> PathBuf {
    let mut source_file = normalize_path(&scope.to_path_buf(string_table));

    // Header diagnostics use a synthetic "file.bst/header_name.header" scope so the terminal and
    // dev-server error pages both need to strip that suffix back to the original source file.
    if source_file
        .file_name()
        .and_then(|file_name| file_name.to_str())
        .is_some_and(|file_name| file_name.ends_with(".header"))
    {
        source_file = match source_file.parent() {
            Some(parent) => parent.to_path_buf(),
            None => source_file,
        };
    }

    match fs::canonicalize(&source_file) {
        Ok(canonical_path) => normalize_path(&canonical_path),
        Err(_) => source_file,
    }
}

pub(crate) fn format_error_guidance_lines(error: &CompilerError) -> Vec<String> {
    let mut lines = Vec::new();

    if let Some(stage) = error.metadata.get(&ErrorMetaDataKey::CompilationStage)
        && error.error_type == ErrorType::Compiler
    {
        lines.push(format!("Stage: {stage}"));
    }

    if let Some(suggestion) = error.metadata.get(&ErrorMetaDataKey::PrimarySuggestion) {
        lines.push(suggestion.to_owned());
    }

    if let Some(alternative) = error.metadata.get(&ErrorMetaDataKey::AlternativeSuggestion) {
        lines.push(format!("Alternative: {alternative}"));
    }

    if let Some(replacement) = error.metadata.get(&ErrorMetaDataKey::SuggestedReplacement) {
        lines.push(format!("Suggested replacement: {replacement}"));
    }

    match (
        error.metadata.get(&ErrorMetaDataKey::SuggestedInsertion),
        error.metadata.get(&ErrorMetaDataKey::SuggestedLocation),
    ) {
        (Some(insertion), Some(location)) => {
            lines.push(format!("Suggested insertion: '{insertion}' {location}"))
        }
        (Some(insertion), None) => lines.push(format!("Suggested insertion: '{insertion}'")),
        (None, Some(location)) => lines.push(format!("Suggested location: {location}")),
        (None, None) => {}
    }

    lines
}

pub fn print_compiler_messages(messages: CompilerMessages) {
    let CompilerMessages {
        errors,
        warnings,
        string_table,
    } = messages;

    // Format and print out the messages:
    for err in errors {
        print_formatted_error(err, &string_table);
    }

    for warning in warnings {
        print_formatted_warning(warning, &string_table);
    }
}

pub fn print_terse_compiler_messages(messages: &CompilerMessages) {
    for line in format_terse_compiler_messages(messages) {
        println!("{line}");
    }
}

pub fn format_terse_compiler_messages(messages: &CompilerMessages) -> Vec<String> {
    let mut lines = Vec::with_capacity(messages.errors.len() + messages.warnings.len());

    for error in &messages.errors {
        lines.push(format_terse_error_line(error, &messages.string_table));
    }

    for warning in &messages.warnings {
        lines.push(format_terse_warning_line(warning, &messages.string_table));
    }

    lines
}

pub fn print_formatted_warning(warning: CompilerWarning, string_table: &StringTable) {
    say!(Yellow Bold "WARNING: ");
    println!(
        "File: {}",
        resolved_display_path(&warning.location.scope, string_table)
    );

    match warning.warning_kind {
        WarningKind::UnusedVariable => println!("Unused variable '{}'", warning.msg),
        WarningKind::UnusedFunction => println!("Unused function '{}'", warning.msg),
        WarningKind::UnusedImport => println!("Unused import '{}'", warning.msg),
        WarningKind::UnusedType => println!("Unused type '{}'", warning.msg),
        WarningKind::UnusedConstant => println!("Unused constant '{}'", warning.msg),
        WarningKind::UnusedFunctionArgument => {
            println!("Unused function argument '{}'", warning.msg)
        }
        WarningKind::UnusedFunctionReturnValue => {
            println!("Unused function return value '{}'", warning.msg)
        }
        WarningKind::UnusedFunctionParameter => {
            println!("Unused function parameter '{}'", warning.msg)
        }
        WarningKind::UnusedFunctionParameterDefaultValue => {
            println!("Unused function parameter default value '{}'", warning.msg)
        }
        WarningKind::PointlessExport => println!("Pointless export '{}'", warning.msg),
        WarningKind::MalformedCssTemplate => println!("Malformed CSS template: {}", warning.msg),
        WarningKind::MalformedHtmlTemplate => {
            println!("Malformed HTML template: {}", warning.msg)
        }
        WarningKind::BstFilePathInTemplateOutput => println!(
            "Path to Beanstalk source file is being inserted into template output: {}",
            warning.msg
        ),
        WarningKind::LargeTrackedAsset => println!("{}", warning.msg),
        WarningKind::IdentifierNamingConvention => {
            println!("Identifier naming convention: {}", warning.msg)
        }
        WarningKind::ImportAliasCaseMismatch => {
            println!("Import alias case mismatch: {}", warning.msg)
        }
    }
}

pub fn print_formatted_error(e: CompilerError, string_table: &StringTable) {
    // Resolve synthetic header scopes back to source files before choosing a human-readable path.
    let relative_dir = resolved_display_path(&e.location.scope, string_table);
    let display_line = display_line_number(e.location.start_pos.line_number);
    let display_column = display_column_number(e.location.start_pos.char_column);

    // Read the file and get the actual line as a string from the code
    // Strip the actual header at the end of the path (.header extension)
    let actual_file = resolve_source_file_path(&e.location.scope, string_table);

    let source_line_index = e.location.start_pos.line_number.max(0) as usize;
    let line = match fs::read_to_string(&actual_file) {
        Ok(file) => file
            .lines()
            .nth(source_line_index)
            .unwrap_or_default()
            .to_string(),
        Err(_) => String::new(),
    };

    say!(
        "\n",
        Bright Bold Red error_display_name(&e.error_type),
        Reset " ",
        Reset error_visual(&e.error_type),
    );

    say!(Reset e.msg.as_str());

    if !relative_dir.is_empty() {
        say!(
            Blue "\n  --> ",
            Reset Magenta relative_dir.as_str(),
            Dark Magenta ":",
            Reset Bold Blue display_line,
            Reset Grey ":",
            Reset Magenta display_column
        );
    } else {
        say!(
            Blue "\n   --> ",
            Reset Magenta display_line,
            Dark Magenta ":",
            Reset Magenta display_column
        );
    }

    if !line.is_empty() {
        say!(Blue "    |");
        let line_label = display_line.to_string();
        let line_padding = " ".repeat(3usize.saturating_sub(line_label.len()));
        say!(Blue line_padding, Bold Blue line_label, " | ", Reset line.as_str());
        print!("{}", " ".repeat(display_line.to_string().len() + 4));

        let underline_start = e.location.start_pos.char_column.max(0) as usize;
        print!("{}", " ".repeat(underline_start));
        let underline_length =
            (e.location.end_pos.char_column - e.location.start_pos.char_column + 1).max(1) as usize;
        say!(Red "^".repeat(underline_length));
    }

    for guidance_line in format_error_guidance_lines(&e) {
        say!(Bright Blue "  ", guidance_line);
    }

    if line.is_empty() && e.location.scope.as_components().is_empty() {
        say!(Dark "     No source location available.");
    }
}

fn error_display_name(error_type: &ErrorType) -> &'static str {
    match error_type {
        ErrorType::Compiler => "Compiler Bug",
        ErrorType::Syntax => "Syntax Error",
        ErrorType::Config => "Malformed Config",
        ErrorType::File => "Missing File or Directory",
        ErrorType::Rule => "Language Rule Error",
        ErrorType::Type => "Type Error",
        ErrorType::DevServer => "Dev Server Issue",
        ErrorType::BorrowChecker => "Borrow Checker Violation",
        ErrorType::HirTransformation => "HIR Transformation Error",
        ErrorType::Backend(BackendErrorType::LirTransformation) => "LIR Transformation Bug",
        ErrorType::Backend(BackendErrorType::WasmGeneration) => "WASM Generation Bug",
    }
}

fn error_visual(error_type: &ErrorType) -> &'static str {
    match error_type {
        ErrorType::Compiler => "🔥 ヽ༼☉ ‿ ⚆༽ﾉ 🔥",
        ErrorType::Syntax => "(╯°□°)╯ 🔥",
        ErrorType::Config => "🔥📄🔥",
        ErrorType::File => "🔥📁🔥",
        ErrorType::Rule => "(˶°o°)ﾉ 🔥",
        ErrorType::Type => "(ಠ_ಠ) 🔥",
        ErrorType::DevServer => "(ﾉ☉_⚆)ﾉ 🔥🖥️🔥",
        ErrorType::BorrowChecker => "(╯°Д°)╯ 🔥",
        ErrorType::HirTransformation => "(☉_☉) 🔥",
        ErrorType::Backend(BackendErrorType::LirTransformation) => "ヽ(°〇°)ﾉ 🔥",
        ErrorType::Backend(BackendErrorType::WasmGeneration) => "(° O °) 🔥",
    }
}

fn format_terse_error_line(error: &CompilerError, string_table: &StringTable) -> String {
    let mut line = format!(
        "E|{}|{}|{}:{}|{}",
        terse_error_type_name(&error.error_type),
        terse_scope_path(&error.location.scope, string_table),
        display_line_number(error.location.start_pos.line_number),
        display_column_number(error.location.start_pos.char_column),
        sanitize_terse_field(&error.msg)
    );

    let mut metadata_fields = error
        .metadata
        .iter()
        .map(|(key, value)| {
            format!(
                "{}={}",
                terse_metadata_key_name(key),
                sanitize_terse_field(value)
            )
        })
        .collect::<Vec<_>>();
    metadata_fields.sort();

    for field in metadata_fields {
        line.push('|');
        line.push_str(&field);
    }

    line
}

fn format_terse_warning_line(warning: &CompilerWarning, string_table: &StringTable) -> String {
    format!(
        "W|{}|{}|{}:{}|{}",
        terse_warning_kind_name(&warning.warning_kind),
        terse_scope_path(&warning.location.scope, string_table),
        display_line_number(warning.location.start_pos.line_number),
        display_column_number(warning.location.start_pos.char_column),
        sanitize_terse_field(&warning.msg)
    )
}

fn terse_scope_path(scope: &InternedPath, string_table: &StringTable) -> String {
    let display_path = resolved_display_path(scope, string_table);
    let sanitized = sanitize_terse_field(&display_path);
    if sanitized.is_empty() {
        String::from("<unknown>")
    } else {
        sanitized
    }
}

fn display_line_number(raw_line: i32) -> i32 {
    raw_line.saturating_add(1).max(1)
}

fn display_column_number(raw_column: i32) -> i32 {
    raw_column.saturating_add(1).max(1)
}

fn sanitize_terse_field(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .replace('|', "/")
}

fn terse_error_type_name(error_type: &ErrorType) -> &'static str {
    match error_type {
        ErrorType::Syntax => "syntax",
        ErrorType::Type => "type",
        ErrorType::Rule => "rule",
        ErrorType::File => "file",
        ErrorType::Config => "config",
        ErrorType::Compiler => "compiler",
        ErrorType::DevServer => "dev_server",
        ErrorType::BorrowChecker => "borrow_checker",
        ErrorType::HirTransformation => "hir_transformation",
        ErrorType::Backend(backend) => match backend {
            BackendErrorType::LirTransformation => "lir_transformation",
            BackendErrorType::WasmGeneration => "wasm_generation",
        },
    }
}

fn terse_warning_kind_name(warning_kind: &WarningKind) -> &'static str {
    match warning_kind {
        WarningKind::UnusedVariable => "unused_variable",
        WarningKind::UnusedFunction => "unused_function",
        WarningKind::UnusedImport => "unused_import",
        WarningKind::UnusedType => "unused_type",
        WarningKind::UnusedConstant => "unused_constant",
        WarningKind::UnusedFunctionArgument => "unused_function_argument",
        WarningKind::UnusedFunctionReturnValue => "unused_function_return_value",
        WarningKind::UnusedFunctionParameter => "unused_function_parameter",
        WarningKind::UnusedFunctionParameterDefaultValue => {
            "unused_function_parameter_default_value"
        }
        WarningKind::PointlessExport => "pointless_export",
        WarningKind::MalformedCssTemplate => "malformed_css_template",
        WarningKind::MalformedHtmlTemplate => "malformed_html_template",
        WarningKind::BstFilePathInTemplateOutput => "bst_file_path_in_template_output",
        WarningKind::LargeTrackedAsset => "large_tracked_asset",
        WarningKind::IdentifierNamingConvention => "identifier_naming_convention",
        WarningKind::ImportAliasCaseMismatch => "import_alias_case_mismatch",
    }
}

fn terse_metadata_key_name(key: &ErrorMetaDataKey) -> &'static str {
    match key {
        ErrorMetaDataKey::VariableName => "variable",
        ErrorMetaDataKey::CompilationStage => "stage",
        ErrorMetaDataKey::PrimarySuggestion => "help",
        ErrorMetaDataKey::AlternativeSuggestion => "alternative",
        ErrorMetaDataKey::SuggestedReplacement => "suggested_replacement",
        ErrorMetaDataKey::SuggestedInsertion => "suggested_insertion",
        ErrorMetaDataKey::SuggestedLocation => "suggested_location",
        ErrorMetaDataKey::ExpectedType => "expected_type",
        ErrorMetaDataKey::FoundType => "found_type",
        ErrorMetaDataKey::InferredType => "inferred_type",
        ErrorMetaDataKey::BorrowKind => "borrow_kind",
        ErrorMetaDataKey::LifetimeHint => "lifetime_hint",
        ErrorMetaDataKey::MovedVariable => "moved_variable",
        ErrorMetaDataKey::BorrowedVariable => "borrowed_variable",
        ErrorMetaDataKey::ConflictingVariable => "conflicting_variable",
        ErrorMetaDataKey::ConflictingPlace => "conflicting_place",
        ErrorMetaDataKey::ExistingBorrowPlace => "existing_borrow_place",
        ErrorMetaDataKey::ConflictType => "conflict_type",
    }
}

#[cfg(test)]
#[path = "tests/display_messages_tests.rs"]
mod display_messages_tests;
