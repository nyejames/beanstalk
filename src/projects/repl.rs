//! Template-focused REPL helper for experimenting with Beanstalk template syntax.
//!
//! This is not the default CLI entrypoint and it is narrower than a full language REPL: input is
//! tokenized from template-head mode and only compile-time template evaluation is supported today.
use crate::compiler_frontend::ast::ast::{ContextKind, ScopeContext};
use crate::compiler_frontend::ast::templates::create_template_node::Template;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::display_messages::print_formatted_error;
use crate::compiler_frontend::host_functions::HostRegistry;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::tokenizer::tokenizer::tokenize;
use crate::compiler_frontend::tokenizer::tokens::TokenizeMode;
use crate::projects::html_project::style_directives::html_project_style_directives;
use saying::say;
use std::env;
use std::io::{self, Write};
use std::path::Path;

/// Start the REPL session
#[allow(dead_code)] // todo
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
        if let Err(error) = io::stdout().flush() {
            say!(Red "Error flushing prompt: ", error);
            break;
        }

        let current_dir = match env::current_dir() {
            Ok(path) => path,
            Err(error) => {
                say!(Red "Error resolving current directory: ", error);
                break;
            }
        };

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
#[allow(dead_code)] // todo
fn compile_beanstalk_to_string(
    source_code: &str,
    source_path: &Path,
) -> Result<String, CompilerError> {
    use crate::compiler_frontend::interned_path::InternedPath;
    use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
    use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
    use crate::compiler_frontend::string_interning::StringTable;
    use std::path::PathBuf;

    // Create a string table for this compilation
    let mut string_table = StringTable::new();

    // Convert path to interned path
    let interned_path = InternedPath::from_path_buf(source_path, &mut string_table);
    let source_root = source_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let canonical_root = std::fs::canonicalize(&source_root).unwrap_or(source_root);
    let project_path_resolver =
        ProjectPathResolver::new(canonical_root.clone(), canonical_root, &[])?;

    // Tokenize the source code
    let style_directives = StyleDirectiveRegistry::merged(&html_project_style_directives())?;
    let mut tokenizer_output = tokenize(
        source_code,
        &interned_path,
        TokenizeMode::TemplateHead,
        &style_directives,
        &mut string_table,
    )?;
    let ast_context = ScopeContext::new(
        ContextKind::Template,
        interned_path.to_owned(),
        &[],
        HostRegistry::new(&mut string_table),
        Vec::new(),
    )
    .with_project_path_resolver(Some(project_path_resolver))
    .with_source_file_scope(interned_path.to_owned())
    .with_path_format_config(PathStringFormatConfig::default());

    // Build Template
    let template = Template::new(
        &mut tokenizer_output,
        &ast_context,
        vec![],
        &mut string_table,
    )?;

    // The helper only needs the final folded UTF-8 output, so resolve the interned result
    // immediately instead of introducing a separate runtime representation here.
    let mut fold_context =
        ast_context.new_template_fold_context(&mut string_table, "repl template folding")?;
    let template_string = template.fold_into_stringid(&mut fold_context)?;

    Ok(string_table.resolve(template_string).to_string())
}
