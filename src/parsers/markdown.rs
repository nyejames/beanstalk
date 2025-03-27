use std::str::Chars;

// Custom flavoured Markdown parser
#[derive(PartialEq, Debug, Clone)]
pub enum MarkdownContext {
    None,
    Default, // Usually P tag. Could also be a list item or something
    Heading(u32),

    // Bool is false if it's inside a P tag
    // If not, this is a naked emphasis tag
    Em(i32)
}

// Only very basics (atm): P, Headings, Bold, Italics
// May add some more later
pub fn to_markdown(content: &str, default_tag: &str) -> String {
    let mut context = MarkdownContext::None;
    const NEWLINES_BEFORE_NEW_P: usize = 2;
    const NEWLINES_BEFORE_BREAK: usize = 3;

    let chars: Chars = content.chars();
    let mut output = String::new();

    // Headings must be at the start of the line
    // So we'll keep track of when we're at the start of a line
    // Any amount of indentation or tabs at the start of a line will be ignored
    let mut newlines = 0;
    let mut prev_whitespace = false;

    // Keeping track of how strong the special context is
    let mut heading_strength = 0;

    // If negative, then it's inside an emphasis tag and tracking the closing count
    let mut em_strength: i32 = 0;

    let mut skip_parsing = false;

    for ch in chars {

        // Special object replace character that signals to ignore parsing a section into markdown
        // This is used to ignore nested scenes that have already been parsed
        // And may not be markdown. e.g. raw strings
        if ch == '\u{FFFC}' {
            skip_parsing = !skip_parsing;
            continue;
        }
        // // Codeblock indicator character (invisible multiply)
        // if ch == '\u{2062}' {
        //     if !skip_parsing {
        //         output.push_str("</code>");
        //         skip_parsing = true;
        //     } else {
        //         output.push_str("<code>");
        //         skip_parsing = false;
        //     }
        //     continue;
        // }
        if skip_parsing {
            output.push(ch);
            continue;
        }


        // HANDLING WHITESPACE
        // Ignore indentation on newlines
        if ch == '\t' || ch == ' ' {
            prev_whitespace = true;

            // Break out of em tags if it hasn't started yet
            // Must have the * immediately before the first character and after a space
            if em_strength > 0 {
                em_strength = 0;
            }

            // If spaces are after a newline, ignore them?
            // if newlines > 0 {
            //     continue
            // }

            // We are now making a heading
            if heading_strength > 0 {
                output.push_str(&format!("<h{}>", heading_strength));
                context = MarkdownContext::Heading(heading_strength);
                heading_strength = 0;
            } else{
                output.push(ch);
            }

            continue
        }

        // Check for new lines
        if ch == '\n' {
            newlines += 1;
            prev_whitespace = true;

            // Newlines are stripped from the output
            // But if we build up enough of them, we need to add a break tag
            if newlines >= NEWLINES_BEFORE_BREAK {
                output.push_str("<br>");

                // Bring the newlines back to 1
                // As this is still considered a newline
                newlines = 1;
            }

           // Stop making our heading
           // Go back to P tag mode
           if let MarkdownContext::Heading(strength) = context {
               output.push_str(&format!("</h{}>", strength));
               context = MarkdownContext::None;
           }

           if let MarkdownContext::Default = context {
               // Close this P tag and start another one
               // If there are at least 2 newlines after the P tag
               if newlines >= NEWLINES_BEFORE_NEW_P {
                   output.push_str(&format!("</{default_tag}>"));
                   context = MarkdownContext::None;
               } else {
                   // Otherwise just add a space
                   // This is so you don't have to add a space before newlines in P tags
                   output.push(' ');
               }
           }

            continue
        }

        // HANDLING SPECIAL CHARACTERS

        // New heading
        // Don't switch context to heading until finished getting strength
        if ch == '#' && newlines > 0 {
            heading_strength += 1;
            prev_whitespace = false;
            newlines = 0;
            continue
        }

        if ch == '*' {
            // Already in emphasis
            // How negative the em strength is the number of consecutive * while inside an emphasis tag
            if let MarkdownContext::Em(strength) = context {
                em_strength -= 1;

                if strength == em_strength.abs() {
                    output.push_str(em_tag_strength(strength, true));

                    context = MarkdownContext::Default;


                    prev_whitespace = false;
                    em_strength = 0;
                }

                continue

            } else if prev_whitespace && em_strength >= 0 {
                // Possible new emphasis tag
                em_strength += 1;
                newlines = 0;

                continue
            }
        }

        // Start a new emphasis tag
        // Only resets if em_strength is positive so tags can be closed
        if em_strength > 0 {
            if let MarkdownContext::Default = context {
                context = MarkdownContext::Em(em_strength);
                output.push_str(em_tag_strength(em_strength, false));
            }

            if let MarkdownContext::None = context {
                context = MarkdownContext::Em(em_strength);
                output.push_str(&format!("<{default_tag}>{}", em_tag_strength(em_strength, false)));
            }

            em_strength = 0;
        }

        // If nothing else special has happened, and we are not inside a P tag
        // Then start a new P tag
        if context == MarkdownContext::None {
            // if prev_whitespace {
            //     output.push_str("&nbsp;");
            // }

            output.push_str(&format!("<{default_tag}>"));
            context = MarkdownContext::Default;
        }

        // Escape HTML characters that might lead to accidental HTML injection
        // You can't write HTML in this flavour of markdown
        if ch == '<' {
            output.push_str("&lt;");
            continue;
        }
        if ch == '>' {
            output.push_str("&gt;");
            continue;
        }
        if ch == '&' {
            output.push_str("&amp;");
            continue;
        }
        if ch == '"' {
            output.push_str("&quot;");
            continue;
        }
        if ch == '\'' {
            output.push_str("&#39;");
            continue;
        }

        // If it's fallen through then strengths and newlines can be reset

        // If heading strength or emphasis is positive (or negative for emphasis)
        // Before it's reset, those characters need to be added to the output
        if heading_strength > 0 {
            output.push_str(&"#".repeat(heading_strength as usize).to_string());
        }

        if em_strength != 0 {
            output.push_str(&"*".repeat(em_strength.unsigned_abs() as usize).to_string());
        }

        newlines = 0;
        heading_strength = 0;
        prev_whitespace = false;
        output.push(ch);
    }

    // Close off final tag if needed
    match context {
        MarkdownContext::Default => {
            output.push_str(&format!("</{default_tag}>"));
        },

        MarkdownContext::Heading(strength) => {
            output.push_str(&format!("</h{strength}>"));
        },

        MarkdownContext::Em(strength) => {
            output.push_str(em_tag_strength(strength, true));
        },

        MarkdownContext::None => {}
    }

    output
}

fn em_tag_strength(strength: i32, closing: bool) -> &'static str {
    if closing {
        match strength {
            2 => "</strong>",
            3 => "</em></strong>",
            _ => "</em>",
        }
    } else {
        match strength {
            2 => "<strong>",
            3 => "<em><strong>",
            _ => "<em>",
        }
    }
}