use crate::compiler_frontend::analysis::borrow_checker::state::{BorrowState, FunctionLayout};
use crate::compiler_frontend::compiler_errors::ErrorLocation;
use crate::compiler_frontend::hir::hir_display::HirLocation;
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FunctionId, HirModule, HirStatement, HirTerminator, HirValueId, LocalId,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;

pub(super) struct BorrowDiagnostics<'a> {
    module: &'a HirModule,
    string_table: &'a StringTable,
}

impl<'a> BorrowDiagnostics<'a> {
    pub(super) fn new(module: &'a HirModule, string_table: &'a StringTable) -> Self {
        Self {
            module,
            string_table,
        }
    }

    pub(super) fn local_name(&self, local_id: LocalId) -> String {
        self.module
            .side_table
            .resolve_local_name(local_id, self.string_table)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{}", local_id))
    }

    pub(super) fn function_name(&self, function_id: FunctionId) -> String {
        self.module
            .side_table
            .resolve_function_name(function_id, self.string_table)
            .map(str::to_owned)
            .unwrap_or_else(|| format!("{}", function_id))
    }

    pub(super) fn path_name(&self, path: &InternedPath) -> String {
        path.name_str(self.string_table)
            .map(str::to_owned)
            .unwrap_or_else(|| path.to_string(self.string_table))
    }

    pub(super) fn statement_error_location(&self, statement: &HirStatement) -> ErrorLocation {
        statement.location.to_error_location(self.string_table)
    }

    pub(super) fn terminator_error_location(
        &self,
        block_id: BlockId,
        _terminator: &HirTerminator,
    ) -> ErrorLocation {
        self.module
            .side_table
            .hir_source_location_for_hir(HirLocation::Terminator(block_id))
            .or_else(|| {
                self.module
                    .side_table
                    .ast_location_for_hir(HirLocation::Terminator(block_id))
            })
            .or_else(|| {
                self.module
                    .side_table
                    .hir_source_location_for_hir(HirLocation::Block(block_id))
            })
            .or_else(|| {
                self.module
                    .side_table
                    .ast_location_for_hir(HirLocation::Block(block_id))
            })
            .map(|location| location.to_error_location(self.string_table))
            .unwrap_or_else(ErrorLocation::default)
    }

    pub(super) fn function_error_location(&self, function_id: FunctionId) -> ErrorLocation {
        self.module
            .side_table
            .hir_source_location_for_hir(HirLocation::Function(function_id))
            .or_else(|| {
                self.module
                    .side_table
                    .ast_location_for_hir(HirLocation::Function(function_id))
            })
            .map(|location| location.to_error_location(self.string_table))
            .unwrap_or_else(ErrorLocation::default)
    }

    pub(super) fn value_error_location(
        &self,
        value_id: HirValueId,
        fallback: ErrorLocation,
    ) -> ErrorLocation {
        self.module
            .side_table
            .value_source_location(value_id)
            .or_else(|| self.module.side_table.value_ast_location(value_id))
            .map(|location| location.to_error_location(self.string_table))
            .unwrap_or(fallback)
    }

    pub(super) fn conflicting_local_for_root(
        &self,
        layout: &FunctionLayout,
        state: &BorrowState,
        actor_index: usize,
        root_index: usize,
    ) -> Option<LocalId> {
        for (candidate_index, candidate_id) in layout.local_ids.iter().enumerate() {
            if candidate_index == actor_index {
                continue;
            }

            let roots = state.effective_roots(candidate_index);
            if roots.contains(root_index) {
                return Some(*candidate_id);
            }
        }

        None
    }
}
