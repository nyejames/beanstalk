use std::iter::Peekable;
use std::str::Chars;
use crate::Token;

// Ignores everything except for the closing brackets
// If there is a greater number of closing brackets than opening brackets,
// Close the codeblock and return the token
pub fn tokenize_codeblock(chars: &mut Peekable<Chars>, line_number:  &mut u32, char_column: &mut u32) -> Token {

    // Codeblock indicator character (invisible multiply)
    // This is used to signal to the Markdown parser that this is a codeblock
    let mut codeblock = String::new();
    let mut brackets = 1;
    let mut raw_mode = false;

    while let Some(ch) = chars.peek() {
        match *ch {
            '[' => {
                if !raw_mode {
                    brackets += 1;
                }
            }
            ']' => {
                if !raw_mode {
                    brackets -= 1;
                }
                
                // Skips adding the final closing bracket to the codeblock
                if brackets == 0 {
                    break;
                }
            }
            '`' => {
                raw_mode = !raw_mode;
            }
            '\n' => {
                *line_number += 1;
                *char_column = 1;
            }
            _ => {
                *char_column += 1;
            }
        }

        codeblock.push(*ch);
        chars.next();
    }

    // codeblock.push('\u{2062}');

    Token::StringLiteral(codeblock)
}
