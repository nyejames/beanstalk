//! HIR Validation
//!
//! Always-on structural validation for new HIR modules.
//! This pass enforces core invariants so downstream analysis/backends can
//! rely on a consistent IR contract.

use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorLocation, ErrorType};
use crate::compiler_frontend::hir::hir_datatypes::{HirTypeKind, TypeId};
use crate::compiler_frontend::hir::hir_display::HirLocation;
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FieldId, FunctionId, HirExpression, HirExpressionKind, HirMatchArm, HirModule,
    HirPattern, HirPlace, HirStatement, HirStatementKind, HirTerminator, LocalId, RegionId,
    StartFragment, StructId, ValueKind,
};
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::TextLocation;
use rustc_hash::{FxHashMap, FxHashSet};

pub(crate) fn validate_hir_module(
    module: &HirModule,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    let mut validator = HirValidator::new(module, string_table);
    validator.validate()
}

struct HirValidator<'a> {
    module: &'a HirModule,
    string_table: &'a StringTable,

    block_ids: FxHashSet<BlockId>,
    function_ids: FxHashSet<FunctionId>,
    struct_ids: FxHashSet<StructId>,
    field_ids: FxHashSet<FieldId>,
    region_ids: FxHashSet<RegionId>,

    local_types: FxHashMap<LocalId, TypeId>,
    field_types: FxHashMap<FieldId, TypeId>,
    field_owner: FxHashMap<FieldId, StructId>,
}

impl<'a> HirValidator<'a> {
    fn new(module: &'a HirModule, string_table: &'a StringTable) -> Self {
        Self {
            module,
            string_table,
            block_ids: FxHashSet::default(),
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
        self.validate_start_function()?;
        self.validate_start_fragments()?;
        self.validate_functions()?;
        self.validate_blocks()?;
        Ok(())
    }

    fn collect_definition_ids(&mut self) -> Result<(), CompilerError> {
        for region in &self.module.regions {
            let id = region.id();
            if !self.region_ids.insert(id) {
                return Err(self.error_with_hir(format!("Duplicate HIR region id {:?}", id), None));
            }
        }

        for block in &self.module.blocks {
            if !self.block_ids.insert(block.id) {
                return Err(self.error_with_hir(
                    format!("Duplicate HIR block id {:?}", block.id),
                    Some(HirLocation::Block(block.id)),
                ));
            }

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

    fn validate_start_fragments(&self) -> Result<(), CompilerError> {
        for (index, fragment) in self.module.start_fragments.iter().enumerate() {
            match fragment {
                StartFragment::ConstString(const_string_id) => {
                    let pool_index = const_string_id.0 as usize;
                    if pool_index >= self.module.const_string_pool.len() {
                        return Err(self.error_with_hir(
                            format!(
                                "Start fragment #{index} references missing const_string_pool entry {}",
                                const_string_id.0
                            ),
                            None,
                        ));
                    }
                }

                StartFragment::RuntimeStringFn(function_id) => {
                    if !self.function_ids.contains(function_id) {
                        return Err(self.error_with_hir(
                            format!(
                                "Start fragment #{index} references missing runtime function {:?}",
                                function_id
                            ),
                            Some(HirLocation::Function(*function_id)),
                        ));
                    }
                }
            }
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

            for local in &function.params {
                self.require_local_id(*local, Some(HirLocation::Function(function.id)))?;
            }
        }

        Ok(())
    }

    fn validate_blocks(&self) -> Result<(), CompilerError> {
        for block in &self.module.blocks {
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
        // TODO: Require side-table mappings for placeholder terminators once lowering no longer
        // emits `Panic { message: None }` as an intermediate sentinel.
        if self
            .module
            .blocks
            .iter()
            .find(|block| block.id == block_id)
            .is_some_and(|block| matches!(block.terminator, HirTerminator::Panic { message: None }))
        {
            return Ok(());
        }

        let terminator_location = HirLocation::Terminator(block_id);
        if self
            .module
            .side_table
            .ast_source_id_for_hir(terminator_location)
            .is_none()
        {
            return Err(self.error_with_hir(
                format!(
                    "Block {} terminator is missing AST->HIR side-table mapping",
                    block_id
                ),
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
                format!(
                    "Block {} terminator is missing HIR source side-table mapping",
                    block_id
                ),
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
                self.require_block_id(*else_block, anchor)?;
            }

            HirTerminator::Match { scrutinee, arms } => {
                self.validate_expression(scrutinee, anchor)?;
                for arm in arms {
                    self.validate_match_arm(arm, anchor)?;
                }
            }

            HirTerminator::Loop { body, break_target } => {
                self.require_block_id(*body, anchor)?;
                self.require_block_id(*break_target, anchor)?;
            }

            HirTerminator::Break { target } | HirTerminator::Continue { target } => {
                self.require_block_id(*target, anchor)?;
            }

            HirTerminator::Return(value) => {
                self.validate_expression(value, anchor)?;
            }

            HirTerminator::Panic { message } => {
                if let Some(message) = message {
                    self.validate_expression(message, anchor)?;
                }
            }
        }

        Ok(())
    }

    fn validate_match_arm(
        &self,
        arm: &HirMatchArm,
        anchor: Option<HirLocation>,
    ) -> Result<(), CompilerError> {
        self.validate_pattern(&arm.pattern, anchor)?;

        if let Some(guard) = &arm.guard {
            self.validate_expression(guard, anchor)?;
        }

        self.require_block_id(arm.body, anchor)?;
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

            HirPattern::Binding { local, subpattern } => {
                self.require_local_id(*local, anchor)?;
                if let Some(subpattern) = subpattern {
                    self.validate_pattern(subpattern, anchor)?;
                }
            }

            HirPattern::Struct { struct_id, fields } => {
                self.require_struct_id(*struct_id, anchor)?;
                for (field_id, subpattern) in fields {
                    self.require_field_owned_by(*field_id, *struct_id, anchor)?;
                    self.validate_pattern(subpattern, anchor)?;
                }
            }

            HirPattern::Tuple { elements } => {
                for element in elements {
                    self.validate_pattern(element, anchor)?;
                }
            }

            HirPattern::Option { inner_pattern, .. } | HirPattern::Result { inner_pattern, .. } => {
                if let Some(inner_pattern) = inner_pattern {
                    self.validate_pattern(inner_pattern, anchor)?;
                }
            }

            HirPattern::Collection { elements, rest } => {
                for element in elements {
                    self.validate_pattern(element, anchor)?;
                }

                if let Some(rest_local) = rest {
                    self.require_local_id(*rest_local, anchor)?;
                }
            }
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
                self.error_with_hir(format!("Unknown local id {:?}", local_id), anchor)
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
                    self.error_with_hir(format!("Unknown field id {:?}", field), anchor)
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

        Err(self.error_with_hir(format!("Unknown HIR block id {:?}", block_id), anchor))
    }

    fn require_function_id(
        &self,
        function_id: FunctionId,
        anchor: Option<HirLocation>,
    ) -> Result<(), CompilerError> {
        if self.function_ids.contains(&function_id) {
            return Ok(());
        }

        Err(self.error_with_hir(format!("Unknown HIR function id {:?}", function_id), anchor))
    }

    fn require_struct_id(
        &self,
        struct_id: StructId,
        anchor: Option<HirLocation>,
    ) -> Result<(), CompilerError> {
        if self.struct_ids.contains(&struct_id) {
            return Ok(());
        }

        Err(self.error_with_hir(format!("Unknown HIR struct id {:?}", struct_id), anchor))
    }

    fn require_field_id(
        &self,
        field_id: FieldId,
        anchor: Option<HirLocation>,
    ) -> Result<(), CompilerError> {
        if self.field_ids.contains(&field_id) {
            return Ok(());
        }

        Err(self.error_with_hir(format!("Unknown HIR field id {:?}", field_id), anchor))
    }

    fn require_local_id(
        &self,
        local_id: LocalId,
        anchor: Option<HirLocation>,
    ) -> Result<(), CompilerError> {
        if self.local_types.contains_key(&local_id) {
            return Ok(());
        }

        Err(self.error_with_hir(format!("Unknown HIR local id {:?}", local_id), anchor))
    }

    fn require_region_id(
        &self,
        region_id: RegionId,
        anchor: Option<HirLocation>,
    ) -> Result<(), CompilerError> {
        if self.region_ids.contains(&region_id) {
            return Ok(());
        }

        Err(self.error_with_hir(format!("Unknown HIR region id {:?}", region_id), anchor))
    }

    fn require_type_id(
        &self,
        type_id: TypeId,
        anchor: Option<HirLocation>,
    ) -> Result<(), CompilerError> {
        if self.module.type_context.contains(type_id) {
            return Ok(());
        }

        Err(self.error_with_hir(format!("Unknown HIR type id {:?}", type_id), anchor))
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
                format!("Field {:?} has no owning struct in HIR metadata", field_id),
                anchor,
            ));
        };

        if owner == struct_id {
            return Ok(());
        }

        Err(self.error_with_hir(
            format!(
                "Field {:?} is owned by struct {:?}, not {:?}",
                field_id, owner, struct_id
            ),
            anchor,
        ))
    }

    fn error_with_text_location(
        &self,
        message: impl Into<String>,
        location: &TextLocation,
    ) -> CompilerError {
        CompilerError::new(
            message,
            location.to_error_location(self.string_table),
            ErrorType::HirTransformation,
        )
    }

    fn error_with_hir(
        &self,
        message: impl Into<String>,
        anchor: Option<HirLocation>,
    ) -> CompilerError {
        let location = anchor
            .and_then(|hir_location| self.hir_error_location(hir_location))
            .unwrap_or_else(ErrorLocation::default);

        CompilerError::new(message, location, ErrorType::HirTransformation)
    }

    fn hir_error_location(&self, location: HirLocation) -> Option<ErrorLocation> {
        self.module
            .side_table
            .hir_source_location_for_hir(location)
            .or_else(|| self.module.side_table.ast_location_for_hir(location))
            .map(|location| location.to_error_location(self.string_table))
    }
}
