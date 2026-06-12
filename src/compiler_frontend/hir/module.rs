//! HIR module container.
//!
//! WHAT: the complete semantic IR payload produced for one Beanstalk module.
//! WHY: backends consume `HirModule` as the stable frontend output after AST lowering and borrow
//! validation.
//!
//! Type identity lives in the frontend `TypeEnvironment` carried beside the module at the
//! compiled-module boundary. HIR nodes store compact frontend `TypeId`s and do not own a separate
//! semantic type table.

use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::hir::blocks::HirBlock;
use crate::compiler_frontend::hir::const_facts::HirConstFacts;
use crate::compiler_frontend::hir::constants::{HirDocFragment, HirModuleConst};
use crate::compiler_frontend::hir::functions::{HirFunction, HirFunctionOrigin};
use crate::compiler_frontend::hir::hir_side_table::HirSideTable;
use crate::compiler_frontend::hir::ids::FunctionId;
use crate::compiler_frontend::hir::regions::HirRegion;
use crate::compiler_frontend::hir::structs::HirStruct;
use crate::compiler_frontend::paths::rendered_path_usage::RenderedPathUsage;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringIdRemap};
use rustc_hash::FxHashMap;

// -------------------------
//  Choice Layout Metadata
// -------------------------

/// Lowering-local choice layout entry.
///
/// WHY: HIR expressions and backends reference variants by stable `ChoiceId` and flat
/// variant/field indices. Semantic type identity (variant names, payload types) lives in the
/// frontend `TypeEnvironment`; `frontend_type_id` traces this entry back to the canonical type.
#[derive(Debug, Clone)]
pub struct HirChoice {
    /// The stable `ChoiceId` assigned during HIR lowering.
    /// WHY: preserved so diagnostics and backend validation can trust the choice
    ///      layout entry's own identity while walking HIR choices.
    pub id: crate::compiler_frontend::hir::ids::ChoiceId,

    /// Trace to the canonical frontend `TypeId` in `TypeEnvironment`.
    /// WHY: validation and backend checks use this to confirm HIR layout entries
    ///      still map back to canonical frontend type identity.
    pub frontend_type_id: TypeId,

    pub variants: Vec<HirChoiceVariant>,
}

#[derive(Debug, Clone)]
pub struct HirChoiceVariant {
    /// The interned variant name from the frontend `TypeEnvironment`.
    /// WHY: preserved for completeness and future diagnostic rendering.
    ///      Currently not read outside tests.
    #[allow(dead_code)]
    pub name: StringId,
    pub fields: Vec<HirChoiceField>,
}

#[derive(Debug, Clone)]
pub struct HirChoiceField {
    pub name: StringId,
    pub ty: TypeId,
}

// -------------------------
//  HIR Module Container
// -------------------------

#[derive(Debug, Clone)]
pub struct HirModule {
    pub blocks: Vec<HirBlock>,
    pub functions: Vec<HirFunction>,
    pub structs: Vec<HirStruct>,
    pub choices: Vec<HirChoice>,
    pub side_table: HirSideTable,

    /// Entry point for execution.
    pub start_function: FunctionId,
    /// Classification for every function in the module.
    ///
    /// WHY: backends/builders need explicit semantic role tagging to keep
    /// entry/runtime-template behavior stable across lowering passes.
    pub function_origins: FxHashMap<FunctionId, HirFunctionOrigin>,

    pub doc_fragments: Vec<HirDocFragment>,
    pub module_constants: Vec<HirModuleConst>,
    pub rendered_path_usages: Vec<RenderedPathUsage>,

    /// Region tree
    pub regions: Vec<HirRegion>,

    /// Advisory const facts projected from the AST for future optimization.
    ///
    /// WHAT: records which declarations are compile-time constants, their scope,
    ///       source, value kind, and source location.
    /// WHY: provides metadata for later borrow-checker and lowering optimizations
    ///      without changing HIR semantics today.
    pub const_facts: HirConstFacts,

    /// Warnings Collected along the way
    pub warnings: Vec<CompilerDiagnostic>,
}

impl HirModule {
    pub fn new() -> Self {
        Self {
            blocks: vec![],
            functions: vec![],
            structs: vec![],
            choices: vec![],
            side_table: HirSideTable::default(),
            start_function: FunctionId(0),
            function_origins: FxHashMap::default(),
            doc_fragments: vec![],
            module_constants: vec![],
            rendered_path_usages: vec![],
            regions: vec![],
            warnings: vec![],
            const_facts: HirConstFacts::default(),
        }
    }

    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.side_table.remap_string_ids(remap);
        self.const_facts.remap_string_ids(remap);

        for fragment in &mut self.doc_fragments {
            fragment.location.remap_string_ids(remap);
        }

        for usage in &mut self.rendered_path_usages {
            usage.source_path.remap_string_ids(remap);
            usage.public_path.remap_string_ids(remap);
            usage.source_file_scope.remap_string_ids(remap);
            usage.render_location.remap_string_ids(remap);
        }

        for warning in &mut self.warnings {
            warning.remap_string_ids(remap);
        }
    }
}
