//! Pass 6: AST node emission.
//!
//! WHAT: iterates sorted headers with full context (resolved signatures, receiver catalog,
//! per-file visibility) and lowers each header into typed AST nodes.
//! WHY: emission is the first pass that touches function/template bodies; all prior passes
//! only collect metadata so emission can proceed in a single, well-typed traversal.

use super::build_state::AstBuildState;
use super::canonical_source_file_for_header;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::function_body_to_ast::function_body_to_ast;
use crate::compiler_frontend::ast::import_bindings::FileImportBindings;
use crate::compiler_frontend::ast::module_ast::scope_context::{
    ContextKind, ReceiverMethodCatalog, ScopeContext,
};
use crate::compiler_frontend::ast::statements::functions::{
    FunctionReturn, FunctionSignature, ReturnSlot,
};
use crate::compiler_frontend::ast::templates::template::TemplateConstValueKind;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::headers::parse_file_headers::{Header, HeaderKind};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::projects::settings::IMPLICIT_START_FUNC_NAME;
use rustc_hash::FxHashMap;
use std::rc::Rc;

impl<'a> AstBuildState<'a> {
    /// Pass 6: Emit AST nodes for each header kind (functions, structs, templates).
    pub(super) fn emit_ast_nodes(
        &mut self,
        sorted_headers: Vec<Header>,
        file_import_bindings: &FxHashMap<InternedPath, FileImportBindings>,
        receiver_methods: &Rc<ReceiverMethodCatalog>,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerMessages> {
        for header in sorted_headers {
            let bindings = file_import_bindings
                .get(&header.source_file)
                .cloned()
                .unwrap_or_default();
            let source_file_scope = canonical_source_file_for_header(&header, string_table);

            match header.kind {
                // --- Functions ---
                HeaderKind::Function { signature: _ } => {
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

                    let mut function_declarations = self.declarations.to_owned();
                    function_declarations
                        .extend(resolved_signature.signature.parameters.to_owned());
                    let mut visible_declarations = bindings.visible_symbol_paths.to_owned();
                    for parameter in &resolved_signature.signature.parameters {
                        visible_declarations.insert(parameter.id.to_owned());
                    }

                    // Function parameters should be available in the function body scope.
                    let mut context = ScopeContext::new(
                        ContextKind::Function,
                        header.tokens.src_path.to_owned(),
                        &function_declarations,
                        self.host_registry.clone(),
                        resolved_signature.signature.return_data_types(),
                    )
                    .with_style_directives(self.style_directives)
                    .with_build_profile(self.build_profile)
                    .with_visible_declarations(visible_declarations)
                    .with_start_import_aliases(bindings.start_aliases.to_owned())
                    .with_project_path_resolver(self.project_path_resolver.clone())
                    .with_path_format_config(self.path_format_config.clone())
                    .with_rendered_path_usage_sink(self.rendered_path_usages.clone())
                    .with_receiver_methods(receiver_methods.clone())
                    .with_source_file_scope(source_file_scope.to_owned());
                    context.expected_error_type = resolved_signature
                        .signature
                        .error_return()
                        .map(|ret| ret.data_type().to_owned());

                    let mut token_stream = header.tokens;

                    let body_result = function_body_to_ast(
                        &mut token_stream,
                        context.to_owned(),
                        &mut self.warnings,
                        string_table,
                    );
                    self.warnings.extend(context.take_emitted_warnings());

                    let body =
                        body_result.map_err(|error| self.error_messages(error, string_table))?;

                    // AST symbol IDs are stored as full InternedPath values and are unique
                    // module-wide, not only within a local scope.
                    self.ast.push(AstNode {
                        kind: NodeKind::Function(
                            token_stream.src_path,
                            resolved_signature.signature,
                            body.to_owned(),
                        ),
                        location: header.name_location,
                        scope: context.scope.clone(),
                    });
                }

                // --- Start (module entry-point) functions ---
                HeaderKind::StartFunction => {
                    let context = ScopeContext::new(
                        ContextKind::Module,
                        header.tokens.src_path.to_owned(),
                        &self.declarations,
                        self.host_registry.clone(),
                        vec![],
                    )
                    .with_style_directives(self.style_directives)
                    .with_build_profile(self.build_profile)
                    .with_visible_declarations(bindings.visible_symbol_paths.to_owned())
                    .with_start_import_aliases(bindings.start_aliases.to_owned())
                    .with_project_path_resolver(self.project_path_resolver.clone())
                    .with_path_format_config(self.path_format_config.clone())
                    .with_rendered_path_usage_sink(self.rendered_path_usages.clone())
                    .with_receiver_methods(receiver_methods.clone())
                    .with_source_file_scope(source_file_scope.to_owned());

                    let mut token_stream = header.tokens;

                    let body_result = function_body_to_ast(
                        &mut token_stream,
                        context.to_owned(),
                        &mut self.warnings,
                        string_table,
                    );
                    self.warnings.extend(context.take_emitted_warnings());

                    let mut body =
                        body_result.map_err(|error| self.error_messages(error, string_table))?;

                    // Add the automatic return statement for the start function.
                    let empty_string = string_table.get_or_intern(String::new());
                    body.push(AstNode {
                        kind: NodeKind::Return(vec![Expression::string_slice(
                            empty_string,
                            token_stream.current_location(),
                            Ownership::ImmutableOwned,
                        )]),
                        location: token_stream.current_location(),
                        scope: context.scope.clone(),
                    });

                    // Create an implicit "start" function that can be called by other modules.
                    let full_name = token_stream
                        .src_path
                        .join_str(IMPLICIT_START_FUNC_NAME, string_table);

                    let main_signature = FunctionSignature {
                        parameters: vec![],
                        returns: vec![ReturnSlot::success(FunctionReturn::Value(
                            DataType::StringSlice,
                        ))],
                    };

                    self.ast.push(AstNode {
                        kind: NodeKind::Function(full_name, main_signature, body),
                        location: header.name_location,
                        scope: context.scope.clone(),
                    });
                }

                // --- Structs ---
                HeaderKind::Struct { .. } => {
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
                        &self.declarations,
                        self.host_registry.clone(),
                        vec![],
                    )
                    .with_style_directives(self.style_directives)
                    .with_build_profile(self.build_profile)
                    .with_visible_declarations(bindings.visible_symbol_paths.to_owned())
                    .with_start_import_aliases(bindings.start_aliases.to_owned())
                    .with_project_path_resolver(self.project_path_resolver.clone())
                    .with_path_format_config(self.path_format_config.clone())
                    .with_rendered_path_usage_sink(self.rendered_path_usages.clone())
                    .with_source_file_scope(source_file_scope);

                    let template_result =
                        Template::new(&mut template_tokens, &context, vec![], string_table);
                    self.warnings.extend(context.take_emitted_warnings());
                    let template = template_result
                        .map_err(|error| self.error_messages(error, string_table))?;

                    match template.const_value_kind() {
                        // WHAT: top-level const templates can be direct strings or wrapper
                        // templates with optional, unfilled slots.
                        // WHY: unfilled slots are rendered as empty strings at compile time.
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

                    let html = match template.fold_into_stringid(&mut fold_context) {
                        Ok(value) => value,
                        Err(error) => {
                            return Err(self.error_messages(error, string_table));
                        }
                    };

                    self.const_templates_by_path
                        .insert(template_tokens.src_path, html);
                }
            }
        }
        Ok(())
    }
}
