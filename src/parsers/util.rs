use crate::tokenizer::TokenPosition;

pub fn string_dimensions(s: &str) -> TokenPosition {
    let (width, height) = s.lines()
        .map(|line| line.len())
        .fold((0, 0), |(max_width, count), len| (max_width.max(len), count + 1));
    
    TokenPosition {
        line_number: height.max(1),
        char_column: width as u32,
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

pub fn count_newlines_at_start_of_string(s: &str) -> usize {
    let mut count = 0;

    for c in s.chars() {
        if c == '\n' {
            count += 1;
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
