//! AST stage modules for module-wide typed syntax construction.
//!
//! WHAT: groups expression/statement parsing, header-to-AST lowering, and template AST handling.

pub(crate) mod signatures;
pub(crate) mod module_ast;
pub(crate) use module_ast as ast;
pub(crate) mod ast_nodes;
pub(crate) mod function_body_to_ast;
pub(crate) mod import_bindings;
pub(crate) mod receiver_methods;
pub(crate) mod type_resolution;
pub(crate) mod expressions {
    pub(crate) mod call_argument;
    pub(crate) mod call_validation;
    pub(crate) mod eval_expression;
    pub(crate) mod expression;
    pub(crate) mod function_calls;
    pub(crate) mod mutation;
    pub(crate) mod parse_expression;
    pub(crate) mod struct_instance;
}
pub(crate) mod statements {
    pub(crate) mod branching;
    pub(crate) mod choices;
    pub(crate) mod collections;
    pub(crate) mod declaration_syntax;
    pub(crate) mod declarations;
    pub(crate) mod functions;
    pub(crate) mod loops;
    pub(crate) mod multi_bind;
    pub(crate) mod result_handling;
    pub(crate) mod structs;
}
pub(crate) mod field_access;
pub(crate) mod templates;
#[cfg(test)]
pub(crate) mod test_support;
