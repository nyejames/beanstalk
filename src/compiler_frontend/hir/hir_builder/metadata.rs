//! Metadata passes for HIR module construction.
//!
//! WHAT: fills non-CFG module metadata after declarations and constants have
//! been prepared.
//! WHY: function origins are executable HIR metadata consumed by builders and later validation.
//! Resolved documentation fragments are non-HIR compiler metadata extracted into the lowering
//! metadata result boundary, not stored on `HirModule`.

use crate::compiler_frontend::ast::Ast;
use crate::compiler_frontend::ast::AstDocFragmentKind;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::functions::HirFunctionOrigin;
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::module_metadata::{ModuleDocFragment, ModuleDocFragmentKind};

impl<'a> HirBuilder<'a> {
    pub(super) fn assign_function_origins(&mut self) -> Result<(), CompilerError> {
        // WHAT: classify every lowered function with a semantic origin tag.
        // WHY: downstream lowering needs explicit role data to avoid heuristic drift.
        self.module.function_origins.clear();

        for function in &self.module.functions {
            self.module
                .function_origins
                .insert(function.id, HirFunctionOrigin::Normal);
        }

        self.module
            .function_origins
            .insert(self.module.start_function, HirFunctionOrigin::EntryStart);

        Ok(())
    }

    pub(super) fn resolve_doc_fragments(&mut self, ast: &Ast) -> Result<(), CompilerError> {
        self.extracted_metadata.doc_fragments.clear();

        for fragment in &ast.doc_fragments {
            let kind = match fragment.kind {
                AstDocFragmentKind::Doc => ModuleDocFragmentKind::Doc,
            };

            self.extracted_metadata
                .doc_fragments
                .push(ModuleDocFragment {
                    kind,
                    rendered_text: self.string_table.resolve(fragment.value).to_owned(),
                    location: fragment.location.to_owned(),
                });
        }

        Ok(())
    }
}
