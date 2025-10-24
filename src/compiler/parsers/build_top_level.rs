// This function takes in a TokenContext from the tokenizer for every file in the module.
// It sorts each type of declaration in the top level scope into:
// - Functions
// - Structs
// - Choices (not yet implemented)
// - Constants
// - Globals (static mutable variables)
// - Implicit Main Function (any other logic in the top level scope implicitly becomes an init function)

// Everything at the top level of a file is visible to the whole module.
// Imports are already parsed in the tokenizer.

use crate::compiler::compiler_errors::CompileError;
use crate::compiler::datatypes::DataType;
use crate::compiler::parsers::ast_nodes::{Arg, AstNode, NodeKind};
use crate::compiler::parsers::build_ast::{AstBlock, ContextKind, ParserOutput, ScopeContext};
use crate::compiler::parsers::builtin_methods::get_builtin_methods;
use crate::compiler::parsers::expressions::mutation::handle_mutation;
use crate::compiler::parsers::expressions::parse_expression::create_multiple_expressions;
use crate::compiler::parsers::statements::branching::create_branch;
use crate::compiler::parsers::statements::functions::parse_function_call;
use crate::compiler::parsers::statements::loops::create_loop;
use crate::compiler::parsers::statements::structs::create_struct_definition;
use crate::compiler::parsers::statements::variables::new_arg;
use crate::compiler::parsers::tokens::{TokenContext, TokenKind, VarVisibility};
use crate::compiler::traits::ContainsReferences;
use crate::tokenizer::END_SCOPE_CHAR;
use crate::{ast_log, return_compiler_error, return_rule_error, return_syntax_error, settings};

pub struct Block {
    body: TokenContext,
    context: ScopeContext,
}

pub enum BlockKind {
    Function,
    StructDefinition,
    Init,
}
