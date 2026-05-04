//! AST node template normalization for HIR preparation.
//!
//! WHAT: Recursively traverses AST nodes to normalize embedded templates by
//! folding compile-time constants, building render plans, and materializing
//! template metadata. Mutates AST nodes in place to prepare them for HIR.
//!
//! WHY: HIR assumes templates are semantically complete with folded constants
//! and render plans. This normalization ensures the AST→HIR boundary contract
//! is satisfied before lowering.
//!
//! ## Normalization Strategy
//!
//! 1. **Constant Folding**: Templates with `RenderableString` const value kinds
//!    are folded into `StringSlice` expressions immediately.
//!
//! 2. **Render Plan Construction**: Runtime templates receive complete render
//!    plans so HIR doesn't need to reconstruct them.
//!
//! 3. **Metadata Completion**: All templates have `content_needs_formatting`
//!    set to false and their kind refreshed from content.
//!
//! 4. **Helper Rejection**: escaped `$insert(...)` helper templates are rejected
//!    if they reach finalization outside immediate wrapper-slot composition.
//!
//! ## AST→HIR Template Boundary
//!
//! AST owns:
//! - Template foldability decisions
//! - Render plan construction
//! - Constant template lowering
//! - Runtime template planning
//!
//! HIR receives:
//! - Folded constant templates as `StringSlice` expressions
//! - Runtime templates with complete render plans
//! - No escaped helper artifacts (`TemplateType::SlotInsert`)
//! - No templates requiring formatting

use super::finalizer::AstFinalizer;
use super::template_helpers::try_fold_template_to_string;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, ResultCallHandling,
};
use crate::compiler_frontend::ast::instrumentation::{AstCounter, increment_ast_counter};
use crate::compiler_frontend::ast::statements::match_patterns::MatchPattern;
use crate::compiler_frontend::ast::templates::template::TemplateConstValueKind;
use crate::compiler_frontend::ast::templates::template::{TemplateAtom, TemplateType};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::value_mode::ValueMode;

impl AstFinalizer<'_, '_> {
    /// Normalizes all templates in the AST for HIR consumption.
    ///
    /// WHAT: Traverses all AST nodes and normalizes embedded templates by
    /// folding constants and building render plans.
    ///
    /// WHY: Ensures HIR receives semantically complete templates without
    /// needing to understand template composition or folding rules.
    pub(crate) fn normalize_ast_templates_for_hir(
        &self,
        ast: &mut [AstNode],
        project_path_resolver: &ProjectPathResolver,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerError> {
        let canonical_source_by_symbol_path = &self
            .environment
            .module_symbols
            .canonical_source_by_symbol_path;
        let path_format_config = &self.context.path_format_config;

        for node in ast {
            let source_file_scope = canonical_source_by_symbol_path
                .get(&node.scope)
                .unwrap_or(&node.location.scope)
                .to_owned();
            normalize_ast_node_templates(
                node,
                &source_file_scope,
                path_format_config,
                project_path_resolver,
                string_table,
            )?;
        }

        Ok(())
    }
}

/// Normalizes templates in an AST node by routing to category-specific handlers.
///
/// WHAT: Dispatcher function that routes AST nodes to specialized normalization
/// functions based on node category (control flow, declarations, calls, etc.).
///
/// WHY: Keeps the main normalization logic organized by node category while
/// providing a single entry point for recursive traversal.
fn normalize_ast_node_templates(
    node: &mut AstNode,
    source_file_scope: &InternedPath,
    path_format_config: &PathStringFormatConfig,
    project_path_resolver: &ProjectPathResolver,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    increment_ast_counter(AstCounter::TemplateNormalizationNodesVisited);

    match &mut node.kind {
        // Control flow nodes
        NodeKind::If(_, _, _)
        | NodeKind::Match { .. }
        | NodeKind::ScopedBlock { .. }
        | NodeKind::RangeLoop { .. }
        | NodeKind::CollectionLoop { .. }
        | NodeKind::WhileLoop(_, _) => normalize_control_flow_templates(
            node,
            source_file_scope,
            path_format_config,
            project_path_resolver,
            string_table,
        ),

        // Declaration and assignment nodes
        NodeKind::VariableDeclaration(_)
        | NodeKind::Assignment { .. }
        | NodeKind::StructDefinition(_, _) => normalize_declaration_templates(
            node,
            source_file_scope,
            path_format_config,
            project_path_resolver,
            string_table,
        ),

        // Function and method call nodes
        NodeKind::FunctionCall { .. }
        | NodeKind::HostFunctionCall { .. }
        | NodeKind::MethodCall { .. }
        | NodeKind::CollectionBuiltinCall { .. }
        | NodeKind::ResultHandledFunctionCall { .. } => normalize_call_templates(
            node,
            source_file_scope,
            path_format_config,
            project_path_resolver,
            string_table,
        ),

        // Simple expression nodes
        NodeKind::MultiBind { value, .. } | NodeKind::Rvalue(value) => {
            normalize_expression_templates(
                value,
                source_file_scope,
                path_format_config,
                project_path_resolver,
                string_table,
            )
        }

        // Function body
        NodeKind::Function(_, _, body) => {
            for statement in body {
                normalize_ast_node_templates(
                    statement,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
            }
            Ok(())
        }

        // Return statements
        NodeKind::Return(values) => {
            for value in values {
                normalize_expression_templates(
                    value,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
            }
            Ok(())
        }

        NodeKind::ReturnError(value) => normalize_expression_templates(
            value,
            source_file_scope,
            path_format_config,
            project_path_resolver,
            string_table,
        ),

        // Field access
        NodeKind::FieldAccess { base, .. } => normalize_ast_node_templates(
            base,
            source_file_scope,
            path_format_config,
            project_path_resolver,
            string_table,
        ),

        // Runtime fragment push — normalize the template expression it carries.
        NodeKind::PushStartRuntimeFragment(expr) => normalize_expression_templates(
            expr,
            source_file_scope,
            path_format_config,
            project_path_resolver,
            string_table,
        ),

        // Terminal nodes (no templates to normalize)
        NodeKind::Break | NodeKind::Continue | NodeKind::Operator(_) => Ok(()),
    }
}

/// Normalizes templates in control flow nodes (if, match, loops).
///
/// WHAT: Handles normalization for if statements, match expressions, and all
/// loop types (range, collection, while) by recursively normalizing conditions
/// and body statements.
///
/// WHY: Control flow nodes have similar structure (condition + body) and can
/// be handled together to avoid duplication.
fn normalize_control_flow_templates(
    node: &mut AstNode,
    source_file_scope: &InternedPath,
    path_format_config: &PathStringFormatConfig,
    project_path_resolver: &ProjectPathResolver,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    match &mut node.kind {
        // If statements
        NodeKind::If(condition, then_body, else_body) => {
            normalize_expression_templates(
                condition,
                source_file_scope,
                path_format_config,
                project_path_resolver,
                string_table,
            )?;
            for statement in then_body {
                normalize_ast_node_templates(
                    statement,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
            }
            if let Some(else_body) = else_body {
                for statement in else_body {
                    normalize_ast_node_templates(
                        statement,
                        source_file_scope,
                        path_format_config,
                        project_path_resolver,
                        string_table,
                    )?;
                }
            }
            Ok(())
        }

        // Match statements
        NodeKind::Match {
            scrutinee,
            arms,
            default,
            exhaustiveness: _,
        } => {
            normalize_expression_templates(
                scrutinee,
                source_file_scope,
                path_format_config,
                project_path_resolver,
                string_table,
            )?;
            for arm in arms {
                match &mut arm.pattern {
                    MatchPattern::Literal(expression)
                    | MatchPattern::Relational {
                        value: expression, ..
                    } => {
                        normalize_expression_templates(
                            expression,
                            source_file_scope,
                            path_format_config,
                            project_path_resolver,
                            string_table,
                        )?;
                    }
                    MatchPattern::Wildcard { .. }
                    | MatchPattern::ChoiceVariant { .. }
                    | MatchPattern::Capture { .. } => {}
                }
                if let Some(guard) = &mut arm.guard {
                    normalize_expression_templates(
                        guard,
                        source_file_scope,
                        path_format_config,
                        project_path_resolver,
                        string_table,
                    )?;
                }
                for statement in &mut arm.body {
                    normalize_ast_node_templates(
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
                    normalize_ast_node_templates(
                        statement,
                        source_file_scope,
                        path_format_config,
                        project_path_resolver,
                        string_table,
                    )?;
                }
            }
            Ok(())
        }

        // Scoped blocks
        NodeKind::ScopedBlock { body } => {
            for statement in body {
                normalize_ast_node_templates(
                    statement,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
            }
            Ok(())
        }

        // Range loops
        NodeKind::RangeLoop {
            bindings,
            range,
            body,
        } => {
            if let Some(item_binding) = &mut bindings.item {
                normalize_expression_templates(
                    &mut item_binding.value,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
            }
            if let Some(index_binding) = &mut bindings.index {
                normalize_expression_templates(
                    &mut index_binding.value,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
            }
            normalize_expression_templates(
                &mut range.start,
                source_file_scope,
                path_format_config,
                project_path_resolver,
                string_table,
            )?;
            normalize_expression_templates(
                &mut range.end,
                source_file_scope,
                path_format_config,
                project_path_resolver,
                string_table,
            )?;
            if let Some(step) = &mut range.step {
                normalize_expression_templates(
                    step,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
            }
            for statement in body {
                normalize_ast_node_templates(
                    statement,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
            }
            Ok(())
        }

        // Collection loops
        NodeKind::CollectionLoop {
            bindings,
            iterable,
            body,
        } => {
            if let Some(item_binding) = &mut bindings.item {
                normalize_expression_templates(
                    &mut item_binding.value,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
            }
            if let Some(index_binding) = &mut bindings.index {
                normalize_expression_templates(
                    &mut index_binding.value,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
            }
            normalize_expression_templates(
                iterable,
                source_file_scope,
                path_format_config,
                project_path_resolver,
                string_table,
            )?;
            for statement in body {
                normalize_ast_node_templates(
                    statement,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
            }
            Ok(())
        }

        // While loops
        NodeKind::WhileLoop(condition, body) => {
            normalize_expression_templates(
                condition,
                source_file_scope,
                path_format_config,
                project_path_resolver,
                string_table,
            )?;
            for statement in body {
                normalize_ast_node_templates(
                    statement,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
            }
            Ok(())
        }

        _ => unreachable!("normalize_control_flow_templates called with non-control-flow node"),
    }
}

/// Normalizes templates in declaration and assignment nodes.
///
/// WHAT: Handles normalization for variable declarations, assignments, and
/// struct definitions by recursively normalizing value expressions and fields.
///
/// WHY: Declaration nodes have similar structure (identifier + value) and can
/// be handled together to avoid duplication.
fn normalize_declaration_templates(
    node: &mut AstNode,
    source_file_scope: &InternedPath,
    path_format_config: &PathStringFormatConfig,
    project_path_resolver: &ProjectPathResolver,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    match &mut node.kind {
        NodeKind::VariableDeclaration(declaration) => normalize_expression_templates(
            &mut declaration.value,
            source_file_scope,
            path_format_config,
            project_path_resolver,
            string_table,
        ),

        NodeKind::Assignment { target, value } => {
            normalize_ast_node_templates(
                target,
                source_file_scope,
                path_format_config,
                project_path_resolver,
                string_table,
            )?;
            normalize_expression_templates(
                value,
                source_file_scope,
                path_format_config,
                project_path_resolver,
                string_table,
            )
        }

        NodeKind::StructDefinition(_, fields) => {
            for field in fields {
                normalize_expression_templates_with_context(
                    &mut field.value,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                    HelperArtifactPolicy::AllowNestedHelperContent,
                )?;
            }
            Ok(())
        }

        _ => unreachable!("normalize_declaration_templates called with non-declaration node"),
    }
}

/// Normalizes templates in call-shaped AST nodes.
///
/// WHAT: Handles normalization for function calls, host function calls, method
/// calls, collection builtin calls, and result-handled function calls by
/// recursively normalizing arguments and result handling.
///
/// WHY: Call nodes have similar structure (receiver/target + arguments) and can
/// be handled together to avoid duplication.
fn normalize_call_templates(
    node: &mut AstNode,
    source_file_scope: &InternedPath,
    path_format_config: &PathStringFormatConfig,
    project_path_resolver: &ProjectPathResolver,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    match &mut node.kind {
        NodeKind::MethodCall { receiver, args, .. }
        | NodeKind::CollectionBuiltinCall { receiver, args, .. } => {
            normalize_ast_node_templates(
                receiver,
                source_file_scope,
                path_format_config,
                project_path_resolver,
                string_table,
            )?;
            for argument in args {
                normalize_expression_templates(
                    &mut argument.value,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
            }
            Ok(())
        }

        NodeKind::FunctionCall { args, .. } | NodeKind::HostFunctionCall { args, .. } => {
            for argument in args {
                normalize_expression_templates(
                    &mut argument.value,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
            }
            Ok(())
        }

        NodeKind::ResultHandledFunctionCall { args, handling, .. } => {
            for argument in args {
                normalize_expression_templates(
                    &mut argument.value,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
            }
            normalize_result_handling_templates(
                handling,
                source_file_scope,
                path_format_config,
                project_path_resolver,
                string_table,
                HelperArtifactPolicy::RejectFinalHelperValue,
            )
        }

        _ => unreachable!("normalize_call_templates called with non-call node"),
    }
}

/// Normalizes templates in result handling constructs.
///
/// WHAT: Handles normalization for result handling (fallback values and error
/// handlers) by recursively normalizing fallback expressions and handler bodies.
///
/// WHY: Result handling can contain templates in fallback values and error
/// handler bodies that must be normalized for HIR.
fn normalize_result_handling_templates(
    handling: &mut ResultCallHandling,
    source_file_scope: &InternedPath,
    path_format_config: &PathStringFormatConfig,
    project_path_resolver: &ProjectPathResolver,
    string_table: &mut StringTable,
    helper_artifact_policy: HelperArtifactPolicy,
) -> Result<(), CompilerError> {
    match handling {
        ResultCallHandling::Fallback(fallback_values) => {
            for fallback in fallback_values {
                normalize_expression_templates_with_context(
                    fallback,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                    helper_artifact_policy,
                )?;
            }
            Ok(())
        }
        ResultCallHandling::Handler { fallback, body, .. } => {
            if let Some(fallback_values) = fallback {
                for fallback in fallback_values {
                    normalize_expression_templates_with_context(
                        fallback,
                        source_file_scope,
                        path_format_config,
                        project_path_resolver,
                        string_table,
                        helper_artifact_policy,
                    )?;
                }
            }
            for statement in body {
                normalize_ast_node_templates(
                    statement,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
            }
            Ok(())
        }
        ResultCallHandling::Propagate => Ok(()),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HelperArtifactPolicy {
    RejectFinalHelperValue,
    AllowNestedHelperContent,
}

/// Normalizes templates in expressions.
///
/// WHAT: Recursively normalizes templates embedded in expressions by folding
/// compile-time constants and building render plans for runtime templates.
///
/// WHY: Expressions can contain templates at any level of nesting, so we need
/// to recursively traverse the expression tree to normalize all templates.
fn normalize_expression_templates(
    expression: &mut Expression,
    source_file_scope: &InternedPath,
    path_format_config: &PathStringFormatConfig,
    project_path_resolver: &ProjectPathResolver,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    normalize_expression_templates_with_context(
        expression,
        source_file_scope,
        path_format_config,
        project_path_resolver,
        string_table,
        HelperArtifactPolicy::RejectFinalHelperValue,
    )
}

fn normalize_expression_templates_with_context(
    expression: &mut Expression,
    source_file_scope: &InternedPath,
    path_format_config: &PathStringFormatConfig,
    project_path_resolver: &ProjectPathResolver,
    string_table: &mut StringTable,
    helper_artifact_policy: HelperArtifactPolicy,
) -> Result<(), CompilerError> {
    let folded_template = match &mut expression.kind {
        // Place and container expressions
        ExpressionKind::Copy(place) => {
            normalize_ast_node_templates(
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
                normalize_ast_node_templates(
                    node,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
            }
            None
        }

        // Function and call expressions
        ExpressionKind::Function(_, body) => {
            for node in body {
                normalize_ast_node_templates(
                    node,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
            }
            None
        }

        ExpressionKind::FunctionCall(_, args) | ExpressionKind::HostFunctionCall(_, args) => {
            for argument in args {
                normalize_expression_templates_with_context(
                    &mut argument.value,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                    helper_artifact_policy,
                )?;
            }
            None
        }

        // Collection and builtin call expressions
        ExpressionKind::Collection(args) => {
            for argument in args {
                normalize_expression_templates_with_context(
                    argument,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                    helper_artifact_policy,
                )?;
            }
            None
        }

        ExpressionKind::ResultHandledFunctionCall { args, handling, .. } => {
            for argument in args {
                normalize_expression_templates_with_context(
                    &mut argument.value,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                    helper_artifact_policy,
                )?;
            }
            normalize_result_handling_templates(
                handling,
                source_file_scope,
                path_format_config,
                project_path_resolver,
                string_table,
                helper_artifact_policy,
            )?;
            None
        }

        // Wrapping and coerced expressions
        ExpressionKind::BuiltinCast { value, .. }
        | ExpressionKind::ResultConstruct { value, .. }
        | ExpressionKind::Coerced { value, .. } => {
            normalize_expression_templates_with_context(
                value,
                source_file_scope,
                path_format_config,
                project_path_resolver,
                string_table,
                helper_artifact_policy,
            )?;
            None
        }

        ExpressionKind::HandledResult { value, handling } => {
            normalize_expression_templates_with_context(
                value,
                source_file_scope,
                path_format_config,
                project_path_resolver,
                string_table,
                helper_artifact_policy,
            )?;
            normalize_result_handling_templates(
                handling,
                source_file_scope,
                path_format_config,
                project_path_resolver,
                string_table,
                helper_artifact_policy,
            )?;
            None
        }

        // Template expressions (may fold to literal)
        ExpressionKind::Template(template) => {
            normalize_template_for_hir(
                template,
                source_file_scope,
                path_format_config,
                project_path_resolver,
                string_table,
            )?;

            let template_const_kind = template.const_value_kind();

            // Fold only fully renderable final template values.
            // Wrapper-shaped values may still represent runtime templates in this path.
            if matches!(
                template_const_kind,
                TemplateConstValueKind::RenderableString
            ) {
                try_fold_template_to_string(
                    template,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?
            } else {
                // Nested helper-owned contribution structure can be legal inside wrapper
                // templates. Reject only when this expression's final value itself is a
                // standalone helper artifact after composition.
                if helper_artifact_policy == HelperArtifactPolicy::RejectFinalHelperValue
                    && is_illegal_final_template_helper_value(template, template_const_kind)
                {
                    return Err(CompilerError::new_rule_error(
                        "Template helper reached AST finalization outside immediate wrapper-slot composition.",
                        template.location.to_owned(),
                    ));
                }

                None
            }
        }

        // Struct and range expressions
        ExpressionKind::StructDefinition(arguments) | ExpressionKind::StructInstance(arguments) => {
            for argument in arguments {
                normalize_expression_templates_with_context(
                    &mut argument.value,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                    helper_artifact_policy,
                )?;
            }
            None
        }

        ExpressionKind::Range(lower, upper) => {
            normalize_expression_templates_with_context(
                lower,
                source_file_scope,
                path_format_config,
                project_path_resolver,
                string_table,
                helper_artifact_policy,
            )?;
            normalize_expression_templates_with_context(
                upper,
                source_file_scope,
                path_format_config,
                project_path_resolver,
                string_table,
                helper_artifact_policy,
            )?;
            None
        }

        // Literals and simple values (nothing to normalize)
        ExpressionKind::NoValue
        | ExpressionKind::OptionNone
        | ExpressionKind::Int(_)
        | ExpressionKind::Float(_)
        | ExpressionKind::StringSlice(_)
        | ExpressionKind::Bool(_)
        | ExpressionKind::Char(_)
        | ExpressionKind::Path(_)
        | ExpressionKind::Reference(_) => None,

        // Choice construct expressions
        ExpressionKind::ChoiceConstruct { fields, .. } => {
            for field in fields {
                normalize_expression_templates(
                    &mut field.value,
                    source_file_scope,
                    path_format_config,
                    project_path_resolver,
                    string_table,
                )?;
            }
            None
        }
    };

    // If we folded a template, replace the expression with a StringSlice
    if let Some(folded_template) = folded_template {
        expression.kind = ExpressionKind::StringSlice(folded_template);
        expression.data_type = DataType::StringSlice;
        expression.value_mode = ValueMode::ImmutableOwned;
    }

    Ok(())
}

/// Normalizes a template for HIR consumption.
///
/// WHAT: Normalizes child templates in template content, sets content_needs_formatting
/// to false, refreshes the template kind, and builds a render plan.
///
/// WHY:
/// - Runtime templates may contain compile-time child templates after wrapper/head
///   composition. We fold those pieces now so HIR sees finalized chunks.
/// - AST may fold compile-time subtemplates inside a runtime template, but must preserve
///   the enclosing runtime template whenever any runtime chunk remains.
/// - Only escaped helper artifacts are invalid after AST composition.
/// - The render plan is built here so HIR doesn't need to reconstruct it.
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
        // chunks plus finalized text pieces. Nested helper-owned contribution
        // structure is allowed at this stage for reusable wrapper templates.
        normalize_expression_templates_with_context(
            &mut segment.expression,
            source_file_scope,
            path_format_config,
            project_path_resolver,
            string_table,
            HelperArtifactPolicy::AllowNestedHelperContent,
        )?;
    }

    // Rebuild final runtime metadata so HIR sees an authoritative post-normalization plan.
    template.resync_runtime_metadata();
    Ok(())
}

fn is_illegal_final_template_helper_value(
    template: &Template,
    const_kind: TemplateConstValueKind,
) -> bool {
    matches!(template.kind, TemplateType::SlotInsert(_))
        || matches!(const_kind, TemplateConstValueKind::SlotInsertHelper)
}
