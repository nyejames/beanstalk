//! Dynamic trait value lowering for the JavaScript backend.
//!
//! WHAT: emits compact wrapper construction, requirement-key dispatch, and method tables from
//! frontend-selected conformance evidence.
//! WHY: dynamic trait semantics are decided before HIR; JS lowering must use those stable IDs
//! instead of re-solving traits or inspecting source declarations.

use crate::backends::js::JsEmitter;
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::ids::FunctionId;
use crate::compiler_frontend::hir::reachability::{
    ReachableDynamicTraitOperation, ReachableDynamicTraitOperationKind,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::traits::ids::{TraitEvidenceId, TraitRequirementId};

impl<'hir> JsEmitter<'hir> {
    pub(crate) fn lower_dynamic_trait_construction(
        &mut self,
        value: String,
        evidence_id: TraitEvidenceId,
    ) -> String {
        self.used_dynamic_trait_constructor = true;
        self.used_dynamic_trait_tables.insert(evidence_id);

        let table_name = dynamic_trait_table_name(evidence_id);
        format!("__bs_dynamic_trait({value}, {table_name})")
    }

    pub(crate) fn lower_dynamic_trait_dispatch(
        &mut self,
        receiver: String,
        requirement_id: TraitRequirementId,
        args: Vec<String>,
    ) -> String {
        self.used_dynamic_trait_dispatch = true;

        let requirement_key = dynamic_trait_requirement_key(requirement_id);
        format!(
            "__bs_dynamic_trait_call({receiver}, \"{requirement_key}\", [{}])",
            args.join(", ")
        )
    }

    pub(crate) fn emit_dynamic_trait_runtime(&mut self) -> Result<(), CompilerError> {
        if self.used_dynamic_trait_tables.is_empty()
            && !self.used_dynamic_trait_constructor
            && !self.used_dynamic_trait_dispatch
        {
            return Ok(());
        }

        if !self.out.is_empty() {
            self.emit_line("");
        }

        self.emit_dynamic_trait_method_tables()?;

        if self.used_dynamic_trait_constructor {
            self.emit_dynamic_trait_constructor_helper();
        }

        if self.used_dynamic_trait_dispatch {
            self.emit_dynamic_trait_dispatch_helper();
        }

        Ok(())
    }

    pub(crate) fn dynamic_trait_method_roots_for_operations(
        &self,
        operations: &[ReachableDynamicTraitOperation],
    ) -> Result<Vec<FunctionId>, CompilerError> {
        let mut evidence_ids = Vec::new();

        for operation in operations {
            if let ReachableDynamicTraitOperationKind::Construct { evidence_id, .. } =
                &operation.kind
            {
                evidence_ids.push(*evidence_id);
            }
        }

        evidence_ids.sort_by_key(|id| id.0);
        evidence_ids.dedup_by_key(|id| id.0);

        let mut method_roots = Vec::new();
        for evidence_id in evidence_ids {
            self.push_method_roots_for_evidence(evidence_id, &mut method_roots)?;
        }

        method_roots.sort_by_key(|function_id| function_id.0);
        method_roots.dedup_by_key(|function_id| function_id.0);
        Ok(method_roots)
    }

    fn emit_dynamic_trait_method_tables(&mut self) -> Result<(), CompilerError> {
        let evidence_ids = self
            .used_dynamic_trait_tables
            .iter()
            .copied()
            .collect::<Vec<_>>();

        for evidence_id in evidence_ids {
            let evidence = self
                .hir
                .trait_evidence_environment
                .get(evidence_id)
                .ok_or_else(|| {
                    CompilerError::compiler_error(format!(
                        "JavaScript backend: dynamic trait evidence {evidence_id:?} is missing"
                    ))
                })?;
            let table_name = dynamic_trait_table_name(evidence_id);

            self.emit_line(&format!("const {table_name} = {{"));
            self.indent += 1;

            let mut requirements = evidence.requirements.iter().collect::<Vec<_>>();
            requirements.sort_by_key(|requirement| requirement.requirement_id.0);

            for requirement in requirements {
                let function_id = self.function_id_for_path(&requirement.method_path)?;
                let function_name = self.function_name(function_id)?;
                let requirement_key = dynamic_trait_requirement_key(requirement.requirement_id);
                self.emit_line(&format!("{requirement_key}: {function_name},"));
            }

            self.indent -= 1;
            self.emit_line("};");
            self.emit_line("");
        }

        Ok(())
    }

    fn emit_dynamic_trait_constructor_helper(&mut self) {
        self.emit_line("function __bs_dynamic_trait(value, methods) {");
        self.with_indent(|emitter| {
            emitter.emit_line(
                "return { __bs_dynamic_trait: true, __bs_value: value, __bs_methods: methods };",
            );
        });
        self.emit_line("}");
        self.emit_line("");
    }

    fn emit_dynamic_trait_dispatch_helper(&mut self) {
        self.emit_line("function __bs_dynamic_trait_call(receiver, requirement, args) {");
        self.with_indent(|emitter| {
            emitter.emit_line(
                "if (!receiver || receiver.__bs_dynamic_trait !== true) { throw new Error(\"invalid dynamic trait value\"); }",
            );
            emitter.emit_line("const method = receiver.__bs_methods[requirement];");
            emitter.emit_line(
                "if (typeof method !== \"function\") { throw new Error(\"missing dynamic trait method\"); }",
            );
            emitter.emit_line("return method(__bs_binding(receiver.__bs_value), ...args);");
        });
        self.emit_line("}");
        self.emit_line("");
    }

    fn push_method_roots_for_evidence(
        &self,
        evidence_id: TraitEvidenceId,
        method_roots: &mut Vec<FunctionId>,
    ) -> Result<(), CompilerError> {
        let evidence = self
            .hir
            .trait_evidence_environment
            .get(evidence_id)
            .ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "JavaScript backend: dynamic trait evidence {evidence_id:?} is missing"
                ))
            })?;

        for requirement in &evidence.requirements {
            method_roots.push(self.function_id_for_path(&requirement.method_path)?);
        }

        Ok(())
    }

    fn function_id_for_path(&self, path: &InternedPath) -> Result<FunctionId, CompilerError> {
        for function in &self.hir.functions {
            if self
                .hir
                .side_table
                .function_name_path(function.id)
                .is_some_and(|function_path| function_path == path)
            {
                return Ok(function.id);
            }
        }

        Err(CompilerError::compiler_error(format!(
            "JavaScript backend: dynamic trait method '{}' was not lowered into HIR",
            path.to_path_buf(self.string_table).display()
        )))
    }
}

fn dynamic_trait_table_name(evidence_id: TraitEvidenceId) -> String {
    format!("__bs_dyn_table_e{}", evidence_id.0)
}

fn dynamic_trait_requirement_key(requirement_id: TraitRequirementId) -> String {
    format!("r{}", requirement_id.0)
}
