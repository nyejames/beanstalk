//! AST module environment builder.
//!
//! WHAT: owns all mutation needed to build the shared semantic environment for a module.
//! WHY: after this phase completes, AST emission can parse bodies against a stable environment
//! instead of depending on pass-order-specific accumulator fields.

use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration};
use crate::compiler_frontend::ast::import_bindings::FileImportBindings;
use crate::compiler_frontend::ast::module_ast::build_context::AstPhaseContext;
use crate::compiler_frontend::ast::module_ast::scope_context::ReceiverMethodCatalog;
use crate::compiler_frontend::ast::type_resolution::ResolvedFunctionSignature;
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::generics::GenericNominalInstantiationCache;
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

pub(in crate::compiler_frontend::ast) struct AstModuleEnvironment {
    pub(in crate::compiler_frontend::ast) module_symbols: ModuleSymbols,
    pub(in crate::compiler_frontend::ast) file_import_bindings:
        FxHashMap<InternedPath, FileImportBindings>,
    pub(in crate::compiler_frontend::ast) warnings: Vec<CompilerWarning>,
    pub(in crate::compiler_frontend::ast) declarations: Vec<Declaration>,
    pub(in crate::compiler_frontend::ast) module_constants: Vec<Declaration>,
    pub(in crate::compiler_frontend::ast) rendered_path_usages: Rc<RefCell<Vec<RenderedPathUsage>>>,
    pub(in crate::compiler_frontend::ast) builtin_struct_ast_nodes: Vec<AstNode>,
    pub(in crate::compiler_frontend::ast) resolved_struct_fields_by_path:
        FxHashMap<InternedPath, Vec<Declaration>>,
    pub(in crate::compiler_frontend::ast) resolved_function_signatures_by_path:
        FxHashMap<InternedPath, ResolvedFunctionSignature>,
    pub(in crate::compiler_frontend::ast) resolved_type_aliases_by_path:
        FxHashMap<InternedPath, DataType>,
    pub(in crate::compiler_frontend::ast) receiver_methods: Rc<ReceiverMethodCatalog>,
    pub(in crate::compiler_frontend::ast) generic_nominal_instantiations:
        Rc<GenericNominalInstantiationCache>,
}

pub(in crate::compiler_frontend::ast) struct AstModuleEnvironmentBuilder<'context, 'services> {
    pub(in crate::compiler_frontend::ast) context: &'context AstPhaseContext<'services>,

    // Header-owned module symbol package from the header/dependency-sort phase.
    pub(in crate::compiler_frontend::ast) module_symbols: ModuleSymbols,

    // Mutable environment-building state.
    pub(in crate::compiler_frontend::ast) warnings: Vec<CompilerWarning>,
    pub(in crate::compiler_frontend::ast) declarations: Vec<Declaration>,
    pub(in crate::compiler_frontend::ast) module_constants: Vec<Declaration>,
    pub(in crate::compiler_frontend::ast) rendered_path_usages: Rc<RefCell<Vec<RenderedPathUsage>>>,
    pub(in crate::compiler_frontend::ast) builtin_struct_ast_nodes: Vec<AstNode>,
    pub(in crate::compiler_frontend::ast) resolved_struct_fields_by_path:
        FxHashMap<InternedPath, Vec<Declaration>>,
    pub(in crate::compiler_frontend::ast) struct_source_by_path:
        FxHashMap<InternedPath, InternedPath>,
    pub(in crate::compiler_frontend::ast) resolved_function_signatures_by_path:
        FxHashMap<InternedPath, ResolvedFunctionSignature>,
    pub(in crate::compiler_frontend::ast) resolved_type_aliases_by_path:
        FxHashMap<InternedPath, DataType>,
    pub(in crate::compiler_frontend::ast) generic_nominal_instantiations:
        Rc<GenericNominalInstantiationCache>,
}

impl<'context, 'services> AstModuleEnvironmentBuilder<'context, 'services> {
    pub(in crate::compiler_frontend::ast) fn new(
        context: &'context AstPhaseContext<'services>,
        mut module_symbols: ModuleSymbols,
    ) -> Self {
        let declarations = std::mem::take(&mut module_symbols.declarations);
        let builtin_struct_ast_nodes = std::mem::take(&mut module_symbols.builtin_struct_ast_nodes);
        let resolved_struct_fields_by_path =
            std::mem::take(&mut module_symbols.resolved_struct_fields_by_path);
        let struct_source_by_path = std::mem::take(&mut module_symbols.struct_source_by_path);

        Self {
            context,
            module_symbols,
            warnings: Vec::new(),
            declarations,
            module_constants: Vec::new(),
            rendered_path_usages: Rc::new(RefCell::new(Vec::new())),
            builtin_struct_ast_nodes,
            resolved_struct_fields_by_path,
            struct_source_by_path,
            resolved_function_signatures_by_path: FxHashMap::default(),
            resolved_type_aliases_by_path: FxHashMap::default(),
            generic_nominal_instantiations: Rc::new(GenericNominalInstantiationCache::new()),
        }
    }

    pub(in crate::compiler_frontend::ast) fn build(
        mut self,
        sorted_headers: &[Header],
        string_table: &mut StringTable,
    ) -> Result<AstModuleEnvironment, CompilerMessages> {
        let environment_start = Instant::now();

        let import_bindings_start = Instant::now();
        let file_import_bindings = self.resolve_import_bindings(string_table)?;
        timer_log!(
            import_bindings_start,
            "AST/environment/import bindings resolved in: "
        );
        let _ = import_bindings_start;

        let type_alias_resolution_start = Instant::now();
        self.resolve_type_aliases(sorted_headers, &file_import_bindings, string_table)?;
        timer_log!(
            type_alias_resolution_start,
            "AST/environment/type aliases resolved in: "
        );
        let _ = type_alias_resolution_start;

        let type_resolution_start = Instant::now();
        self.resolve_types(sorted_headers, &file_import_bindings, string_table)?;
        timer_log!(
            type_resolution_start,
            "AST/environment/nominal types completed in: "
        );
        let _ = type_resolution_start;

        let function_signatures_start = Instant::now();
        self.resolve_function_signatures(sorted_headers, &file_import_bindings, string_table)?;
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
            file_import_bindings,
            warnings: self.warnings,
            declarations: self.declarations,
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

    pub(in crate::compiler_frontend::ast) fn replace_declaration(
        &mut self,
        declaration: Declaration,
    ) -> Result<(), CompilerError> {
        let Some(existing) = self
            .declarations
            .iter_mut()
            .find(|candidate| candidate.id == declaration.id)
        else {
            return Err(CompilerError::compiler_error(
                "Resolved top-level declaration was not registered before AST resolution.",
            ));
        };

        *existing = declaration;
        Ok(())
    }

    pub(in crate::compiler_frontend::ast) fn error_messages(
        &self,
        error: CompilerError,
        string_table: &StringTable,
    ) -> CompilerMessages {
        CompilerMessages::from_error_with_warnings(error, self.warnings.clone(), string_table)
    }
}
