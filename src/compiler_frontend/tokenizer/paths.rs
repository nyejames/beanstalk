use crate::compiler_frontend::basic_utility_functions::NumericalParsing;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{Token, TokenKind, TokenStream};
use crate::{return_syntax_error, return_token};

pub fn parse_file_path(
    stream: &mut TokenStream,
    string_table: &mut StringTable,
) -> Result<Token, CompilerError> {
    // Path Syntax
    // @(path/to/file)
    // Path to multiple items within the same directory (used for imports)
    // @(path/to/file/ {import1, import2, import3})
    // or @(path/to/file/ {
    //        import1,
    //        import2
    // })
    let mut base_components = Vec::with_capacity(2);
    let mut segment = String::new();

    // Skip initial non-newline whitespace
    while let Some(c) = stream.peek() {
        if c.is_non_newline_whitespace() && c != &'\n' {
            stream.next();
            continue;
        }
        break;
    }

    if stream.peek() == Some(&'(') {
        stream.next();
    } else {
        return_syntax_error!(
            "Path must start with an open parenthesis after '@'",
            stream.new_location().to_error_location(string_table), {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Use an open parenthesis after '@' to start a path",
            }
        )
    }

    let mut imports: Vec<InternedPath> = Vec::with_capacity(1);
    let mut grouped_imports = false;

    while let Some(c) = stream.peek().copied() {
        match c {
            ')' => {
                push_segment_if_non_empty(&mut base_components, &mut segment, string_table);
                stream.next();
                break;
            }

            '{' => {
                grouped_imports = true;
                push_segment_if_non_empty(&mut base_components, &mut segment, string_table);
                stream.next();
                break;
            }

            '/' | '\\' => {
                push_segment_if_non_empty(&mut base_components, &mut segment, string_table);
                stream.next();
            }

            _ => {
                segment.push(c);
                stream.next();
            }
        }
    }

    if base_components.is_empty() {
        return_syntax_error!(
            "Import path cannot be empty",
            stream.new_location().to_error_location(string_table), {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Provide a valid file path",
            }
        )
    }

    if !grouped_imports {
        imports.push(InternedPath::from_components(base_components));
        return_token!(TokenKind::Path(imports), stream);
    }

    let mut expect_symbol = true;
    let mut parsed_symbols = 0usize;

    while let Some(c) = stream.peek().copied() {
        if c.is_whitespace() {
            stream.next();
            continue;
        }

        match c {
            '}' => {
                if expect_symbol && parsed_symbols > 0 {
                    return_syntax_error!(
                        "Trailing comma is not allowed in grouped imports.",
                        stream.new_location().to_error_location(string_table), {
                            CompilationStage => "Tokenization",
                            PrimarySuggestion => "Remove the trailing comma before '}'",
                        }
                    )
                }

                stream.next();
                break;
            }

            ',' => {
                if expect_symbol {
                    return_syntax_error!(
                        "Multiple commas in a row are not allowed in grouped imports.",
                        stream.new_location().to_error_location(string_table), {
                            CompilationStage => "Tokenization",
                            PrimarySuggestion => "Remove the extra comma",
                        }
                    )
                }

                expect_symbol = true;
                stream.next();
            }

            _ => {
                if !expect_symbol {
                    return_syntax_error!(
                        "Grouped import symbols must be separated by commas.",
                        stream.new_location().to_error_location(string_table), {
                            CompilationStage => "Tokenization",
                            PrimarySuggestion => "Add a comma between grouped import symbols",
                        }
                    )
                }

                if !(c.is_alphabetic() || c == '_') {
                    return_syntax_error!(
                        "Invalid grouped import symbol. Symbols must start with a letter or underscore.",
                        stream.new_location().to_error_location(string_table), {
                            CompilationStage => "Tokenization",
                            PrimarySuggestion => "Use a valid grouped import symbol name",
                        }
                    )
                }

                let mut symbol = String::new();
                symbol.push(c);
                stream.next();

                while let Some(next) = stream.peek().copied() {
                    if next.is_alphanumeric() || next == '_' {
                        symbol.push(next);
                        stream.next();
                    } else {
                        break;
                    }
                }

                let mut full_components = base_components.clone();
                full_components.push(string_table.intern(&symbol));
                imports.push(InternedPath::from_components(full_components));
                parsed_symbols += 1;
                expect_symbol = false;
            }
        }
    }

    if imports.is_empty() {
        return_syntax_error!(
            "Grouped imports require at least one symbol.",
            stream.new_location().to_error_location(string_table), {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Add at least one import symbol inside '{}'",
            }
        )
    }

    // Skip trailing whitespace and enforce final ')'
    while let Some(c) = stream.peek() {
        if c.is_whitespace() {
            stream.next();
        } else {
            break;
        }
    }

    if stream.peek() != Some(&')') {
        return_syntax_error!(
            "Invalid character, expected a closing parenthesis after grouped imports.",
            stream.new_location().to_error_location(string_table), {
                CompilationStage => "Tokenization",
                PrimarySuggestion => "Add a closing parenthesis after the grouped imports",
            }
        )
    }
    stream.next();

    return_token!(TokenKind::Path(imports), stream)
}

fn push_segment_if_non_empty(
    components: &mut Vec<crate::compiler_frontend::string_interning::StringId>,
    segment: &mut String,
    string_table: &mut StringTable,
) {
    let trimmed = segment.trim();
    if !trimmed.is_empty() {
        components.push(string_table.intern(trimmed));
    }
    segment.clear();
}

#[cfg(test)]
#[path = "tests/paths_tests.rs"]
mod paths_tests;
