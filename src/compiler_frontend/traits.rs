use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::string_interning::StringId;

pub trait ContainsReferences {
    fn get_reference(&self, name: &StringId) -> Option<&Declaration>;
    #[allow(dead_code)]
    fn get_reference_mut(&mut self, name: &StringId) -> Option<&mut Declaration>;
}

impl ContainsReferences for Vec<Declaration> {
    fn get_reference(&self, name: &StringId) -> Option<&Declaration> {
        self.iter().rfind(|arg| arg.id.name() == Some(*name))
    }
    fn get_reference_mut(&mut self, name: &StringId) -> Option<&mut Declaration> {
        self.iter_mut().rfind(|arg| arg.id.name() == Some(*name))
    }
}

impl ContainsReferences for ScopeContext {
    fn get_reference(&self, name: &StringId) -> Option<&Declaration> {
        self.declarations
            .iter()
            .rfind(|arg| arg.id.name() == Some(*name))
    }
    fn get_reference_mut(&mut self, name: &StringId) -> Option<&mut Declaration> {
        self.declarations
            .iter_mut()
            .rfind(|arg| arg.id.name() == Some(*name))
    }
}
