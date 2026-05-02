//! Case parser module - Parses benchmark case definitions from cases.txt
//!
//! This module provides functionality to parse the `benchmarks/cases.txt` file
//! into structured BenchmarkCase instances that can be executed by the
//! benchmark orchestrator.
//!
//! # File Format
//!
//! The cases.txt file uses a simple line-based format:
//! - Lines starting with `#` are comments and are ignored
//! - Empty lines are ignored
//! - Each benchmark case is: `<command> <arg1> <arg2> ...`
//! - Multiple consecutive spaces are treated as a single separator
//! - Arguments containing spaces can be quoted with double quotes

// Allow dead code for now - this module will be used by the orchestrator
#![allow(dead_code)]

use std::fs;
use std::path::Path;

/// A single benchmark case parsed from cases.txt
///
/// Each case represents a command to execute against the bean binary,
/// along with its arguments and a sanitized name for use in filenames.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BenchmarkCase {
    /// Sanitized name derived from command + args (spaces/special chars replaced with underscores)
    pub name: String,
    /// The command to execute (e.g., "check", "build", "run")
    pub command: String,
    /// Arguments to pass to the command
    pub args: Vec<String>,
}

/// Parse benchmark cases from a file
///
/// Reads the file at the given path and parses each non-comment, non-empty
/// line as a benchmark case.
///
/// # Arguments
///
/// * `path` - Path to the cases.txt file
///
/// # Returns
///
/// A vector of parsed BenchmarkCase instances, or an error message if parsing fails.
///
/// # Errors
///
/// Returns an error if:
/// - The file cannot be read
/// - The file contains invalid UTF-8
/// - A line cannot be parsed (empty command)
///
/// # Example
///
/// ```ignore
/// use std::path::Path;
/// use case_parser::parse_cases;
///
/// let cases = parse_cases(Path::new("benchmarks/cases.txt"))?;
/// for case in cases {
///     println!("{}: {} {:?}", case.name, case.command, case.args);
/// }
/// ```
pub fn parse_cases(path: &Path) -> Result<Vec<BenchmarkCase>, String> {
    // Read file contents as UTF-8
    let contents = fs::read_to_string(path)
        .map_err(|e| format!("Failed to read cases file '{}': {}", path.display(), e))?;

    let mut cases = Vec::new();

    for (line_num, line) in contents.lines().enumerate() {
        let trimmed = line.trim();

        // Skip empty lines
        if trimmed.is_empty() {
            continue;
        }

        // Skip comment lines (starting with #)
        if trimmed.starts_with('#') {
            continue;
        }

        // Parse the line as a benchmark case
        match parse_line(trimmed) {
            Ok(case) => cases.push(case),
            Err(e) => {
                return Err(format!(
                    "Failed to parse cases file '{}' at line {}: {}",
                    path.display(),
                    line_num + 1,
                    e
                ));
            }
        }
    }

    Ok(cases)
}

/// Parse a single line into a BenchmarkCase
///
/// The line format is: `<command> <arg1> <arg2> ...`
/// Multiple consecutive spaces are treated as a single separator.
/// Arguments containing spaces can be quoted with double quotes.
#[allow(dead_code)]
fn parse_line(line: &str) -> Result<BenchmarkCase, String> {
    let tokens = tokenize_line(line)?;

    if tokens.is_empty() {
        return Err("Empty line (no command found)".to_string());
    }

    // First token is the command
    let command = tokens[0].clone();

    // Remaining tokens are arguments
    let args: Vec<String> = tokens[1..].to_vec();

    // Generate sanitized name from command + args
    let name = sanitize_case_name(&command, &args);

    Ok(BenchmarkCase {
        name,
        command,
        args,
    })
}

/// Tokenize a line, handling quoted arguments
///
/// Splits the line by whitespace, but treats quoted strings as single tokens.
/// Multiple consecutive whitespace characters are treated as a single separator.
#[allow(dead_code)]
fn tokenize_line(line: &str) -> Result<Vec<String>, String> {
    let mut tokens = Vec::new();
    let mut current_token = String::new();
    let mut in_quotes = false;

    for ch in line.chars() {
        match ch {
            // Handle quote characters
            '"' => {
                if in_quotes {
                    // End of quoted section
                    in_quotes = false;
                    // Don't include the quote in the token
                } else {
                    // Start of quoted section
                    in_quotes = true;
                    // Don't include the quote in the token
                }
            }
            // Handle whitespace outside quotes
            ' ' | '\t' if !in_quotes => {
                if !current_token.is_empty() {
                    tokens.push(current_token.clone());
                    current_token.clear();
                }
                // Skip consecutive whitespace
            }
            // Regular character or whitespace inside quotes
            _ => {
                current_token.push(ch);
            }
        }
    }

    // Handle the last token
    if !current_token.is_empty() {
        tokens.push(current_token);
    }

    // Check for unclosed quotes
    if in_quotes {
        return Err("Unclosed quote in line".to_string());
    }

    Ok(tokens)
}

/// Generate a sanitized case name from command and args
///
/// Replaces spaces and special characters with underscores to create
/// a valid filename for logs and reports.
///
/// WHAT: Creates filesystem-safe names from benchmark case definitions
/// WHY: Ensures log files and reports have valid, consistent filenames
#[allow(dead_code)]
fn sanitize_case_name(command: &str, args: &[String]) -> String {
    let mut parts = vec![command.to_string()];
    parts.extend(args.iter().cloned());

    let full_name = parts.join("_");

    // Replace special characters with underscores
    let sanitized: String = full_name
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();

    // Collapse multiple consecutive underscores into one
    let mut result = String::new();
    let mut prev_underscore = false;

    for ch in sanitized.chars() {
        if ch == '_' {
            if !prev_underscore {
                result.push(ch);
            }
            prev_underscore = true;
        } else {
            result.push(ch);
            prev_underscore = false;
        }
    }

    // Remove leading/trailing underscores
    result.trim_matches('_').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_line_simple() {
        let line = "check speed-test.bst";
        let case = parse_line(line).unwrap();
        assert_eq!(case.command, "check");
        assert_eq!(case.args, vec!["speed-test.bst"]);
        assert_eq!(case.name, "check_speed-test_bst");
    }

    #[test]
    fn test_parse_line_multiple_args() {
        let line = "build docs src output";
        let case = parse_line(line).unwrap();
        assert_eq!(case.command, "build");
        assert_eq!(case.args, vec!["docs", "src", "output"]);
        assert_eq!(case.name, "build_docs_src_output");
    }

    #[test]
    fn test_parse_line_multiple_spaces() {
        let line = "check   speed-test.bst";
        let case = parse_line(line).unwrap();
        assert_eq!(case.command, "check");
        assert_eq!(case.args, vec!["speed-test.bst"]);
    }

    #[test]
    fn test_parse_line_quoted_arg() {
        let line = r#"check "path with spaces.bst""#;
        let case = parse_line(line).unwrap();
        assert_eq!(case.command, "check");
        assert_eq!(case.args, vec!["path with spaces.bst"]);
    }

    #[test]
    fn test_parse_line_empty() {
        let line = "";
        let result = parse_line(line);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_line_unclosed_quote() {
        let line = r#"check "unclosed"#;
        let result = parse_line(line);
        assert!(result.is_err());
    }

    #[test]
    fn test_sanitize_case_name() {
        assert_eq!(
            sanitize_case_name("check", &["speed-test.bst".to_string()]),
            "check_speed-test_bst"
        );
        assert_eq!(
            sanitize_case_name("check", &["path/to/file.bst".to_string()]),
            "check_path_to_file_bst"
        );
    }

    #[test]
    fn test_tokenize_line() {
        let tokens = tokenize_line("check file.bst").unwrap();
        assert_eq!(tokens, vec!["check", "file.bst"]);

        let tokens = tokenize_line("check   file.bst").unwrap();
        assert_eq!(tokens, vec!["check", "file.bst"]);

        let tokens = tokenize_line(r#"check "file name.bst""#).unwrap();
        assert_eq!(tokens, vec!["check", "file name.bst"]);
    }
}
