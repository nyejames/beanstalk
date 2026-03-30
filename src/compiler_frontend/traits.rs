//! Small shared traits used by frontend scope and declaration lookups.
//!
//! These helpers centralize common "find the most recent visible declaration" behavior across
//! AST-time containers.

use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::string_interning::StringId;

pub trait ContainsReferences {
    fn get_reference(&self, name: &StringId) -> Option<&Declaration>;
    #[allow(dead_code)] // Reserved for future AST rewrites that mutate declarations in place
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
        self.declarations.iter().rfind(|declaration| {
            declaration.id.name() == Some(*name)
                && !matches!(
                    &declaration.value.data_type,
                    DataType::Function(receiver, _) if receiver.as_ref().is_some()
                )
                && match self.visible_declaration_ids.as_ref() {
                    Some(visible) => visible.contains(&declaration.id),
                    None => true,
                }
        })
    }
    fn get_reference_mut(&mut self, name: &StringId) -> Option<&mut Declaration> {
        let visible = self.visible_declaration_ids.clone();
        self.declarations.iter_mut().rfind(|declaration| {
            declaration.id.name() == Some(*name)
                && !matches!(
                    &declaration.value.data_type,
                    DataType::Function(receiver, _) if receiver.as_ref().is_some()
                )
                && match visible.as_ref() {
                    Some(allowed) => allowed.contains(&declaration.id),
                    None => true,
                }
        })
    }
}
