//! Module-level HIR metadata validation.
//!
//! WHAT: checks doc fragment locations and folded module constant payloads after lowering.
//! WHY: these values are consumed by builders outside the executable CFG, so they need explicit
//! validation instead of relying on statement or expression walks.

use super::HirValidator;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::constants::{HirConstValue, HirDocFragmentKind};
use crate::compiler_frontend::hir::hir_side_table::HirLocation;

impl<'a> HirValidator<'a> {
    // -------------------------
    //  Metadata Validation
    // -------------------------

    pub(super) fn validate_doc_fragments(&self) -> Result<(), CompilerError> {
        for (index, fragment) in self.module.doc_fragments.iter().enumerate() {
            if matches!(fragment.kind, HirDocFragmentKind::Doc)
                && fragment
                    .location
                    .start_pos
                    .line_number
                    .gt(&fragment.location.end_pos.line_number)
            {
                return Err(self.error_with_hir(
                    format!(
                        "Doc fragment #{index} has invalid location: start line {} is after end line {}",
                        fragment.location.start_pos.line_number, fragment.location.end_pos.line_number
                    ),
                    None,
                ));
            }

            if fragment.location.start_pos.line_number == fragment.location.end_pos.line_number
                && fragment.location.start_pos.char_column > fragment.location.end_pos.char_column
            {
                return Err(self.error_with_hir(
                    format!(
                        "Doc fragment #{index} has invalid location columns: start {} is after end {}",
                        fragment.location.start_pos.char_column, fragment.location.end_pos.char_column
                    ),
                    None,
                ));
            }
        }

        Ok(())
    }

    pub(super) fn validate_reactive_metadata(&self) -> Result<(), CompilerError> {
        for source in self.module.side_table.reactive_sources() {
            let anchor = Some(HirLocation::Local(source.local_id));
            self.require_local_id(source.local_id, anchor)?;
            self.require_type_id(source.type_id, anchor)?;

            let Some(local_type) = self.local_types.get(&source.local_id).copied() else {
                return Err(self.error_with_hir(
                    format!(
                        "Reactive source {:?} points at local {:?} without a registered type",
                        source.id, source.local_id
                    ),
                    anchor,
                ));
            };

            if local_type != source.type_id {
                return Err(self.error_with_hir(
                    format!(
                        "Reactive source {:?} type {:?} does not match local {:?} type {:?}",
                        source.id, source.type_id, source.local_id, local_type
                    ),
                    anchor,
                ));
            }

            if self.module.side_table.local_name_path(source.local_id) != Some(&source.path) {
                return Err(self.error_with_hir(
                    format!(
                        "Reactive source {:?} path does not match local {:?}'s side-table name",
                        source.id, source.local_id
                    ),
                    anchor,
                ));
            }

            if self
                .module
                .side_table
                .reactive_source_id_for_local(source.local_id)
                != Some(source.id)
            {
                return Err(self.error_with_hir(
                    format!(
                        "Reactive source {:?} is not indexed by its local {:?}",
                        source.id, source.local_id
                    ),
                    anchor,
                ));
            }

            if self
                .module
                .side_table
                .reactive_source_id_for_path(&source.path)
                != Some(source.id)
            {
                return Err(self.error_with_hir(
                    format!("Reactive source {:?} is not indexed by its path", source.id),
                    anchor,
                ));
            }
        }

        for template in self.module.side_table.reactive_templates() {
            let anchor = Some(HirLocation::Value(template.value_id));
            if self
                .module
                .side_table
                .reactive_template_id_for_value(template.value_id)
                != Some(template.id)
            {
                return Err(self.error_with_hir(
                    format!(
                        "Reactive template {:?} is not indexed by its value {:?}",
                        template.id, template.value_id
                    ),
                    anchor,
                ));
            }

            for dependency in &template.dependencies {
                self.require_type_id(dependency.type_id, anchor)?;
                let Some(source) = self.module.side_table.reactive_source(dependency.source) else {
                    return Err(self.error_with_hir(
                        format!(
                            "Reactive template {:?} depends on unknown source {:?}",
                            template.id, dependency.source
                        ),
                        anchor,
                    ));
                };

                if source.type_id != dependency.type_id {
                    return Err(self.error_with_hir(
                        format!(
                            "Reactive template {:?} dependency type {:?} does not match source {:?} type {:?}",
                            template.id, dependency.type_id, source.id, source.type_id
                        ),
                        anchor,
                    ));
                }
            }

            for dependency in &template.template_value_parameters {
                self.require_local_id(dependency.parameter, anchor)?;
            }
        }

        Ok(())
    }

    pub(super) fn validate_module_constants(&self) -> Result<(), CompilerError> {
        for module_constant in &self.module.module_constants {
            if module_constant.name.trim().is_empty() {
                return Err(self.error_with_hir(
                    format!(
                        "Module constant {:?} has an empty constant name",
                        module_constant.id
                    ),
                    None,
                ));
            }

            self.require_type_id(module_constant.ty, None)?;
            self.validate_module_const_value(&module_constant.value)?;
        }

        Ok(())
    }

    pub(super) fn validate_module_const_value(
        &self,
        value: &HirConstValue,
    ) -> Result<(), CompilerError> {
        match value {
            HirConstValue::Collection(values) => {
                for value in values {
                    self.validate_module_const_value(value)?;
                }
            }
            HirConstValue::Record(fields) => {
                for field in fields {
                    if field.name.trim().is_empty() {
                        return Err(self.error_with_hir(
                            "Module constant record contains an empty field name",
                            None,
                        ));
                    }
                    self.validate_module_const_value(&field.value)?;
                }
            }
            HirConstValue::Range(start, end) => {
                self.validate_module_const_value(start)?;
                self.validate_module_const_value(end)?;
            }
            HirConstValue::Result { value, .. } => {
                self.validate_module_const_value(value)?;
            }
            HirConstValue::Choice { fields, .. } => {
                for field in fields {
                    if field.name.trim().is_empty() {
                        return Err(self.error_with_hir(
                            "Module constant choice contains an empty field name",
                            None,
                        ));
                    }
                    self.validate_module_const_value(&field.value)?;
                }
            }
            HirConstValue::Int(_)
            | HirConstValue::Float(_)
            | HirConstValue::Bool(_)
            | HirConstValue::Char(_)
            | HirConstValue::String(_) => {}
        }

        Ok(())
    }
}
