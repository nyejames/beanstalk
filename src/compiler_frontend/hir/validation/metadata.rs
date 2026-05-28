//! Module-level HIR metadata validation.
//!
//! WHAT: checks doc fragment locations and folded module constant payloads after lowering.
//! WHY: these values are consumed by builders outside the executable CFG, so they need explicit
//! validation instead of relying on statement or expression walks.

use super::HirValidator;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::constants::{HirConstValue, HirDocFragmentKind};

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
