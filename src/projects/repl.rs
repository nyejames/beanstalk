// This is early prototype code, so ignore placeholder unused stuff for now
#![allow(unused)]

// For running Beanstalk string templates in the REPL,
// Starts in the template head rather than body (unlike .mt files which will start in the body).
// This will ALWAYS return a UTF-8 string

// ONLY DOES COMPILE TIME TEMPLATE ATM.
// Function templates are not yet supported

use crate::backends::function_registry::HostRegistry;
use crate::compiler_frontend::ast::ast::{ContextKind, ScopeContext};
use crate::compiler_frontend::ast::templates::create_template_node::Template;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::display_messages::print_formatted_error;
use crate::compiler_frontend::tokenizer::tokenizer::tokenize;
use crate::compiler_frontend::tokenizer::tokens::TokenizeMode;
use saying::say;
use std::env;
use std::io::{self, Write};
use std::path::Path;

/// Start the REPL session
pub fn start_repl_session() {
    say!("Beanstalk string template REPL");
    say!(Green "Enter Beanstalk template snippets.");
    say!(Bright Black
        "Type 'exit' to quit. and 'clear' to restart the REPL or type 'show' to see the current code."
    );
    say!(Bright Black "This starts inside the template head. \n");

    // Just to avoid extra allocations, memory will not be much of a constraint in the repl (I think)
    const EXPECTED_INPUT_LENGTH: usize = 30;
    let mut code = String::with_capacity(EXPECTED_INPUT_LENGTH);

    loop {
        print!(">>> ");
        io::stdout().flush().unwrap();

        let current_dir = env::current_dir().unwrap();

        let mut new_code = String::new();
        match io::stdin().read_line(&mut new_code) {
            Ok(_) => {
                if new_code.trim() == "exit" {
                    println!("Closing REPL session.");
                }

                if new_code.trim() == "clear" {
                    code.clear();
                    continue;
                }

                if new_code.trim() == "show" {
                    println!("{code}");
                    continue;
                }

                let next_code = format!("{code}{new_code}");

                // Compile and execute the input
                match compile_beanstalk_to_string(&next_code, &current_dir) {
                    Ok(result) => {
                        println!("{result}");
                        code.push_str(&new_code);
                    }
                    Err(e) => {
                        print_formatted_error(e);
                    }
                }
            }
            Err(e) => {
                say!(Red "Error reading input: ", e);
                break;
            }
        }
    }
}

/// Compile Beanstalk source code to a string
fn compile_beanstalk_to_string(
    source_code: &str,
    source_path: &Path,
) -> Result<String, CompilerError> {
    use crate::compiler_frontend::interned_path::InternedPath;
    use crate::compiler_frontend::string_interning::StringTable;

    // Create a string table for this compilation
    let mut string_table = StringTable::new();

    // Convert path to interned path
    let interned_path = InternedPath::from_path_buf(source_path, &mut string_table);

    // Tokenize the source code
    let mut tokenizer_output = tokenize(
        source_code,
        &interned_path,
        TokenizeMode::TemplateHead,
        &mut string_table,
    )?;
    let ast_context = ScopeContext::new(
        ContextKind::Template,
        interned_path,
        &[],
        HostRegistry::new(&mut string_table),
        Vec::new(),
    );

    // Build Template
    let template = Template::new(
        &mut tokenizer_output,
        &ast_context,
        vec![],
        &mut string_table,
    )?;

    // TODO: INSTEAD OF ALL THIS WAIT UNTIL RUST INTERPRETER IS DONE

    // This is super gross as we are interning then resolving immediately
    let template_string = template.fold_into_stringid(&None, &mut string_table)?;

    Ok(string_table.resolve(template_string).to_string())
}
