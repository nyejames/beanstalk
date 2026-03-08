//! Built-in `$code` template style support.
//!
//! This module owns both halves of the feature:
//! - parsing the narrow `$code` / `$code("ext")` directive syntax
//! - converting compile-time body string runs into highlighted HTML

use crate::compiler_frontend::ast::templates::create_template_node::Template;
use crate::compiler_frontend::ast::templates::markdown::HIDDEN_SKIP_CHAR;
use crate::compiler_frontend::ast::templates::template::{Formatter, TemplateFormatter};
use crate::compiler_frontend::basic_utility_functions::NumericalParsing;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::return_syntax_error;
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CodeLanguage {
    Generic,
    Beanstalk,
    JavaScript,
    TypeScript,
    Python,
}

impl CodeLanguage {
    pub(crate) fn from_alias(alias: &str) -> Option<Self> {
        match alias {
            "bst" | "beanstalk" => Some(Self::Beanstalk),
            "js" | "javascript" => Some(Self::JavaScript),
            "ts" | "typescript" => Some(Self::TypeScript),
            "py" | "python" => Some(Self::Python),
            _ => None,
        }
    }

    pub(crate) fn supported_aliases() -> &'static str {
        "\"bst\"/\"beanstalk\", \"js\"/\"javascript\", \"ts\"/\"typescript\", \"py\"/\"python\""
    }

    fn comment_prefix(self) -> Option<&'static str> {
        match self {
            Self::Generic => None,
            Self::Beanstalk => Some("--"),
            Self::JavaScript | Self::TypeScript => Some("//"),
            Self::Python => Some("#"),
        }
    }
}

#[derive(Debug)]
struct CodeTemplateFormatter {
    language: CodeLanguage,
}

impl TemplateFormatter for CodeTemplateFormatter {
    fn format(&self, content: &mut String) {
        let highlighted = highlight_code_html(content, self.language);
        *content = format!("<code class='codeblock'>{highlighted}</code>");
    }
}

pub(crate) fn code_formatter(language: CodeLanguage) -> Formatter {
    Formatter {
        id: "code",
        skip_if_already_formatted: false,
        formatter: Arc::new(CodeTemplateFormatter { language }),
    }
}

pub(crate) fn configure_code_style(
    token_stream: &mut FileTokens,
    template: &mut Template,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    // `$code` is the generic highlighter, while `$code("...")` narrows the
    // token rules to one of the built-in language profiles.
    let language = if token_stream.peek_next_token() != Some(&TokenKind::OpenParenthesis) {
        CodeLanguage::Generic
    } else {
        parse_code_language_argument(token_stream, string_table)?
    };

    template.style.id = "code";
    template.style.formatter = Some(code_formatter(language));
    template.style.formatter_precedence = 0;
    Ok(())
}

pub(crate) fn highlight_code_html(source: &str, language: CodeLanguage) -> String {
    // Normalise indentation first so highlighted output reflects the code the user
    // meant to show, not the template indentation needed to keep the source tidy.
    let normalized_source = dedent_code_block(source);
    let chars: Vec<char> = normalized_source.chars().collect();

    let mut highlighted = String::with_capacity(normalized_source.len() + 16);
    let mut word = String::new();
    let mut index = 0usize;

    while index < chars.len() {
        let current = chars[index];

        if current == HIDDEN_SKIP_CHAR {
            flush_word(&mut highlighted, &mut word, language);
            index += 1;

            // Nested templates can already contain formatted HTML spans. Those sections
            // are wrapped in the shared hidden guard char so parent formatters copy them
            // through without trying to tokenize the generated markup again.
            while index < chars.len() && chars[index] != HIDDEN_SKIP_CHAR {
                highlighted.push(chars[index]);
                index += 1;
            }

            if index < chars.len() {
                index += 1;
            }

            continue;
        }

        // Comments are matched before operators so prefixes like `//` and `--`
        // become a single comment run instead of two separate operator tokens.
        if matches_comment_prefix(&chars, index, language.comment_prefix()) {
            flush_word(&mut highlighted, &mut word, language);
            let prefix = language.comment_prefix().unwrap_or_default();
            highlighted.push_str("<span class='bst-code-comment'>");

            for comment_char in prefix.chars() {
                push_escaped_char(&mut highlighted, comment_char);
            }

            index += prefix.chars().count();

            while index < chars.len() && chars[index] != '\n' {
                push_escaped_char(&mut highlighted, chars[index]);
                index += 1;
            }

            highlighted.push_str("</span>");
            continue;
        }

        if current == '"' || current == '\'' {
            flush_word(&mut highlighted, &mut word, language);
            index = highlight_string(&chars, index, &mut highlighted);
            continue;
        }

        if starts_number_literal(&chars, index) {
            flush_word(&mut highlighted, &mut word, language);
            index = highlight_number_literal(&chars, index, &mut highlighted);
            continue;
        }

        if current.is_bracket() {
            flush_word(&mut highlighted, &mut word, language);
            highlighted.push_str("<span class='bst-code-parenthesis'>");
            push_escaped_char(&mut highlighted, current);
            highlighted.push_str("</span>");
            index += 1;
            continue;
        }

        if is_operator_char(current) {
            flush_word(&mut highlighted, &mut word, language);
            highlighted.push_str("<span class='bst-code-operator'>");
            push_escaped_char(&mut highlighted, current);
            highlighted.push_str("</span>");
            index += 1;
            continue;
        }

        if current.is_whitespace() {
            flush_word(&mut highlighted, &mut word, language);
            highlighted.push(current);
            index += 1;
            continue;
        }

        word.push(current);
        index += 1;
    }

    flush_word(&mut highlighted, &mut word, language);
    highlighted
}

fn parse_code_language_argument(
    token_stream: &mut FileTokens,
    string_table: &StringTable,
) -> Result<CodeLanguage, CompilerError> {
    // The directive syntax intentionally stays narrow for now so style parsing
    // remains independent from the general expression parser.
    // Move from `StyleDirective("code")` to the opening `(` so the helper can
    // validate only the directive-local tokens and leave the outer parser at the
    // closing `)` token.
    token_stream.advance();

    token_stream.advance();
    let argument_token = token_stream.current_token_kind().to_owned();

    match argument_token {
        TokenKind::CloseParenthesis => {
            return_syntax_error!(
                "The '$code()' directive cannot use empty parentheses. Omit the argument entirely for generic highlighting.",
                token_stream.current_location().to_error_location(string_table),
                {
                    PrimarySuggestion => "Use '$code' for generic highlighting or '$code(\"bst\")' to select a built-in language",
                }
            )
        }

        TokenKind::StringSliceLiteral(language_name) => {
            let language_text = string_table.resolve(language_name);
            let Some(language) = CodeLanguage::from_alias(language_text) else {
                return_syntax_error!(
                    format!(
                        "Unsupported '$code(...)' language \"{language_text}\". Supported aliases are {}.",
                        CodeLanguage::supported_aliases()
                    ),
                    token_stream.current_location().to_error_location(string_table),
                    {
                        PrimarySuggestion => "Use one of the supported built-in aliases or omit the argument for generic highlighting",
                    }
                )
            };

            token_stream.advance();

            match token_stream.current_token_kind() {
                TokenKind::CloseParenthesis => Ok(language),
                TokenKind::Comma => {
                    return_syntax_error!(
                        "The '$code(...)' directive supports only one language argument.",
                        token_stream.current_location().to_error_location(string_table),
                        {
                            PrimarySuggestion => "Pass a single quoted string literal such as '$code(\"bst\")'",
                        }
                    )
                }
                TokenKind::Eof => {
                    return_syntax_error!(
                        "Unexpected end of template head while parsing '$code(...)'. Missing ')' to close the directive.",
                        token_stream.current_location().to_error_location(string_table),
                        {
                            PrimarySuggestion => "Close the '$code(...)' directive with ')'",
                            SuggestedInsertion => ")",
                        }
                    )
                }
                _ => {
                    return_syntax_error!(
                        "Expected ')' after the '$code(...)' language argument.",
                        token_stream.current_location().to_error_location(string_table),
                        {
                            PrimarySuggestion => "Close the '$code(...)' directive immediately after the quoted string literal",
                            SuggestedInsertion => ")",
                        }
                    )
                }
            }
        }

        TokenKind::Eof => {
            return_syntax_error!(
                "Unexpected end of template head while parsing '$code(...)'. Missing a quoted string argument and closing ')'.",
                token_stream.current_location().to_error_location(string_table),
                {
                    PrimarySuggestion => "Use '$code' or complete the directive as '$code(\"bst\")'",
                }
            )
        }

        _ => {
            return_syntax_error!(
                "The '$code(...)' directive requires a single quoted string literal argument like '$code(\"bst\")'.",
                token_stream.current_location().to_error_location(string_table),
                {
                    PrimarySuggestion => "Use a quoted string literal or omit the argument entirely for generic highlighting",
                }
            )
        }
    }
}

fn dedent_code_block(source: &str) -> String {
    let mut min_indent: Option<usize> = None;

    for line in source.lines() {
        if line.trim().is_empty() {
            continue;
        }

        let indent = line
            .chars()
            .take_while(|ch| ch.is_non_newline_whitespace())
            .count();

        min_indent = Some(match min_indent {
            Some(existing) => existing.min(indent),
            None => indent,
        });
    }

    let Some(min_indent) = min_indent else {
        return source.to_string();
    };

    let mut dedented = String::with_capacity(source.len());

    for (line_index, line) in source.lines().enumerate() {
        if line_index > 0 {
            dedented.push('\n');
        }

        let mut chars = line.chars();
        let mut removed = 0usize;

        // The template formatter runs on body string slices, which often inherit the
        // surrounding template indentation. Strip the smallest shared indentation so
        // the rendered code keeps its intended relative structure instead of the AST
        // layout indentation.
        while removed < min_indent {
            match chars.next() {
                Some(ch) if ch.is_non_newline_whitespace() => removed += 1,
                Some(ch) => {
                    dedented.push(ch);
                    break;
                }
                None => break,
            }
        }

        dedented.extend(chars);
    }

    if source.ends_with('\n') {
        dedented.push('\n');
    }

    dedented
}

fn matches_comment_prefix(chars: &[char], index: usize, prefix: Option<&str>) -> bool {
    let Some(prefix) = prefix else {
        return false;
    };

    for (offset, expected) in prefix.chars().enumerate() {
        if chars.get(index + offset) != Some(&expected) {
            return false;
        }
    }

    true
}

fn starts_number_literal(chars: &[char], index: usize) -> bool {
    // Starting only on an actual digit avoids swallowing operators from compact
    // expressions like `1+2` into the numeric span.
    chars[index].is_numeric()
}

fn highlight_string(chars: &[char], mut index: usize, output: &mut String) -> usize {
    let quote = chars[index];
    output.push_str("<span class='bst-code-string'>");
    push_escaped_char(output, quote);
    index += 1;

    while index < chars.len() {
        let current = chars[index];
        push_escaped_char(output, current);
        index += 1;

        if current == '\\' && index < chars.len() {
            push_escaped_char(output, chars[index]);
            index += 1;
            continue;
        }

        if current == quote {
            break;
        }
    }

    output.push_str("</span>");
    index
}

fn highlight_number_literal(chars: &[char], mut index: usize, output: &mut String) -> usize {
    output.push_str("<span class='bst-code-number'>");

    // Keep this deliberately narrow for now. The generic highlighter is only
    // meant to recognise obvious numeric runs, not fully parse every literal form.
    while index < chars.len()
        && (chars[index].is_numeric() || chars[index] == '.' || chars[index] == '_')
    {
        push_escaped_char(output, chars[index]);
        index += 1;
    }

    output.push_str("</span>");
    index
}

fn flush_word(output: &mut String, word: &mut String, language: CodeLanguage) {
    if word.is_empty() {
        return;
    }

    let escaped = escape_html(word);

    // Bare identifier runs are classified after the scanner hits a boundary
    // such as whitespace or punctuation. That keeps keyword matching simple and
    // lets generic mode leave unknown identifiers untouched.
    if is_keyword(word, language) {
        output.push_str("<span class='bst-code-keyword'>");
        output.push_str(&escaped);
        output.push_str("</span>");
    } else if is_type_keyword(word, language) {
        output.push_str("<span class='bst-code-type'>");
        output.push_str(&escaped);
        output.push_str("</span>");
    } else if language != CodeLanguage::Generic
        && word.chars().next().is_some_and(|ch| ch.is_uppercase())
    {
        output.push_str("<span class='bst-code-struct'>");
        output.push_str(&escaped);
        output.push_str("</span>");
    } else {
        output.push_str(&escaped);
    }

    word.clear();
}

fn is_keyword(word: &str, language: CodeLanguage) -> bool {
    match language {
        CodeLanguage::Generic => false,
        CodeLanguage::Beanstalk => matches!(
            word,
            "if" | "else"
                | "return"
                | "break"
                | "continue"
                | "loop"
                | "in"
                | "to"
                | "upto"
                | "by"
                | "as"
                | "copy"
        ),
        CodeLanguage::JavaScript => matches!(
            word,
            "if" | "else"
                | "return"
                | "break"
                | "continue"
                | "for"
                | "while"
                | "in"
                | "function"
                | "const"
                | "let"
                | "var"
        ),
        CodeLanguage::TypeScript => matches!(
            word,
            "if" | "else"
                | "return"
                | "break"
                | "continue"
                | "for"
                | "while"
                | "in"
                | "function"
                | "const"
                | "let"
                | "var"
                | "type"
                | "interface"
                | "enum"
        ),
        CodeLanguage::Python => matches!(
            word,
            "if" | "elif"
                | "else"
                | "return"
                | "break"
                | "continue"
                | "for"
                | "while"
                | "in"
                | "def"
                | "class"
                | "import"
                | "from"
                | "as"
        ),
    }
}

fn is_type_keyword(word: &str, language: CodeLanguage) -> bool {
    match language {
        CodeLanguage::Generic => false,
        CodeLanguage::Beanstalk => {
            matches!(
                word,
                "Int" | "Float" | "Bool" | "String" | "None" | "True" | "False"
            )
        }
        CodeLanguage::JavaScript => matches!(word, "true" | "false" | "null" | "undefined"),
        CodeLanguage::TypeScript => matches!(
            word,
            "number"
                | "string"
                | "boolean"
                | "unknown"
                | "never"
                | "void"
                | "any"
                | "true"
                | "false"
                | "null"
                | "undefined"
        ),
        CodeLanguage::Python => matches!(word, "True" | "False" | "None"),
    }
}

fn is_operator_char(ch: char) -> bool {
    matches!(
        ch,
        '=' | ':'
            | '+'
            | '-'
            | '*'
            | '/'
            | '%'
            | '^'
            | '!'
            | '?'
            | '|'
            | '&'
            | '<'
            | '>'
            | '~'
            | '@'
            | '#'
            | '$'
            | '`'
    )
}

fn push_escaped_char(output: &mut String, ch: char) {
    match ch {
        '&' => output.push_str("&amp;"),
        '<' => output.push_str("&lt;"),
        '>' => output.push_str("&gt;"),
        '"' => output.push_str("&quot;"),
        '\'' => output.push_str("&#39;"),
        _ => output.push(ch),
    }
}

fn escape_html(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());

    for ch in text.chars() {
        push_escaped_char(&mut escaped, ch);
    }

    escaped
}
