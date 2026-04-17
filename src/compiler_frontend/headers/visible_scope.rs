//! Header-stage symbol/style visibility classification.
//!
//! WHAT: answers whether a name is visible from one source file while header parsing is still
//! building declaration shells.
//! WHY: header-stage struct defaults and template heads need to distinguish "truly unknown" names
//! from names that are visible but not yet semantically resolved until AST.

use crate::compiler_frontend::headers::module_symbols::{DeclarationStub, ModuleSymbols};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HeaderStageVisibility {
    SameFile,
    Imported,
    Builtin,
    StyleDirective,
    Unknown,
}

pub(crate) struct HeaderStageVisibleScope<'a> {
    module_symbols: &'a ModuleSymbols,
    style_directives: &'a StyleDirectiveRegistry,
}

impl<'a> HeaderStageVisibleScope<'a> {
    pub(crate) fn new(
        module_symbols: &'a ModuleSymbols,
        style_directives: &'a StyleDirectiveRegistry,
    ) -> Self {
        Self {
            module_symbols,
            style_directives,
        }
    }

    pub(crate) fn resolve(
        &self,
        name: StringId,
        source_file: &InternedPath,
        string_table: &StringTable,
    ) -> HeaderStageVisibility {
        if self
            .module_symbols
            .declared_names_by_file
            .get(source_file)
            .is_some_and(|names| names.contains(&name))
        {
            return HeaderStageVisibility::SameFile;
        }

        if self
            .module_symbols
            .file_imports_by_source
            .get(source_file)
            .is_some_and(|imports| {
                imports
                    .iter()
                    .any(|import| import.header_path.name() == Some(name))
            })
        {
            return HeaderStageVisibility::Imported;
        }

        if self
            .module_symbols
            .builtin_visible_symbol_paths
            .iter()
            .any(|path| path.name() == Some(name))
        {
            return HeaderStageVisibility::Builtin;
        }

        if self
            .style_directives
            .find(string_table.resolve(name))
            .is_some()
        {
            return HeaderStageVisibility::StyleDirective;
        }

        HeaderStageVisibility::Unknown
    }

    pub(crate) fn visible_stub(
        &self,
        name: StringId,
        source_file: &InternedPath,
        string_table: &StringTable,
    ) -> Option<&DeclarationStub> {
        match self.resolve(name, source_file, string_table) {
            HeaderStageVisibility::SameFile
            | HeaderStageVisibility::Imported
            | HeaderStageVisibility::Builtin => self
                .visible_stub_paths(name, source_file, string_table)
                .into_iter()
                .find_map(|path| self.module_symbols.declaration_stubs_by_path.get(&path)),
            HeaderStageVisibility::StyleDirective | HeaderStageVisibility::Unknown => None,
        }
    }

    fn visible_stub_paths(
        &self,
        name: StringId,
        source_file: &InternedPath,
        string_table: &StringTable,
    ) -> Vec<InternedPath> {
        let mut visible_paths = Vec::new();

        if let Some(paths) = self.module_symbols.declared_paths_by_file.get(source_file) {
            visible_paths.extend(
                paths
                    .iter()
                    .filter(|path| path.name() == Some(name))
                    .cloned(),
            );
        }

        if let Some(imports) = self.module_symbols.file_imports_by_source.get(source_file) {
            for import in imports {
                if import.header_path.name() != Some(name) {
                    continue;
                }

                if let Some(path) = resolve_stub_path(
                    &self.module_symbols.declaration_stubs_by_path,
                    &import.header_path,
                    string_table,
                ) {
                    visible_paths.push(path);
                }
            }
        }

        if let Some(builtin_path) = self
            .module_symbols
            .builtin_visible_symbol_paths
            .iter()
            .find(|path| path.name() == Some(name))
        {
            visible_paths.push(builtin_path.to_owned());
        }

        visible_paths
    }
}

fn resolve_stub_path(
    stubs: &rustc_hash::FxHashMap<InternedPath, DeclarationStub>,
    requested_path: &InternedPath,
    string_table: &StringTable,
) -> Option<InternedPath> {
    if stubs.contains_key(requested_path) {
        return Some(requested_path.to_owned());
    }

    let matches = stubs
        .keys()
        .filter(|candidate| {
            candidate.ends_with(requested_path)
                || components_match_with_optional_bst_extension(
                    candidate.as_components(),
                    requested_path.as_components(),
                    string_table,
                )
        })
        .cloned()
        .collect::<Vec<_>>();

    match matches.as_slice() {
        [single] => Some(single.to_owned()),
        _ => None,
    }
}

fn components_match_with_optional_bst_extension(
    candidate_components: &[StringId],
    requested_components: &[StringId],
    string_table: &StringTable,
) -> bool {
    if candidate_components.len() != requested_components.len() {
        return false;
    }

    candidate_components
        .iter()
        .zip(requested_components.iter())
        .all(|(candidate_component, requested_component)| {
            if candidate_component == requested_component {
                return true;
            }

            let candidate_str = string_table.resolve(*candidate_component);
            let requested_str = string_table.resolve(*requested_component);
            candidate_str == requested_str
                || candidate_str
                    .strip_suffix(".bst")
                    .is_some_and(|trimmed| trimmed == requested_str)
        })
}
