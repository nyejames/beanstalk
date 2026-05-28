//! AST module environment builder.
//!
//! WHAT: consumes header-built import visibility and resolves declarations, constants, nominal
//! types, function signatures, and receiver catalog data into a stable semantic environment.
//! WHY: after this phase completes, AST emission can parse bodies against a stable environment
//! instead of depending on pass-order-specific accumulator fields.

use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::generic_functions::GenericFunctionTemplate;
use crate::compiler_frontend::ast::module_ast::build_context::AstPhaseContext;
use crate::compiler_frontend::ast::module_ast::environment::{
    AstEnvironmentInput, AstModuleEnvironment, AstModuleLookups, TopLevelDeclarationTable,
};
use crate::compiler_frontend::ast::module_ast::scope_context::ReceiverMethodCatalog;
use crate::compiler_frontend::ast::type_resolution::ResolvedFunctionSignature;
use crate::compiler_frontend::ast::type_resolution::{
    TypeResolutionContext, TypeResolutionContextInputs, resolve_diagnostic_type_to_type_id_checked,
};
use crate::compiler_frontend::builtins::error_type::builtin_error_type_path;
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::definitions::{FieldDefinition, StructTypeDefinition};
use crate::compiler_frontend::datatypes::environment::{
    RegisteredGenericParameterList, TypeEnvironment,
};
use crate::compiler_frontend::datatypes::generic_parameters::GenericParameterScope;
use crate::compiler_frontend::datatypes::ids::{NominalTypeId, TypeId};
use crate::compiler_frontend::declaration_syntax::choice::ChoiceVariant;
use crate::compiler_frontend::headers::import_environment::{
    FileVisibility, HeaderImportEnvironment,
};
use crate::compiler_frontend::headers::module_symbols::{
    GenericDeclarationMetadata, ModuleSymbols,
};
use crate::compiler_frontend::headers::parse_file_headers::Header;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::rendered_path_usage::RenderedPathUsage;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::value_mode::ValueMode;
use crate::{benchmark_timer_log, timer_log};
use rustc_hash::FxHashMap;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;

pub(crate) struct AstModuleEnvironmentBuilder<'context, 'services> {
    pub(crate) context: &'context AstPhaseContext<'services>,

    // Header-owned module symbol package from the header/dependency-sort phase.
    pub(crate) module_symbols: ModuleSymbols,

    // Header-built import visibility consumed directly; AST does not rebuild import bindings.
    pub(crate) import_environment: HeaderImportEnvironment,

    // Mutable environment-building state.
    pub(crate) warnings: Vec<CompilerDiagnostic>,
    pub(crate) declaration_table: Rc<TopLevelDeclarationTable>,
    pub(crate) module_constants: Vec<Declaration>,
    pub(crate) rendered_path_usages: Rc<RefCell<Vec<RenderedPathUsage>>>,
    pub(crate) builtin_struct_ast_nodes: Vec<AstNode>,
    pub(crate) resolved_struct_fields_by_path: FxHashMap<InternedPath, Vec<Declaration>>,
    pub(crate) struct_source_by_path: FxHashMap<InternedPath, InternedPath>,
    pub(crate) choice_variant_shells_by_path: FxHashMap<InternedPath, Vec<ChoiceVariant>>,
    pub(crate) resolved_function_signatures_by_path:
        FxHashMap<InternedPath, ResolvedFunctionSignature>,
    pub(crate) generic_function_templates_by_path: FxHashMap<InternedPath, GenericFunctionTemplate>,
    pub(crate) resolved_type_aliases_by_path: FxHashMap<InternedPath, DataType>,
    pub(crate) generic_parameter_lists_by_path:
        FxHashMap<InternedPath, RegisteredGenericParameterList>,

    // Frontend semantic type identity built during environment construction.
    // WHY: parsed types are resolved into canonical TypeIds as declarations are processed.
    pub(crate) type_environment: TypeEnvironment,

    // Canonical TypeId for each nominal struct/choice registered in type_environment.
    pub(crate) nominal_type_ids_by_path: FxHashMap<InternedPath, TypeId>,
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
            choice_variant_shells_by_path: FxHashMap::default(),
            resolved_function_signatures_by_path: FxHashMap::default(),
            generic_function_templates_by_path: FxHashMap::default(),
            resolved_type_aliases_by_path: FxHashMap::default(),
            generic_parameter_lists_by_path: FxHashMap::default(),
            type_environment: TypeEnvironment::new(),
            nominal_type_ids_by_path: FxHashMap::default(),
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

        // ------------------------------------
        //  Register builtin semantic types
        // ------------------------------------
        self.register_builtin_structs_in_type_environment(string_table)?;

        // ----------------------
        //  Resolve type aliases
        // ----------------------
        let type_alias_resolution_start = Instant::now();
        self.resolve_type_aliases(sorted_headers, string_table)?;
        timer_log!(
            type_alias_resolution_start,
            "AST/environment/type aliases resolved in: "
        );
        let _ = type_alias_resolution_start;

        // -----------------------
        //  Resolve nominal types
        // -----------------------
        let type_resolution_start = Instant::now();
        self.resolve_types(sorted_headers, string_table)?;
        timer_log!(
            type_resolution_start,
            "AST/environment/nominal types completed in: "
        );
        let _ = type_resolution_start;

        // -----------------------------
        //  Resolve function signatures
        // -----------------------------
        let function_signatures_start = Instant::now();
        self.resolve_function_signatures(sorted_headers, string_table)?;
        timer_log!(
            function_signatures_start,
            "AST/environment/function signatures resolved in: "
        );
        let _ = function_signatures_start;

        // ------------------------
        //  Build receiver catalog
        // ------------------------
        let receiver_catalog_start = Instant::now();
        let receiver_methods = self.build_receiver_catalog(sorted_headers, string_table)?;
        self.validate_receiver_method_import_visibility(&receiver_methods, string_table)?;
        timer_log!(
            receiver_catalog_start,
            "AST/environment/receiver catalog built in: "
        );
        let _ = receiver_catalog_start;

        benchmark_timer_log!(
            environment_start,
            "ast_build_environment_ms",
            "AST/build environment completed in: "
        );
        let _ = environment_start;

        // Extract generic declarations before `self` is consumed by `finish_environment`.
        let generic_declarations_by_path =
            std::mem::take(&mut self.module_symbols.generic_declarations_by_path);

        Ok(self.finish_environment(receiver_methods, generic_declarations_by_path))
    }

    /// Assemble the completed immutable environment package consumed by body emission.
    ///
    /// WHAT: moves the builder's resolved side tables into `AstModuleLookups`
    /// and pairs them with the canonical `TypeEnvironment`.
    /// WHY: keeping final assembly in one helper makes `build` read as the
    /// semantic phase pipeline instead of ending with a large structural move.
    fn finish_environment(
        self,
        receiver_methods: Rc<ReceiverMethodCatalog>,
        generic_declarations_by_path: FxHashMap<InternedPath, GenericDeclarationMetadata>,
    ) -> AstModuleEnvironment {
        AstModuleEnvironment {
            lookups: Rc::new(AstModuleLookups {
                module_symbols: self.module_symbols,
                import_environment: self.import_environment,
                warnings: self.warnings,
                declaration_table: self.declaration_table,
                module_constants: self.module_constants,
                rendered_path_usages: self.rendered_path_usages,
                builtin_struct_ast_nodes: self.builtin_struct_ast_nodes,

                resolved_struct_fields_by_path: Rc::new(self.resolved_struct_fields_by_path),
                resolved_function_signatures_by_path: Rc::new(
                    self.resolved_function_signatures_by_path,
                ),
                generic_function_templates_by_path: Rc::new(
                    self.generic_function_templates_by_path,
                ),
                resolved_type_aliases_by_path: Rc::new(self.resolved_type_aliases_by_path),
                choice_variant_shells_by_path: Rc::new(self.choice_variant_shells_by_path),

                receiver_methods,
                generic_declarations_by_path: Rc::new(generic_declarations_by_path),
                nominal_type_ids_by_path: Rc::new(self.nominal_type_ids_by_path),

                external_package_registry: self.context.external_package_registry.clone(),
                style_directives: self.context.style_directives.clone(),
                build_profile: self.context.build_profile,
                project_path_resolver: self.context.project_path_resolver.clone(),
                path_format_config: self.context.path_format_config.clone(),
            }),
            type_environment: self.type_environment,
        }
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

    /// Register builtin struct definitions in the TypeEnvironment and update their
    /// declaration-table entries with real TypeIds.
    ///
    /// WHAT: builtin structs are created programmatically during header parsing with
    /// `TypeId(0)` placeholders. They must be canonicalised in `TypeEnvironment` before
    /// any expression parsing that touches their fields (e.g. `error.message`).
    /// WHY: body parsing queries `TypeEnvironment` via the `ScopeContext` environment;
    /// unregistered builtins return empty field lists and break field access.
    pub(crate) fn register_builtin_structs_in_type_environment(
        &mut self,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerMessages> {
        let builtin_paths = [builtin_error_type_path(string_table)];

        for path in &builtin_paths {
            let Some(fields) = self.resolved_struct_fields_by_path.get(path).cloned() else {
                continue;
            };

            let field_definitions =
                self.field_definitions_from_declarations(&fields, string_table)?;

            let struct_def = StructTypeDefinition {
                id: NominalTypeId(0),
                path: path.clone(),
                fields: field_definitions,
                generic_parameters: None,
                const_record: false,
            };
            let (_, struct_type_id) = self.type_environment.register_nominal_struct(struct_def);
            self.nominal_type_ids_by_path
                .insert(path.clone(), struct_type_id);

            // Build a placeholder declaration so the builtin struct is reachable
            // through the declaration table during body parsing.
            let declaration_location = fields
                .first()
                .map(|field| field.value.location.clone())
                .unwrap_or_default();

            self.replace_declaration(Declaration {
                id: path.clone(),
                value: Expression::new(
                    ExpressionKind::NoValue,
                    declaration_location,
                    struct_type_id,
                    DataType::runtime_struct(path.clone(), struct_type_id),
                    ValueMode::ImmutableReference,
                ),
            })
            .map_err(|error| self.error_messages(error, string_table))?;
        }

        Ok(())
    }

    /// Build a `TypeResolutionContext` from the current environment state and file visibility.
    ///
    /// WHAT: centralizes the repeated `TypeResolutionContext::from_inputs(...)` construction
    /// across type alias, struct field, choice variant, and function signature resolution.
    /// WHY: avoids duplicating the same 8-field initialization in four different files.
    pub(crate) fn type_resolution_context_for<'a>(
        &'a mut self,
        visibility: &'a FileVisibility,
        generic_parameters: Option<&'a GenericParameterScope>,
    ) -> TypeResolutionContext<'a> {
        let mut context = TypeResolutionContext::from_inputs(TypeResolutionContextInputs {
            declaration_table: &self.declaration_table,
            visible_declaration_ids: Some(&visibility.visible_declaration_paths),
            visible_external_symbols: Some(&visibility.visible_external_symbols),
            visible_source_bindings: Some(&visibility.visible_source_names),
            visible_type_aliases: Some(&visibility.visible_type_alias_names),
            resolved_type_aliases: Some(&self.resolved_type_aliases_by_path),
            generic_declarations_by_path: Some(&self.module_symbols.generic_declarations_by_path),
            resolved_struct_fields_by_path: Some(&self.resolved_struct_fields_by_path),
            type_environment: &mut self.type_environment,
            visible_namespace_records: Some(&visibility.visible_namespace_records),
        });
        if let Some(gp) = generic_parameters {
            context = context.with_generic_parameters(Some(gp));
        }
        context
    }

    /// Convert resolved AST member declarations into canonical type-environment fields.
    ///
    /// WHAT: struct fields and choice payload fields are resolved as AST `Declaration`s first,
    /// then written into `TypeEnvironment` as compact semantic member definitions.
    /// WHY: keeping the conversion on the environment builder centralizes diagnostic mapping
    /// at the AST environment boundary and avoids repeated large-error iterator closures.
    pub(crate) fn field_definitions_from_declarations(
        &mut self,
        fields: &[Declaration],
        string_table: &StringTable,
    ) -> Result<Box<[FieldDefinition]>, CompilerMessages> {
        let mut definitions = Vec::with_capacity(fields.len());

        for field in fields {
            let type_id = match resolve_diagnostic_type_to_type_id_checked(
                &field.value.diagnostic_type,
                &mut self.type_environment,
                &field.value.location,
            ) {
                Ok(type_id) => type_id,
                Err(diagnostic) => {
                    return Err(self.diagnostic_messages(*diagnostic, string_table));
                }
            };

            definitions.push(FieldDefinition {
                name: field.id.clone(),
                type_id,
                location: field.value.location.clone(),
            });
        }

        Ok(definitions.into_boxed_slice())
    }

    pub(crate) fn error_messages(
        &self,
        error: CompilerError,
        string_table: &StringTable,
    ) -> CompilerMessages {
        CompilerMessages::from_error_with_warnings(error, self.warnings.clone(), string_table)
            .with_type_context_for_all_diagnostics(self.type_environment.clone())
    }

    pub(crate) fn diagnostic_messages(
        &self,
        diagnostic: CompilerDiagnostic,
        string_table: &StringTable,
    ) -> CompilerMessages {
        CompilerMessages::from_diagnostic_with_warnings(
            diagnostic,
            self.warnings.clone(),
            string_table,
        )
        .with_type_context_for_all_diagnostics(self.type_environment.clone())
    }
}
