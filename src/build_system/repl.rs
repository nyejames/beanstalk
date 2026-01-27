// For running Beanstalk string templates in the REPL,
// Starts in the template head rather than body (unlike .mt files which will start in the body).
// This will ALWAYS return a UTF-8 string

// ONLY DOES COMPILE TIME TEMPLATE ATM.
// Function templates are not yet supported

use crate::compiler::compiler_errors::CompilerError;
use crate::compiler::host_functions::registry::HostRegistry;
use crate::compiler::parsers::ast::{ContextKind, ScopeContext};
use crate::compiler::parsers::statements::create_template_node::Template;
use crate::compiler::parsers::tokenizer::tokenizer::tokenize;
use crate::compiler::parsers::tokenizer::tokens::TokenizeMode;
use std::env;
use std::io::{self, Write};
use std::path::Path;

/// Start the REPL session
pub fn start_repl_session() {
    use crate::compiler::compiler_errors::print_formatted_error;
    use colour::{green_ln_bold, grey_ln, red_ln};

    green_ln_bold!("Beanstalk string template REPL");
    grey_ln!("Enter Beanstalk template snippets.");
    grey_ln!(
        "Type 'exit' to quit. and 'clear' to restart the REPL or type 'show' to see the current code."
    );
    grey_ln!("This starts inside the template head.");
    println!();

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
                red_ln!("Error reading input: {}", e);
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
    use crate::compiler::interned_path::InternedPath;
    use crate::compiler::string_interning::StringTable;

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
        HostRegistry::new(),
        Vec::new(),
    );

    // Build Template
    let template = Template::new(&mut tokenizer_output, &ast_context, None, &mut string_table)?;

    // TODO: put all this into an AST block, then lower it to wasm and run it
    // There is currently no codegen for templates.
    // They need to be lowered to a function that returns a string.
    // Temporary gross function that should work for constants but doesn't atm

    // For now, this will be able to return a string if it can be folded at compile time
    // If not, it will throw an error.
    // Since the repl is purely inside the string template, new variables or functions can't be used anyway.

    // This is super gross as we are interning then resolving immediately
    let template_string = template.fold(&None, &mut string_table)?;

    Ok(string_table.resolve(template_string).to_string())
}
