// This crate currently has a lot of dead code.
// But some of these may become useful again in the future.
#![allow(dead_code)]

use crate::compiler::compiler_errors::CompilerError;
use crate::return_file_error;
use std::path::{Path, PathBuf};

pub fn is_valid_var_char(char: &char) -> bool {
    (char.is_alphanumeric() || *char == '_') && !char.is_ascii_punctuation()
}

// Checks the path and converts it to a PathBuf
// Resolves mixing unix and windows paths
pub fn check_if_valid_path(path: &str) -> Result<PathBuf, CompilerError> {
    // If it contains Unix-style slashes, convert them
    let path = if cfg!(windows) && path.contains('/') {
        // Replace forward slashes with backslashes
        &path.replace('/', "\\")
    } else {
        path
    };

    let path = Path::new(path);

    // Check if the path exists
    if !path.exists() {
        return_file_error!(path, "Path does not exist", {
            CompilationStage => "Build system path checking"
        });
    }

    Ok(path.to_path_buf())
}
pub fn combine_two_slices_to_vec<T: Clone>(a: &[T], b: &[T]) -> Vec<T> {
    let mut combined = Vec::with_capacity(a.len() + b.len());
    combined.extend_from_slice(a);
    combined.extend_from_slice(b);

    combined
}

pub fn find_first_missing(indexes_filled: &[usize]) -> usize {
    let mut i = 0;
    while indexes_filled.contains(&i) {
        i += 1;
    }
    i
}
pub fn first_letter_is_capitalised(s: &str) -> bool {
    let mut c = s.chars();
    match c.next() {
        None => false,
        Some(f) => f.is_uppercase(),
    }
}

pub fn count_newlines_at_end_of_string(s: &str) -> usize {
    let mut count = 0;
    for c in s.chars().rev() {
        if c == '\n' {
            count += 1;
            continue;
        }

        if c.is_whitespace() {
            continue;
        }

        break;
    }

    count
}

// Traits for builtin types to help with parsing
pub trait NumericalParsing {
    fn is_non_newline_whitespace(&self) -> bool;
    fn is_number_operation_char(&self) -> bool;
    fn is_bracket(&self) -> bool;
}
impl NumericalParsing for char {
    fn is_non_newline_whitespace(&self) -> bool {
        self.is_whitespace() && self != &'\n'
    }
    fn is_number_operation_char(&self) -> bool {
        self.is_numeric()
            || self == &'.'
            || self == &'_'
            || self == &'-'
            || self == &'+'
            || self == &'*'
            || self == &'/'
            || self == &'%'
            || self == &'^'
    }
    fn is_bracket(&self) -> bool {
        matches!(self, '(' | ')' | '{' | '}' | '[' | ']')
    }
}

// Convert snake_case to PascalCase
pub fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(first) => {
                    first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase()
                }
            }
        })
        .collect()
}
