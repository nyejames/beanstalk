// This crate currently has a lot of dead code.
// But some of these may become useful again in the future.
// So cli.rs is using #[allow(dead_code)] on this crate. This attribute should be removed in the future.

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
