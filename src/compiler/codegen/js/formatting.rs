//! JS formatting helpers shared by the codegen backend.
//!
//! Keeping string escaping, property access rules, and operator rendering here
//! avoids duplicating subtle JS concerns across the emitter. Most operators are
//! wrapped in parens to preserve evaluation order when expressions are composed.

use crate::compiler::hir::nodes::{BinOp, UnaryOp};

pub fn format_bin_op(left: &str, op: BinOp, right: &str) -> String {
    match op {
        BinOp::Add => format!("({} + {})", left, right),
        BinOp::Sub => format!("({} - {})", left, right),
        BinOp::Mul => format!("({} * {})", left, right),
        BinOp::Div => format!("({} / {})", left, right),
        BinOp::Mod => format!("({} % {})", left, right),
        BinOp::Eq => format!("({} === {})", left, right),
        BinOp::Ne => format!("({} !== {})", left, right),
        BinOp::Lt => format!("({} < {})", left, right),
        BinOp::Le => format!("({} <= {})", left, right),
        BinOp::Gt => format!("({} > {})", left, right),
        BinOp::Ge => format!("({} >= {})", left, right),
        BinOp::And => format!("({} && {})", left, right),
        BinOp::Or => format!("({} || {})", left, right),
        BinOp::Root => format!("Math.pow({}, 1 / {})", left, right),
        BinOp::Exponent => format!("({} ** {})", left, right),
    }
}

pub fn format_unary_op(op: UnaryOp, operand: &str) -> String {
    match op {
        UnaryOp::Neg => format!("(-{})", operand),
        UnaryOp::Not => format!("(!{})", operand),
    }
}

pub fn format_js_string(value: &str) -> String {
    format!("\"{}\"", escape_js_string(value))
}

pub fn format_property_access(base: &str, field: &str) -> String {
    if is_valid_js_identifier(field) {
        format!("({}).{}", base, field)
    } else {
        format!("({})[{}]", base, format_js_string(field))
    }
}

pub fn format_js_property_name(field: &str) -> String {
    if is_valid_js_identifier(field) {
        field.to_owned()
    } else {
        format_js_string(field)
    }
}

pub fn sanitize_identifier(raw: &str) -> String {
    let mut result = String::new();
    let mut chars = raw.chars();
    if let Some(first) = chars.next() {
        if is_identifier_start(first) {
            result.push(first);
        } else {
            result.push('_');
            if is_identifier_continue(first) {
                result.push(first);
            }
        }
        for ch in chars {
            if is_identifier_continue(ch) {
                result.push(ch);
            } else {
                result.push('_');
            }
        }
    }

    if result.is_empty() {
        result.push('_');
    }

    if is_reserved_word(&result) {
        result.insert(0, '_');
    }

    result
}

pub fn is_valid_js_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    if !is_identifier_start(first) {
        return false;
    }
    if chars.any(|ch| !is_identifier_continue(ch)) {
        return false;
    }
    !is_reserved_word(name)
}

fn escape_js_string(value: &str) -> String {
    let mut escaped = String::new();
    for ch in value.chars() {
        match ch {
            '"' => escaped.push_str("\\\""),
            '\\' => escaped.push_str("\\\\"),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\u{08}' => escaped.push_str("\\b"),
            '\u{0c}' => escaped.push_str("\\f"),
            '\0' => escaped.push_str("\\0"),
            ch if ch.is_ascii_graphic() || ch == ' ' => escaped.push(ch),
            // Emit explicit escapes for non-ASCII to keep output stable in source files.
            ch => escaped.push_str(&format!("\\u{{{:x}}}", ch as u32)),
        }
    }
    escaped
}

fn is_identifier_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_' || ch == '$'
}

fn is_identifier_continue(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_' || ch == '$'
}

fn is_reserved_word(word: &str) -> bool {
    matches!(
        word,
        "await"
            | "break"
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
            | "false"
            | "finally"
            | "for"
            | "function"
            | "if"
            | "import"
            | "in"
            | "instanceof"
            | "new"
            | "null"
            | "return"
            | "super"
            | "switch"
            | "this"
            | "throw"
            | "true"
            | "try"
            | "typeof"
            | "var"
            | "void"
            | "while"
            | "with"
            | "yield"
            | "let"
            | "enum"
            | "implements"
            | "interface"
            | "package"
            | "private"
            | "protected"
            | "public"
            | "static"
    )
}
