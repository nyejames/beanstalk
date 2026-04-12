//! finalization and the top-level `Ast::new` orchestrator.
//!
//! WHAT: assembles the final `Ast` output from build state — strips doc-comment templates,
//! synthesizes start-template items, prepends builtin structs, then runs all passes in order.
//! WHY: keeping the orchestrator here makes the full pass sequence readable in one place.

use super::build_state::AstBuildState;
use crate::compiler_frontend::FrontendBuildProfile;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, ResultCallHandling,
};
use crate::compiler_frontend::ast::templates::template::TemplateAtom;
use crate::compiler_frontend::ast::templates::template::TemplateConstValueKind;
use crate::compiler_frontend::ast::templates::template_folding::TemplateFoldContext;
use crate::compiler_frontend::ast::templates::template_render_plan::TemplateRenderPlan;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::ast::templates::top_level_templates::{
    AstDocFragment, AstStartTemplateItem, collect_and_strip_comment_templates,
    synthesize_start_template_items,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::headers::parse_file_headers::{Header, TopLevelTemplateItem};
use crate::compiler_frontend::host_functions::HostRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::paths::rendered_path_usage::RenderedPathUsage;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;

/// Unified AST output for all source files in one compilation unit.
pub struct Ast {
    pub nodes: Vec<AstNode>,
    pub module_constants: Vec<Declaration>,
    pub doc_fragments: Vec<AstDocFragment>,

    // The path to the original entry point file.
    pub entry_path: InternedPath,

    pub start_template_items: Vec<AstStartTemplateItem>,
    pub rendered_path_usages: Vec<RenderedPathUsage>,
    pub warnings: Vec<CompilerWarning>,
}

/// Shared dependencies/configuration required to build one module AST.
///
/// WHAT: groups the long-lived frontend services and per-build settings used across all AST passes.
/// WHY: `Ast::new` should describe its high-level inputs without a long parameter list.
pub struct AstBuildContext<'a> {
    pub host_registry: &'a HostRegistry,
    pub style_directives: &'a StyleDirectiveRegistry,
    pub string_table: &'a mut StringTable,
    pub entry_dir: InternedPath,
    pub build_profile: FrontendBuildProfile,
    pub project_path_resolver: Option<ProjectPathResolver>,
    pub path_format_config: PathStringFormatConfig,
}

impl<'a> AstBuildState<'a> {
    fn normalize_ast_templates_for_hir(
        &mut self,
        project_path_resolver: &ProjectPathResolver,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerError> {
        let canonical_source_by_symbol_path = &self.canonical_source_by_symbol_path;
        let path_format_config = self.path_format_config;

        for node in &mut self.ast {
            let source_file_scope = canonical_source_by_symbol_path
                .get(&node.scope)
                .unwrap_or(&node.location.scope)
                .to_owned();
            Self::normalize_ast_node_templates(
                node,
                &source_file_scope,
                path_format_config,
                project_path_resolver,
                string_table,
            )?;
        }

        Ok(())
    }

    fn normalize_ast_node_templates(
        node: &mut AstNode,
        source_file_scope: &InternedPath,
        path_format_config: &PathStringFormatConfig,
        project_path_resolver: &ProjectPathResolver,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerError> {
        match &mut node.kind {
            NodeKind::VariableDeclaration(declaration) => Self::normalize_expression_templates(
                &mut declaration.value,
                source_file_scope,
                path_format_config,
                project_path_resolver,
                string_table,
            )?,

            NodeKind::Assignment { target, value } => {
                Self::normalize_ast_node_templates(
                    target,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
                Self::normalize_expression_templates(
                    value,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
            }

            NodeKind::FieldAccess { base, .. } => {
                Self::normalize_ast_node_templates(
                    base,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
            }

            NodeKind::MethodCall { receiver, args, .. } => {
                Self::normalize_ast_node_templates(
                    receiver,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
                for argument in args {
                    Self::normalize_expression_templates(
                        &mut argument.value,
                        source_file_scope,
                        path_format_config,
                        project_path_resolver,
                        string_table,
                    )?;
                }
            }

            NodeKind::FunctionCall { args, .. } | NodeKind::HostFunctionCall { args, .. } => {
                for argument in args {
                    Self::normalize_expression_templates(
                        &mut argument.value,
                        source_file_scope,
                        path_format_config,
                        project_path_resolver,
                        string_table,
                    )?;
                }
            }

            NodeKind::ResultHandledFunctionCall { args, handling, .. } => {
                for argument in args {
                    Self::normalize_expression_templates(
                        &mut argument.value,
                        source_file_scope,
                        path_format_config,
                        project_path_resolver,
                        string_table,
                    )?;
                }
                Self::normalize_result_handling_templates(
                    handling,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
            }

            NodeKind::MultiBind { value, .. } | NodeKind::Rvalue(value) => {
                Self::normalize_expression_templates(
                    value,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
            }

            NodeKind::StructDefinition(_, fields) => {
                for field in fields {
                    Self::normalize_expression_templates(
                        &mut field.value,
                        source_file_scope,
                        path_format_config,
                        project_path_resolver,
                        string_table,
                    )?;
                }
            }

            NodeKind::Function(_, _, body) => {
                for statement in body {
                    Self::normalize_ast_node_templates(
                        statement,
                        source_file_scope,
                        path_format_config,
                        project_path_resolver,
                        string_table,
                    )?;
                }
            }

            NodeKind::Return(values) => {
                for value in values {
                    Self::normalize_expression_templates(
                        value,
                        source_file_scope,
                        path_format_config,
                        project_path_resolver,
                        string_table,
                    )?;
                }
            }

            NodeKind::ReturnError(value) => Self::normalize_expression_templates(
                value,
                source_file_scope,
                path_format_config,
                project_path_resolver,
                string_table,
            )?,

            NodeKind::If(condition, then_body, else_body) => {
                Self::normalize_expression_templates(
                    condition,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
                for statement in then_body {
                    Self::normalize_ast_node_templates(
                        statement,
                        source_file_scope,
                        path_format_config,
                        project_path_resolver,
                        string_table,
                    )?;
                }
                if let Some(else_body) = else_body {
                    for statement in else_body {
                        Self::normalize_ast_node_templates(
                            statement,
                            source_file_scope,
                            path_format_config,
                            project_path_resolver,
                            string_table,
                        )?;
                    }
                }
            }

            NodeKind::Match(scrutinee, arms, default) => {
                Self::normalize_expression_templates(
                    scrutinee,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
                for arm in arms {
                    Self::normalize_expression_templates(
                        &mut arm.condition,
                        source_file_scope,
                        path_format_config,
                        project_path_resolver,
                        string_table,
                    )?;
                    for statement in &mut arm.body {
                        Self::normalize_ast_node_templates(
                            statement,
                            source_file_scope,
                            path_format_config,
                            project_path_resolver,
                            string_table,
                        )?;
                    }
                }
                if let Some(default_body) = default {
                    for statement in default_body {
                        Self::normalize_ast_node_templates(
                            statement,
                            source_file_scope,
                            path_format_config,
                            project_path_resolver,
                            string_table,
                        )?;
                    }
                }
            }

            NodeKind::RangeLoop {
                bindings,
                range,
                body,
            } => {
                if let Some(item_binding) = &mut bindings.item {
                    Self::normalize_expression_templates(
                        &mut item_binding.value,
                        source_file_scope,
                        path_format_config,
                        project_path_resolver,
                        string_table,
                    )?;
                }
                if let Some(index_binding) = &mut bindings.index {
                    Self::normalize_expression_templates(
                        &mut index_binding.value,
                        source_file_scope,
                        path_format_config,
                        project_path_resolver,
                        string_table,
                    )?;
                }
                Self::normalize_expression_templates(
                    &mut range.start,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
                Self::normalize_expression_templates(
                    &mut range.end,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
                if let Some(step) = &mut range.step {
                    Self::normalize_expression_templates(
                        step,
                        source_file_scope,
                        path_format_config,
                        project_path_resolver,
                        string_table,
                    )?;
                }
                for statement in body {
                    Self::normalize_ast_node_templates(
                        statement,
                        source_file_scope,
                        path_format_config,
                        project_path_resolver,
                        string_table,
                    )?;
                }
            }

            NodeKind::CollectionLoop {
                bindings,
                iterable,
                body,
            } => {
                if let Some(item_binding) = &mut bindings.item {
                    Self::normalize_expression_templates(
                        &mut item_binding.value,
                        source_file_scope,
                        path_format_config,
                        project_path_resolver,
                        string_table,
                    )?;
                }
                if let Some(index_binding) = &mut bindings.index {
                    Self::normalize_expression_templates(
                        &mut index_binding.value,
                        source_file_scope,
                        path_format_config,
                        project_path_resolver,
                        string_table,
                    )?;
                }
                Self::normalize_expression_templates(
                    iterable,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
                for statement in body {
                    Self::normalize_ast_node_templates(
                        statement,
                        source_file_scope,
                        path_format_config,
                        project_path_resolver,
                        string_table,
                    )?;
                }
            }

            NodeKind::WhileLoop(condition, body) => {
                Self::normalize_expression_templates(
                    condition,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
                for statement in body {
                    Self::normalize_ast_node_templates(
                        statement,
                        source_file_scope,
                        path_format_config,
                        project_path_resolver,
                        string_table,
                    )?;
                }
            }

            NodeKind::Break | NodeKind::Continue | NodeKind::Operator(_) => {}
        }

        Ok(())
    }

    fn normalize_result_handling_templates(
        handling: &mut ResultCallHandling,
        source_file_scope: &InternedPath,
        path_format_config: &PathStringFormatConfig,
        project_path_resolver: &ProjectPathResolver,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerError> {
        match handling {
            ResultCallHandling::Fallback(fallback_values) => {
                for fallback in fallback_values {
                    Self::normalize_expression_templates(
                        fallback,
                        source_file_scope,
                        path_format_config,
                        project_path_resolver,
                        string_table,
                    )?;
                }
            }
            ResultCallHandling::Handler { fallback, body, .. } => {
                if let Some(fallback_values) = fallback {
                    for fallback in fallback_values {
                        Self::normalize_expression_templates(
                            fallback,
                            source_file_scope,
                            path_format_config,
                            project_path_resolver,
                            string_table,
                        )?;
                    }
                }
                for statement in body {
                    Self::normalize_ast_node_templates(
                        statement,
                        source_file_scope,
                        path_format_config,
                        project_path_resolver,
                        string_table,
                    )?;
                }
            }
            ResultCallHandling::Propagate => {}
        }

        Ok(())
    }

    fn normalize_expression_templates(
        expression: &mut Expression,
        source_file_scope: &InternedPath,
        path_format_config: &PathStringFormatConfig,
        project_path_resolver: &ProjectPathResolver,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerError> {
        let folded_template = match &mut expression.kind {
            ExpressionKind::Copy(place) => {
                Self::normalize_ast_node_templates(
                    place,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
                None
            }

            ExpressionKind::Runtime(nodes) => {
                for node in nodes {
                    Self::normalize_ast_node_templates(
                        node,
                        source_file_scope,
                        path_format_config,
                        project_path_resolver,
                        string_table,
                    )?;
                }
                None
            }

            ExpressionKind::Function(_, body) => {
                for node in body {
                    Self::normalize_ast_node_templates(
                        node,
                        source_file_scope,
                        path_format_config,
                        project_path_resolver,
                        string_table,
                    )?;
                }
                None
            }

            ExpressionKind::FunctionCall(_, args)
            | ExpressionKind::HostFunctionCall(_, args)
            | ExpressionKind::Collection(args) => {
                for argument in args {
                    Self::normalize_expression_templates(
                        argument,
                        source_file_scope,
                        path_format_config,
                        project_path_resolver,
                        string_table,
                    )?;
                }
                None
            }

            ExpressionKind::ResultHandledFunctionCall { args, handling, .. } => {
                for argument in args {
                    Self::normalize_expression_templates(
                        argument,
                        source_file_scope,
                        path_format_config,
                        project_path_resolver,
                        string_table,
                    )?;
                }
                Self::normalize_result_handling_templates(
                    handling,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
                None
            }

            ExpressionKind::BuiltinCast { value, .. }
            | ExpressionKind::ResultConstruct { value, .. }
            | ExpressionKind::Coerced { value, .. } => {
                Self::normalize_expression_templates(
                    value,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
                None
            }

            ExpressionKind::HandledResult { value, handling } => {
                Self::normalize_expression_templates(
                    value,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
                Self::normalize_result_handling_templates(
                    handling,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
                None
            }

            ExpressionKind::Template(template) => {
                Self::normalize_template_for_hir(
                    template,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;

                match template.const_value_kind() {
                    TemplateConstValueKind::RenderableString
                    | TemplateConstValueKind::WrapperTemplate => {
                        let mut fold_context = TemplateFoldContext {
                            string_table,
                            project_path_resolver,
                            path_format_config,
                            source_file_scope,
                        };
                        Some(template.fold_into_stringid(&mut fold_context)?)
                    }
                    TemplateConstValueKind::SlotInsertHelper => {
                        return Err(CompilerError::compiler_error(
                            "Template helper reached AST finalization outside immediate wrapper-slot composition.",
                        ));
                    }
                    TemplateConstValueKind::NonConst => None,
                }
            }

            ExpressionKind::StructDefinition(arguments)
            | ExpressionKind::StructInstance(arguments) => {
                for argument in arguments {
                    Self::normalize_expression_templates(
                        &mut argument.value,
                        source_file_scope,
                        path_format_config,
                        project_path_resolver,
                        string_table,
                    )?;
                }
                None
            }

            ExpressionKind::Range(lower, upper) => {
                Self::normalize_expression_templates(
                    lower,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
                Self::normalize_expression_templates(
                    upper,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
                None
            }

            ExpressionKind::NoValue
            | ExpressionKind::OptionNone
            | ExpressionKind::Int(_)
            | ExpressionKind::Float(_)
            | ExpressionKind::StringSlice(_)
            | ExpressionKind::Bool(_)
            | ExpressionKind::Char(_)
            | ExpressionKind::Path(_)
            | ExpressionKind::Reference(_) => None,
        };

        if let Some(folded_template) = folded_template {
            expression.kind = ExpressionKind::StringSlice(folded_template);
            expression.data_type = DataType::StringSlice;
            expression.ownership = Ownership::ImmutableOwned;
        }

        Ok(())
    }

    fn normalize_template_for_hir(
        template: &mut Template,
        source_file_scope: &InternedPath,
        path_format_config: &PathStringFormatConfig,
        project_path_resolver: &ProjectPathResolver,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerError> {
        for atom in &mut template.content.atoms {
            let TemplateAtom::Content(segment) = atom else {
                continue;
            };

            // Runtime templates may still contain compile-time child templates after
            // wrapper/head composition. Fold those now so HIR only sees real runtime
            // chunks plus finalized text pieces.
            Self::normalize_expression_templates(
                &mut segment.expression,
                source_file_scope,
                path_format_config,
                project_path_resolver,
                string_table,
            )?;
        }

        template.content_needs_formatting = false;
        template.refresh_kind_from_content();
        template.render_plan = Some(TemplateRenderPlan::from_content(&template.content));
        Ok(())
    }

    fn normalize_module_constants_for_hir(
        &self,
        project_path_resolver: &ProjectPathResolver,
        string_table: &mut StringTable,
    ) -> Result<Vec<Declaration>, CompilerError> {
        let mut normalized_constants = Vec::with_capacity(self.module_constants.len());

        for declaration in &self.module_constants {
            if let ExpressionKind::Template(template) = &declaration.value.kind
                && matches!(
                    template.const_value_kind(),
                    TemplateConstValueKind::SlotInsertHelper
                )
            {
                // `$insert(..)` helper constants only exist so AST template composition can
                // splice them into an immediate parent wrapper. They do not have a stable
                // backend-facing value shape, so HIR must not receive them as module consts.
                continue;
            }

            let source_file_scope = self
                .canonical_source_by_symbol_path
                .get(&declaration.id)
                .unwrap_or(&declaration.value.location.scope);

            normalized_constants.push(Declaration {
                id: declaration.id.to_owned(),
                value: self.normalize_module_constant_expression(
                    &declaration.value,
                    source_file_scope,
                    project_path_resolver,
                    string_table,
                )?,
            });
        }

        Ok(normalized_constants)
    }

    fn normalize_module_constant_expression(
        &self,
        expression: &Expression,
        source_file_scope: &InternedPath,
        project_path_resolver: &ProjectPathResolver,
        string_table: &mut StringTable,
    ) -> Result<Expression, CompilerError> {
        let mut normalized = expression.to_owned();
        normalized.kind = match &expression.kind {
            ExpressionKind::Template(template) => match template.const_value_kind() {
                TemplateConstValueKind::RenderableString
                | TemplateConstValueKind::WrapperTemplate => {
                    let mut fold_context = TemplateFoldContext {
                        string_table,
                        project_path_resolver,
                        path_format_config: self.path_format_config,
                        source_file_scope,
                    };
                    let folded = template.fold_into_stringid(&mut fold_context)?;
                    normalized.data_type = DataType::StringSlice;
                    ExpressionKind::StringSlice(folded)
                }
                TemplateConstValueKind::SlotInsertHelper => expression.kind.to_owned(),
                TemplateConstValueKind::NonConst => {
                    return Err(CompilerError::compiler_error(
                        "Non-constant template reached AST finalization in module constant metadata.",
                    ));
                }
            },
            ExpressionKind::Collection(items) => ExpressionKind::Collection(
                items
                    .iter()
                    .map(|item| {
                        self.normalize_module_constant_expression(
                            item,
                            source_file_scope,
                            project_path_resolver,
                            string_table,
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            ),
            ExpressionKind::StructInstance(fields) => ExpressionKind::StructInstance(
                fields
                    .iter()
                    .map(|field| {
                        Ok(Declaration {
                            id: field.id.to_owned(),
                            value: self.normalize_module_constant_expression(
                                &field.value,
                                source_file_scope,
                                project_path_resolver,
                                string_table,
                            )?,
                        })
                    })
                    .collect::<Result<Vec<_>, CompilerError>>()?,
            ),
            ExpressionKind::Range(start, end) => ExpressionKind::Range(
                Box::new(self.normalize_module_constant_expression(
                    start,
                    source_file_scope,
                    project_path_resolver,
                    string_table,
                )?),
                Box::new(self.normalize_module_constant_expression(
                    end,
                    source_file_scope,
                    project_path_resolver,
                    string_table,
                )?),
            ),
            ExpressionKind::ResultConstruct { variant, value } => ExpressionKind::ResultConstruct {
                variant: *variant,
                value: Box::new(self.normalize_module_constant_expression(
                    value,
                    source_file_scope,
                    project_path_resolver,
                    string_table,
                )?),
            },
            ExpressionKind::Coerced { value, to_type } => ExpressionKind::Coerced {
                value: Box::new(self.normalize_module_constant_expression(
                    value,
                    source_file_scope,
                    project_path_resolver,
                    string_table,
                )?),
                to_type: to_type.to_owned(),
            },
            _ => expression.kind.to_owned(),
        };
        Ok(normalized)
    }

    /// Pass 7: Assemble the final `Ast` from accumulated build state.
    pub(super) fn finalize(
        mut self,
        entry_dir: InternedPath,
        top_level_template_items: &[TopLevelTemplateItem],
        string_table: &mut StringTable,
    ) -> Result<Ast, CompilerMessages> {
        let project_path_resolver = self.project_path_resolver.as_ref().ok_or_else(|| {
            self.error_messages(
                CompilerError::compiler_error(
                    "AST construction requires a project path resolver for template folding and path coercion.",
                ),
                string_table,
            )
        })?;

        let doc_fragments = collect_and_strip_comment_templates(
            &mut self.ast,
            project_path_resolver,
            self.path_format_config,
            string_table,
        )
        .map_err(|error| self.error_messages(error, string_table))?;

        let start_template_items = synthesize_start_template_items(
            &mut self.ast,
            &entry_dir,
            top_level_template_items,
            &self.const_templates_by_path,
            project_path_resolver,
            self.path_format_config,
            string_table,
        )
        .map_err(|error| self.error_messages(error, string_table))?;

        self.normalize_ast_templates_for_hir(project_path_resolver, string_table)
            .map_err(|error| self.error_messages(error, string_table))?;

        let module_constants = self
            .normalize_module_constants_for_hir(project_path_resolver, string_table)
            .map_err(|error| self.error_messages(error, string_table))?;

        if !self.builtin_struct_ast_nodes.is_empty() {
            let mut ast_nodes = self.builtin_struct_ast_nodes;
            ast_nodes.extend(self.ast);
            self.ast = ast_nodes;
        }

        Ok(Ast {
            nodes: self.ast,
            module_constants,
            doc_fragments,
            entry_path: entry_dir,
            start_template_items,
            rendered_path_usages: std::mem::take(&mut *self.rendered_path_usages.borrow_mut()),
            warnings: self.warnings,
        })
    }
}

impl Ast {
    pub fn new(
        sorted_headers: Vec<Header>,
        top_level_template_items: Vec<TopLevelTemplateItem>,
        context: AstBuildContext<'_>,
    ) -> Result<Ast, CompilerMessages> {
        let AstBuildContext {
            host_registry,
            style_directives,
            string_table,
            entry_dir,
            build_profile,
            project_path_resolver,
            path_format_config,
        } = context;

        let mut state = AstBuildState::new(
            host_registry,
            style_directives,
            build_profile,
            &project_path_resolver,
            &path_format_config,
            sorted_headers.len(),
        );

        state.collect_declarations(&sorted_headers, string_table)?;

        let file_import_bindings = state.resolve_import_bindings(string_table)?;

        state.resolve_types(&sorted_headers, &file_import_bindings, string_table)?;

        state.resolve_function_signatures(&sorted_headers, &file_import_bindings, string_table)?;

        let receiver_methods = state.build_receiver_catalog(&sorted_headers, string_table)?;

        state.emit_ast_nodes(
            sorted_headers,
            &file_import_bindings,
            &receiver_methods,
            string_table,
        )?;

        state.finalize(entry_dir, &top_level_template_items, string_table)
    }
}
