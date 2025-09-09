// For running Beanstalk string templates in the REPL,
// but starts in the template head, rather than body (like .mt files).
// This will ALWAYS return a UTF-8 string
// NOT REALLY WORKING YET - JUST SOME SCAFFOLDING

use std::collections::HashMap;
use std::env;
use crate::build_system::build_system::{BuildTarget, ProjectBuilder};
use crate::compiler::compiler_errors::CompileError;
use crate::settings::Config;
use crate::{Flag, InputModule, OutputFile, Project};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::time::Instant;
use crate::compiler::parsers::ast_nodes::{AstNode, NodeKind};
use crate::compiler::parsers::build_ast::AstBlock;
use crate::compiler::parsers::expressions::expression::Expression;
use crate::compiler::parsers::statements::create_template_node::new_template;
use crate::compiler::parsers::template::Style;
use crate::compiler::parsers::tokens::TokenizeMode;

/// Start the REPL session
pub fn start_repl_session() {
    use crate::compiler::compiler_errors::print_formatted_error;
    use colour::{green_ln_bold, grey_ln, red_ln};

    green_ln_bold!("Beanstalk string template REPL");
    grey_ln!("Enter Beanstalk template snippets. Type 'exit' to quit.");
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
                    break;
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
) -> Result<String, CompileError> {
    use crate::compiler::parsers::build_ast::{ContextKind, ScopeContext};
    use crate::compiler::parsers::tokenizer;

    // Tokenize the source code
    let mut tokenizer_output = tokenizer::tokenize(source_code, source_path, TokenizeMode::TemplateHead)?;
    let ast_context = ScopeContext::new(ContextKind::Template, source_path.to_path_buf(), &[]);

    // Build Template
    let mut template = new_template(
        &mut tokenizer_output,
        &ast_context,
        &mut HashMap::new(),
        &mut Style::default(),
    )?;

    let template_string = template.parse_into_string(
        None,
        &tokenizer_output.current_location(),
    )?;

    Ok(template_string)
}