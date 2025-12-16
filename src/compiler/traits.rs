use crate::compiler::parsers::ast::ScopeContext;
use crate::compiler::parsers::ast_nodes::Arg;
use crate::compiler::string_interning::StringId;

pub trait ContainsReferences {
    fn get_reference(&self, name: &StringId) -> Option<&Arg>;
    #[allow(dead_code)]
    fn get_reference_mut(&mut self, name: &StringId) -> Option<&mut Arg>;
}

impl ContainsReferences for Vec<Arg> {
    fn get_reference(&self, name: &StringId) -> Option<&Arg> {
        self.iter().rfind(|arg| &arg.id == name)
    }
    fn get_reference_mut(&mut self, name: &StringId) -> Option<&mut Arg> {
        self.iter_mut().rfind(|arg| &arg.id == name)
    }
}

impl ContainsReferences for ScopeContext {
    fn get_reference(&self, name: &StringId) -> Option<&Arg> {
        self.declarations.iter().rfind(|arg| &arg.id == name)
    }
    fn get_reference_mut(&mut self, name: &StringId) -> Option<&mut Arg> {
        self.declarations.iter_mut().rfind(|arg| &arg.id == name)
    }
}
