//! AST module environment builder.
//!
//! WHAT: consumes header-built import visibility and resolves declarations, constants, nominal
//! types, function signatures, and receiver catalog data into a stable semantic environment.
//! WHY: after this phase completes, AST emission can parse bodies against a stable environment
//! instead of depending on pass-order-specific accumulator fields.

use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration};
use crate::compiler_frontend::ast::module_ast::build_context::AstPhaseContext;
use crate::compiler_frontend::ast::module_ast::environment::TopLevelDeclarationTable;
use crate::compiler_frontend::ast::module_ast::scope_context::ReceiverMethodCatalog;
use crate::compiler_frontend::ast::type_resolution::ResolvedFunctionSignature;
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::generics::GenericNominalInstantiationCache;
use crate::compiler_frontend::declaration_syntax::type_syntax::{
    TypeResolutionContext, TypeResolutionContextInputs,
};
use crate::compiler_frontend::headers::import_environment::{
    FileVisibility, HeaderImportEnvironment,
};
use crate::compiler_frontend::headers::module_symbols::ModuleSymbols;
use crate::compiler_frontend::headers::parse_file_headers::Header;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::rendered_path_usage::RenderedPathUsage;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::timer_log;
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

pub(crate) struct AstModuleEnvironment {
    pub(crate) module_symbols: ModuleSymbols,
    pub(crate) import_environment: HeaderImportEnvironment,
    pub(crate) warnings: Vec<CompilerWarning>,
    pub(crate) declaration_table: Rc<TopLevelDeclarationTable>,
    pub(crate) module_constants: Vec<Declaration>,
    pub(crate) rendered_path_usages: Rc<RefCell<Vec<RenderedPathUsage>>>,
    pub(crate) builtin_struct_ast_nodes: Vec<AstNode>,
    pub(crate) resolved_struct_fields_by_path: FxHashMap<InternedPath, Vec<Declaration>>,
    pub(crate) resolved_function_signatures_by_path:
        FxHashMap<InternedPath, ResolvedFunctionSignature>,
    pub(crate) resolved_type_aliases_by_path: FxHashMap<InternedPath, DataType>,
    pub(crate) receiver_methods: Rc<ReceiverMethodCatalog>,
    pub(crate) generic_nominal_instantiations: Rc<GenericNominalInstantiationCache>,
}

/// Header-stage outputs consumed by AST environment construction.
///
/// WHAT: bundles module symbols and the header-built import environment into one named contract.
/// WHY: AST should receive header/dependency-sort output as a single type, not as loose
/// arguments split across `new` and `build`.
pub(crate) struct AstEnvironmentInput {
    pub(crate) module_symbols: ModuleSymbols,
    pub(crate) import_environment: HeaderImportEnvironment,
}

pub(crate) struct AstModuleEnvironmentBuilder<'context, 'services> {
    pub(crate) context: &'context AstPhaseContext<'services>,

    // Header-owned module symbol package from the header/dependency-sort phase.
    pub(crate) module_symbols: ModuleSymbols,

    // Header-built import visibility consumed directly; AST does not rebuild import bindings.
    pub(crate) import_environment: HeaderImportEnvironment,

    // Mutable environment-building state.
    pub(crate) warnings: Vec<CompilerWarning>,
    pub(crate) declaration_table: Rc<TopLevelDeclarationTable>,
    pub(crate) module_constants: Vec<Declaration>,
    pub(crate) rendered_path_usages: Rc<RefCell<Vec<RenderedPathUsage>>>,
    pub(crate) builtin_struct_ast_nodes: Vec<AstNode>,
    pub(crate) resolved_struct_fields_by_path: FxHashMap<InternedPath, Vec<Declaration>>,
    pub(crate) struct_source_by_path: FxHashMap<InternedPath, InternedPath>,
    pub(crate) resolved_function_signatures_by_path:
        FxHashMap<InternedPath, ResolvedFunctionSignature>,
    pub(crate) resolved_type_aliases_by_path: FxHashMap<InternedPath, DataType>,
    pub(crate) generic_nominal_instantiations: Rc<GenericNominalInstantiationCache>,
}

impl<'context, 'services> AstModuleEnvironmentBuilder<'context, 'services> {
    pub(crate) fn new(context: &'context AstPhaseContext<'services>) -> Self {
        Self {
            context,
            module_symbols: ModuleSymbols::empty(),
            import_environment: HeaderImportEnvironment::default(),
            warnings: Vec::new(),
            declaration_table: Rc::new(TopLevelDeclarationTable::new(Vec::new())),
            module_constants: Vec::new(),
            rendered_path_usages: Rc::new(RefCell::new(Vec::new())),
            builtin_struct_ast_nodes: Vec::new(),
            resolved_struct_fields_by_path: FxHashMap::default(),
            struct_source_by_path: FxHashMap::default(),
            resolved_function_signatures_by_path: FxHashMap::default(),
            resolved_type_aliases_by_path: FxHashMap::default(),
            generic_nominal_instantiations: Rc::new(GenericNominalInstantiationCache::new()),
        }
    }

    pub(crate) fn build(
        mut self,
        sorted_headers: &[Header],
        input: AstEnvironmentInput,
        string_table: &mut StringTable,
    ) -> Result<AstModuleEnvironment, CompilerMessages> {
        let AstEnvironmentInput {
            mut module_symbols,
            import_environment,
        } = input;

        // Move header-owned data into the builder state.
        let declarations = std::mem::take(&mut module_symbols.declarations);
        let builtin_struct_ast_nodes = std::mem::take(&mut module_symbols.builtin_struct_ast_nodes);
        let resolved_struct_fields_by_path =
            std::mem::take(&mut module_symbols.resolved_struct_fields_by_path);
        let struct_source_by_path = std::mem::take(&mut module_symbols.struct_source_by_path);

        self.module_symbols = module_symbols;
        self.import_environment = import_environment;
        self.warnings = self.import_environment.warnings.clone();
        self.declaration_table = Rc::new(TopLevelDeclarationTable::new(declarations));
        self.builtin_struct_ast_nodes = builtin_struct_ast_nodes;
        self.resolved_struct_fields_by_path = resolved_struct_fields_by_path;
        self.struct_source_by_path = struct_source_by_path;

        let environment_start = Instant::now();

        let type_alias_resolution_start = Instant::now();
        self.resolve_type_aliases(sorted_headers, string_table)?;
        timer_log!(
            type_alias_resolution_start,
            "AST/environment/type aliases resolved in: "
        );
        let _ = type_alias_resolution_start;

        let type_resolution_start = Instant::now();
        self.resolve_types(sorted_headers, string_table)?;
        timer_log!(
            type_resolution_start,
            "AST/environment/nominal types completed in: "
        );
        let _ = type_resolution_start;

        let function_signatures_start = Instant::now();
        self.resolve_function_signatures(sorted_headers, string_table)?;
        timer_log!(
            function_signatures_start,
            "AST/environment/function signatures resolved in: "
        );
        let _ = function_signatures_start;

        let receiver_catalog_start = Instant::now();
        let receiver_methods = self.build_receiver_catalog(sorted_headers, string_table)?;
        timer_log!(
            receiver_catalog_start,
            "AST/environment/receiver catalog built in: "
        );
        let _ = receiver_catalog_start;

        timer_log!(environment_start, "AST/build environment completed in: ");
        let _ = environment_start;

        Ok(AstModuleEnvironment {
            module_symbols: self.module_symbols,
            import_environment: self.import_environment,
            warnings: self.warnings,
            declaration_table: self.declaration_table,
            module_constants: self.module_constants,
            rendered_path_usages: self.rendered_path_usages,
            builtin_struct_ast_nodes: self.builtin_struct_ast_nodes,
            resolved_struct_fields_by_path: self.resolved_struct_fields_by_path,
            resolved_function_signatures_by_path: self.resolved_function_signatures_by_path,
            resolved_type_aliases_by_path: self.resolved_type_aliases_by_path,
            receiver_methods,
            generic_nominal_instantiations: self.generic_nominal_instantiations,
        })
    }

    pub(crate) fn replace_declaration(
        &mut self,
        declaration: Declaration,
    ) -> Result<(), CompilerError> {
        if self
            .declaration_table_mut()?
            .replace_by_path(declaration)
            .is_none()
        {
            return Err(CompilerError::compiler_error(
                "Resolved top-level declaration was not registered before AST resolution.",
            ));
        }

        Ok(())
    }

    pub(crate) fn declaration_table_mut(
        &mut self,
    ) -> Result<&mut TopLevelDeclarationTable, CompilerError> {
        Rc::get_mut(&mut self.declaration_table).ok_or_else(|| {
            CompilerError::compiler_error(
                "AST declaration table was still shared while environment construction tried to mutate it.",
            )
        })
    }

    /// Build a `TypeResolutionContext` from the current environment state and file visibility.
    ///
    /// WHAT: centralizes the repeated `TypeResolutionContext::from_inputs(...)` construction
    /// across type alias, struct field, choice variant, and function signature resolution.
    /// WHY: avoids duplicating the same 8-field initialization in four different files.
    pub(crate) fn type_resolution_context_for<'a>(
        &'a self,
        visibility: &'a FileVisibility,
        generic_parameters: Option<
            &'a crate::compiler_frontend::datatypes::generics::GenericParameterScope,
        >,
    ) -> TypeResolutionContext<'a> {
        let mut ctx = TypeResolutionContext::from_inputs(TypeResolutionContextInputs {
            declaration_table: &self.declaration_table,
            visible_declaration_ids: Some(&visibility.visible_declaration_paths),
            visible_external_symbols: Some(&visibility.visible_external_symbols),
            visible_source_bindings: Some(&visibility.visible_source_names),
            visible_type_aliases: Some(&visibility.visible_type_alias_names),
            resolved_type_aliases: Some(&self.resolved_type_aliases_by_path),
            generic_declarations_by_path: Some(&self.module_symbols.generic_declarations_by_path),
            resolved_struct_fields_by_path: Some(&self.resolved_struct_fields_by_path),
            generic_nominal_instantiations: Some(self.generic_nominal_instantiations.as_ref()),
        });
        if let Some(gp) = generic_parameters {
            ctx = ctx.with_generic_parameters(Some(gp));
        }
        ctx
    }

    pub(crate) fn error_messages(
        &self,
        error: CompilerError,
        string_table: &StringTable,
    ) -> CompilerMessages {
        CompilerMessages::from_error_with_warnings(error, self.warnings.clone(), string_table)
    }
}
