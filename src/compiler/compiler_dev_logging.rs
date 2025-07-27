use crate::compiler::datatypes::DataType;
use crate::compiler::parsers::ast_nodes::{AstNode, NodeKind};
use crate::compiler::parsers::expressions::expression::ExpressionKind;
use colour::{blue_ln, blue_ln_bold, cyan_ln, green_bold, green_ln, green_ln_bold};

// TOKEN LOGGING MACROS
#[macro_export]
#[cfg(feature = "show_tokens")]
macro_rules! token_log {
    ($token:expr) => {
        eprintln!("{}", $token.to_string())
    };
}

#[macro_export]
#[cfg(not(feature = "show_tokens"))]
macro_rules! token_log {
    ($tokens:expr) => {
        // Nothing
    };
}

// AST LOGGING MACROS
#[macro_export]
#[cfg(feature = "verbose_ast_logging")]
macro_rules! ast_log {
    ($($arg:tt)*) => {
        eprintln!($($arg)*);
    };
}

#[macro_export]
#[cfg(not(feature = "verbose_ast_logging"))]
macro_rules! ast_log {
    ($($arg:tt)*) => {
        // Nothing
    };
}

// EVAL LOGGING MACROS
#[macro_export]
#[cfg(feature = "verbose_eval_logging")]
macro_rules! eval_log {
    ($($arg:tt)*) => {
        eprintln!($($arg)*);
    };
}

#[macro_export]
#[cfg(not(feature = "verbose_eval_logging"))]
macro_rules! eval_log {
    ($($arg:tt)*) => {
        // Nothing
    };
}

// CODEGEN LOGGING MACROS
#[macro_export]
#[cfg(feature = "verbose_codegen_logging")]
macro_rules! codegen_log {
    ($($arg:tt)*) => {
        eprintln!($($arg)*);
    };
}

#[macro_export]
#[cfg(not(feature = "verbose_codegen_logging"))]
macro_rules! codegen_log {
    ($($arg:tt)*) => {
        // Nothing
    };
}

pub fn print_ast_output(ast: &[AstNode]) {
    for node in ast {
        match &node.kind {
            NodeKind::Reference(value) => match value.data_type {
                DataType::Template(_) => {
                    print_template(&value.kind, 0);
                }
                _ => {
                    cyan_ln!("{:?}", value);
                }
            },
            NodeKind::Comment(..) => {
                // grey_ln!("{:?}", node);
            }
            NodeKind::Declaration(name, expr, ..) => {
                blue_ln!("Variable: {:?}", name);
                green_ln_bold!("Expr: {:#?}", expr);
            }
            NodeKind::FunctionCall(name, args, ..) => {
                blue_ln!("Function Call: {:?}", name);
                green_bold!("Arguments: ");
                for (i, arg) in args.iter().enumerate() {
                    green_ln_bold!("    {}: {:?}", i, arg);
                }
            }
            _ => {
                println!("{:?}", node);
            }
        }
        println!("\n");
    }

    fn print_template(scene: &ExpressionKind, template_nesting_level: u32) {
        // Indent the scene by how nested it is
        let mut indentation = String::new();
        for _ in 0..template_nesting_level {
            indentation.push('\t');
        }

        if let ExpressionKind::Template(nodes, style, ..) = scene {
            blue_ln_bold!("\n{}Scene Styles: ", indentation);

            green_ln!("{}  {:?}", indentation, style.format);
            green_ln!("{}  {:?}", indentation, style.child_default);
            green_ln!("{}  {:?}", indentation, style.unlocked_templates);

            blue_ln_bold!("{}Scene Body:", indentation);

            for scene_node in nodes.flatten() {
                println!("{}  {:?}", indentation, scene_node);
            }
        }
    }
}
