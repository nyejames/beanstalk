//! Stable top-level declaration table for AST environment construction.
//!
//! WHAT: stores one slot per top-level declaration discovered by the header/dependency stages.
//! WHY: AST environment construction updates placeholders in place as declarations are resolved,
//! so body emission and type resolution can share one indexed declaration source without
//! reconstructing lookup indexes.

use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringId;
use rustc_hash::{FxHashMap, FxHashSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(in crate::compiler_frontend::ast) struct DeclarationId(u32);

#[derive(Debug)]
pub(crate) struct TopLevelDeclarationTable {
    declarations: Vec<Declaration>,
    by_path: FxHashMap<InternedPath, DeclarationId>,
    by_name: FxHashMap<StringId, Vec<DeclarationId>>,
}

impl TopLevelDeclarationTable {
    pub(crate) fn new(declarations: Vec<Declaration>) -> Self {
        let mut by_path = FxHashMap::default();
        let mut by_name: FxHashMap<StringId, Vec<DeclarationId>> = FxHashMap::default();

        for (index, declaration) in declarations.iter().enumerate() {
            let declaration_id = DeclarationId(index as u32);
            by_path.insert(declaration.id.to_owned(), declaration_id);
            if let Some(name) = declaration.id.name() {
                by_name.entry(name).or_default().push(declaration_id);
            }
        }

        Self {
            declarations,
            by_path,
            by_name,
        }
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = &Declaration> {
        self.declarations.iter()
    }

    pub(crate) fn get_by_path(&self, path: &InternedPath) -> Option<&Declaration> {
        self.by_path
            .get(path)
            .and_then(|declaration_id| self.get_by_id(*declaration_id))
    }

    pub(in crate::compiler_frontend::ast) fn get_mut_by_path(
        &mut self,
        path: &InternedPath,
    ) -> Option<&mut Declaration> {
        let declaration_id = *self.by_path.get(path)?;
        self.declarations.get_mut(declaration_id.index())
    }

    pub(in crate::compiler_frontend::ast) fn replace_by_path(
        &mut self,
        declaration: Declaration,
    ) -> Option<DeclarationId> {
        let declaration_id = *self.by_path.get(&declaration.id)?;
        self.declarations[declaration_id.index()] = declaration;
        Some(declaration_id)
    }

    pub(in crate::compiler_frontend::ast) fn get_visible_non_receiver_by_name(
        &self,
        name: StringId,
        visible: Option<&FxHashSet<InternedPath>>,
    ) -> Option<&Declaration> {
        self.find_visible_by_name(name, visible, |declaration| {
            !is_receiver_method_declaration(declaration)
        })
    }

    pub(crate) fn get_visible_resolved_by_path(
        &self,
        path: &InternedPath,
        visible: Option<&FxHashSet<InternedPath>>,
    ) -> Option<&Declaration> {
        let declaration = self.get_by_path(path)?;
        if declaration.is_unresolved_constant_placeholder() {
            return None;
        }
        if let Some(visible) = visible
            && !visible.contains(&declaration.id)
        {
            return None;
        }
        Some(declaration)
    }

    pub(crate) fn get_visible_resolved_by_name(
        &self,
        name: StringId,
        visible: Option<&FxHashSet<InternedPath>>,
    ) -> Option<&Declaration> {
        self.find_visible_by_name(name, visible, |declaration| {
            !declaration.is_unresolved_constant_placeholder()
        })
    }

    pub(in crate::compiler_frontend::ast) fn has_unresolved_constant_placeholder(&self) -> bool {
        self.declarations
            .iter()
            .any(Declaration::is_unresolved_constant_placeholder)
    }

    fn get_by_id(&self, declaration_id: DeclarationId) -> Option<&Declaration> {
        self.declarations.get(declaration_id.index())
    }

    fn find_visible_by_name(
        &self,
        name: StringId,
        visible: Option<&FxHashSet<InternedPath>>,
        predicate: impl Fn(&Declaration) -> bool,
    ) -> Option<&Declaration> {
        let declaration_ids = self.by_name.get(&name)?;

        declaration_ids
            .iter()
            .filter_map(|declaration_id| self.get_by_id(*declaration_id))
            .find(|declaration| {
                predicate(declaration)
                    && match visible {
                        Some(visible) => visible.contains(&declaration.id),
                        None => true,
                    }
            })
    }
}

impl DeclarationId {
    fn index(self) -> usize {
        self.0 as usize
    }
}

fn is_receiver_method_declaration(declaration: &Declaration) -> bool {
    matches!(
        &declaration.value.data_type,
        DataType::Function(receiver, _) if receiver.as_ref().is_some()
    )
}
