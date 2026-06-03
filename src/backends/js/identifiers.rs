//! Generated JS identifier safety and uniqueness.
//!
//! WHAT: generates unique temporary identifiers and sanitizes raw names so they
//! are valid and non-conflicting JavaScript identifiers.
//! WHY: the JS backend and the symbol-map builder both need to agree on what
//! makes a safe JS name, and temporary names must not collide with user symbols.
//!
//! This module must not own source text emission, symbol lookup, or CFG
//! reachability. Those responsibilities belong to their focused owners.

use crate::backends::js::JsEmitter;

impl<'hir> JsEmitter<'hir> {
    pub(crate) fn next_temp_identifier(&mut self, prefix: &str) -> String {
        loop {
            let raw = format!("{}_{}", prefix, self.temp_counter);
            self.temp_counter += 1;
            let candidate = sanitize_identifier(&raw);

            if !self.used_identifiers.contains(&candidate) {
                self.used_identifiers.insert(candidate.clone());
                return candidate;
            }
        }
    }
}

pub(crate) fn sanitize_identifier(raw: &str) -> String {
    let mut result = String::new();

    for (index, ch) in raw.chars().enumerate() {
        let is_valid = if index == 0 {
            ch == '_' || ch == '$' || ch.is_ascii_alphabetic()
        } else {
            ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()
        };

        if is_valid {
            result.push(ch);
        } else {
            result.push('_');
        }
    }

    if result.is_empty() {
        "_value".to_owned()
    } else if result
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_digit())
    {
        format!("_{result}")
    } else {
        result
    }
}

pub(crate) fn is_js_reserved(name: &str) -> bool {
    matches!(
        name,
        "break"
            | "case"
            | "catch"
            | "class"
            | "const"
            | "continue"
            | "debugger"
            | "default"
            | "delete"
            | "do"
            | "else"
            | "export"
            | "extends"
            | "finally"
            | "for"
            | "function"
            | "if"
            | "import"
            | "in"
            | "instanceof"
            | "new"
            | "return"
            | "super"
            | "switch"
            | "this"
            | "throw"
            | "try"
            | "typeof"
            | "var"
            | "void"
            | "while"
            | "with"
            | "yield"
            | "enum"
            | "implements"
            | "interface"
            | "let"
            | "package"
            | "private"
            | "protected"
            | "public"
            | "static"
            | "await"
            | "abstract"
            | "boolean"
            | "byte"
            | "char"
            | "double"
            | "final"
            | "float"
            | "goto"
            | "int"
            | "long"
            | "native"
            | "short"
            | "synchronized"
            | "throws"
            | "transient"
            | "volatile"
            | "undefined"
            | "null"
            | "true"
            | "false"
            | "NaN"
            | "Infinity"
            | "eval"
            | "arguments"
    )
}
