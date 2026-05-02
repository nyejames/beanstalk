//! Pass 6: AST node emission.
//!
//! WHAT: iterates sorted headers with full context (resolved signatures, receiver catalog,
//! per-file visibility) and lowers each header into typed AST nodes.
//! WHY: emission is the ONLY pass that parses executable bodies (function bodies, template
//! bodies, start body). All prior passes consume header shells without body parsing.
//! Top-level declaration shell reparsing does NOT happen here — shells were fully parsed
//! by the header stage and resolved by passes 2–5.
//!
//! Constants and choices are handled in earlier passes; they do not emit nodes here.
//! Struct node emission reads `resolved_struct_fields_by_path` populated in pass 3.

use super::build_state::AstBuildState;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::function_body_to_ast;
use crate::compiler_frontend::ast::import_bindings::FileImportBindings;
use crate::compiler_frontend::ast::module_ast::scope_context::{
    ContextKind, ReceiverMethodCatalog, ScopeContext, TopLevelDeclarationIndex,
};
use crate::compiler_frontend::ast::statements::functions::{
    FunctionReturn, FunctionSignature, ReturnSlot,
};
use crate::compiler_frontend::ast::templates::template::TemplateConstValueKind;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::headers::parse_file_headers::{Header, HeaderKind};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::projects::settings::IMPLICIT_START_FUNC_NAME;
use crate::timer_log;
use rustc_hash::FxHashMap;
use std::rc::Rc;

#[cfg(feature = "detailed_timers")]
use crate::compiler_frontend::compiler_messages::compiler_dev_logging::log_aggregated_duration;
#[cfg(feature = "detailed_timers")]
use std::time::Duration;
use std::time::Instant;

impl<'a> AstBuildState<'a> {
    /// Pass 6: Emit AST nodes for each header kind (functions, structs, templates).
    pub(in crate::compiler_frontend::ast) fn emit_ast_nodes(
        &mut self,
        sorted_headers: Vec<Header>,
        file_import_bindings: &FxHashMap<InternedPath, FileImportBindings>,
        receiver_methods: &Rc<ReceiverMethodCatalog>,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerMessages> {
        // Build the shared top-level declaration store once, after passes 3–4 have
        // fully resolved all declarations. Every function and start body clones only
        // the Rc pointer, not declaration data.
        let declaration_index_start = Instant::now();
        let top_level_declarations =
            Rc::new(TopLevelDeclarationIndex::new(self.declarations.clone()));
        timer_log!(
            declaration_index_start,
            "AST/node emission/top-level declaration index built in: "
        );
        let _ = declaration_index_start;

        let resolved_type_aliases = Rc::new(self.resolved_type_aliases_by_path.clone());

        #[cfg(feature = "detailed_timers")]
        let mut total_function_body_parse_time = Duration::default();
        #[cfg(feature = "detailed_timers")]
        let mut total_start_body_parse_time = Duration::default();
        #[cfg(feature = "detailed_timers")]
        let mut total_const_template_parse_time = Duration::default();
        #[cfg(feature = "detailed_timers")]
        let mut total_const_template_fold_time = Duration::default();
        #[cfg(feature = "detailed_timers")]
        let mut function_headers_emitted = 0usize;
        #[cfg(feature = "detailed_timers")]
        let mut start_headers_emitted = 0usize;
        #[cfg(feature = "detailed_timers")]
        let mut struct_headers_emitted = 0usize;
        #[cfg(feature = "detailed_timers")]
        let mut const_templates_emitted = 0usize;

        for header in sorted_headers {
            let bindings = file_import_bindings
                .get(&header.source_file)
                .cloned()
                .unwrap_or_default();
            let source_file_scope = header.canonical_source_file(string_table);

            match header.kind {
                // --- Functions ---
                HeaderKind::Function { .. } => {
                    let Some(resolved_signature) = self
                        .resolved_function_signatures_by_path
                        .get(&header.tokens.src_path)
                        .cloned()
                    else {
                        return Err(self.error_messages(
                            CompilerError::compiler_error(
                                "Function signature was not resolved before AST emission.",
                            ),
                            string_table,
                        ));
                    };

                    let mut visible_declarations = bindings.visible_symbol_paths.to_owned();
                    for parameter in &resolved_signature.signature.parameters {
                        visible_declarations.insert(parameter.id.to_owned());
                    }

                    // Build the function body context: top-level declarations are shared via Rc
                    // (no data copy); parameters live in local_declarations.
                    let mut context = ScopeContext::new(
                        ContextKind::Function,
                        header.tokens.src_path.to_owned(),
                        Rc::clone(&top_level_declarations),
                        self.external_package_registry.clone(),
                        resolved_signature.signature.return_data_types(),
                    )
                    .with_style_directives(self.style_directives)
                    .with_build_profile(self.build_profile)
                    .with_visible_declarations(visible_declarations)
                    .with_visible_external_symbols(bindings.visible_external_symbols.clone())
                    .with_visible_source_bindings(bindings.visible_source_bindings.clone())
                    .with_visible_type_aliases(bindings.visible_type_aliases.clone())
                    .with_resolved_type_aliases((*resolved_type_aliases).clone())
                    .with_project_path_resolver(self.project_path_resolver.clone())
                    .with_path_format_config(self.path_format_config.clone())
                    .with_rendered_path_usage_sink(self.rendered_path_usages.clone())
                    .with_receiver_methods(receiver_methods.clone())
                    .with_source_file_scope(source_file_scope.to_owned());
                    context.expected_error_type = resolved_signature
                        .signature
                        .error_return()
                        .map(|ret| ret.data_type().to_owned());
                    // Parameters belong in the local layer, not in top-level declarations.
                    context
                        .set_local_declarations(resolved_signature.signature.parameters.to_owned());

                    let mut token_stream = header.tokens;
                    let function_scope = context.scope.clone();

                    #[cfg(feature = "detailed_timers")]
                    let function_body_parse_start = Instant::now();
                    let body_result = function_body_to_ast(
                        &mut token_stream,
                        context,
                        &mut self.warnings,
                        string_table,
                    );

                    #[cfg(feature = "detailed_timers")]
                    {
                        total_function_body_parse_time += function_body_parse_start.elapsed();
                        function_headers_emitted += 1;
                    }

                    let body =
                        body_result.map_err(|error| self.error_messages(error, string_table))?;

                    // AST symbol IDs are stored as full InternedPath values and are unique
                    // module-wide, not only within a local scope.
                    self.ast.push(AstNode {
                        kind: NodeKind::Function(
                            token_stream.src_path,
                            resolved_signature.signature,
                            body,
                        ),
                        location: header.name_location,
                        scope: function_scope,
                    });
                }

                // --- Entry start function ---
                //
                // WHAT: lowers the entry-file top-level body into the implicit `start` function.
                // WHY: only the module entry file produces a start function. The body contains
                // `PushStartRuntimeFragment` nodes for each top-level template. The function
                // returns `Vec<String>` — the accumulated runtime fragment list. The HIR builder
                // adds the implicit return of the fragment vec.
                // Start functions are build-system-only and are not importable or callable.
                HeaderKind::StartFunction => {
                    let context = ScopeContext::new(
                        ContextKind::Module,
                        header.tokens.src_path.to_owned(),
                        Rc::clone(&top_level_declarations),
                        self.external_package_registry.clone(),
                        vec![],
                    )
                    .with_style_directives(self.style_directives)
                    .with_build_profile(self.build_profile)
                    .with_visible_declarations(bindings.visible_symbol_paths.to_owned())
                    .with_visible_external_symbols(bindings.visible_external_symbols.clone())
                    .with_visible_source_bindings(bindings.visible_source_bindings.clone())
                    .with_visible_type_aliases(bindings.visible_type_aliases.clone())
                    .with_resolved_type_aliases((*resolved_type_aliases).clone())
                    .with_project_path_resolver(self.project_path_resolver.clone())
                    .with_path_format_config(self.path_format_config.clone())
                    .with_rendered_path_usage_sink(self.rendered_path_usages.clone())
                    .with_receiver_methods(receiver_methods.clone())
                    .with_source_file_scope(source_file_scope.to_owned());

                    let mut token_stream = header.tokens;
                    let start_scope = context.scope.clone();

                    #[cfg(feature = "detailed_timers")]
                    let start_body_parse_start = Instant::now();
                    let body_result = function_body_to_ast(
                        &mut token_stream,
                        context,
                        &mut self.warnings,
                        string_table,
                    );

                    #[cfg(feature = "detailed_timers")]
                    {
                        total_start_body_parse_time += start_body_parse_start.elapsed();
                        start_headers_emitted += 1;
                    }

                    let body =
                        body_result.map_err(|error| self.error_messages(error, string_table))?;

                    let full_name = token_stream
                        .src_path
                        .join_str(IMPLICIT_START_FUNC_NAME, string_table);

                    // WHAT: entry start() returns Collection(StringSlice, MutableOwned),
                    //       which is the Beanstalk frontend type for Vec<String>.
                    // WHY: compiler-design-overview.md describes the return type as Vec<String>;
                    //      DataType::Collection(StringSlice) is the same contract
                    //      expressed in frontend DataType terms. The HIR builder adds the implicit
                    //      return of the accumulated fragment vec at function end.
                    let start_signature = FunctionSignature {
                        parameters: vec![],
                        returns: vec![ReturnSlot::success(FunctionReturn::Value(
                            DataType::Collection(Box::new(DataType::StringSlice)),
                        ))],
                    };

                    self.ast.push(AstNode {
                        kind: NodeKind::Function(full_name, start_signature, body),
                        location: header.name_location,
                        scope: start_scope,
                    });
                }

                // --- Structs ---
                HeaderKind::Struct { .. } => {
                    #[cfg(feature = "detailed_timers")]
                    {
                        struct_headers_emitted += 1;
                    }
                    let fields = self
                        .resolved_struct_fields_by_path
                        .get(&header.tokens.src_path)
                        .cloned()
                        .ok_or_else(|| {
                            self.error_messages(
                                CompilerError::compiler_error(
                                    "Struct fields were not resolved before AST emission.",
                                ),
                                string_table,
                            )
                        })?;

                    self.ast.push(AstNode {
                        kind: NodeKind::StructDefinition(header.tokens.src_path.to_owned(), fields),
                        location: header.name_location,
                        scope: header.tokens.src_path,
                    });
                }

                // Constants and choices are fully handled in earlier passes.
                HeaderKind::Constant { .. } | HeaderKind::Choice { .. } => {}

                // --- Const templates ---
                HeaderKind::ConstTemplate => {
                    let mut template_tokens = header.tokens;
                    let context = ScopeContext::new(
                        ContextKind::Constant,
                        template_tokens.src_path.to_owned(),
                        Rc::clone(&top_level_declarations),
                        self.external_package_registry.clone(),
                        vec![],
                    )
                    .with_style_directives(self.style_directives)
                    .with_build_profile(self.build_profile)
                    .with_visible_declarations(bindings.visible_symbol_paths.to_owned())
                    .with_visible_external_symbols(bindings.visible_external_symbols.clone())
                    .with_visible_source_bindings(bindings.visible_source_bindings.clone())
                    .with_visible_type_aliases(bindings.visible_type_aliases.clone())
                    .with_resolved_type_aliases((*resolved_type_aliases).clone())
                    .with_project_path_resolver(self.project_path_resolver.clone())
                    .with_path_format_config(self.path_format_config.clone())
                    .with_rendered_path_usage_sink(self.rendered_path_usages.clone())
                    .with_source_file_scope(source_file_scope);

                    #[cfg(feature = "detailed_timers")]
                    let const_template_parse_start = Instant::now();

                    let template_result =
                        Template::new(&mut template_tokens, &context, vec![], string_table);

                    #[cfg(feature = "detailed_timers")]
                    {
                        total_const_template_parse_time += const_template_parse_start.elapsed();
                    }
                    self.warnings.extend(context.take_emitted_warnings());
                    let template = template_result
                        .map_err(|error| self.error_messages(error, string_table))?;

                    match template.const_value_kind() {
                        // WHAT: top-level const templates can be direct strings or wrapper
                        // templates with optional, unfilled slots.
                        // WHY: unfilled slots are rendered as empty strings at compile time.
                        // Nested helper-owned contribution structure is legal while composing a
                        // wrapper, but the final top-level const value itself cannot be a raw
                        // `$insert(...)` helper artifact.
                        TemplateConstValueKind::RenderableString
                        | TemplateConstValueKind::WrapperTemplate => {}
                        TemplateConstValueKind::SlotInsertHelper => {
                            return Err(self.error_messages(
                                CompilerError::new_rule_error(
                                    "Top-level const templates cannot evaluate to '$insert(...)' helpers. Apply this insert while filling an immediate parent '$slot' template.",
                                    template.location,
                                ),
                                string_table,
                            ));
                        }
                        TemplateConstValueKind::NonConst => {
                            return Err(self.error_messages(
                                CompilerError::new_rule_error(
                                    "Top-level const templates must be fully foldable at compile time.",
                                    template.location,
                                ),
                                string_table,
                            ));
                        }
                    }

                    let mut fold_context = match context
                        .new_template_fold_context(string_table, "top-level const template folding")
                    {
                        Ok(ctx) => ctx,
                        Err(error) => {
                            return Err(self.error_messages(error, string_table));
                        }
                    };

                    #[cfg(feature = "detailed_timers")]
                    let const_template_fold_start = Instant::now();
                    let html = match template.fold_into_stringid(&mut fold_context) {
                        Ok(value) => value,
                        Err(error) => {
                            return Err(self.error_messages(error, string_table));
                        }
                    };

                    #[cfg(feature = "detailed_timers")]
                    {
                        total_const_template_fold_time += const_template_fold_start.elapsed();
                        const_templates_emitted += 1;
                    }

                    self.const_templates_by_path
                        .insert(template_tokens.src_path, html);
                }

                HeaderKind::TypeAlias { .. } => {
                    // Type aliases are compile-time-only metadata; they do not emit runtime nodes.
                }
            }
        }

        #[cfg(feature = "detailed_timers")]
        {
            log_aggregated_duration(
                "AST/node emission/function bodies parsed in: ",
                total_function_body_parse_time,
            );
            log_aggregated_duration(
                "AST/node emission/start bodies parsed in: ",
                total_start_body_parse_time,
            );
            log_aggregated_duration(
                "AST/node emission/const templates parsed in: ",
                total_const_template_parse_time,
            );
            log_aggregated_duration(
                "AST/node emission/const templates folded in: ",
                total_const_template_fold_time,
            );
            saying::say!(
                "AST/node emission/headers emitted: \n functions = ", Dark Green function_headers_emitted ,
                Reset "\n starts = ", Dark Green start_headers_emitted,
                Reset "\n structs = ", Dark Green struct_headers_emitted,
                Reset "\n const templates = ", Dark Green const_templates_emitted
            );
        }

        Ok(())
    }
}
