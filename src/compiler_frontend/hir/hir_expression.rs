//! HIR Expression Lowering
//!
//! Lowers typed AST expressions into HIR expressions and statement preludes.
//! This file contains the high-level dispatcher and shared expression utilities on `HirBuilder`.

use crate::compiler_frontend::ast::expressions::expression::{
    BuiltinCastKind, Expression, ExpressionKind, ResultVariant as AstResultVariant,
};
use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::external_packages::CallTarget;
use crate::compiler_frontend::hir::blocks::{HirBlock, HirLocal};
use crate::compiler_frontend::hir::expressions::{
    HirBuiltinCastKind, HirExpression, HirExpressionKind, OptionVariant, ResultVariant, ValueKind,
};
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_datatypes::{HirTypeKind, TypeId};
use crate::compiler_frontend::hir::hir_side_table::HirLocalOriginKind;
use crate::compiler_frontend::hir::ids::{LocalId, RegionId};
use crate::compiler_frontend::hir::places::HirPlace;
use crate::compiler_frontend::hir::statements::{HirStatement, HirStatementKind};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::format_compile_time_paths;
use crate::hir_log;
use crate::return_hir_transformation_error;

mod calls;
mod literals;
mod operators;
mod places;
mod runtime;
mod templates;
mod types;

#[derive(Debug, Clone)]
pub(crate) struct LoweredExpression {
    // WHAT: Statements that must execute before evaluating `value`.
    // WHY: HIR requires expression side effects to be linearized into explicit statements.
    pub prelude: Vec<HirStatement>,
    pub value: HirExpression,
}

impl<'a> HirBuilder<'a> {
    // WHAT: lowers one typed AST expression into a linearized HIR prelude/value pair.
    // WHY: HIR cannot keep nested side effects inside expressions, so every entry point must
    //      return both the value and any statements required to produce it.
    pub(crate) fn lower_expression(
        &mut self,
        expr: &Expression,
    ) -> Result<LoweredExpression, CompilerError> {
        self.log_expression_input(expr);

        let lowered = match &expr.kind {
            ExpressionKind::ChoiceVariant {
                nominal_path,
                variant: _,
                tag,
            } => {
                let DataType::Choices { variants, .. } = &expr.data_type else {
                    return_hir_transformation_error!(
                        "ChoiceVariant expression has non-choice data type",
                        self.hir_error_location(&expr.location)
                    );
                };
                let choice_id =
                    self.resolve_or_create_choice_id(nominal_path, variants, &expr.location)?;
                let region = self.current_region_or_error(&expr.location)?;
                let ty = self.lower_data_type(&expr.data_type, &expr.location)?;

                Ok(LoweredExpression {
                    prelude: vec![],
                    value: self.make_expression(
                        &expr.location,
                        HirExpressionKind::ChoiceVariant {
                            choice_id,
                            variant_index: *tag,
                        },
                        ty,
                        ValueKind::Const,
                        region,
                    ),
                })
            }

            ExpressionKind::Int(value) => self.lower_literal_expression(
                &expr.location,
                &expr.data_type,
                HirExpressionKind::Int(*value),
            ),

            ExpressionKind::Float(value) => self.lower_literal_expression(
                &expr.location,
                &expr.data_type,
                HirExpressionKind::Float(*value),
            ),

            ExpressionKind::Bool(value) => self.lower_literal_expression(
                &expr.location,
                &expr.data_type,
                HirExpressionKind::Bool(*value),
            ),

            ExpressionKind::Char(value) => self.lower_literal_expression(
                &expr.location,
                &expr.data_type,
                HirExpressionKind::Char(*value),
            ),

            ExpressionKind::StringSlice(value) => self.lower_literal_expression(
                &expr.location,
                &expr.data_type,
                HirExpressionKind::StringLiteral(self.string_table.resolve(*value).to_owned()),
            ),

            ExpressionKind::Path(compile_time_paths) => {
                // Compile-time path values lower to string literals in HIR.
                // Formatting applies #origin for root-based paths and trailing
                // slash for directories through the shared path formatter.
                let path_string = format_compile_time_paths(
                    compile_time_paths,
                    &self.path_format_config,
                    self.string_table,
                );

                self.lower_literal_expression(
                    &expr.location,
                    &DataType::StringSlice,
                    HirExpressionKind::StringLiteral(path_string),
                )
            }

            ExpressionKind::Reference(name) => {
                self.lower_reference_expression(name, &expr.data_type, &expr.location)
            }

            ExpressionKind::Copy(place) => {
                let region = self.current_region_or_error(&expr.location)?;
                let (prelude, place) = self.lower_ast_node_to_place(place)?;
                let ty = self.lower_data_type(&expr.data_type, &expr.location)?;

                Ok(LoweredExpression {
                    prelude,
                    value: self.make_expression(
                        &expr.location,
                        HirExpressionKind::Copy(place),
                        ty,
                        ValueKind::RValue,
                        region,
                    ),
                })
            }

            ExpressionKind::Runtime(nodes) => {
                self.lower_runtime_rpn_expression(nodes, &expr.location, &expr.data_type)
            }

            ExpressionKind::FunctionCall(name, args) => {
                let function_id = self.resolve_function_id_or_error(name, &expr.location)?;
                self.lower_call_expression(
                    CallTarget::UserFunction(function_id),
                    args,
                    &self.extract_return_types_from_datatype(&expr.data_type),
                    &expr.location,
                )
            }

            ExpressionKind::ResultHandledFunctionCall {
                name,
                args,
                handling,
            } => {
                let function_id = self.resolve_function_id_or_error(name, &expr.location)?;
                self.lower_result_handled_call_expression(
                    CallTarget::UserFunction(function_id),
                    args,
                    &self.extract_return_types_from_datatype(&expr.data_type),
                    handling,
                    true,
                    &expr.location,
                )
            }

            ExpressionKind::BuiltinCast { kind, value } => {
                self.lower_builtin_cast_expression(*kind, value, &expr.location, &expr.data_type)
            }

            ExpressionKind::ResultConstruct { variant, value } => {
                let lowered_value = self.lower_expression(value)?;
                let region = self.current_region_or_error(&expr.location)?;
                let ty = self.lower_data_type(&expr.data_type, &expr.location)?;
                let hir_variant = match variant {
                    AstResultVariant::Ok => ResultVariant::Ok,
                    AstResultVariant::Err => ResultVariant::Err,
                };

                Ok(LoweredExpression {
                    prelude: lowered_value.prelude,
                    value: self.make_expression(
                        &expr.location,
                        HirExpressionKind::ResultConstruct {
                            variant: hir_variant,
                            value: Box::new(lowered_value.value),
                        },
                        ty,
                        ValueKind::RValue,
                        region,
                    ),
                })
            }

            ExpressionKind::HandledResult { value, handling } => self
                .lower_handled_result_expression(value, handling, &expr.location, &expr.data_type),

            ExpressionKind::HostFunctionCall(host_id, args) => self.lower_call_expression(
                CallTarget::ExternalFunction(host_id.to_owned()),
                args,
                &self.extract_return_types_from_datatype(&expr.data_type),
                &expr.location,
            ),

            ExpressionKind::Collection(items) => {
                let mut prelude = Vec::new();
                let mut lowered_items = Vec::with_capacity(items.len());

                for item in items {
                    let lowered_item = self.lower_expression(item)?;
                    prelude.extend(lowered_item.prelude);
                    lowered_items.push(lowered_item.value);
                }

                let region = self.current_region_or_error(&expr.location)?;
                let ty = self.lower_data_type(&expr.data_type, &expr.location)?;

                Ok(LoweredExpression {
                    prelude,
                    value: self.make_expression(
                        &expr.location,
                        HirExpressionKind::Collection(lowered_items),
                        ty,
                        ValueKind::RValue,
                        region,
                    ),
                })
            }

            ExpressionKind::Range(start, end) => {
                let mut prelude = Vec::new();
                let lowered_start = self.lower_expression(start)?;
                let lowered_end = self.lower_expression(end)?;
                prelude.extend(lowered_start.prelude);
                prelude.extend(lowered_end.prelude);

                let region = self.current_region_or_error(&expr.location)?;
                let ty = self.lower_data_type(&expr.data_type, &expr.location)?;

                Ok(LoweredExpression {
                    prelude,
                    value: self.make_expression(
                        &expr.location,
                        HirExpressionKind::Range {
                            start: Box::new(lowered_start.value),
                            end: Box::new(lowered_end.value),
                        },
                        ty,
                        ValueKind::RValue,
                        region,
                    ),
                })
            }

            ExpressionKind::StructInstance(args) => {
                let Some(nominal_path) = expr.data_type.struct_nominal_path() else {
                    return_hir_transformation_error!(
                        "Struct instance reached HIR lowering without a nominal struct identity",
                        self.hir_error_location(&expr.location)
                    );
                };
                let struct_id =
                    self.resolve_struct_id_from_nominal_path(nominal_path, &expr.location)?;
                let mut prelude = Vec::new();
                let mut fields = Vec::with_capacity(args.len());

                for arg in args {
                    let field_id =
                        self.resolve_field_id_or_error(struct_id, &arg.id, &expr.location)?;
                    let lowered_value = self.lower_expression(&arg.value)?;
                    prelude.extend(lowered_value.prelude);
                    fields.push((field_id, lowered_value.value));
                }

                let region = self.current_region_or_error(&expr.location)?;
                let ty = self.lower_data_type(&expr.data_type, &expr.location)?;

                Ok(LoweredExpression {
                    prelude,
                    value: self.make_expression(
                        &expr.location,
                        HirExpressionKind::StructConstruct { struct_id, fields },
                        ty,
                        ValueKind::RValue,
                        region,
                    ),
                })
            }

            ExpressionKind::Template(template) => {
                self.lower_runtime_template_expression(template.as_ref(), &expr.location)
            }

            // Lower the inner value and override the HIR type with the declared
            // coercion target. The actual numeric conversion (e.g. Int → Float)
            // is expected to be resolved by the code generation backend based on
            // the type annotation. For constant int literals that were already
            // folded to float in `type_coercion::numeric`, this arm will not be
            // reached.
            ExpressionKind::Coerced { value, .. } => {
                let mut lowered = self.lower_expression(value)?;
                let coerced_ty = self.lower_data_type(&expr.data_type, &expr.location)?;
                lowered.value.ty = coerced_ty;
                Ok(lowered)
            }

            ExpressionKind::Function(_, _) => {
                return_hir_transformation_error!(
                    "Function expressions are not lowered in this phase",
                    self.hir_error_location(&expr.location)
                )
            }

            ExpressionKind::StructDefinition(_) => {
                return_hir_transformation_error!(
                    "Struct definition expressions are not lowered in this phase",
                    self.hir_error_location(&expr.location)
                )
            }

            ExpressionKind::NoValue => {
                let region = self.current_region_or_error(&expr.location)?;
                Ok(LoweredExpression {
                    prelude: vec![],
                    value: self.unit_expression(&expr.location, region),
                })
            }

            ExpressionKind::OptionNone => {
                let region = self.current_region_or_error(&expr.location)?;
                let ty = self.lower_data_type(&expr.data_type, &expr.location)?;
                Ok(LoweredExpression {
                    prelude: vec![],
                    value: self.make_expression(
                        &expr.location,
                        HirExpressionKind::OptionConstruct {
                            variant: OptionVariant::None,
                            value: None,
                        },
                        ty,
                        ValueKind::RValue,
                        region,
                    ),
                })
            }
        }?;

        self.log_expression_output(expr, &lowered.value);
        Ok(lowered)
    }

    fn lower_builtin_cast_expression(
        &mut self,
        kind: BuiltinCastKind,
        value: &Expression,
        location: &SourceLocation,
        result_type: &DataType,
    ) -> Result<LoweredExpression, CompilerError> {
        let lowered_value = self.lower_expression(value)?;
        let region = self.current_region_or_error(location)?;
        let ty = self.lower_data_type(result_type, location)?;
        let hir_kind = match kind {
            BuiltinCastKind::Int => HirBuiltinCastKind::Int,
            BuiltinCastKind::Float => HirBuiltinCastKind::Float,
        };

        Ok(LoweredExpression {
            prelude: lowered_value.prelude,
            value: self.make_expression(
                location,
                HirExpressionKind::BuiltinCast {
                    kind: hir_kind,
                    value: Box::new(lowered_value.value),
                },
                ty,
                ValueKind::RValue,
                region,
            ),
        })
    }

    // WHAT: appends a prebuilt statement to the current block.
    // WHY: expression helpers sometimes manufacture statements outside the main statement
    //      dispatcher but still need to preserve explicit execution order.
    pub(crate) fn emit_statement_to_current_block(
        &mut self,
        statement: HirStatement,
        source_location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let block = self.current_block_mut_or_error(source_location)?;
        block.statements.push(statement);
        Ok(())
    }

    // WHAT: emits one `Assign(Local, value)` statement in the current block.
    // WHY: runtime short-circuit and handled-result branching both need consistent temp-local
    //      assignment behavior and source mapping.
    pub(crate) fn emit_assign_local_statement(
        &mut self,
        local: LocalId,
        value: HirExpression,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let assign_statement = HirStatement {
            id: self.allocate_node_id(),
            kind: HirStatementKind::Assign {
                target: HirPlace::Local(local),
                value,
            },
            location: location.to_owned(),
        };

        self.side_table.map_statement(location, &assign_statement);
        self.emit_statement_to_current_block(assign_statement, location)
    }

    // WHAT: allocates an unnamed temporary local in the current block.
    // WHY: complex expression lowering needs scratch storage to preserve evaluation order and
    //      explicit place/value distinctions in HIR.
    pub(crate) fn allocate_temp_local(
        &mut self,
        ty: TypeId,
        source_info: Option<SourceLocation>,
    ) -> Result<LocalId, CompilerError> {
        self.allocate_compiler_local(
            ty,
            source_info,
            HirLocalOriginKind::CompilerTemp,
            None,
            None,
        )
    }

    pub(crate) fn allocate_fresh_mutable_call_arg_local(
        &mut self,
        ty: TypeId,
        source_info: Option<SourceLocation>,
        call_location: &SourceLocation,
        argument_index: usize,
    ) -> Result<LocalId, CompilerError> {
        self.allocate_compiler_local(
            ty,
            source_info,
            HirLocalOriginKind::CompilerFreshMutableArg,
            Some(call_location),
            Some(argument_index),
        )
    }

    fn allocate_compiler_local(
        &mut self,
        ty: TypeId,
        source_info: Option<SourceLocation>,
        origin: HirLocalOriginKind,
        call_location: Option<&SourceLocation>,
        argument_index: Option<usize>,
    ) -> Result<LocalId, CompilerError> {
        let location = source_info.to_owned().unwrap_or_default();
        let region = self.current_region_or_error(&location)?;
        let block_id = self.current_block_id_or_error(&location)?;
        let local_id = self.allocate_local_id();

        let local = HirLocal {
            id: local_id,
            ty,
            mutable: true,
            region,
            source_info,
        };

        self.side_table.map_local_source(&local);
        self.register_local_in_block(block_id, local, &location)?;

        let temp_name = format!("__hir_tmp_{}", self.temp_local_counter);
        self.temp_local_counter += 1;
        let temp_name_id = InternedPath::from_single_str(&temp_name, self.string_table);

        // Compiler-introduced temporaries are intentionally excluded from AST symbol resolution.
        // They are named only for diagnostics/debug rendering via the side table.
        self.side_table.bind_local_name(local_id, temp_name_id);
        self.side_table
            .bind_local_origin(local_id, origin, call_location, argument_index);

        Ok(local_id)
    }

    // WHAT: returns mutable access to the active block or a structured lowering error.
    // WHY: most expression helpers need to append locals or statements, and failing early
    //      produces clearer diagnostics than assuming block state exists.
    pub(crate) fn current_block_mut_or_error(
        &mut self,
        location: &SourceLocation,
    ) -> Result<&mut HirBlock, CompilerError> {
        let block_id = self.current_block_id_or_error(location)?;
        self.block_mut_by_id_or_error(block_id, location)
    }

    // WHAT: allocates one HIR expression node with its identity and typing metadata attached.
    // WHY: centralizing expression construction keeps IDs, source mappings, and value kinds
    //      uniform across every lowering helper.
    pub(crate) fn make_expression(
        &mut self,
        location: &SourceLocation,
        kind: HirExpressionKind,
        ty: TypeId,
        value_kind: ValueKind,
        region: RegionId,
    ) -> HirExpression {
        let id = self.allocate_value_id();
        self.side_table.map_value(location, id, location);

        HirExpression {
            id,
            kind,
            ty,
            value_kind,
            region,
        }
    }

    // WHAT: creates a canonical load expression for one local.
    // WHY: runtime/result branching paths frequently reconstruct this node shape and should share
    //      one helper for readability and consistency.
    pub(crate) fn make_local_load_expression(
        &mut self,
        local: LocalId,
        ty: TypeId,
        location: &SourceLocation,
        region: RegionId,
    ) -> HirExpression {
        self.make_expression(
            location,
            HirExpressionKind::Load(HirPlace::Local(local)),
            ty,
            ValueKind::RValue,
            region,
        )
    }

    // WHAT: builds the canonical HIR representation of unit.
    // WHY: unit values should lower through the same tuple machinery every other pass expects.
    pub(crate) fn unit_expression(
        &mut self,
        location: &SourceLocation,
        region: RegionId,
    ) -> HirExpression {
        let unit_ty = self.intern_type_kind(HirTypeKind::Unit);
        self.make_expression(
            location,
            HirExpressionKind::TupleConstruct { elements: vec![] },
            unit_ty,
            ValueKind::Const,
            region,
        )
    }

    // WHAT: converts a frontend text location into the shared compiler error-location format.
    // WHY: HIR lowering uses one helper so all transformation errors preserve consistent source metadata.
    pub(crate) fn hir_error_location(&self, location: &SourceLocation) -> SourceLocation {
        location.clone()
    }

    fn log_expression_input(&self, _expr: &Expression) {
        hir_log!(format!(
            "[HIR] Lowering expression {:?} @ {:?}",
            _expr.kind, _expr.location
        ));
    }

    fn log_expression_output(&self, _input: &Expression, _output: &HirExpression) {
        hir_log!(format!(
            "[HIR] Lowered expression {:?} -> {}",
            _input.kind,
            _output.display_with_context(
                &crate::compiler_frontend::hir::hir_display::HirDisplayContext::new(
                    self.string_table,
                )
                .with_side_table(&self.side_table)
                .with_type_context(&self.type_context),
            )
        ));
    }

    fn log_call_result_binding(
        &self,
        _location: &SourceLocation,
        _local: Option<LocalId>,
        _value: &HirExpression,
    ) {
        hir_log!(format!(
            "[HIR] Emitted call binding @ {:?}: result={:?}, value={}",
            _location,
            _local,
            _value.display_with_context(
                &crate::compiler_frontend::hir::hir_display::HirDisplayContext::new(
                    self.string_table,
                )
                .with_side_table(&self.side_table)
                .with_type_context(&self.type_context),
            )
        ));
    }

    /// Resolve a choice declaration to its stable HIR `ChoiceId`, creating one on first use.
    ///
    /// WHAT: lazily registers `HirChoice` entries because AST does not emit top-level
    /// choice-definition nodes; choices are discovered via type references and expressions.
    /// WHY: keeps choice registration simple without a separate pre-scan pass.
    pub(crate) fn resolve_or_create_choice_id(
        &mut self,
        nominal_path: &InternedPath,
        variants: &[crate::compiler_frontend::declaration_syntax::choice::ChoiceVariant],
        _location: &SourceLocation,
    ) -> Result<crate::compiler_frontend::hir::ids::ChoiceId, CompilerError> {
        use crate::compiler_frontend::hir::module::{HirChoice, HirChoiceVariant};

        if let Some(&choice_id) = self.choices_by_name.get(nominal_path) {
            return Ok(choice_id);
        }

        let choice_id = self.allocate_choice_id();
        let hir_variants: Vec<HirChoiceVariant> = variants
            .iter()
            .map(|v| HirChoiceVariant { name: v.id })
            .collect();

        self.choices_by_name
            .insert(nominal_path.to_owned(), choice_id);
        self.side_table
            .bind_choice_name(choice_id, nominal_path.to_owned());
        self.module.choices.push(HirChoice {
            id: choice_id,
            variants: hir_variants,
        });

        Ok(choice_id)
    }
}
