//! HIR module container.
//!
//! WHAT: the complete semantic IR payload produced for one Beanstalk module.
//! WHY: backends consume `HirModule` as the stable frontend output after AST lowering and borrow
//! validation.

use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::hir::blocks::HirBlock;
use crate::compiler_frontend::hir::constants::{HirDocFragment, HirModuleConst};
use crate::compiler_frontend::hir::functions::{HirFunction, HirFunctionOrigin};
use crate::compiler_frontend::hir::hir_datatypes::TypeContext;
use crate::compiler_frontend::hir::hir_side_table::HirSideTable;
use crate::compiler_frontend::hir::ids::FunctionId;
use crate::compiler_frontend::hir::regions::HirRegion;
use crate::compiler_frontend::hir::structs::HirStruct;
use crate::compiler_frontend::paths::rendered_path_usage::RenderedPathUsage;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringIdRemap};
use rustc_hash::FxHashMap;

/// Registry entry for a nominal choice type.
///
/// WHY: the `choices` vec provides a dense `ChoiceId` namespace.
/// Alpha scope supports unit variants only; payload fields are intentionally omitted.
#[derive(Debug, Clone)]
pub struct HirChoice {
    #[allow(dead_code)]
    // Stored during lowering; existence checked by ChoiceId index in validation.
    pub id: crate::compiler_frontend::hir::ids::ChoiceId,
    #[allow(dead_code)] // Stored during lowering; not walked in Alpha validation.
    pub variants: Vec<HirChoiceVariant>,
}

#[derive(Debug, Clone)]
pub struct HirChoiceVariant {
    #[allow(dead_code)] // Stored during lowering; not read back in Alpha paths.
    pub name: StringId,
}

#[derive(Debug, Clone)]
pub struct HirModule {
    pub blocks: Vec<HirBlock>,
    pub functions: Vec<HirFunction>,
    pub structs: Vec<HirStruct>,
    pub choices: Vec<HirChoice>,
    pub type_context: TypeContext,
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

    /// Warnings Collected along the way
    pub warnings: Vec<CompilerWarning>,
}

impl HirModule {
    pub fn new() -> Self {
        Self {
            blocks: vec![],
            functions: vec![],
            structs: vec![],
            choices: vec![],
            type_context: TypeContext::default(),
            side_table: HirSideTable::default(),
            start_function: FunctionId(0),
            function_origins: FxHashMap::default(),
            doc_fragments: vec![],
            module_constants: vec![],
            rendered_path_usages: vec![],
            regions: vec![],
            warnings: vec![],
        }
    }

    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.side_table.remap_string_ids(remap);

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
