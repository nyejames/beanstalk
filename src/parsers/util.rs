use crate::parsers::ast_nodes::Arg;
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

pub fn find_first_missing(indexes_filled: &[usize]) -> usize {
    let mut i = 0;
    while indexes_filled.contains(&i) {
        i += 1;
    }
    i
}

pub fn sort_unnamed_args_last(args: &[Arg]) -> Vec<Arg> {
    let length = args.len();
    let mut args_in_sorted = Vec::with_capacity(length);
    let mut empty: Vec<Arg> = Vec::with_capacity(length);

    for a in args {
        if a.name.is_empty() {
            empty.push(a.to_owned());
        } else {
            args_in_sorted.push(a.to_owned());
        }
    }

    args_in_sorted.extend(empty);

    args_in_sorted
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

pub fn _count_newlines_at_start_of_string(s: &str) -> usize {
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
    fn _is_non_newline_whitespace(&self) -> bool;
    fn is_number_operation_char(&self) -> bool;
    fn is_bracket(&self) -> bool;
}
impl NumericalParsing for char {
    fn _is_non_newline_whitespace(&self) -> bool {
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
