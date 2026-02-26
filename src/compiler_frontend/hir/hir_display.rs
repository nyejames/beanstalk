//! HIR Display
//!
//! Responsible for providing a way to get location and variable name information back from HIR.
//!
//! This will be used to help the rest of the HIR and borrow checker stages to create and return useful errors and warnings.
//! (CompilerMessages)
//! It will also enable printing out Hir structures for easy debugging also.

use crate::backends::function_registry::CallTarget;
use crate::compiler_frontend::hir::hir_datatypes::{HirTypeKind, TypeContext, TypeId};
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FieldId, FunctionId, HirBinOp, HirBlock, HirExpression, HirExpressionKind, HirField,
    HirFunction, HirLocal, HirMatchArm, HirModule, HirNodeId, HirPattern, HirPlace, HirStatement,
    HirStatementKind, HirStruct, HirTerminator, HirValueId, LocalId, OptionVariant, RegionId,
    ResultVariant, StructId, ValueKind,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::TextLocation;
use rustc_hash::FxHashMap;
use std::fmt::{Display, Formatter, Result as FmtResult, Write as _};

const MAX_TYPE_RENDER_DEPTH: usize = 24;
const EMPTY_HIR_LOCATIONS: [HirLocation; 0] = [];

// ============================================================================
// HIR Location Side Table
// ============================================================================

/// Stable identifier for an interned source location.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct SourceLocationId(pub u32);

/// Canonical references into the HIR graph for source mapping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum HirLocation {
    Block(BlockId),
    Function(FunctionId),
    Struct(StructId),
    Field(FieldId),
    Local(LocalId),
    Statement(HirNodeId),
    Value(HirValueId),
    Terminator(BlockId),
}

impl Display for HirLocation {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            HirLocation::Block(id) => write!(f, "block({})", id),
            HirLocation::Function(id) => write!(f, "function({})", id),
            HirLocation::Struct(id) => write!(f, "struct({})", id),
            HirLocation::Field(id) => write!(f, "field({})", id),
            HirLocation::Local(id) => write!(f, "local({})", id),
            HirLocation::Statement(id) => write!(f, "statement({})", id),
            HirLocation::Value(id) => write!(f, "value({})", id),
            HirLocation::Terminator(block) => write!(f, "terminator({})", block),
        }
    }
}

impl From<BlockId> for HirLocation {
    fn from(value: BlockId) -> Self {
        HirLocation::Block(value)
    }
}

impl From<FunctionId> for HirLocation {
    fn from(value: FunctionId) -> Self {
        HirLocation::Function(value)
    }
}

impl From<StructId> for HirLocation {
    fn from(value: StructId) -> Self {
        HirLocation::Struct(value)
    }
}

impl From<FieldId> for HirLocation {
    fn from(value: FieldId) -> Self {
        HirLocation::Field(value)
    }
}

impl From<LocalId> for HirLocation {
    fn from(value: LocalId) -> Self {
        HirLocation::Local(value)
    }
}

impl From<HirNodeId> for HirLocation {
    fn from(value: HirNodeId) -> Self {
        HirLocation::Statement(value)
    }
}

impl From<HirValueId> for HirLocation {
    fn from(value: HirValueId) -> Self {
        HirLocation::Value(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TextLocationKey {
    scope: InternedPath,
    start_line: i32,
    start_column: i32,
    end_line: i32,
    end_column: i32,
}

impl From<&TextLocation> for TextLocationKey {
    fn from(value: &TextLocation) -> Self {
        Self {
            scope: value.scope.clone(),
            start_line: value.start_pos.line_number,
            start_column: value.start_pos.char_column,
            end_line: value.end_pos.line_number,
            end_column: value.end_pos.char_column,
        }
    }
}

/// Side-table for reversible AST <-> HIR source mapping plus human-readable names for HIR IDs.
///
/// Design goals:
/// - O(1) average lookups for all forward/backward mappings
/// - Location interning to avoid repeated `TextLocation` cloning
/// - Zero string formatting work during mapping writes
#[derive(Debug, Clone, Default)]
pub(crate) struct HirSideTable {
    source_locations: Vec<TextLocation>,
    source_location_index: FxHashMap<TextLocationKey, SourceLocationId>,

    ast_to_hir: FxHashMap<SourceLocationId, Vec<HirLocation>>,
    hir_to_ast: FxHashMap<HirLocation, SourceLocationId>,
    hir_to_source: FxHashMap<HirLocation, SourceLocationId>,

    // Store canonical path identity. Rendering and diagnostics derive leaf names from these.
    local_names: FxHashMap<LocalId, InternedPath>,
    function_names: FxHashMap<FunctionId, InternedPath>,
    struct_names: FxHashMap<StructId, InternedPath>,
    field_names: FxHashMap<FieldId, InternedPath>,
}

impl HirSideTable {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn with_capacities(location_capacity: usize, mapping_capacity: usize) -> Self {
        Self {
            source_locations: Vec::with_capacity(location_capacity),
            source_location_index: FxHashMap::with_capacity_and_hasher(
                location_capacity,
                Default::default(),
            ),
            ast_to_hir: FxHashMap::with_capacity_and_hasher(mapping_capacity, Default::default()),
            hir_to_ast: FxHashMap::with_capacity_and_hasher(mapping_capacity, Default::default()),
            hir_to_source: FxHashMap::with_capacity_and_hasher(
                mapping_capacity,
                Default::default(),
            ),
            local_names: FxHashMap::default(),
            function_names: FxHashMap::default(),
            struct_names: FxHashMap::default(),
            field_names: FxHashMap::default(),
        }
    }

    pub(crate) fn clear(&mut self) {
        self.source_locations.clear();
        self.source_location_index.clear();
        self.ast_to_hir.clear();
        self.hir_to_ast.clear();
        self.hir_to_source.clear();
        self.local_names.clear();
        self.function_names.clear();
        self.struct_names.clear();
        self.field_names.clear();
    }

    #[inline]
    pub(crate) fn intern_source_location(&mut self, location: &TextLocation) -> SourceLocationId {
        let key = TextLocationKey::from(location);

        if let Some(existing_id) = self.source_location_index.get(&key) {
            return *existing_id;
        }

        let new_id = SourceLocationId(self.source_locations.len() as u32);
        self.source_locations.push(location.clone());
        self.source_location_index.insert(key, new_id);

        new_id
    }

    #[inline]
    pub(crate) fn source_location(&self, id: SourceLocationId) -> Option<&TextLocation> {
        self.source_locations.get(id.0 as usize)
    }

    #[inline]
    pub(crate) fn source_id_for_location(
        &self,
        location: &TextLocation,
    ) -> Option<SourceLocationId> {
        let key = TextLocationKey::from(location);
        self.source_location_index.get(&key).copied()
    }

    #[inline]
    pub(crate) fn map_ast_to_hir(
        &mut self,
        ast_location: &TextLocation,
        hir_location: HirLocation,
    ) {
        let ast_id = self.intern_source_location(ast_location);

        let entry = self.ast_to_hir.entry(ast_id).or_default();
        if !entry.contains(&hir_location) {
            entry.push(hir_location);
        }

        self.hir_to_ast.insert(hir_location, ast_id);
    }

    pub(crate) fn map_ast_to_hir_many<I>(&mut self, ast_location: &TextLocation, hir_locations: I)
    where
        I: IntoIterator<Item = HirLocation>,
    {
        for hir_location in hir_locations {
            self.map_ast_to_hir(ast_location, hir_location);
        }
    }

    #[inline]
    pub(crate) fn map_hir_source_location(
        &mut self,
        hir_location: HirLocation,
        hir_source: &TextLocation,
    ) {
        let source_id = self.intern_source_location(hir_source);
        self.hir_to_source.insert(hir_location, source_id);
    }

    pub(crate) fn map_statement(&mut self, ast_location: &TextLocation, statement: &HirStatement) {
        let hir_location = HirLocation::Statement(statement.id);
        self.map_ast_to_hir(ast_location, hir_location);
        self.map_hir_source_location(hir_location, &statement.location);
    }

    pub(crate) fn map_value(
        &mut self,
        ast_location: &TextLocation,
        value_id: HirValueId,
        source_location: &TextLocation,
    ) {
        let hir_location = HirLocation::Value(value_id);
        self.map_ast_to_hir(ast_location, hir_location);
        self.map_hir_source_location(hir_location, source_location);
    }

    pub(crate) fn map_function(&mut self, ast_location: &TextLocation, function: &HirFunction) {
        let hir_location = HirLocation::Function(function.id);
        self.map_ast_to_hir(ast_location, hir_location);
        self.map_hir_source_location(hir_location, ast_location);
    }

    pub(crate) fn map_block(&mut self, ast_location: &TextLocation, block: &HirBlock) {
        let hir_location = HirLocation::Block(block.id);
        self.map_ast_to_hir(ast_location, hir_location);
        self.map_hir_source_location(hir_location, ast_location);
    }

    pub(crate) fn map_terminator(&mut self, ast_location: &TextLocation, block_id: BlockId) {
        let hir_location = HirLocation::Terminator(block_id);
        self.map_ast_to_hir(ast_location, hir_location);
        self.map_hir_source_location(hir_location, ast_location);
    }

    pub(crate) fn map_local_source(&mut self, local: &HirLocal) {
        if let Some(location) = &local.source_info {
            self.map_hir_source_location(HirLocation::Local(local.id), location);
        }
    }

    #[inline]
    pub(crate) fn value_source_location(&self, value_id: HirValueId) -> Option<&TextLocation> {
        self.hir_source_location_for_hir(HirLocation::Value(value_id))
    }

    #[inline]
    pub(crate) fn value_ast_location(&self, value_id: HirValueId) -> Option<&TextLocation> {
        self.ast_location_for_hir(HirLocation::Value(value_id))
    }

    #[inline]
    pub(crate) fn hir_locations_for_ast(&self, ast_location: &TextLocation) -> &[HirLocation] {
        let Some(ast_id) = self.source_id_for_location(ast_location) else {
            return &EMPTY_HIR_LOCATIONS;
        };
        self.hir_locations_for_source_id(ast_id)
    }

    #[inline]
    pub(crate) fn hir_locations_for_source_id(
        &self,
        ast_source: SourceLocationId,
    ) -> &[HirLocation] {
        self.ast_to_hir
            .get(&ast_source)
            .map(Vec::as_slice)
            .unwrap_or(&EMPTY_HIR_LOCATIONS)
    }

    #[inline]
    pub(crate) fn ast_source_id_for_hir(
        &self,
        hir_location: HirLocation,
    ) -> Option<SourceLocationId> {
        self.hir_to_ast.get(&hir_location).copied()
    }

    #[inline]
    pub(crate) fn ast_location_for_hir(&self, hir_location: HirLocation) -> Option<&TextLocation> {
        let source_id = self.ast_source_id_for_hir(hir_location)?;
        self.source_location(source_id)
    }

    #[inline]
    pub(crate) fn hir_source_id_for_hir(
        &self,
        hir_location: HirLocation,
    ) -> Option<SourceLocationId> {
        self.hir_to_source.get(&hir_location).copied()
    }

    #[inline]
    pub(crate) fn hir_source_location_for_hir(
        &self,
        hir_location: HirLocation,
    ) -> Option<&TextLocation> {
        let source_id = self.hir_source_id_for_hir(hir_location)?;
        self.source_location(source_id)
    }

    #[inline]
    pub(crate) fn bind_local_name(&mut self, local_id: LocalId, name: InternedPath) {
        self.local_names.insert(local_id, name);
    }

    #[inline]
    pub(crate) fn bind_function_name(&mut self, function_id: FunctionId, name: InternedPath) {
        self.function_names.insert(function_id, name);
    }

    #[inline]
    pub(crate) fn bind_struct_name(&mut self, struct_id: StructId, name: InternedPath) {
        self.struct_names.insert(struct_id, name);
    }

    #[inline]
    pub(crate) fn bind_field_name(&mut self, field_id: FieldId, name: InternedPath) {
        self.field_names.insert(field_id, name);
    }

    #[inline]
    pub(crate) fn local_name_path(&self, local_id: LocalId) -> Option<&InternedPath> {
        self.local_names.get(&local_id)
    }

    #[inline]
    pub(crate) fn function_name_path(&self, function_id: FunctionId) -> Option<&InternedPath> {
        self.function_names.get(&function_id)
    }

    #[inline]
    pub(crate) fn struct_name_path(&self, struct_id: StructId) -> Option<&InternedPath> {
        self.struct_names.get(&struct_id)
    }

    #[inline]
    pub(crate) fn field_name_path(&self, field_id: FieldId) -> Option<&InternedPath> {
        self.field_names.get(&field_id)
    }

    #[inline]
    pub(crate) fn local_name_id(&self, local_id: LocalId) -> Option<StringId> {
        self.local_name_path(local_id).and_then(InternedPath::name)
    }

    #[inline]
    pub(crate) fn function_name_id(&self, function_id: FunctionId) -> Option<StringId> {
        self.function_name_path(function_id)
            .and_then(InternedPath::name)
    }

    #[inline]
    pub(crate) fn struct_name_id(&self, struct_id: StructId) -> Option<StringId> {
        self.struct_name_path(struct_id)
            .and_then(InternedPath::name)
    }

    #[inline]
    pub(crate) fn field_name_id(&self, field_id: FieldId) -> Option<StringId> {
        self.field_name_path(field_id).and_then(InternedPath::name)
    }

    #[inline]
    pub(crate) fn resolve_local_name<'a>(
        &self,
        local_id: LocalId,
        string_table: &'a StringTable,
    ) -> Option<&'a str> {
        self.local_name_path(local_id)
            .and_then(|path| path.name_str(string_table))
    }

    #[inline]
    pub(crate) fn resolve_function_name<'a>(
        &self,
        function_id: FunctionId,
        string_table: &'a StringTable,
    ) -> Option<&'a str> {
        self.function_name_path(function_id)
            .and_then(|path| path.name_str(string_table))
    }

    #[inline]
    pub(crate) fn resolve_struct_name<'a>(
        &self,
        struct_id: StructId,
        string_table: &'a StringTable,
    ) -> Option<&'a str> {
        self.struct_name_path(struct_id)
            .and_then(|path| path.name_str(string_table))
    }

    #[inline]
    pub(crate) fn resolve_field_name<'a>(
        &self,
        field_id: FieldId,
        string_table: &'a StringTable,
    ) -> Option<&'a str> {
        self.field_name_path(field_id)
            .and_then(|path| path.name_str(string_table))
    }
}

// ============================================================================
// Rendering
// ============================================================================

#[derive(Debug, Clone, Copy)]
pub(crate) struct HirDisplayOptions {
    pub include_ids: bool,
    pub include_types: bool,
    pub include_value_kinds: bool,
    pub include_regions: bool,
    pub include_locations: bool,
    pub multiline_match_arms: bool,
}

impl Default for HirDisplayOptions {
    fn default() -> Self {
        Self {
            include_ids: true,
            include_types: true,
            include_value_kinds: false,
            include_regions: true,
            include_locations: false,
            multiline_match_arms: true,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct HirDisplayContext<'a> {
    string_table: &'a StringTable,
    side_table: Option<&'a HirSideTable>,
    type_context: Option<&'a TypeContext>,
    options: HirDisplayOptions,
}

impl<'a> HirDisplayContext<'a> {
    pub(crate) fn new(string_table: &'a StringTable) -> Self {
        Self {
            string_table,
            side_table: None,
            type_context: None,
            options: HirDisplayOptions::default(),
        }
    }

    pub(crate) fn with_side_table(mut self, side_table: &'a HirSideTable) -> Self {
        self.side_table = Some(side_table);
        self
    }

    pub(crate) fn with_type_context(mut self, type_context: &'a TypeContext) -> Self {
        self.type_context = Some(type_context);
        self
    }

    pub(crate) fn with_options(mut self, options: HirDisplayOptions) -> Self {
        self.options = options;
        self
    }

    pub(crate) fn render_module(&self, module: &HirModule) -> String {
        let mut out = String::with_capacity(
            module.blocks.len() * 160 + module.functions.len() * 64 + module.structs.len() * 64,
        );

        out.push_str("hir_module {\n");
        let _ = writeln!(
            out,
            "  start_function: {}",
            self.function_label(module.start_function)
        );
        let _ = writeln!(out, "  start_fragments: {}", module.start_fragments.len());
        let _ = writeln!(
            out,
            "  const_string_pool: {}",
            module.const_string_pool.len()
        );

        let _ = writeln!(out, "  regions: {}", module.regions.len());

        out.push_str("  functions:\n");
        if module.functions.is_empty() {
            out.push_str("    (none)\n");
        } else {
            for function in &module.functions {
                self.push_indented_line(&mut out, 4, &self.render_function(function));
            }
        }

        out.push_str("  structs:\n");
        if module.structs.is_empty() {
            out.push_str("    (none)\n");
        } else {
            for hir_struct in &module.structs {
                self.push_indented_line(&mut out, 4, &self.render_struct(hir_struct));
            }
        }

        out.push_str("  blocks:\n");
        if module.blocks.is_empty() {
            out.push_str("    (none)\n");
        } else {
            for block in &module.blocks {
                let block_rendered = self.render_block(block);
                self.push_indented_multiline(&mut out, 4, &block_rendered);
            }
        }

        if !module.warnings.is_empty() {
            let _ = writeln!(out, "  warnings: {}", module.warnings.len());
        }

        out.push('}');
        out
    }

    pub(crate) fn render_struct(&self, hir_struct: &HirStruct) -> String {
        let mut out = String::new();
        let _ = write!(out, "{} {{ ", self.struct_label(hir_struct.id));

        for (idx, field) in hir_struct.fields.iter().enumerate() {
            if idx > 0 {
                out.push_str(", ");
            }
            out.push_str(&self.render_field(field));
        }

        out.push_str(" }");
        out
    }

    pub(crate) fn render_field(&self, field: &HirField) -> String {
        if self.options.include_types {
            format!(
                "{}: {}",
                self.field_label(field.id),
                self.type_label(field.ty)
            )
        } else {
            self.field_label(field.id)
        }
    }

    pub(crate) fn render_function(&self, function: &HirFunction) -> String {
        let mut out = String::new();
        let params = function
            .params
            .iter()
            .map(|param| self.local_label(*param))
            .collect::<Vec<_>>()
            .join(", ");

        let _ = write!(out, "{}({})", self.function_label(function.id), params);

        if self.options.include_types {
            let _ = write!(out, " -> {}", self.type_label(function.return_type));
        }

        let _ = write!(out, " [entry: {}]", self.block_label(function.entry));
        out
    }

    pub(crate) fn render_block(&self, block: &HirBlock) -> String {
        let mut out = String::new();
        let _ = write!(out, "{} ", self.block_label(block.id));

        if self.options.include_regions {
            let _ = write!(out, "[region: {}]", self.region_label(block.region));
        }

        out.push('\n');

        if block.locals.is_empty() {
            out.push_str("  locals: (none)\n");
        } else {
            out.push_str("  locals:\n");
            for local in &block.locals {
                let rendered = self.render_local(local);
                self.push_indented_line(&mut out, 4, &rendered);
            }
        }

        if block.statements.is_empty() {
            out.push_str("  statements: (none)\n");
        } else {
            out.push_str("  statements:\n");
            for statement in &block.statements {
                let rendered = self.render_statement(statement);
                self.push_indented_line(&mut out, 4, &rendered);
            }
        }

        out.push_str("  terminator: ");
        if self.options.include_locations {
            if let Some(location) = self.side_table.and_then(|side| {
                side.hir_source_location_for_hir(HirLocation::Terminator(block.id))
            }) {
                let _ = write!(out, "@{} ", self.render_text_location(location));
            }
        }
        out.push_str(&self.render_terminator(&block.terminator));
        out.push('\n');

        out
    }

    pub(crate) fn render_local(&self, local: &HirLocal) -> String {
        let mut out = String::new();

        if local.mutable {
            out.push_str("mut ");
        }

        out.push_str(&self.local_label(local.id));

        if self.options.include_types {
            let _ = write!(out, ": {}", self.type_label(local.ty));
        }

        if self.options.include_regions {
            let _ = write!(out, " [{}]", self.region_label(local.region));
        }

        if self.options.include_locations {
            if let Some(location) = local.source_info.as_ref().or_else(|| {
                self.side_table
                    .and_then(|side| side.hir_source_location_for_hir(HirLocation::Local(local.id)))
            }) {
                let _ = write!(out, " @{}", self.render_text_location(location));
            }
        }

        out
    }

    pub(crate) fn render_statement(&self, statement: &HirStatement) -> String {
        let mut out = String::new();

        if self.options.include_ids {
            let _ = write!(out, "[{}] ", self.node_label(statement.id));
        }

        if self.options.include_locations {
            let _ = write!(out, "@{} ", self.render_text_location(&statement.location));
        }

        out.push_str(&self.render_statement_kind(&statement.kind));
        out
    }

    pub(crate) fn render_statement_kind(&self, kind: &HirStatementKind) -> String {
        match kind {
            HirStatementKind::Assign { target, value } => {
                format!(
                    "{} = {}",
                    self.render_place(target),
                    self.render_expression(value)
                )
            }
            HirStatementKind::Call {
                target,
                args,
                result,
            } => {
                let mut out = String::new();

                if let Some(local) = result {
                    let _ = write!(out, "{} = ", self.local_label(*local));
                }

                let args_rendered = args
                    .iter()
                    .map(|arg| self.render_expression(arg))
                    .collect::<Vec<_>>()
                    .join(", ");

                let _ = write!(
                    out,
                    "call {}({})",
                    self.render_call_target(target),
                    args_rendered
                );

                out
            }
            HirStatementKind::Expr(expr) => self.render_expression(expr),
            HirStatementKind::Drop(local) => format!("drop {}", self.local_label(*local)),
        }
    }

    pub(crate) fn render_terminator(&self, terminator: &HirTerminator) -> String {
        match terminator {
            HirTerminator::Jump { target, args } => {
                if args.is_empty() {
                    format!("jump {}", self.block_label(*target))
                } else {
                    let args_rendered = args
                        .iter()
                        .map(|arg| self.local_label(*arg))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("jump {}({})", self.block_label(*target), args_rendered)
                }
            }
            HirTerminator::If {
                condition,
                then_block,
                else_block,
            } => {
                format!(
                    "if {} -> {} else {}",
                    self.render_expression(condition),
                    self.block_label(*then_block),
                    self.block_label(*else_block)
                )
            }
            HirTerminator::Match { scrutinee, arms } => {
                if !self.options.multiline_match_arms {
                    let arms_rendered = arms
                        .iter()
                        .map(|arm| self.render_match_arm(arm))
                        .collect::<Vec<_>>()
                        .join(", ");
                    return format!(
                        "match {} {{ {} }}",
                        self.render_expression(scrutinee),
                        arms_rendered
                    );
                }

                let mut out = String::new();
                let _ = writeln!(out, "match {} {{", self.render_expression(scrutinee));
                for arm in arms {
                    let _ = writeln!(out, "  {},", self.render_match_arm(arm));
                }
                out.push('}');
                out
            }
            HirTerminator::Loop { body, break_target } => {
                format!(
                    "loop body: {} break: {}",
                    self.block_label(*body),
                    self.block_label(*break_target)
                )
            }
            HirTerminator::Break { target } => format!("break {}", self.block_label(*target)),
            HirTerminator::Continue { target } => {
                format!("continue {}", self.block_label(*target))
            }
            HirTerminator::Return(value) => format!("return {}", self.render_expression(value)),
            HirTerminator::Panic { message } => match message {
                Some(msg) => format!("panic {}", self.render_expression(msg)),
                None => "panic".to_owned(),
            },
        }
    }

    pub(crate) fn render_expression(&self, expr: &HirExpression) -> String {
        let mut out = String::new();

        if self.options.include_ids {
            let _ = write!(out, "[{}] ", self.value_label(expr.id));
        }

        out.push_str(&self.render_expression_kind(&expr.kind));

        if self.options.include_types {
            let _ = write!(out, " : {}", self.type_label(expr.ty));
        }

        if self.options.include_value_kinds {
            let _ = write!(out, " [{}]", self.value_kind_label(expr.value_kind));
        }

        out
    }

    pub(crate) fn render_expression_kind(&self, kind: &HirExpressionKind) -> String {
        match kind {
            HirExpressionKind::Int(value) => value.to_string(),
            HirExpressionKind::Float(value) => value.to_string(),
            HirExpressionKind::Bool(value) => value.to_string(),
            HirExpressionKind::Char(value) => format!("'{}'", value.escape_debug()),
            HirExpressionKind::StringLiteral(value) => {
                format!("\"{}\"", value.escape_debug())
            }
            HirExpressionKind::Load(place) => self.render_place(place),
            HirExpressionKind::BinOp { left, op, right } => format!(
                "({} {} {})",
                self.render_expression(left),
                op,
                self.render_expression(right)
            ),
            HirExpressionKind::UnaryOp { op, operand } => {
                format!("({}{})", op, self.render_expression(operand))
            }
            HirExpressionKind::StructConstruct { struct_id, fields } => {
                let mut out = String::new();
                let _ = write!(out, "{} {{ ", self.struct_label(*struct_id));

                for (idx, (field_id, expr)) in fields.iter().enumerate() {
                    if idx > 0 {
                        out.push_str(", ");
                    }
                    let _ = write!(
                        out,
                        "{}: {}",
                        self.field_label(*field_id),
                        self.render_expression(expr)
                    );
                }

                out.push_str(" }");
                out
            }
            HirExpressionKind::Collection(elements) => {
                let joined = elements
                    .iter()
                    .map(|element| self.render_expression(element))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("[{}]", joined)
            }
            HirExpressionKind::Range { start, end } => {
                format!(
                    "{}..{}",
                    self.render_expression(start),
                    self.render_expression(end)
                )
            }
            HirExpressionKind::TupleConstruct { elements } => {
                if elements.is_empty() {
                    return "()".to_owned();
                }

                let joined = elements
                    .iter()
                    .map(|element| self.render_expression(element))
                    .collect::<Vec<_>>()
                    .join(", ");

                if elements.len() == 1 {
                    format!("({},)", joined)
                } else {
                    format!("({})", joined)
                }
            }
            HirExpressionKind::OptionConstruct { variant, value } => match (variant, value) {
                (OptionVariant::Some, Some(expr)) => {
                    format!("Some({})", self.render_expression(expr))
                }
                (OptionVariant::None, None) => "None".to_owned(),
                (OptionVariant::Some, None) => "Some(<missing>)".to_owned(),
                (OptionVariant::None, Some(_)) => "None(<unexpected>)".to_owned(),
            },
            HirExpressionKind::ResultConstruct { variant, value } => {
                format!("{}({})", variant, self.render_expression(value))
            }
        }
    }

    pub(crate) fn render_place(&self, place: &HirPlace) -> String {
        match place {
            HirPlace::Local(local_id) => self.local_label(*local_id),
            HirPlace::Field { base, field } => {
                format!("{}.{}", self.render_place(base), self.field_label(*field))
            }
            HirPlace::Index { base, index } => {
                format!(
                    "{}[{}]",
                    self.render_place(base),
                    self.render_expression(index)
                )
            }
        }
    }

    pub(crate) fn render_pattern(&self, pattern: &HirPattern) -> String {
        match pattern {
            HirPattern::Literal(expr) => self.render_expression(expr),
            HirPattern::Wildcard => "_".to_owned(),
            HirPattern::Binding { local, subpattern } => match subpattern {
                Some(inner) => {
                    format!(
                        "{} @ {}",
                        self.local_label(*local),
                        self.render_pattern(inner)
                    )
                }
                None => self.local_label(*local),
            },
            HirPattern::Struct { struct_id, fields } => {
                let mut out = String::new();
                let _ = write!(out, "{} {{ ", self.struct_label(*struct_id));
                for (idx, (field_id, field_pattern)) in fields.iter().enumerate() {
                    if idx > 0 {
                        out.push_str(", ");
                    }
                    let _ = write!(
                        out,
                        "{}: {}",
                        self.field_label(*field_id),
                        self.render_pattern(field_pattern)
                    );
                }
                out.push_str(" }");
                out
            }
            HirPattern::Tuple { elements } => {
                let joined = elements
                    .iter()
                    .map(|element| self.render_pattern(element))
                    .collect::<Vec<_>>()
                    .join(", ");
                if elements.len() == 1 {
                    format!("({},)", joined)
                } else {
                    format!("({})", joined)
                }
            }
            HirPattern::Option {
                variant,
                inner_pattern,
            } => match (variant, inner_pattern) {
                (OptionVariant::Some, Some(pattern)) => {
                    format!("Some({})", self.render_pattern(pattern))
                }
                (OptionVariant::None, None) => "None".to_owned(),
                (OptionVariant::Some, None) => "Some(_)".to_owned(),
                (OptionVariant::None, Some(_)) => "None(_)".to_owned(),
            },
            HirPattern::Result {
                variant,
                inner_pattern,
            } => match inner_pattern {
                Some(pattern) => format!("{}({})", variant, self.render_pattern(pattern)),
                None => variant.to_string(),
            },
            HirPattern::Collection { elements, rest } => {
                let mut parts = elements
                    .iter()
                    .map(|element| self.render_pattern(element))
                    .collect::<Vec<_>>();

                if let Some(rest_local) = rest {
                    parts.push(format!("..{}", self.local_label(*rest_local)));
                }

                format!("[{}]", parts.join(", "))
            }
        }
    }

    pub(crate) fn render_match_arm(&self, arm: &HirMatchArm) -> String {
        let mut out = String::new();
        out.push_str(&self.render_pattern(&arm.pattern));

        if let Some(guard) = &arm.guard {
            let _ = write!(out, " if {}", self.render_expression(guard));
        }

        let _ = write!(out, " => {}", self.block_label(arm.body));
        out
    }

    fn render_call_target(&self, target: &CallTarget) -> String {
        target.as_string(self.string_table)
    }

    fn value_kind_label(&self, value_kind: ValueKind) -> &'static str {
        match value_kind {
            ValueKind::Place => "place",
            ValueKind::RValue => "rvalue",
            ValueKind::Const => "const",
        }
    }

    fn type_label(&self, ty: TypeId) -> String {
        let Some(type_context) = self.type_context else {
            return format!("t{}", ty.0);
        };

        self.render_type_with_context(type_context, ty, 0)
    }

    fn render_type_with_context(
        &self,
        type_context: &TypeContext,
        ty: TypeId,
        depth: usize,
    ) -> String {
        if depth >= MAX_TYPE_RENDER_DEPTH {
            return format!("t{}", ty.0);
        }

        let kind = &type_context.get(ty).kind;
        match kind {
            HirTypeKind::Bool => "Bool".to_owned(),
            HirTypeKind::Int => "Int".to_owned(),
            HirTypeKind::Float => "Float".to_owned(),
            HirTypeKind::Decimal => "Decimal".to_owned(),
            HirTypeKind::Char => "Char".to_owned(),
            HirTypeKind::String => "String".to_owned(),
            HirTypeKind::Range => "Range".to_owned(),
            HirTypeKind::Unit => "()".to_owned(),
            HirTypeKind::Tuple { fields } => {
                if fields.is_empty() {
                    return "()".to_owned();
                }

                let joined = fields
                    .iter()
                    .map(|field| self.render_type_with_context(type_context, *field, depth + 1))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("({})", joined)
            }
            HirTypeKind::Collection { element } => {
                format!(
                    "[{}]",
                    self.render_type_with_context(type_context, *element, depth + 1)
                )
            }
            HirTypeKind::Struct { struct_id } => self.struct_label(*struct_id),
            HirTypeKind::Function {
                receiver,
                params,
                returns,
            } => {
                let receiver = receiver.map(|recv| {
                    format!(
                        "{}.",
                        self.render_type_with_context(type_context, recv, depth + 1)
                    )
                });

                let params = params
                    .iter()
                    .map(|param| self.render_type_with_context(type_context, *param, depth + 1))
                    .collect::<Vec<_>>()
                    .join(", ");

                let returns = if returns.is_empty() {
                    "()".to_owned()
                } else if returns.len() == 1 {
                    self.render_type_with_context(type_context, returns[0], depth + 1)
                } else {
                    let joined = returns
                        .iter()
                        .map(|ret| self.render_type_with_context(type_context, *ret, depth + 1))
                        .collect::<Vec<_>>()
                        .join(", ");
                    format!("({})", joined)
                };

                match receiver {
                    Some(receiver) => format!("fn({}{})->{}", receiver, params, returns),
                    None => format!("fn({})->{}", params, returns),
                }
            }
            HirTypeKind::Option { inner } => {
                format!(
                    "Option<{}>",
                    self.render_type_with_context(type_context, *inner, depth + 1)
                )
            }
            HirTypeKind::Result { ok, err } => format!(
                "Result<{}, {}>",
                self.render_type_with_context(type_context, *ok, depth + 1),
                self.render_type_with_context(type_context, *err, depth + 1)
            ),
            HirTypeKind::Union { variants } => {
                let joined = variants
                    .iter()
                    .map(|variant| self.render_type_with_context(type_context, *variant, depth + 1))
                    .collect::<Vec<_>>()
                    .join(" | ");
                format!("union({})", joined)
            }
        }
    }

    fn local_label(&self, local_id: LocalId) -> String {
        if let Some(name) = self
            .side_table
            .and_then(|side| side.resolve_local_name(local_id, self.string_table))
        {
            return name.to_owned();
        }

        format!("l{}", local_id.0)
    }

    fn function_label(&self, function_id: FunctionId) -> String {
        if let Some(name) = self
            .side_table
            .and_then(|side| side.resolve_function_name(function_id, self.string_table))
        {
            return name.to_owned();
        }

        format!("fn{}", function_id.0)
    }

    fn struct_label(&self, struct_id: StructId) -> String {
        if let Some(name) = self
            .side_table
            .and_then(|side| side.resolve_struct_name(struct_id, self.string_table))
        {
            return name.to_owned();
        }

        format!("struct{}", struct_id.0)
    }

    fn field_label(&self, field_id: FieldId) -> String {
        if let Some(name) = self
            .side_table
            .and_then(|side| side.resolve_field_name(field_id, self.string_table))
        {
            return name.to_owned();
        }

        format!("field{}", field_id.0)
    }

    fn block_label(&self, block_id: BlockId) -> String {
        format!("bb{}", block_id.0)
    }

    fn node_label(&self, node_id: HirNodeId) -> String {
        format!("n{}", node_id.0)
    }

    fn value_label(&self, value_id: HirValueId) -> String {
        format!("v{}", value_id.0)
    }

    fn region_label(&self, region_id: RegionId) -> String {
        format!("r{}", region_id.0)
    }

    fn render_text_location(&self, location: &TextLocation) -> String {
        let scope = location.scope.to_string(self.string_table);
        format!(
            "{}:{}:{}-{}:{}",
            scope,
            location.start_pos.line_number,
            location.start_pos.char_column,
            location.end_pos.line_number,
            location.end_pos.char_column
        )
    }

    fn push_indented_line(&self, out: &mut String, indent: usize, line: &str) {
        for _ in 0..indent {
            out.push(' ');
        }
        out.push_str(line);
        out.push('\n');
    }

    fn push_indented_multiline(&self, out: &mut String, indent: usize, text: &str) {
        for line in text.lines() {
            self.push_indented_line(out, indent, line);
        }
    }
}

// ============================================================================
// Convenience Display Hooks
// ============================================================================

impl HirModule {
    pub(crate) fn display_with_table(&self, string_table: &StringTable) -> String {
        HirDisplayContext::new(string_table).render_module(self)
    }

    pub(crate) fn display_with_context(&self, display: &HirDisplayContext<'_>) -> String {
        display.render_module(self)
    }

    pub(crate) fn debug_string(&self, string_table: &StringTable) -> String {
        self.display_with_table(string_table)
    }
}

impl HirBlock {
    pub(crate) fn display_with_context(&self, display: &HirDisplayContext<'_>) -> String {
        display.render_block(self)
    }
}

impl HirFunction {
    pub(crate) fn display_with_context(&self, display: &HirDisplayContext<'_>) -> String {
        display.render_function(self)
    }
}

impl HirStruct {
    pub(crate) fn display_with_context(&self, display: &HirDisplayContext<'_>) -> String {
        display.render_struct(self)
    }
}

impl HirStatement {
    pub(crate) fn display_with_context(&self, display: &HirDisplayContext<'_>) -> String {
        display.render_statement(self)
    }
}

impl HirTerminator {
    pub(crate) fn display_with_context(&self, display: &HirDisplayContext<'_>) -> String {
        display.render_terminator(self)
    }
}

impl HirExpression {
    pub(crate) fn display_with_context(&self, display: &HirDisplayContext<'_>) -> String {
        display.render_expression(self)
    }
}

impl HirPlace {
    pub(crate) fn display_with_context(&self, display: &HirDisplayContext<'_>) -> String {
        display.render_place(self)
    }
}

impl HirPattern {
    pub(crate) fn display_with_context(&self, display: &HirDisplayContext<'_>) -> String {
        display.render_pattern(self)
    }
}

impl HirMatchArm {
    pub(crate) fn display_with_context(&self, display: &HirDisplayContext<'_>) -> String {
        display.render_match_arm(self)
    }
}

// ============================================================================
// Simple Token Displays
// ============================================================================

impl Display for BlockId {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "bb{}", self.0)
    }
}

impl Display for FunctionId {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "fn{}", self.0)
    }
}

impl Display for StructId {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "struct{}", self.0)
    }
}

impl Display for FieldId {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "field{}", self.0)
    }
}

impl Display for LocalId {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "l{}", self.0)
    }
}

impl Display for RegionId {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "r{}", self.0)
    }
}

impl Display for HirNodeId {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "n{}", self.0)
    }
}

impl Display for HirValueId {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        write!(f, "v{}", self.0)
    }
}

impl Display for HirBinOp {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            HirBinOp::Add => write!(f, "+"),
            HirBinOp::Sub => write!(f, "-"),
            HirBinOp::Mul => write!(f, "*"),
            HirBinOp::Div => write!(f, "/"),
            HirBinOp::Mod => write!(f, "%"),
            HirBinOp::Eq => write!(f, "=="),
            HirBinOp::Ne => write!(f, "!="),
            HirBinOp::Lt => write!(f, "<"),
            HirBinOp::Le => write!(f, "<="),
            HirBinOp::Gt => write!(f, ">"),
            HirBinOp::Ge => write!(f, ">="),
            HirBinOp::And => write!(f, "&&"),
            HirBinOp::Or => write!(f, "||"),
            HirBinOp::Root => write!(f, "root"),
            HirBinOp::Exponent => write!(f, "^"),
        }
    }
}

impl Display for crate::compiler_frontend::hir::hir_nodes::HirUnaryOp {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            crate::compiler_frontend::hir::hir_nodes::HirUnaryOp::Neg => write!(f, "-"),
            crate::compiler_frontend::hir::hir_nodes::HirUnaryOp::Not => write!(f, "!"),
        }
    }
}

impl Display for OptionVariant {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            OptionVariant::Some => write!(f, "Some"),
            OptionVariant::None => write!(f, "None"),
        }
    }
}

impl Display for ResultVariant {
    fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
        match self {
            ResultVariant::Ok => write!(f, "Ok"),
            ResultVariant::Err => write!(f, "Err"),
        }
    }
}
