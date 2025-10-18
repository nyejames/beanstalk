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

use crate::compiler::parsers::build_ast::ScopeContext;
use crate::compiler::parsers::tokens::TokenContext;


// TODO: use this for lowering the tokens in the AST
pub struct AstContext {
    token_stream: TokenContext,
    context: ScopeContext,
}

// It provides the shape and signature of structs and functions for the AST parser to use for type checking.
pub fn parse_declarations(context: &TokenContext) -> AstContext {
    
    while context.index < context.length {
        let current_token = context.current_token_kind();

    }

}