//! HIR Validation
//!
//! Always-on structural validation for new HIR modules.
//! This pass enforces core invariants so downstream analysis/backends can
//! rely on a consistent IR contract.
use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorType, SourceLocation};
use crate::compiler_frontend::hir::hir_datatypes::{HirTypeKind, TypeId};
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FieldId, FunctionId, HirConstValue, HirDocFragmentKind, HirExpression,
    HirExpressionKind, HirFunctionOrigin, HirMatchArm, HirModule, HirPattern, HirPlace,
    HirStatement, HirStatementKind, HirTerminator, LocalId, RegionId, StructId, ValueKind,
};
use crate::compiler_frontend::hir::hir_side_table::HirLocation;
use crate::compiler_frontend::hir::utils::terminator_targets;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;

pub(crate) fn validate_hir_module(module: &HirModule) -> Result<(), CompilerError> {
    let mut validator = HirValidator::new(module);
    validator.validate()
}

struct HirValidator<'a> {
    module: &'a HirModule,

    block_ids: FxHashSet<BlockId>,
    block_index_by_id: FxHashMap<BlockId, usize>,
    block_owner_by_id: FxHashMap<BlockId, FunctionId>,
    function_ids: FxHashSet<FunctionId>,
    struct_ids: FxHashSet<StructId>,
    field_ids: FxHashSet<FieldId>,
    region_ids: FxHashSet<RegionId>,

    local_types: FxHashMap<LocalId, TypeId>,
    field_types: FxHashMap<FieldId, TypeId>,
    field_owner: FxHashMap<FieldId, StructId>,
}

impl<'a> HirValidator<'a> {
    fn new(module: &'a HirModule) -> Self {
        Self {
            module,
            block_ids: FxHashSet::default(),
            block_index_by_id: FxHashMap::default(),
            block_owner_by_id: FxHashMap::default(),
            function_ids: FxHashSet::default(),
            struct_ids: FxHashSet::default(),
            field_ids: FxHashSet::default(),
            region_ids: FxHashSet::default(),
            local_types: FxHashMap::default(),
            field_types: FxHashMap::default(),
            field_owner: FxHashMap::default(),
        }
    }

    fn validate(&mut self) -> Result<(), CompilerError> {
        self.collect_definition_ids()?;
        self.validate_region_graph()?;
        self.validate_start_function()?;
        self.validate_function_origins()?;
        self.validate_doc_fragments()?;
        self.validate_module_constants()?;
        self.validate_functions()?;
        self.validate_function_cfg_ownership()?;
        self.validate_blocks()?;
        Ok(())
    }

    fn collect_definition_ids(&mut self) -> Result<(), CompilerError> {
        for region in &self.module.regions {
            let id = region.id();
            if !self.region_ids.insert(id) {
                return Err(self.error_with_hir(format!("Duplicate HIR region id {id:?}"), None));
            }
        }

        for block in &self.module.blocks {
            if !self.block_ids.insert(block.id) {
                return Err(self.error_with_hir(
                    format!("Duplicate HIR block id {:?}", block.id),
                    Some(HirLocation::Block(block.id)),
                ));
            }
            self.block_index_by_id
                .insert(block.id, self.block_index_by_id.len());

            for local in &block.locals {
                if self.local_types.insert(local.id, local.ty).is_some() {
                    return Err(self.error_with_hir(
                        format!("Duplicate HIR local id {:?}", local.id),
                        Some(HirLocation::Block(block.id)),
                    ));
                }
            }
        }

        for hir_struct in &self.module.structs {
            if !self.struct_ids.insert(hir_struct.id) {
                return Err(self.error_with_hir(
                    format!("Duplicate HIR struct id {:?}", hir_struct.id),
                    Some(HirLocation::Struct(hir_struct.id)),
                ));
            }

            for field in &hir_struct.fields {
                if !self.field_ids.insert(field.id) {
                    return Err(self.error_with_hir(
                        format!("Duplicate HIR field id {:?}", field.id),
                        Some(HirLocation::Struct(hir_struct.id)),
                    ));
                }

                self.field_types.insert(field.id, field.ty);
                self.field_owner.insert(field.id, hir_struct.id);
            }
        }

        for function in &self.module.functions {
            if !self.function_ids.insert(function.id) {
                return Err(self.error_with_hir(
                    format!("Duplicate HIR function id {:?}", function.id),
                    Some(HirLocation::Function(function.id)),
                ));
            }
        }

        Ok(())
    }

    fn validate_region_graph(&self) -> Result<(), CompilerError> {
        let parent_by_region = self
            .module
            .regions
            .iter()
            .map(|region| (region.id(), region.parent()))
            .collect::<FxHashMap<_, _>>();

        for region in &self.module.regions {
            if let Some(parent) = region.parent()
                && !self.region_ids.contains(&parent)
            {
                return Err(self.error_with_hir(
                    format!(
                        "Region {} references missing parent region {}",
                        region.id().0,
                        parent.0
                    ),
                    None,
                ));
            }
        }

        for region in &self.module.regions {
            let mut chain = FxHashSet::default();
            let mut current = Some(region.id());

            while let Some(region_id) = current {
                if !chain.insert(region_id) {
                    return Err(self.error_with_hir(
                        format!(
                            "Region parent graph contains a cycle at region {}",
                            region_id.0
                        ),
                        None,
                    ));
                }

                current = parent_by_region.get(&region_id).copied().flatten();
            }
        }

        Ok(())
    }

    fn validate_start_function(&self) -> Result<(), CompilerError> {
        if !self.function_ids.contains(&self.module.start_function) {
            return Err(self.error_with_hir(
                format!(
                    "HIR start_function {:?} is not present in module functions",
                    self.module.start_function
                ),
                Some(HirLocation::Function(self.module.start_function)),
            ));
        }

        Ok(())
    }

    fn validate_function_origins(&self) -> Result<(), CompilerError> {
        // WHAT: enforce complete and consistent function-origin coverage.
        // WHY: backends rely on this map to preserve entry/runtime semantics.
        if self.module.function_origins.len() != self.module.functions.len() {
            return Err(self.error_with_hir(
                format!(
                    "HIR function_origins contains {} entries, but module has {} functions",
                    self.module.function_origins.len(),
                    self.module.functions.len()
                ),
                None,
            ));
        }

        for function in &self.module.functions {
            if !self.module.function_origins.contains_key(&function.id) {
                return Err(self.error_with_hir(
                    format!("HIR function {:?} is missing an origin entry", function.id),
                    Some(HirLocation::Function(function.id)),
                ));
            }
        }

        if !matches!(
            self.module
                .function_origins
                .get(&self.module.start_function),
            Some(HirFunctionOrigin::EntryStart)
        ) {
            return Err(self.error_with_hir(
                format!(
                    "HIR start function {:?} must be tagged as EntryStart",
                    self.module.start_function
                ),
                Some(HirLocation::Function(self.module.start_function)),
            ));
        }

        Ok(())
    }

    fn validate_doc_fragments(&self) -> Result<(), CompilerError> {
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

    fn validate_module_constants(&self) -> Result<(), CompilerError> {
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

    fn validate_module_const_value(&self, value: &HirConstValue) -> Result<(), CompilerError> {
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
            HirConstValue::Int(_)
            | HirConstValue::Float(_)
            | HirConstValue::Bool(_)
            | HirConstValue::Char(_)
            | HirConstValue::String(_) => {}
        }

        Ok(())
    }

    fn validate_functions(&self) -> Result<(), CompilerError> {
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

    fn expected_return_alias_slots(
        &self,
        return_type: TypeId,
        function_id: FunctionId,
    ) -> Result<usize, CompilerError> {
        self.require_type_id(return_type, Some(HirLocation::Function(function_id)))?;
        let slot_count_for_value_type = |ty: TypeId| -> Result<usize, CompilerError> {
            self.require_type_id(ty, Some(HirLocation::Function(function_id)))?;
            Ok(match &self.module.type_context.get(ty).kind {
                HirTypeKind::Unit => 0,
                HirTypeKind::Tuple { fields } => fields.len(),
                _ => 1,
            })
        };

        match &self.module.type_context.get(return_type).kind {
            HirTypeKind::Result { ok, .. } => slot_count_for_value_type(*ok),
            _ => slot_count_for_value_type(return_type),
        }
    }

    fn validate_function_cfg_ownership(&mut self) -> Result<(), CompilerError> {
        self.block_owner_by_id.clear();

        for function in &self.module.functions {
            let mut queue = VecDeque::new();
            let mut visited = FxHashSet::default();
            queue.push_back(function.entry);

            while let Some(block_id) = queue.pop_front() {
                if !visited.insert(block_id) {
                    continue;
                }

                self.require_block_id(block_id, Some(HirLocation::Function(function.id)))?;

                if let Some(existing_owner) = self.block_owner_by_id.get(&block_id).copied() {
                    if existing_owner != function.id {
                        return Err(self.error_with_hir(
                            format!(
                                "Block {} is reachable from multiple functions ({:?} and {:?})",
                                block_id, existing_owner, function.id
                            ),
                            Some(HirLocation::Block(block_id)),
                        ));
                    }
                } else {
                    self.block_owner_by_id.insert(block_id, function.id);
                }

                let block = self.block_by_id(block_id)?;
                for successor in terminator_targets(&block.terminator) {
                    queue.push_back(successor);
                }
            }
        }

        for block in &self.module.blocks {
            if self.block_owner_by_id.contains_key(&block.id) {
                continue;
            }

            return Err(self.error_with_hir(
                format!(
                    "Block {} is not reachable from any function entry and has no CFG owner",
                    block.id
                ),
                Some(HirLocation::Block(block.id)),
            ));
        }

        Ok(())
    }

    fn validate_blocks(&self) -> Result<(), CompilerError> {
        for block in &self.module.blocks {
            if matches!(block.terminator, HirTerminator::Panic { message: None }) {
                return Err(self.error_with_hir(
                    format!(
                        "Block {} still has placeholder terminator Panic(None) after HIR lowering",
                        block.id
                    ),
                    Some(HirLocation::Block(block.id)),
                ));
            }

            self.require_region_id(block.region, Some(HirLocation::Block(block.id)))?;

            for local in &block.locals {
                self.require_type_id(local.ty, Some(HirLocation::Local(local.id)))?;
                self.require_region_id(local.region, Some(HirLocation::Local(local.id)))?;
            }

            for statement in &block.statements {
                self.validate_statement_mappings(statement)?;
                self.validate_statement(statement)?;
            }

            self.validate_terminator_mapping(block.id)?;
            self.validate_terminator(block.id, &block.terminator)?;
        }

        Ok(())
    }

    fn validate_statement_mappings(&self, statement: &HirStatement) -> Result<(), CompilerError> {
        let statement_location = HirLocation::Statement(statement.id);
        if self
            .module
            .side_table
            .ast_source_id_for_hir(statement_location)
            .is_none()
        {
            return Err(self.error_with_text_location(
                format!(
                    "Statement {} is missing AST->HIR side-table mapping",
                    statement.id
                ),
                &statement.location,
            ));
        }

        if self
            .module
            .side_table
            .hir_source_id_for_hir(statement_location)
            .is_none()
        {
            return Err(self.error_with_text_location(
                format!(
                    "Statement {} is missing HIR source side-table mapping",
                    statement.id
                ),
                &statement.location,
            ));
        }

        Ok(())
    }

    fn validate_terminator_mapping(&self, block_id: BlockId) -> Result<(), CompilerError> {
        let terminator_location = HirLocation::Terminator(block_id);
        if self
            .module
            .side_table
            .ast_source_id_for_hir(terminator_location)
            .is_none()
        {
            return Err(self.error_with_hir(
                format!("Block {block_id} terminator is missing AST->HIR side-table mapping"),
                Some(terminator_location),
            ));
        }

        if self
            .module
            .side_table
            .hir_source_id_for_hir(terminator_location)
            .is_none()
        {
            return Err(self.error_with_hir(
                format!("Block {block_id} terminator is missing HIR source side-table mapping",),
                Some(terminator_location),
            ));
        }

        Ok(())
    }

    fn validate_statement(&self, statement: &HirStatement) -> Result<(), CompilerError> {
        let anchor = Some(HirLocation::Statement(statement.id));
        match &statement.kind {
            HirStatementKind::Assign { target, value } => {
                let _ = self.validate_place(target, anchor)?;
                self.validate_expression(value, anchor)?;
            }

            HirStatementKind::Call { args, result, .. } => {
                for arg in args {
                    self.validate_expression(arg, anchor)?;
                }

                if let Some(local_id) = result {
                    self.require_local_id(*local_id, anchor)?;
                }
            }

            HirStatementKind::Expr(expression) => {
                self.validate_expression(expression, anchor)?;
            }

            HirStatementKind::Drop(local) => {
                self.require_local_id(*local, anchor)?;
            }

            HirStatementKind::PushRuntimeFragment { vec_local, value } => {
                self.require_local_id(*vec_local, anchor)?;
                self.validate_expression(value, anchor)?;
            }
        }

        Ok(())
    }

    fn validate_terminator(
        &self,
        block_id: BlockId,
        terminator: &HirTerminator,
    ) -> Result<(), CompilerError> {
        let anchor = Some(HirLocation::Terminator(block_id));

        match terminator {
            HirTerminator::Jump { target, args } => {
                self.require_block_id(*target, anchor)?;
                self.require_same_function_cfg_owner(block_id, *target, anchor)?;
                for local in args {
                    self.require_local_id(*local, anchor)?;
                }
            }

            HirTerminator::If {
                condition,
                then_block,
                else_block,
            } => {
                self.validate_expression(condition, anchor)?;
                self.require_block_id(*then_block, anchor)?;
                self.require_same_function_cfg_owner(block_id, *then_block, anchor)?;
                self.require_block_id(*else_block, anchor)?;
                self.require_same_function_cfg_owner(block_id, *else_block, anchor)?;
            }

            HirTerminator::Match { scrutinee, arms } => {
                self.validate_expression(scrutinee, anchor)?;
                for arm in arms {
                    self.validate_match_arm(block_id, arm, anchor)?;
                }
            }

            HirTerminator::Break { target } | HirTerminator::Continue { target } => {
                self.require_block_id(*target, anchor)?;
                self.require_same_function_cfg_owner(block_id, *target, anchor)?;
            }

            HirTerminator::Return(value) => {
                self.validate_expression(value, anchor)?;
            }

            HirTerminator::Panic { message } => {
                if message.is_none() {
                    return Err(self.error_with_hir(
                        "Placeholder Panic(None) terminators are not allowed in validated HIR",
                        anchor,
                    ));
                }
                if let Some(message) = message {
                    self.validate_expression(message, anchor)?;
                }
            }
        }

        Ok(())
    }

    fn validate_match_arm(
        &self,
        source_block_id: BlockId,
        arm: &HirMatchArm,
        anchor: Option<HirLocation>,
    ) -> Result<(), CompilerError> {
        self.validate_pattern(&arm.pattern, anchor)?;

        if let Some(guard) = &arm.guard {
            self.validate_expression(guard, anchor)?;
        }

        self.require_block_id(arm.body, anchor)?;
        self.require_same_function_cfg_owner(source_block_id, arm.body, anchor)?;
        Ok(())
    }

    fn validate_pattern(
        &self,
        pattern: &HirPattern,
        anchor: Option<HirLocation>,
    ) -> Result<(), CompilerError> {
        match pattern {
            HirPattern::Literal(value) => {
                self.validate_literal_pattern_expression(value, anchor)?;
            }

            HirPattern::Wildcard => {}
        }

        Ok(())
    }

    fn validate_literal_pattern_expression(
        &self,
        expression: &HirExpression,
        anchor: Option<HirLocation>,
    ) -> Result<(), CompilerError> {
        self.validate_expression(expression, anchor)?;

        if expression.value_kind != ValueKind::Const {
            return Err(
                self.error_with_hir("Match literal pattern must have ValueKind::Const", anchor)
            );
        }

        if !matches!(
            expression.kind,
            HirExpressionKind::Int(_)
                | HirExpressionKind::Float(_)
                | HirExpressionKind::Bool(_)
                | HirExpressionKind::Char(_)
                | HirExpressionKind::StringLiteral(_)
        ) {
            return Err(self.error_with_hir(
                "Match literal pattern must be int/float/bool/char/string",
                anchor,
            ));
        }

        Ok(())
    }

    fn validate_expression(
        &self,
        expression: &HirExpression,
        anchor: Option<HirLocation>,
    ) -> Result<(), CompilerError> {
        let value_location = HirLocation::Value(expression.id);
        if self
            .module
            .side_table
            .ast_source_id_for_hir(value_location)
            .is_none()
        {
            return Err(self.error_with_hir(
                format!(
                    "Value {} is missing AST->HIR side-table mapping",
                    expression.id
                ),
                anchor,
            ));
        }

        if self
            .module
            .side_table
            .hir_source_id_for_hir(value_location)
            .is_none()
        {
            return Err(self.error_with_hir(
                format!(
                    "Value {} is missing HIR source side-table mapping",
                    expression.id
                ),
                anchor,
            ));
        }

        self.require_type_id(expression.ty, anchor)?;
        self.require_region_id(expression.region, anchor)?;

        match &expression.kind {
            HirExpressionKind::Int(_)
            | HirExpressionKind::Float(_)
            | HirExpressionKind::Bool(_)
            | HirExpressionKind::Char(_)
            | HirExpressionKind::StringLiteral(_) => {}

            HirExpressionKind::Load(place) => {
                let _ = self.validate_place(place, anchor)?;
            }

            HirExpressionKind::Copy(place) => {
                let _ = self.validate_place(place, anchor)?;
            }

            HirExpressionKind::BinOp { left, right, .. } => {
                self.validate_expression(left, anchor)?;
                self.validate_expression(right, anchor)?;
            }

            HirExpressionKind::UnaryOp { operand, .. } => {
                self.validate_expression(operand, anchor)?;
            }

            HirExpressionKind::StructConstruct { struct_id, fields } => {
                self.require_struct_id(*struct_id, anchor)?;
                for (field_id, field_expression) in fields {
                    self.require_field_owned_by(*field_id, *struct_id, anchor)?;
                    self.validate_expression(field_expression, anchor)?;
                }
            }

            HirExpressionKind::Collection(elements)
            | HirExpressionKind::TupleConstruct { elements } => {
                for element in elements {
                    self.validate_expression(element, anchor)?;
                }
            }

            HirExpressionKind::TupleGet { tuple, .. } => {
                self.validate_expression(tuple, anchor)?;
            }

            HirExpressionKind::Range { start, end } => {
                self.validate_expression(start, anchor)?;
                self.validate_expression(end, anchor)?;
            }

            HirExpressionKind::OptionConstruct { variant, value } => match (variant, value) {
                (crate::compiler_frontend::hir::hir_nodes::OptionVariant::Some, Some(value)) => {
                    self.validate_expression(value, anchor)?;
                }

                (crate::compiler_frontend::hir::hir_nodes::OptionVariant::None, None) => {}

                (crate::compiler_frontend::hir::hir_nodes::OptionVariant::Some, None)
                | (crate::compiler_frontend::hir::hir_nodes::OptionVariant::None, Some(_)) => {
                    return Err(self.error_with_hir(
                        "Invalid OptionConstruct variant/value pairing in HIR expression",
                        anchor,
                    ));
                }
            },

            HirExpressionKind::ResultConstruct { value, .. } => {
                self.validate_expression(value, anchor)?;
            }

            HirExpressionKind::ResultPropagate { result } => {
                self.validate_expression(result, anchor)?;
            }

            HirExpressionKind::ResultIsOk { result }
            | HirExpressionKind::ResultUnwrapOk { result }
            | HirExpressionKind::ResultUnwrapErr { result }
            | HirExpressionKind::BuiltinCast { value: result, .. } => {
                self.validate_expression(result, anchor)?;
            }
        }

        Ok(())
    }

    fn validate_place(
        &self,
        place: &HirPlace,
        anchor: Option<HirLocation>,
    ) -> Result<TypeId, CompilerError> {
        match place {
            HirPlace::Local(local_id) => self.local_types.get(local_id).copied().ok_or_else(|| {
                self.error_with_hir(format!("Unknown local id {local_id:?}"), anchor)
            }),

            HirPlace::Field { base, field } => {
                let base_type = self.validate_place(base, anchor)?;
                self.require_type_id(base_type, anchor)?;

                let base_struct_id = match &self.module.type_context.get(base_type).kind {
                    HirTypeKind::Struct { struct_id } => *struct_id,
                    _ => {
                        return Err(self.error_with_hir(
                            "Field place base does not resolve to struct type",
                            anchor,
                        ));
                    }
                };

                self.require_field_owned_by(*field, base_struct_id, anchor)?;
                self.field_types.get(field).copied().ok_or_else(|| {
                    self.error_with_hir(format!("Unknown field id {field:?}"), anchor)
                })
            }

            HirPlace::Index { base, index } => {
                self.validate_expression(index, anchor)?;
                let base_type = self.validate_place(base, anchor)?;
                self.require_type_id(base_type, anchor)?;

                match &self.module.type_context.get(base_type).kind {
                    HirTypeKind::Collection { element } => Ok(*element),
                    _ => Err(self.error_with_hir(
                        "Index place base does not resolve to collection type",
                        anchor,
                    )),
                }
            }
        }
    }

    fn require_block_id(
        &self,
        block_id: BlockId,
        anchor: Option<HirLocation>,
    ) -> Result<(), CompilerError> {
        if self.block_ids.contains(&block_id) {
            return Ok(());
        }

        Err(self.error_with_hir(format!("Unknown HIR block id {block_id:?}"), anchor))
    }

    fn require_struct_id(
        &self,
        struct_id: StructId,
        anchor: Option<HirLocation>,
    ) -> Result<(), CompilerError> {
        if self.struct_ids.contains(&struct_id) {
            return Ok(());
        }

        Err(self.error_with_hir(format!("Unknown HIR struct id {struct_id:?}"), anchor))
    }

    fn require_field_id(
        &self,
        field_id: FieldId,
        anchor: Option<HirLocation>,
    ) -> Result<(), CompilerError> {
        if self.field_ids.contains(&field_id) {
            return Ok(());
        }

        Err(self.error_with_hir(format!("Unknown HIR field id {field_id:?}"), anchor))
    }

    fn require_local_id(
        &self,
        local_id: LocalId,
        anchor: Option<HirLocation>,
    ) -> Result<(), CompilerError> {
        if self.local_types.contains_key(&local_id) {
            return Ok(());
        }

        Err(self.error_with_hir(format!("Unknown HIR local id {local_id:?}"), anchor))
    }

    fn require_region_id(
        &self,
        region_id: RegionId,
        anchor: Option<HirLocation>,
    ) -> Result<(), CompilerError> {
        if self.region_ids.contains(&region_id) {
            return Ok(());
        }

        Err(self.error_with_hir(format!("Unknown HIR region id {region_id:?}"), anchor))
    }

    fn require_type_id(
        &self,
        type_id: TypeId,
        anchor: Option<HirLocation>,
    ) -> Result<(), CompilerError> {
        if self.module.type_context.contains(type_id) {
            return Ok(());
        }

        Err(self.error_with_hir(format!("Unknown HIR type id {type_id:?}"), anchor))
    }

    fn require_same_function_cfg_owner(
        &self,
        source_block: BlockId,
        target_block: BlockId,
        anchor: Option<HirLocation>,
    ) -> Result<(), CompilerError> {
        let Some(source_owner) = self.block_owner_by_id.get(&source_block).copied() else {
            return Err(self.error_with_hir(
                format!("Block {source_block} has no function CFG owner"),
                anchor,
            ));
        };
        let Some(target_owner) = self.block_owner_by_id.get(&target_block).copied() else {
            return Err(self.error_with_hir(
                format!("Block {target_block} has no function CFG owner"),
                anchor,
            ));
        };

        if source_owner == target_owner {
            return Ok(());
        }

        Err(self.error_with_hir(
            format!(
                "CFG edge from block {source_block} to block {target_block} crosses function boundary ({source_owner:?} -> {target_owner:?})"
            ),
            anchor,
        ))
    }

    fn block_by_id(
        &self,
        block_id: BlockId,
    ) -> Result<&crate::compiler_frontend::hir::hir_nodes::HirBlock, CompilerError> {
        let Some(index) = self.block_index_by_id.get(&block_id).copied() else {
            return Err(self.error_with_hir(
                format!("Unknown HIR block id {block_id:?}"),
                Some(HirLocation::Block(block_id)),
            ));
        };

        Ok(&self.module.blocks[index])
    }

    fn require_field_owned_by(
        &self,
        field_id: FieldId,
        struct_id: StructId,
        anchor: Option<HirLocation>,
    ) -> Result<(), CompilerError> {
        self.require_field_id(field_id, anchor)?;
        self.require_struct_id(struct_id, anchor)?;

        let Some(owner) = self.field_owner.get(&field_id).copied() else {
            return Err(self.error_with_hir(
                format!("Field {field_id:?} has no owning struct in HIR metadata"),
                anchor,
            ));
        };

        if owner == struct_id {
            return Ok(());
        }

        Err(self.error_with_hir(
            format!("Field {field_id:?} is owned by struct {owner:?}, not {struct_id:?}"),
            anchor,
        ))
    }

    fn error_with_text_location(
        &self,
        message: impl Into<String>,
        location: &SourceLocation,
    ) -> CompilerError {
        CompilerError::new(message, location.clone(), ErrorType::HirTransformation)
    }

    fn error_with_hir(
        &self,
        message: impl Into<String>,
        anchor: Option<HirLocation>,
    ) -> CompilerError {
        let location = anchor
            .and_then(|hir_location| self.hir_error_location(hir_location))
            .unwrap_or_default();

        CompilerError::new(message, location, ErrorType::HirTransformation)
    }

    fn hir_error_location(&self, location: HirLocation) -> Option<SourceLocation> {
        self.module
            .side_table
            .hir_source_location_for_hir(location)
            .or_else(|| self.module.side_table.ast_location_for_hir(location))
            .cloned()
    }
}
