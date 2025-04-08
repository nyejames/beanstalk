use crate::parsers::util::NumericalParsing;

// This function will take a code block and a language and return a highlighted version of the code block.
// It parses the code block then adds spans with classes for each token.
pub fn _highlight_code_block(code_block: &str, language: &String) -> String {
    let mut highlighted_code = String::new();
    let mut chars = code_block.chars().peekable();
    let mut char_scope: Option<char> = None;
    let mut keyword = String::new();

    let comment = match language.as_str() {
        "js" | "javascript" => "//",
        "python" | "py" => "#",
        _ => "--",
    };
    let comm_len = comment.chars().count();

    // Figure out how many indentations there are at the start of the code block
    let mut indentations = 0;
    while let Some(c) = chars.peek() {
        if c == &'\n' {
            chars.next();
            indentations = 0;
            continue;
        }

        if c.is_whitespace() {
            indentations += 1;
            chars.next();
        } else {
            break;
        }
    }

    while let Some(c) = chars.peek() {
        if char_scope.is_some() {
            highlighted_code.push(*c);
            if c == &char_scope.unwrap() {
                highlighted_code.push_str("</span>");
                char_scope = None;
            }
            chars.next();
            continue;
        }

        match c {
            c if match &comment.chars().nth(0) {
                Some(ch) => c == ch,
                None => false,
            } =>
            {
                // Check if next nth characters are the same as the comment
                let mut comment_char_matches = 0;

                'outer: while let Some(c) = chars.peek() {
                    if match &comment.chars().nth(comment_char_matches) {
                        Some(ch) => c == ch,
                        None => false,
                    } {
                        comment_char_matches += 1;
                        // If it doesn't end up matching, we'll need to highlight all of these normally
                        // So storing them in keyword
                        keyword.push(*c);

                        if comment_char_matches == comm_len {
                            // Now we know it's a comment we can start adding everything to the block
                            highlighted_code
                                .push_str(&format!("<span class='bs-code-comment'>{keyword}"));

                            // Add chars until first newline, then break out of comment
                            // Will also break out if it's the end of the
                            chars.next();
                            while let Some(c) = chars.peek() {
                                if c == &'\n' {
                                    highlighted_code.push_str("</span>");
                                    break 'outer;
                                }
                                highlighted_code.push(*c);
                                chars.next();
                            }
                        }

                        chars.next();
                    } else {
                        // If it didn't end up matching the comment,
                        // The characters need to be highlighted normally and added to the block
                        // keyword.push(*c);
                        let not_comment_string = _highlight_code_block(&keyword, language);
                        highlighted_code.push_str(&not_comment_string);
                        break 'outer;
                    }
                }

                keyword.clear();
                continue;
            }
            '=' | ':' | '+' | '*' | '/' | '%' | '^' | '!' | '?' | '|' | '&' | '<' | '>' | '~'
            | '@' | '#' | '$' | '`' => highlighted_code.push_str(&format!(
                "{keyword}<span class='bs-code-operator'>{c}</span>"
            )),
            c if c.is_bracket() && keyword.is_empty() => highlighted_code.push_str(&format!(
                "{keyword}<span class='bs-code-parenthesis'>{c}</span>"
            )),
            '"' | '\'' => {
                highlighted_code.push_str(&format!("{keyword}<span class='bs-code-string'>\""));
                char_scope = Some(*c);
            }
            c if c.is_number_operation_char()
                && match keyword.chars().last() {
                    Some(ch) => {
                        ch.is_whitespace() || ch.is_number_operation_char() || ch.is_bracket()
                    }
                    None => true,
                } =>
            {
                highlighted_code
                    .push_str(&format!("{keyword}<span class='bs-code-number'>{c}</span>"))
            }
            c if (c.is_whitespace() || c.is_bracket()) && !keyword.is_empty() => {
                if keyword_is_in_language(keyword.as_str(), language) {
                    highlighted_code
                        .push_str(&format!("<span class='bs-code-keyword'>{keyword}</span>"));
                } else if type_is_in_language(keyword.as_str(), language) {
                    highlighted_code
                        .push_str(&format!("<span class='bs-code-type'>{keyword}</span>"));
                } else {
                    if keyword.chars().nth(0).is_some_and(|c| c.is_uppercase()) {
                        highlighted_code
                            .push_str(&format!("<span class='bs-code-struct'>{keyword}</span>"));
                    } else {
                        highlighted_code.push_str(&format!("{keyword}"));
                    }
                }
                keyword.clear();
                continue;
            }
            '\n' => {
                highlighted_code.push_str(&format!("{keyword}\n"));
                keyword.clear();

                // Check if the next n whitespace characters are the same as the indentations
                // If they are, remove them
                let mut whitespace = 0;
                chars.next();
                while let Some(c) = chars.peek() {
                    if c._is_non_newline_whitespace() && whitespace <= indentations {
                        whitespace += 1;
                        chars.next();
                    } else {
                        break;
                    }
                }
                continue;
            }
            _ => {
                keyword.push(*c);
                chars.next();
                continue;
            }
        }

        keyword.clear();
        chars.next();
    }

    highlighted_code
}

fn keyword_is_in_language(keyword: &str, language: &str) -> bool {
    match language {
        "js" | "javascript" => match keyword {
            "if" | "else" | "while" | "for" | "return" | "break" | "continue" | "in" => true,
            _ => false,
        },
        _ => match keyword {
            "if" | "else" | "while" | "return" | "break" | "continue" | "loop" | "end"
            | "defer" | "panic" | "print" | "in" | "as" => true,
            _ => false,
        },
    }
}

fn type_is_in_language(type_: &str, language: &str) -> bool {
    match language {
        _ => match type_ {
            "Int" | "Float" | "Unit" | "Bool" | "String" | "Scene" | "Choice" | "choice"
            | "copy" | "Type" | "Error" | "Style" | "Path" | "True" | "False" | "true"
            | "false" | "fn" | "type" => true,
            _ => false,
        },
    }
}
