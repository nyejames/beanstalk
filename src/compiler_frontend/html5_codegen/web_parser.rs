use crate::compiler::compiler_errors::ErrorType;

use super::js_parser::{expression_to_js, expressions_to_js};
use crate::compiler::compiler_errors::CompileError;
use crate::compiler::html5_codegen::js_parser::combine_vec_to_js;
use crate::compiler::parsers::ast_nodes::{Arg, AstNode};
use crate::compiler::string_interning::StringTable;
use crate::settings::Config;
use crate::{
    compiler::datatypes::DataType, compiler::parsers::ast_nodes::NodeKind, return_compiler_error,
    return_type_error, settings::BS_VAR_PREFIX,
};

pub const JS_INDENT: &str = "    ";

pub struct ParserOutput {
    // For web this would be what's written to the HTML file
    pub content_output: String,

    // This is JS atm, but will be Wasm in the future
    pub js: String,

    pub css: String,
}

// Parse ast into valid WAT, JS, HTML and CSS
pub fn parse_to_html5(
    ast: &[AstNode],
    indentation: &str,
    config: &Config,
    string_table: StringTable,
) -> Result<ParserOutput, CompileError> {
    let mut code_module = String::new();
    let mut content_output = String::new();
    let mut css = String::new();
    // let mut page_title = String::new();

    // Parse the Markthrough file into HTML and JS.
    // Eventually we can separate this out to Wasm
    // CSS is a bit separate as it will be written directly into template styles as a string, or imported directly
    for node in ast {
        match &node.kind {
            // JAVASCRIPT / WASM
            NodeKind::VariableDeclaration(arg) => {
                code_module.push_str(&format!("\n{indentation}"));

                let declaration_keyword = if arg.value.ownership.is_mutable() {
                    "let"
                } else {
                    "const"
                };

                let mut declaration = format!(
                    "{} {declaration_keyword} = {};",
                    string_table.resolve(arg.id),
                    expression_to_js(&arg.value, &string_table)?
                );

                declaration.push(';');
            }

            NodeKind::FunctionCall(name, arguments, ..) => {
                code_module.push_str(indentation);
                code_module.push_str(&format!(
                    "{BS_VAR_PREFIX}{}({})",
                    name,
                    combine_vec_to_js(arguments)?
                ));
            }

            NodeKind::Return(expressions, ..) => {
                code_module.push_str(&format!("\n{indentation}"));
                code_module.push_str(&format!("return {};", expressions_to_js(expressions, "")?));
            }

            NodeKind::If(condition, if_block_body, ..) => {
                code_module.push_str(&format!("\n\n{indentation}"));

                code_module.push_str(&format!(
                    "if ({}) {{\n{}\n{indentation}}}\n{indentation}",
                    expression_to_js(condition, "")?,
                    parse_to_html5(
                        &if_block_body.ast,
                        &format!("{indentation}{JS_INDENT}"),
                        target
                    )?
                    .js
                ));
            }

            // DIRECT INSERTION OF JS / CSS into the page
            NodeKind::JS(js_string, ..) => {
                code_module.push_str(js_string);
            }

            NodeKind::Css(css_string, ..) => {
                css.push_str(css_string);
            }

            NodeKind::ForLoop(index, iterated_item, loop_body, ..) => {
                code_module.push_str(&format!("\n\n{indentation}"));

                let length_access = if iterated_item.kind.is_iterable() {
                    ".length()"
                } else {
                    ""
                };

                code_module.push_str(&format!(
                    "for (let {BS_VAR_PREFIX}{} = 0; {BS_VAR_PREFIX}{} < {}{}; {BS_VAR_PREFIX}{}++) {{{}\n{indentation}}}\n{indentation}",
                    index.name,
                    index.name,
                    expression_to_js(iterated_item, "")?,
                    length_access,
                    index.name,
                    parse_to_html5(&loop_body.ast, &format!("{indentation}{JS_INDENT}"), target)?.js
                ));
            }

            NodeKind::WhileLoop(condition, loop_body, ..) => {
                code_module.push_str(&format!("\n\n{indentation}"));

                code_module.push_str(&format!(
                    "while ({}) {{\n{}\n{indentation}}}\n{indentation}",
                    expression_to_js(condition, "")?,
                    parse_to_html5(&loop_body.ast, &format!("{indentation}{JS_INDENT}"), target)?
                        .js
                ));
            }

            NodeKind::Expression(expr, ..) => {
                code_module.push_str(&expression_to_js(expr, "")?);
            }

            // Ignore Everything Else
            _ => {
                return_compiler_error!(
                    "Unexpected node type: {:?}. Compiler bug, should be caught at earlier stage",
                    node
                )
            }
        }
    }

    // if config.html_meta.auto_site_title {
    //     page_title += &(" | ".to_owned() + &config.html_meta.site_title.clone());
    // }

    Ok(ParserOutput {
        content_output,
        js: code_module,
    })
}
