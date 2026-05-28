//! Function-level HIR validation.
//!
//! WHAT: checks function entries, return types, parameters, and return-alias slot metadata.
//! WHY: borrow summaries depend on function alias metadata matching the canonical return shape.

use super::HirValidator;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::hir::hir_side_table::HirLocation;
use crate::compiler_frontend::hir::ids::FunctionId;
use rustc_hash::FxHashSet;

impl<'a> HirValidator<'a> {
    // -------------------------
    //  Function Validation
    // -------------------------

    pub(super) fn validate_functions(&self) -> Result<(), CompilerError> {
        for function in &self.module.functions {
            self.require_block_id(function.entry, Some(HirLocation::Function(function.id)))?;
            self.require_type_id(
                function.return_type,
                Some(HirLocation::Function(function.id)),
            )?;

            let expected_slots =
                self.expected_return_alias_slots(function.return_type, function.id)?;
            if function.return_aliases.len() != expected_slots {
                return Err(self.error_with_hir(
                    format!(
                        "Function {:?} return_aliases has {} slot(s), expected {} from return type",
                        function.id,
                        function.return_aliases.len(),
                        expected_slots
                    ),
                    Some(HirLocation::Function(function.id)),
                ));
            }

            for (slot_index, alias_candidates) in function.return_aliases.iter().enumerate() {
                let Some(alias_candidates) = alias_candidates.as_ref() else {
                    continue;
                };
                if alias_candidates.is_empty() {
                    return Err(self.error_with_hir(
                        format!(
                            "Function {:?} return_aliases slot {} uses an empty alias list",
                            function.id, slot_index
                        ),
                        Some(HirLocation::Function(function.id)),
                    ));
                }

                let mut seen = FxHashSet::default();
                for param_index in alias_candidates {
                    if *param_index >= function.params.len() {
                        return Err(self.error_with_hir(
                            format!(
                                "Function {:?} return_aliases slot {} contains out-of-range parameter index {}",
                                function.id, slot_index, param_index
                            ),
                            Some(HirLocation::Function(function.id)),
                        ));
                    }
                    if !seen.insert(*param_index) {
                        return Err(self.error_with_hir(
                            format!(
                                "Function {:?} return_aliases slot {} contains duplicate parameter index {}",
                                function.id, slot_index, param_index
                            ),
                            Some(HirLocation::Function(function.id)),
                        ));
                    }
                }
            }

            for local in &function.params {
                self.require_local_id(*local, Some(HirLocation::Function(function.id)))?;
            }
        }

        Ok(())
    }

    pub(super) fn expected_return_alias_slots(
        &self,
        return_type: TypeId,
        function_id: FunctionId,
    ) -> Result<usize, CompilerError> {
        self.require_type_id(return_type, Some(HirLocation::Function(function_id)))?;
        let slot_count_for_value_type = |ty: TypeId| -> Result<usize, CompilerError> {
            self.require_type_id(ty, Some(HirLocation::Function(function_id)))?;
            Ok(if ty == self.type_environment.builtins().none {
                0
            } else if let Some(fields) = self.type_environment.tuple_field_ids(ty) {
                fields.len()
            } else {
                1
            })
        };

        match self.type_environment.fallible_carrier_slots(return_type) {
            Some((ok, _)) => slot_count_for_value_type(ok),
            None => slot_count_for_value_type(return_type),
        }
    }
}
