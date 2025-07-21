use crate::compiler::parsers::ast_nodes::Arg;
use crate::compiler::parsers::build_ast::ScopeContext;

/// Get Reference
/// args, symbol being searched
/// $args.iter().rfind(|arg| arg.name == $name)
macro_rules! get_reference {
    ($args:expr, $name:expr) => {
        $args.iter().rfind(|arg| arg.name == $name)
    };
}

pub trait ContainsReferences {
    fn find_reference(&self, name: &str) -> Option<&Arg>;
}

impl ContainsReferences for Vec<Arg> {
    fn find_reference(&self, name: &str) -> Option<&Arg> {
        get_reference!(self, name)
    }
}

impl ContainsReferences for ScopeContext {
    fn find_reference(&self, name: &str) -> Option<&Arg> {
        get_reference!(self.declarations, name)
    }
}
