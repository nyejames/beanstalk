use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::Var;
use crate::compiler_frontend::string_interning::StringId;

pub trait ContainsReferences {
    fn get_reference(&self, name: &StringId) -> Option<&Var>;
    #[allow(dead_code)]
    fn get_reference_mut(&mut self, name: &StringId) -> Option<&mut Var>;
}

impl ContainsReferences for Vec<Var> {
    fn get_reference(&self, name: &StringId) -> Option<&Var> {
        self.iter().rfind(|arg| &arg.id == name)
    }
    fn get_reference_mut(&mut self, name: &StringId) -> Option<&mut Var> {
        self.iter_mut().rfind(|arg| &arg.id == name)
    }
}

impl ContainsReferences for ScopeContext {
    fn get_reference(&self, name: &StringId) -> Option<&Var> {
        self.declarations.iter().rfind(|arg| &arg.id == name)
    }
    fn get_reference_mut(&mut self, name: &StringId) -> Option<&mut Var> {
        self.declarations.iter_mut().rfind(|arg| &arg.id == name)
    }
}
