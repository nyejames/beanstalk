use crate::compiler_frontend::compiler_errors::{
    CompilerError, CompilerMessages, ErrorMetaDataKey, ErrorType,
};
use crate::compiler_frontend::compiler_warnings::print_formatted_warning;
use saying::say;
use std::path::{Path, PathBuf};
use std::{env, fs};

pub(crate) fn normalize_display_path(path: &Path) -> PathBuf {
    let path_string = path.to_string_lossy();
    if let Some(stripped) = path_string.strip_prefix(r"\\?\") {
        return PathBuf::from(stripped);
    }

    path.to_path_buf()
}

pub(crate) fn relative_display_path_from_root(scope: &Path, root: &Path) -> String {
    let normalized_scope = normalize_display_path(scope);
    let normalized_root = normalize_display_path(root);

    normalized_scope
        .strip_prefix(&normalized_root)
        .unwrap_or(&normalized_scope)
        .to_string_lossy()
        .to_string()
}

fn relative_display_path(scope: &Path) -> String {
    match env::current_dir() {
        Ok(dir) => relative_display_path_from_root(scope, &dir),
        Err(err) => {
            say!(Red
                "Compiler failed to find the file to give you the snippet. Another compiler_frontend developer skill issue. ",
                err
            );
            normalize_display_path(scope).to_string_lossy().to_string()
        }
    }
}

pub(crate) fn resolve_source_file_path(scope: &Path) -> PathBuf {
    let mut source_file = normalize_display_path(scope);

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
        Ok(canonical_path) => normalize_display_path(&canonical_path),
        Err(_) => source_file,
    }
}

pub(crate) fn format_error_guidance_lines(error: &CompilerError) -> Vec<String> {
    let mut lines = Vec::new();

    if let Some(stage) = error.metadata.get(&ErrorMetaDataKey::CompilationStage) {
        lines.push(format!("Stage: {stage}"));
    }

    if let Some(suggestion) = error.metadata.get(&ErrorMetaDataKey::PrimarySuggestion) {
        lines.push(format!("Help: {suggestion}"));
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
    // Format and print out the messages:
    for err in messages.errors {
        print_formatted_error(err);
    }

    for warning in messages.warnings {
        print_formatted_warning(warning);
    }
}

pub fn print_formatted_error(e: CompilerError) {
    // Walk back through the file path until it's the current directory.
    // Normalize windows extended paths first (e.g. \\?\C:\...) for readable output.
    let relative_dir = relative_display_path(&e.location.scope);

    let line_number = e.location.start_pos.line_number as usize;

    // Read the file and get the actual line as a string from the code
    // Strip the actual header at the end of the path (.header extension)
    let actual_file = resolve_source_file_path(&e.location.scope);

    let line = match fs::read_to_string(&actual_file) {
        Ok(file) => file
            .lines()
            .nth(line_number)
            .unwrap_or_default()
            .to_string(),
        Err(_) => {
            // say!(Red
            //     "Compiler Skill Issue: Error with printing error. File path is invalid: {}",
            //     actual_file.display()
            // );
            "".to_string()
        }
    };

    // say!(Red "Error with printing error ヽ༼☉ ‿ ⚆༽ﾉ Line number is out of range of file. If you see this, it confirms the compiler_frontend developer is an idiot");

    // e_dark_yellow!("Error: ");

    match e.error_type {
        ErrorType::Syntax => {
            if !relative_dir.is_empty() {
                say!("\n(╯°□°)╯  🔥🔥 ", Dark Magenta relative_dir, " 🔥🔥  Σ(°△°;) ");
            }

            say!(Red "Syntax");
            say!(Dark Magenta "Line ", Bright {line_number + 1});
        }

        ErrorType::Type => {
            if !relative_dir.is_empty() {
                say!("\n(ಠ_ಠ) ", Dark Magenta relative_dir);
                say!(Inline " ( ._. ) ");
            }

            say!(Red "Type Error");
            say!(Dark Magenta "Line ", Bright {line_number + 1});
        }

        ErrorType::Rule => {
            if !relative_dir.is_empty() {
                say!("\nヽ(˶°o°)ﾉ  🔥🔥🔥 ", Dark Magenta relative_dir, " 🔥🔥🔥  ╰(°□°╰) ");
            }

            say!(Red "Rule");
            say!(Dark Magenta "Line ", Bright {line_number + 1});
        }

        ErrorType::File => {
            say!(Yellow "🏚 Can't find/read file or directory: ", relative_dir);
            say!(e.msg);
            return;
        }

        ErrorType::Compiler => {
            if !relative_dir.is_empty() {
                say!("\nヽ༼☉ ‿ ⚆༽ﾉ  🔥🔥🔥🔥 ", Dark Magenta relative_dir, " 🔥🔥🔥🔥  ╰(° _ o╰) ");
            }
            say!(Yellow "COMPILER BUG - ");
            say!(Dark Yellow "compiler_frontend developer skill issue (not your fault)");
        }

        ErrorType::Config => {
            if !relative_dir.is_empty() {
                say!("\n (-_-)  🔥🔥🔥🔥 ", Dark Magenta relative_dir, " 🔥🔥🔥🔥  <(^~^)/ ");
            }
            say!(Yellow "CONFIG FILE ISSUE- ");
            say!(
                Dark Yellow "Malformed config file, something doesn't make sense inside the project config)"
            );
        }

        ErrorType::DevServer => {
            if !relative_dir.is_empty() {
                say!("\n(ﾉ☉_⚆)ﾉ  🔥 ", Dark Magenta relative_dir, " 🔥 ╰(° O °)╯ ");
            }

            say!(Yellow "Dev Server whoopsie: ", Red e.msg);
            return;
        }

        ErrorType::BorrowChecker => {
            if !relative_dir.is_empty() {
                say!("\n(╯°Д°)╯  🔥🔥 ", Dark Magenta relative_dir, " 🔥🔥  ╰(°□°╰) ");
            }

            say!(Red "Borrow Checker");
            say!(Dark Magenta "Line ", Bright {line_number + 1});
        }

        ErrorType::HirTransformation => {
            if !relative_dir.is_empty() {
                say!("\nヽ༼☉ ‿ ⚆༽ﾉ  🔥🔥🔥 ", Dark Magenta relative_dir, " 🔥🔥🔥  ╰(°□°╰) ");
            }

            say!(Yellow "HIR TRANSFORMATION BUG - ");
            say!(Dark Yellow "compiler_frontend developer skill issue (not your fault)");
        }

        ErrorType::LirTransformation => {
            if !relative_dir.is_empty() {
                say!("\nヽ༼☉ ‿ ⚆༽ﾉ  🔥🔥🔥 ", Dark Magenta relative_dir, " 🔥🔥🔥  ╰(° _ o╰) ");
            }

            say!(Yellow "LIR TRANSFORMATION BUG - ");
            say!(Dark Yellow "compiler_frontend developer skill issue (not your fault)");
        }

        ErrorType::WasmGeneration => {
            if !relative_dir.is_empty() {
                say!("\nヽ༼☉ ‿ ⚆༽ﾉ  🔥🔥🔥🔥 ", Dark Magenta relative_dir, " 🔥🔥🔥🔥  ╰(° O °)╯ ");
                say!(Yellow "WASM GENERATION BUG - ", Dark "compiler_frontend developer skill issue (not your fault)");
            }
        }
    }

    say!(Red e.msg);
    for guidance_line in format_error_guidance_lines(&e) {
        say!(Dark Yellow guidance_line);
    }

    println!("\n{line}");

    // spaces before the relevant part of the line
    print!(
        "{}",
        " ".repeat((e.location.start_pos.char_column - 1).max(0) as usize)
    );

    let length_of_underline =
        (e.location.end_pos.char_column - e.location.start_pos.char_column + 1).max(1) as usize;
    say!(Red { "^".repeat(length_of_underline) });
}

#[cfg(test)]
#[path = "tests/display_messages_tests.rs"]
mod display_messages_tests;
