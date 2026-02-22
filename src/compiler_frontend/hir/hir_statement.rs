//! HIR Statement Lowering
//!
//! Lowers AST statements/control-flow nodes into explicit HIR blocks,
//! statements, and terminators.

use crate::backends::function_registry::CallTarget;
use crate::compiler_frontend::ast::ast::Ast;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind, TextLocation, Var};
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::statements::branching::MatchArm;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_datatypes::HirTypeKind;
use crate::compiler_frontend::hir::hir_display::{HirDisplayContext, HirLocation};
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FieldId, FunctionId, HirBlock, HirExpression, HirExpressionKind, HirField,
    HirFunction, HirLocal, HirMatchArm, HirPattern, HirRegion, HirStatement, HirStatementKind,
    HirStruct, HirTerminator, LocalId, StructId, ValueKind,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::projects::settings::IMPLICIT_START_FUNC_NAME;
use crate::return_hir_transformation_error;

use crate::hir_log;

impl<'a> HirBuilder<'a> {
    pub(crate) fn prepare_hir_declarations(&mut self, ast: &Ast) -> Result<(), CompilerError> {
        for node in &ast.nodes {
            if let NodeKind::StructDefinition(name, fields) = &node.kind {
                self.register_struct_declaration(name, fields, &node.location)?;
            }
        }

        for node in &ast.nodes {
            if let NodeKind::Function(name, signature, _) = &node.kind {
                self.register_function_declaration(name, signature, &node.location)?;
            }
        }

        self.resolve_start_function(ast)
    }

    pub(crate) fn lower_top_level_node(&mut self, node: &AstNode) -> Result<(), CompilerError> {
        match &node.kind {
            NodeKind::Function(name, signature, body) => {
                self.lower_function_body(name, signature, body, &node.location)
            }

            NodeKind::StructDefinition(_, _) => Ok(()),

            NodeKind::Warning(_) | NodeKind::Empty | NodeKind::Newline | NodeKind::Spaces(_) => {
                Ok(())
            }

            NodeKind::Return(_) => return_hir_transformation_error!(
                "Top-level return is not valid during HIR lowering",
                self.hir_error_location(&node.location)
            ),

            _ => return_hir_transformation_error!(
                format!(
                    "Top-level AST node is not a supported declaration: {:?}",
                    node.kind
                ),
                self.hir_error_location(&node.location)
            ),
        }
    }

    pub(crate) fn lower_function_body(
        &mut self,
        function_name: &InternedPath,
        signature: &FunctionSignature,
        body: &[AstNode],
        location: &TextLocation,
    ) -> Result<(), CompilerError> {
        let function_id = self.resolve_function_id_or_error(function_name, location)?;

        self.enter_function(function_id, location)?;

        let lower_result = self.lower_function_body_inner(function_id, signature, body, location);
        self.leave_function();

        lower_result
    }

    pub(crate) fn lower_statement_sequence(
        &mut self,
        nodes: &[AstNode],
    ) -> Result<(), CompilerError> {
        for node in nodes {
            let current_block = self.current_block_id_or_error(&node.location)?;
            if self.block_has_explicit_terminator(current_block, &node.location)? {
                break;
            }

            self.lower_statement_node(node)?;
        }

        Ok(())
    }

    pub(crate) fn lower_statement_node(&mut self, node: &AstNode) -> Result<(), CompilerError> {
        self.log_statement_input(node);

        let result = match &node.kind {
            NodeKind::VariableDeclaration(var) => {
                self.lower_variable_declaration_statement(var, &node.location)
            }

            NodeKind::Assignment { target, value } => {
                self.lower_assignment_statement(target, value, &node.location)
            }

            NodeKind::FunctionCall {
                name,
                args,
                returns: _,
                location,
            } => self.lower_call_statement(CallTarget::UserFunction(name.clone()), args, location),

            NodeKind::HostFunctionCall {
                name: host_function_id,
                args,
                returns: _,
                location,
            } => self.lower_call_statement(
                CallTarget::HostFunction(host_function_id.clone()),
                args,
                location,
            ),

            NodeKind::Rvalue(expr) => self.lower_expression_statement(expr, &node.location),

            NodeKind::FieldAccess { .. } => self.lower_field_access_statement(node, &node.location),

            NodeKind::Return(values) => self.lower_return_statement(values, &node.location),

            NodeKind::If(condition, then_body, else_body) => {
                self.lower_if_statement(condition, then_body, else_body.as_deref(), &node.location)
            }

            NodeKind::WhileLoop(condition, body) => {
                self.lower_while_statement(condition, body, &node.location)
            }

            NodeKind::Match(scrutinee, arms, default) => {
                self.lower_match_statement(scrutinee, arms, default.as_deref(), &node.location)
            }

            NodeKind::ForLoop(_, _, _) => return_hir_transformation_error!(
                "For-loop lowering is not implemented in this HIR phase",
                self.hir_error_location(&node.location)
            ),

            NodeKind::Warning(_)
            | NodeKind::Operator(_)
            | NodeKind::Empty
            | NodeKind::Newline
            | NodeKind::Spaces(_) => Ok(()),

            _ => return_hir_transformation_error!(
                format!(
                    "Unsupported AST statement node during HIR lowering: {:?}",
                    node.kind
                ),
                self.hir_error_location(&node.location)
            ),
        };

        if result.is_ok() {
            self.log_statement_output(node);
        }

        result
    }

    fn register_struct_declaration(
        &mut self,
        name: &InternedPath,
        fields: &[Var],
        location: &TextLocation,
    ) -> Result<(), CompilerError> {
        if self.structs_by_name.contains_key(name) {
            return_hir_transformation_error!(
                format!(
                    "Duplicate struct declaration '{}' during HIR lowering",
                    self.symbol_name_for_diagnostics(name)
                ),
                self.hir_error_location(location)
            );
        }

        let struct_id = self.allocate_struct_id();
        let mut hir_fields = Vec::with_capacity(fields.len());

        for field in fields {
            // AST guarantees module-wide unique InternedPath symbols. For struct fields this
            // means each field path must be prefixed by its parent struct path.
            let Some(parent) = field.id.parent() else {
                return_hir_transformation_error!(
                    format!(
                        "Field '{}' has no parent struct path during HIR lowering",
                        self.symbol_name_for_diagnostics(&field.id)
                    ),
                    self.hir_error_location(location)
                );
            };

            if parent != *name {
                return_hir_transformation_error!(
                    format!(
                        "Field '{}' is not prefixed by struct '{}'",
                        self.symbol_name_for_diagnostics(&field.id),
                        self.symbol_name_for_diagnostics(name)
                    ),
                    self.hir_error_location(location)
                );
            }

            if self
                .fields_by_struct_and_name
                .contains_key(&(struct_id, field.id.clone()))
            {
                return_hir_transformation_error!(
                    format!(
                        "Duplicate field '{}' in struct '{}'",
                        self.symbol_name_for_diagnostics(&field.id),
                        self.symbol_name_for_diagnostics(name)
                    ),
                    self.hir_error_location(location)
                );
            }

            let field_location = if field.value.location == TextLocation::default() {
                location.clone()
            } else {
                field.value.location.clone()
            };

            let field_type = self.lower_data_type(&field.value.data_type, &field_location)?;
            let field_id = self.allocate_field_id();

            self.fields_by_struct_and_name
                .insert((struct_id, field.id.clone()), field_id);
            self.side_table.bind_field_name(field_id, field.id.clone());
            self.side_table
                .map_ast_to_hir(&field_location, HirLocation::Field(field_id));
            self.side_table
                .map_hir_source_location(HirLocation::Field(field_id), &field_location);

            hir_fields.push(HirField {
                id: field_id,
                ty: field_type,
            });
        }

        let hir_struct = HirStruct {
            id: struct_id,
            fields: hir_fields,
        };

        self.structs_by_name.insert(name.clone(), struct_id);
        self.side_table.bind_struct_name(struct_id, name.clone());
        self.side_table
            .map_ast_to_hir(location, HirLocation::Struct(struct_id));
        self.side_table
            .map_hir_source_location(HirLocation::Struct(struct_id), location);

        self.module.structs.push(hir_struct);

        Ok(())
    }

    fn register_function_declaration(
        &mut self,
        name: &InternedPath,
        signature: &FunctionSignature,
        location: &TextLocation,
    ) -> Result<(), CompilerError> {
        if self.functions_by_name.contains_key(name) {
            return_hir_transformation_error!(
                format!(
                    "Duplicate function declaration '{}' during HIR lowering",
                    self.symbol_name_for_diagnostics(name)
                ),
                self.hir_error_location(location)
            );
        }

        let function_id = self.allocate_function_id();
        let return_type = self.lower_data_type(
            &crate::compiler_frontend::datatypes::DataType::Returns(signature.returns.clone()),
            location,
        )?;

        let region_id = self.allocate_region_id();
        self.push_region(HirRegion::lexical(region_id, None));

        let entry_block_id = self.allocate_block_id();
        let entry_block = HirBlock {
            id: entry_block_id,
            region: region_id,
            locals: vec![],
            statements: vec![],
            terminator: HirTerminator::Panic { message: None },
        };

        self.side_table.map_block(location, &entry_block);
        self.push_block(entry_block);

        let function = HirFunction {
            id: function_id,
            entry: entry_block_id,
            params: vec![],
            return_type,
        };

        self.functions_by_name.insert(name.clone(), function_id);
        self.side_table
            .bind_function_name(function_id, name.clone());
        self.side_table.map_function(location, &function);
        self.push_function(function);

        Ok(())
    }

    fn resolve_start_function(&mut self, ast: &Ast) -> Result<(), CompilerError> {
        let start_name = ast
            .entry_path
            .join_str(IMPLICIT_START_FUNC_NAME, self.string_table);

        let Some(start_function) = self.functions_by_name.get(&start_name).copied() else {
            let error_location = ast
                .nodes
                .first()
                .map(|node| node.location.clone())
                .unwrap_or_default();

            return_hir_transformation_error!(
                format!(
                    "Failed to resolve module start function '{}' during HIR lowering",
                    self.symbol_name_for_diagnostics(&start_name)
                ),
                self.hir_error_location(&error_location)
            );
        };

        self.module.start_function = start_function;
        Ok(())
    }

    fn lower_function_body_inner(
        &mut self,
        function_id: FunctionId,
        signature: &FunctionSignature,
        body: &[AstNode],
        location: &TextLocation,
    ) -> Result<(), CompilerError> {
        self.lower_parameter_locals(function_id, signature, location)?;
        self.lower_statement_sequence(body)?;

        let current_block = self.current_block_id_or_error(location)?;
        if self.block_has_explicit_terminator(current_block, location)? {
            return Ok(());
        }

        let return_type = self
            .function_by_id_or_error(function_id, location)?
            .return_type;

        if self.is_unit_type(return_type) {
            let region = self.current_region_or_error(location)?;
            let unit = self.unit_expression(region);
            self.emit_terminator(current_block, HirTerminator::Return(unit), location)?;
            return Ok(());
        }

        let function_name = self
            .side_table
            .resolve_function_name(function_id, self.string_table)
            .unwrap_or("<unknown>");

        return_hir_transformation_error!(
            format!(
                "Function '{}' can fall through without returning a value",
                function_name
            ),
            self.hir_error_location(location)
        )
    }

    fn lower_parameter_locals(
        &mut self,
        function_id: FunctionId,
        signature: &FunctionSignature,
        fallback_location: &TextLocation,
    ) -> Result<(), CompilerError> {
        for param in &signature.parameters {
            let param_location = if param.value.location == TextLocation::default() {
                fallback_location.clone()
            } else {
                param.value.location.clone()
            };

            let param_type = self.lower_data_type(&param.value.data_type, &param_location)?;
            let local_id = self.allocate_named_local(
                param.id.clone(),
                param_type,
                param.value.ownership.is_mutable(),
                Some(param_location.clone()),
            )?;

            let function = self.function_mut_by_id_or_error(function_id, &param_location)?;
            function.params.push(local_id);
        }

        Ok(())
    }

    fn lower_variable_declaration_statement(
        &mut self,
        variable: &Var,
        location: &TextLocation,
    ) -> Result<(), CompilerError> {
        let source_location = if variable.value.location == TextLocation::default() {
            location.clone()
        } else {
            variable.value.location.clone()
        };

        let local_type = self.lower_data_type(&variable.value.data_type, &source_location)?;
        let local_id = self.allocate_named_local(
            variable.id.clone(),
            local_type,
            variable.value.ownership.is_mutable(),
            Some(source_location),
        )?;

        let lowered = self.lower_expression(&variable.value)?;

        for prelude in lowered.prelude {
            self.emit_statement_to_current_block(prelude, location)?;
        }

        self.emit_statement_kind(
            HirStatementKind::Assign {
                target: crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(local_id),
                value: lowered.value,
            },
            location,
        )
    }

    fn lower_assignment_statement(
        &mut self,
        target: &AstNode,
        value: &Expression,
        location: &TextLocation,
    ) -> Result<(), CompilerError> {
        let (target_prelude, target_place) = self.lower_ast_node_to_place(target)?;
        let lowered_value = self.lower_expression(value)?;

        for prelude in target_prelude {
            self.emit_statement_to_current_block(prelude, location)?;
        }

        for prelude in lowered_value.prelude {
            self.emit_statement_to_current_block(prelude, location)?;
        }

        self.emit_statement_kind(
            HirStatementKind::Assign {
                target: target_place,
                value: lowered_value.value,
            },
            location,
        )
    }

    fn lower_call_statement(
        &mut self,
        target: CallTarget,
        args: &[Expression],
        location: &TextLocation,
    ) -> Result<(), CompilerError> {
        if let CallTarget::UserFunction(name) = &target {
            let _ = self.resolve_function_id_or_error(name, location)?;
        }

        let mut lowered_args = Vec::with_capacity(args.len());

        for arg in args {
            let lowered = self.lower_expression(arg)?;
            for prelude in lowered.prelude {
                self.emit_statement_to_current_block(prelude, location)?;
            }
            lowered_args.push(lowered.value);
        }

        self.emit_statement_kind(
            HirStatementKind::Call {
                target,
                args: lowered_args,
                result: None,
            },
            location,
        )
    }

    fn lower_expression_statement(
        &mut self,
        expression: &Expression,
        location: &TextLocation,
    ) -> Result<(), CompilerError> {
        let lowered = self.lower_expression(expression)?;

        for prelude in lowered.prelude {
            self.emit_statement_to_current_block(prelude, location)?;
        }

        if self.is_unit_type(lowered.value.ty) {
            return Ok(());
        }

        self.emit_statement_kind(HirStatementKind::Expr(lowered.value), location)
    }

    fn lower_field_access_statement(
        &mut self,
        field_access_node: &AstNode,
        location: &TextLocation,
    ) -> Result<(), CompilerError> {
        let lowered = self.lower_ast_node_as_expression(field_access_node)?;

        for prelude in lowered.prelude {
            self.emit_statement_to_current_block(prelude, location)?;
        }

        if self.is_unit_type(lowered.value.ty) {
            return Ok(());
        }

        self.emit_statement_kind(HirStatementKind::Expr(lowered.value), location)
    }

    fn lower_return_statement(
        &mut self,
        values: &[Expression],
        location: &TextLocation,
    ) -> Result<(), CompilerError> {
        let mut lowered_values = Vec::with_capacity(values.len());

        for value in values {
            let lowered = self.lower_expression(value)?;
            for prelude in lowered.prelude {
                self.emit_statement_to_current_block(prelude, location)?;
            }
            lowered_values.push(lowered.value);
        }

        let return_value = self.expression_from_return_values(&lowered_values, location)?;
        let current_block = self.current_block_id_or_error(location)?;

        self.emit_terminator(current_block, HirTerminator::Return(return_value), location)
    }

    fn lower_if_statement(
        &mut self,
        condition: &Expression,
        then_body: &[AstNode],
        else_body: Option<&[AstNode]>,
        location: &TextLocation,
    ) -> Result<(), CompilerError> {
        let current_block = self.current_block_id_or_error(location)?;
        let condition_lowered = self.lower_expression(condition)?;

        for prelude in condition_lowered.prelude {
            self.emit_statement_to_current_block(prelude, location)?;
        }

        let region = self.current_region_or_error(location)?;
        let then_block = self.create_block(region, location, "if-then")?;
        let else_block = self.create_block(region, location, "if-else")?;

        self.emit_terminator(
            current_block,
            HirTerminator::If {
                condition: condition_lowered.value,
                then_block,
                else_block,
            },
            location,
        )?;

        self.log_control_flow_edge(current_block, then_block, "if.true");
        self.log_control_flow_edge(current_block, else_block, "if.false");

        let mut terminated_anchor: Option<BlockId> = None;

        self.set_current_block(then_block, location)?;
        self.lower_statement_sequence(then_body)?;
        let then_terminated = self.block_has_explicit_terminator(then_block, location)?;
        if then_terminated {
            terminated_anchor = Some(then_block);
        }

        self.set_current_block(else_block, location)?;
        if let Some(else_nodes) = else_body {
            self.lower_statement_sequence(else_nodes)?;
        }

        let else_terminated = self.block_has_explicit_terminator(else_block, location)?;
        if else_terminated && terminated_anchor.is_none() {
            terminated_anchor = Some(else_block);
        }

        if then_terminated && else_terminated {
            // No continuation path exists after this branch.
            return self.set_current_block(terminated_anchor.unwrap_or(then_block), location);
        }

        let merge_block = self.create_block(region, location, "if-merge")?;
        if !then_terminated {
            self.emit_jump_to(then_block, merge_block, location, "if.then.merge")?;
        }
        if !else_terminated {
            self.emit_jump_to(else_block, merge_block, location, "if.else.merge")?;
        }

        self.set_current_block(merge_block, location)
    }

    fn lower_while_statement(
        &mut self,
        condition: &Expression,
        body: &[AstNode],
        location: &TextLocation,
    ) -> Result<(), CompilerError> {
        let pre_header_block = self.current_block_id_or_error(location)?;
        let region = self.current_region_or_error(location)?;

        let header_block = self.create_block(region, location, "while-header")?;
        let body_block = self.create_block(region, location, "while-body")?;
        let exit_block = self.create_block(region, location, "while-exit")?;

        self.emit_jump_to(pre_header_block, header_block, location, "while.enter")?;

        self.set_current_block(header_block, location)?;
        let lowered_condition = self.lower_expression(condition)?;
        for prelude in lowered_condition.prelude {
            self.emit_statement_to_current_block(prelude, location)?;
        }

        self.emit_terminator(
            header_block,
            HirTerminator::If {
                condition: lowered_condition.value,
                then_block: body_block,
                else_block: exit_block,
            },
            location,
        )?;

        self.log_control_flow_edge(header_block, body_block, "while.true");
        self.log_control_flow_edge(header_block, exit_block, "while.false");

        self.set_current_block(body_block, location)?;
        self.lower_statement_sequence(body)?;

        if !self.block_has_explicit_terminator(body_block, location)? {
            self.emit_jump_to(body_block, header_block, location, "while.backedge")?;
        }

        self.set_current_block(exit_block, location)
    }

    fn lower_match_statement(
        &mut self,
        scrutinee: &Expression,
        arms: &[MatchArm],
        default: Option<&[AstNode]>,
        location: &TextLocation,
    ) -> Result<(), CompilerError> {
        let current_block = self.current_block_id_or_error(location)?;

        let lowered_scrutinee = self.lower_expression(scrutinee)?;
        for prelude in lowered_scrutinee.prelude {
            self.emit_statement_to_current_block(prelude, location)?;
        }

        let region = self.current_region_or_error(location)?;
        let mut arm_blocks = Vec::with_capacity(arms.len());
        for _ in arms {
            arm_blocks.push(self.create_block(region, location, "match-arm")?);
        }

        let default_block = if default.is_some() {
            Some(self.create_block(region, location, "match-default")?)
        } else {
            None
        };
        let mut merge_block = if default.is_none() {
            Some(self.create_block(region, location, "match-merge")?)
        } else {
            None
        };

        let mut hir_arms = Vec::with_capacity(arms.len() + 1);
        for (index, arm) in arms.iter().enumerate() {
            let lowered_pattern = self.lower_match_literal_pattern(&arm.condition)?;

            hir_arms.push(HirMatchArm {
                pattern: HirPattern::Literal(lowered_pattern),
                guard: None,
                body: arm_blocks[index],
            });
        }

        if let Some(default_block_id) = default_block {
            hir_arms.push(HirMatchArm {
                pattern: HirPattern::Wildcard,
                guard: None,
                body: default_block_id,
            });
        } else {
            hir_arms.push(HirMatchArm {
                pattern: HirPattern::Wildcard,
                guard: None,
                body: merge_block.expect("match merge block exists when default arm is absent"),
            });
        }

        self.emit_terminator(
            current_block,
            HirTerminator::Match {
                scrutinee: lowered_scrutinee.value,
                arms: hir_arms,
            },
            location,
        )?;

        let mut terminated_anchor: Option<BlockId> = None;

        for (index, arm) in arms.iter().enumerate() {
            let arm_block = arm_blocks[index];
            self.set_current_block(arm_block, location)?;
            self.lower_statement_sequence(&arm.body)?;

            let arm_terminated = self.block_has_explicit_terminator(arm_block, location)?;
            if arm_terminated {
                if terminated_anchor.is_none() {
                    terminated_anchor = Some(arm_block);
                }
            } else {
                let merge_target =
                    self.ensure_match_merge_block(region, location, &mut merge_block)?;
                self.emit_jump_to(arm_block, merge_target, location, "match.arm.merge")?;
            }
        }

        if let (Some(default_block_id), Some(default_body)) = (default_block, default) {
            self.set_current_block(default_block_id, location)?;
            self.lower_statement_sequence(default_body)?;

            let default_terminated =
                self.block_has_explicit_terminator(default_block_id, location)?;
            if default_terminated {
                if terminated_anchor.is_none() {
                    terminated_anchor = Some(default_block_id);
                }
            } else {
                let merge_target =
                    self.ensure_match_merge_block(region, location, &mut merge_block)?;
                self.emit_jump_to(
                    default_block_id,
                    merge_target,
                    location,
                    "match.default.merge",
                )?;
            }
        }

        if let Some(merge_block_id) = merge_block {
            return self.set_current_block(merge_block_id, location);
        }

        if let Some(anchor_block) = terminated_anchor {
            return self.set_current_block(anchor_block, location);
        }

        self.set_current_block(current_block, location)
    }

    fn lower_match_literal_pattern(
        &mut self,
        condition: &Expression,
    ) -> Result<HirExpression, CompilerError> {
        let lowered_pattern = self.lower_expression(condition)?;
        if !lowered_pattern.prelude.is_empty() {
            return_hir_transformation_error!(
                "Match arm pattern lowering produced side-effect statements; only literal patterns are supported",
                self.hir_error_location(&condition.location)
            );
        }

        if lowered_pattern.value.value_kind != ValueKind::Const {
            return_hir_transformation_error!(
                "Match arm patterns must be compile-time literals",
                self.hir_error_location(&condition.location)
            );
        }

        if !matches!(
            lowered_pattern.value.kind,
            HirExpressionKind::Int(_)
                | HirExpressionKind::Float(_)
                | HirExpressionKind::Bool(_)
                | HirExpressionKind::Char(_)
                | HirExpressionKind::StringLiteral(_)
        ) {
            return_hir_transformation_error!(
                "Match arm patterns currently support only literal int/float/bool/char/string values",
                self.hir_error_location(&condition.location)
            );
        }

        Ok(lowered_pattern.value)
    }

    fn ensure_match_merge_block(
        &mut self,
        region: crate::compiler_frontend::hir::hir_nodes::RegionId,
        location: &TextLocation,
        merge_block: &mut Option<BlockId>,
    ) -> Result<BlockId, CompilerError> {
        if let Some(existing) = *merge_block {
            return Ok(existing);
        }

        let created = self.create_block(region, location, "match-merge")?;
        *merge_block = Some(created);
        Ok(created)
    }

    fn create_block(
        &mut self,
        region: crate::compiler_frontend::hir::hir_nodes::RegionId,
        source_location: &TextLocation,
        label: &str,
    ) -> Result<BlockId, CompilerError> {
        let block = HirBlock {
            id: self.allocate_block_id(),
            region,
            locals: vec![],
            statements: vec![],
            terminator: HirTerminator::Panic { message: None },
        };

        self.side_table.map_block(source_location, &block);
        self.log_block_created(block.id, label, source_location);

        let id = block.id;
        self.push_block(block);
        Ok(id)
    }

    fn allocate_named_local(
        &mut self,
        name: InternedPath,
        ty: crate::compiler_frontend::hir::hir_datatypes::TypeId,
        mutable: bool,
        source_info: Option<TextLocation>,
    ) -> Result<LocalId, CompilerError> {
        let local_location = source_info.clone().unwrap_or_default();

        // AST forbids shadowing and provides module-wide unique symbol paths, so a duplicate
        // path here indicates invalid redeclaration in the current function lowering context.
        if self.locals_by_name.contains_key(&name) {
            return_hir_transformation_error!(
                format!(
                    "Local '{}' is already declared in this function scope",
                    self.symbol_name_for_diagnostics(&name)
                ),
                self.hir_error_location(&local_location)
            );
        }

        let region = self.current_region_or_error(&local_location)?;
        let local_id = LocalId(self.next_local_id);
        self.next_local_id += 1;

        let local = HirLocal {
            id: local_id,
            ty,
            mutable,
            region,
            source_info,
        };

        {
            let block = self.current_block_mut_or_error(&local_location)?;
            block.locals.push(local.clone());
        }

        self.locals_by_name.insert(name.clone(), local_id);
        self.side_table.bind_local_name(local_id, name);
        self.side_table.map_local_source(&local);
        self.side_table
            .map_ast_to_hir(&local_location, HirLocation::Local(local_id));

        Ok(local_id)
    }

    fn expression_from_return_values(
        &mut self,
        values: &[HirExpression],
        location: &TextLocation,
    ) -> Result<HirExpression, CompilerError> {
        let region = self.current_region_or_error(location)?;

        match values {
            [] => Ok(self.unit_expression(region)),
            [single] => Ok(single.clone()),
            many => {
                let field_types = many.iter().map(|value| value.ty).collect::<Vec<_>>();
                let tuple_type = self.intern_type_kind(HirTypeKind::Tuple {
                    fields: field_types,
                });

                Ok(HirExpression {
                    kind: HirExpressionKind::TupleConstruct {
                        elements: many.to_vec(),
                    },
                    ty: tuple_type,
                    value_kind: ValueKind::RValue,
                    region,
                })
            }
        }
    }

    fn emit_statement_kind(
        &mut self,
        kind: HirStatementKind,
        location: &TextLocation,
    ) -> Result<(), CompilerError> {
        let statement = HirStatement {
            id: self.allocate_node_id(),
            kind,
            location: location.clone(),
        };

        self.side_table.map_statement(location, &statement);
        self.emit_statement_to_current_block(statement, location)
    }

    fn emit_jump_to(
        &mut self,
        from_block: BlockId,
        target: BlockId,
        location: &TextLocation,
        edge_label: &str,
    ) -> Result<(), CompilerError> {
        self.emit_terminator(
            from_block,
            HirTerminator::Jump {
                target,
                args: vec![],
            },
            location,
        )?;

        self.log_control_flow_edge(from_block, target, edge_label);
        Ok(())
    }

    fn emit_terminator(
        &mut self,
        block_id: BlockId,
        terminator: HirTerminator,
        location: &TextLocation,
    ) -> Result<(), CompilerError> {
        self.log_terminator_emitted(block_id, &terminator, location);
        self.set_block_terminator(block_id, terminator, location)
    }

    fn is_unit_type(&self, ty: crate::compiler_frontend::hir::hir_datatypes::TypeId) -> bool {
        matches!(self.type_context.get(ty).kind, HirTypeKind::Unit)
    }

    #[cfg(feature = "show_hir")]
    fn log_statement_input(&self, node: &AstNode) {
        hir_log!(format!("[HIR][Stmt] Lowering {:?}", node.kind));
    }

    #[cfg(not(feature = "show_hir"))]
    fn log_statement_input(&self, _node: &AstNode) {}

    #[cfg(feature = "show_hir")]
    fn log_statement_output(&self, node: &AstNode) {
        hir_log!(format!("[HIR][Stmt] Lowered {:?}", node.kind));
    }

    #[cfg(not(feature = "show_hir"))]
    fn log_statement_output(&self, _node: &AstNode) {}

    #[cfg(feature = "show_hir")]
    fn log_block_created(&self, block_id: BlockId, label: &str, location: &TextLocation) {
        hir_log!(format!(
            "[HIR][CFG] Created block {} ({}) @ {:?}",
            block_id, label, location
        ));
    }

    #[cfg(not(feature = "show_hir"))]
    fn log_block_created(&self, _block_id: BlockId, _label: &str, _location: &TextLocation) {}

    #[cfg(feature = "show_hir")]
    fn log_control_flow_edge(&self, from: BlockId, to: BlockId, label: &str) {
        hir_log!(format!("[HIR][CFG] Edge {} -> {} ({})", from, to, label));
    }

    #[cfg(not(feature = "show_hir"))]
    fn log_control_flow_edge(&self, _from: BlockId, _to: BlockId, _label: &str) {}

    #[cfg(feature = "show_hir")]
    fn log_terminator_emitted(
        &self,
        block_id: BlockId,
        terminator: &HirTerminator,
        location: &TextLocation,
    ) {
        let display = HirDisplayContext::new(self.string_table)
            .with_side_table(&self.side_table)
            .with_type_context(&self.type_context);

        let rendered = terminator.display_with_context(&display);
        hir_log!(format!(
            "[HIR][CFG] Terminator for {} @ {:?}: {}",
            block_id, location, rendered
        ));
    }

    #[cfg(not(feature = "show_hir"))]
    fn log_terminator_emitted(
        &self,
        _block_id: BlockId,
        _terminator: &HirTerminator,
        _location: &TextLocation,
    ) {
    }
}
