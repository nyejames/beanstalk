use crate::compiler::parsers::ast_nodes::Arg;
use crate::compiler::parsers::build_ast::ScopeContext;

pub trait ContainsReferences {
    fn get_reference(&self, name: &str) -> Option<&Arg>;
    #[allow(dead_code)]
    fn get_reference_mut(&mut self, name: &str) -> Option<&mut Arg>;
}

impl ContainsReferences for Vec<Arg> {
    fn get_reference(&self, name: &str) -> Option<&Arg> {
        self.iter().rfind(|arg| arg.name == name)
    }
    fn get_reference_mut(&mut self, name: &str) -> Option<&mut Arg> {
        self.iter_mut().rfind(|arg| arg.name == name)
    }
}

impl ContainsReferences for ScopeContext {
    fn get_reference(&self, name: &str) -> Option<&Arg> {
        self.declarations.iter().rfind(|arg| arg.name == name)
    }
    fn get_reference_mut(&mut self, name: &str) -> Option<&mut Arg> {
        self.declarations.iter_mut().rfind(|arg| arg.name == name)
    }   
}
