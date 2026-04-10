//! Start-template/fragment synthesis for the entry file.
//!
//! The header stage tracks top-level template declarations, and the AST stage
//! turns them into ordered start fragments:
//! - compile-time strings become `ConstString` fragments
//! - runtime templates become generated `__bst_frag_N` functions

use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::call_argument::{CallAccessMode, CallArgument};
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, ResultCallHandling,
};
use crate::compiler_frontend::ast::statements::functions::{
    FunctionReturn, FunctionSignature, ReturnSlot,
};
use crate::compiler_frontend::ast::templates::template::{CommentDirectiveKind, TemplateType};
use crate::compiler_frontend::ast::templates::template_folding::TemplateFoldContext;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::headers::parse_file_headers::{
    TopLevelTemplateItem, TopLevelTemplateKind,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::projects::settings::{IMPLICIT_START_FUNC_NAME, TOP_LEVEL_TEMPLATE_NAME};
use rustc_hash::{FxHashMap, FxHashSet};

#[derive(Clone, Debug)]
pub enum AstStartTemplateItem {
    ConstString {
        value: StringId,
        #[allow(dead_code)] // Preserved for future source-mapping and error reporting
        location: SourceLocation,
    },
    RuntimeStringFunction {
        function: InternedPath,
        location: SourceLocation,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AstDocFragmentKind {
    Doc,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AstDocFragment {
    pub kind: AstDocFragmentKind,
    pub value: StringId,
    pub location: SourceLocation,
}

#[derive(Clone)]
struct RuntimeTemplateCandidate {
    template_expression: Expression,
    location: SourceLocation,
    scope: InternedPath,
    source_index: usize,
    preceding_statements: Vec<AstNode>,
}

#[derive(Clone)]
struct RuntimeTemplateExtraction {
    runtime_candidates: Vec<RuntimeTemplateCandidate>,
    entry_scope: InternedPath,
    non_template_body: Vec<AstNode>,
}

struct RuntimeFragmentCapturePlan {
    fragment_body: Vec<AstNode>,
    captured_symbols: FxHashSet<InternedPath>,
}

pub(crate) fn synthesize_start_template_items(
    ast_nodes: &mut Vec<AstNode>,
    entry_dir: &InternedPath,
    top_level_template_items: &[TopLevelTemplateItem],
    const_templates_by_path: &FxHashMap<InternedPath, StringId>,
    project_path_resolver: &ProjectPathResolver,
    path_format_config: &PathStringFormatConfig,
    string_table: &mut StringTable,
) -> Result<Vec<AstStartTemplateItem>, CompilerError> {
    // Phase 1: locate the entry start function and extract runtime `#template`
    // declarations so they can be lowered into start fragments.
    let entry_start_function_name = entry_dir.join_str(IMPLICIT_START_FUNC_NAME, string_table);
    let Some(entry_start_index) = ast_nodes.iter().position(|node| {
        matches!(
            &node.kind,
            NodeKind::Function(name, _, _) if *name == entry_start_function_name
        )
    }) else {
        return Err(CompilerError::compiler_error(format!(
            "Failed to find entry start function '{}' while synthesizing start fragments.",
            entry_start_function_name.to_string(string_table)
        )));
    };

    let RuntimeTemplateExtraction {
        runtime_candidates,
        entry_scope,
        non_template_body,
    } = extract_runtime_template_candidates(ast_nodes, entry_start_index, string_table)?;

    // Phase 2: collect const/runtime fragment sources and order them by source location.
    let mut ordered_header_items = top_level_template_items.to_owned();
    ordered_header_items.sort_by_key(|item| item.file_order);

    let mut next_fragment_index = 0usize;
    let mut ordered_fragment_sources: Vec<OrderedFragmentSource> =
        Vec::with_capacity(ordered_header_items.len() + runtime_candidates.len());
    let mut captured_runtime_symbols = FxHashSet::default();

    for template_item in ordered_header_items {
        if let TopLevelTemplateKind::ConstTemplate { header_path } = template_item.kind {
            let Some(value) = const_templates_by_path.get(&header_path).copied() else {
                return Err(CompilerError::compiler_error(format!(
                    "Missing const template value for '{}'",
                    header_path.to_string(string_table)
                )));
            };

            ordered_fragment_sources.push(OrderedFragmentSource::Const {
                value,
                location: template_item.location.to_owned(),
            });
        }
    }

    // Runtime fragment ordering comes from extracted top-level template declarations.
    // This avoids treating templates nested inside unrelated expressions as start fragments.
    for candidate in runtime_candidates {
        ordered_fragment_sources.push(OrderedFragmentSource::Runtime(candidate));
    }

    ordered_fragment_sources.sort_by(compare_fragment_locations);

    let mut start_template_items = Vec::with_capacity(ordered_fragment_sources.len());
    for source in ordered_fragment_sources {
        match source {
            OrderedFragmentSource::Const { value, location } => {
                start_template_items.push(AstStartTemplateItem::ConstString { value, location });
            }

            OrderedFragmentSource::Runtime(candidate) => {
                let ExpressionKind::Template(template) = &candidate.template_expression.kind else {
                    return Err(CompilerError::compiler_error(
                        "Top-level runtime template candidate expression was not parsed as a template.",
                    ));
                };

                // Runtime template expressions can still fold to constants after AST folding.
                // Keep them as const fragments to avoid generating unnecessary wrapper functions.
                if template.is_const_renderable_string() {
                    let folded = fold_template_with_context(
                        template,
                        &template.location.scope,
                        project_path_resolver,
                        path_format_config,
                        string_table,
                    )?;
                    start_template_items.push(AstStartTemplateItem::ConstString {
                        value: folded,
                        location: candidate.location.to_owned(),
                    });
                    continue;
                }

                let fragment_name =
                    entry_dir.join_str(&format!("__bst_frag_{next_fragment_index}"), string_table);
                next_fragment_index += 1;

                let capture_plan = build_runtime_fragment_capture_plan(&candidate)?;
                captured_runtime_symbols.extend(capture_plan.captured_symbols);
                let mut fragment_body = capture_plan.fragment_body;
                fragment_body.push(AstNode {
                    kind: NodeKind::Return(vec![candidate.template_expression.to_owned()]),
                    location: candidate.location.to_owned(),
                    scope: candidate.scope.to_owned(),
                });

                ast_nodes.push(AstNode {
                    kind: NodeKind::Function(
                        fragment_name.to_owned(),
                        FunctionSignature {
                            parameters: vec![],
                            returns: vec![ReturnSlot::success(FunctionReturn::Value(
                                DataType::StringSlice,
                            ))],
                        },
                        fragment_body,
                    ),
                    location: candidate.location.to_owned(),
                    scope: entry_scope.to_owned(),
                });

                start_template_items.push(AstStartTemplateItem::RuntimeStringFunction {
                    function: fragment_name,
                    location: candidate.location,
                });
            }
        }
    }

    let pruned_start_body =
        prune_template_only_captured_declarations(non_template_body, &captured_runtime_symbols);
    replace_entry_start_body(ast_nodes, entry_start_index, pruned_start_body)?;

    Ok(start_template_items)
}

pub(crate) fn collect_and_strip_comment_templates(
    ast_nodes: &mut [AstNode],
    project_path_resolver: &ProjectPathResolver,
    path_format_config: &PathStringFormatConfig,
    string_table: &mut StringTable,
) -> Result<Vec<AstDocFragment>, CompilerError> {
    let mut fragments = Vec::new();

    for node in ast_nodes.iter_mut() {
        let NodeKind::Function(_, _, body) = &mut node.kind else {
            continue;
        };

        let mut retained = Vec::with_capacity(body.len());
        for statement in std::mem::take(body) {
            if let Some(comment_template) =
                as_top_level_template_comment_declaration(&statement, string_table)
            {
                collect_doc_fragments(
                    comment_template,
                    &mut fragments,
                    project_path_resolver,
                    path_format_config,
                    string_table,
                )?;
                continue;
            }

            retained.push(statement);
        }

        *body = retained;
    }

    fragments.sort_by_key(|fragment| {
        (
            fragment.location.scope.to_string(string_table),
            fragment.location.start_pos.line_number,
            fragment.location.start_pos.char_column,
        )
    });

    Ok(fragments)
}

fn as_top_level_template_comment_declaration<'a>(
    node: &'a AstNode,
    string_table: &StringTable,
) -> Option<&'a crate::compiler_frontend::ast::templates::template_types::Template> {
    let declaration = as_top_level_template_declaration(node, string_table)?;
    let ExpressionKind::Template(template) = &declaration.value.kind else {
        return None;
    };

    matches!(template.kind, TemplateType::Comment(_)).then_some(template.as_ref())
}

fn collect_doc_fragments(
    template: &crate::compiler_frontend::ast::templates::template_types::Template,
    fragments: &mut Vec<AstDocFragment>,
    project_path_resolver: &ProjectPathResolver,
    path_format_config: &PathStringFormatConfig,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    if matches!(
        template.kind,
        TemplateType::Comment(CommentDirectiveKind::Doc)
    ) {
        let rendered = fold_template_with_context(
            template,
            &template.location.scope,
            project_path_resolver,
            path_format_config,
            string_table,
        )?;
        fragments.push(AstDocFragment {
            kind: AstDocFragmentKind::Doc,
            value: rendered,
            location: template.location.to_owned(),
        });
    }

    for child in &template.doc_children {
        collect_doc_fragments(
            child,
            fragments,
            project_path_resolver,
            path_format_config,
            string_table,
        )?;
    }

    Ok(())
}

fn fold_template_with_context(
    template: &crate::compiler_frontend::ast::templates::template_types::Template,
    source_file_scope: &InternedPath,
    project_path_resolver: &ProjectPathResolver,
    path_format_config: &PathStringFormatConfig,
    string_table: &mut StringTable,
) -> Result<StringId, CompilerError> {
    let mut fold_context = TemplateFoldContext {
        string_table,
        project_path_resolver,
        path_format_config,
        source_file_scope,
    };
    template.fold_into_stringid(&mut fold_context)
}

#[allow(clippy::large_enum_variant)]
#[derive(Clone)]
enum OrderedFragmentSource {
    Const {
        value: StringId,
        location: SourceLocation,
    },
    Runtime(RuntimeTemplateCandidate),
}

fn compare_fragment_locations(
    lhs: &OrderedFragmentSource,
    rhs: &OrderedFragmentSource,
) -> std::cmp::Ordering {
    let lhs_location = fragment_source_location(lhs);
    let rhs_location = fragment_source_location(rhs);

    lhs_location
        .start_pos
        .line_number
        .cmp(&rhs_location.start_pos.line_number)
        .then(
            lhs_location
                .start_pos
                .char_column
                .cmp(&rhs_location.start_pos.char_column),
        )
        .then(fragment_source_index(lhs).cmp(&fragment_source_index(rhs)))
}

fn fragment_source_location(source: &OrderedFragmentSource) -> &SourceLocation {
    match source {
        OrderedFragmentSource::Const { location, .. } => location,
        OrderedFragmentSource::Runtime(candidate) => &candidate.location,
    }
}

fn fragment_source_index(source: &OrderedFragmentSource) -> usize {
    match source {
        OrderedFragmentSource::Const { .. } => usize::MAX,
        OrderedFragmentSource::Runtime(candidate) => candidate.source_index,
    }
}

fn extract_runtime_template_candidates(
    ast_nodes: &mut [AstNode],
    entry_start_index: usize,
    string_table: &StringTable,
) -> Result<RuntimeTemplateExtraction, CompilerError> {
    let Some(entry_start_node) = ast_nodes.get_mut(entry_start_index) else {
        return Err(CompilerError::compiler_error(
            "Entry start function index is out of bounds while extracting runtime templates.",
        ));
    };

    let entry_scope = entry_start_node.scope.to_owned();
    let NodeKind::Function(_, _, body) = &mut entry_start_node.kind else {
        return Err(CompilerError::compiler_error(
            "Entry start function node is not a function while extracting runtime templates.",
        ));
    };

    // Work on a snapshot so extraction is independent of in-place mutation.
    let original_body = body.to_owned();
    let mut runtime_candidates = Vec::new();
    let mut non_template_body = Vec::with_capacity(original_body.len());

    for (index, node) in original_body.iter().enumerate() {
        if let Some(declaration) = as_top_level_template_declaration(node, string_table) {
            runtime_candidates.push(RuntimeTemplateCandidate {
                template_expression: declaration.value.to_owned(),
                location: node.location.to_owned(),
                scope: node.scope.to_owned(),
                source_index: index,
                preceding_statements: original_body[..index]
                    .iter()
                    .filter(|statement| {
                        as_top_level_template_declaration(statement, string_table).is_none()
                    })
                    .map(ToOwned::to_owned)
                    .collect(),
            });
            continue;
        }

        if let Some(template_expression) = as_top_level_template_return_expression(node) {
            runtime_candidates.push(RuntimeTemplateCandidate {
                template_expression: template_expression.to_owned(),
                location: node.location.to_owned(),
                scope: node.scope.to_owned(),
                source_index: index,
                preceding_statements: original_body[..index]
                    .iter()
                    .filter(|statement| {
                        as_top_level_template_declaration(statement, string_table).is_none()
                            && as_top_level_template_return_expression(statement).is_none()
                    })
                    .map(ToOwned::to_owned)
                    .collect(),
            });
            continue;
        }

        non_template_body.push(node.to_owned());
    }

    Ok(RuntimeTemplateExtraction {
        runtime_candidates,
        entry_scope,
        non_template_body,
    })
}

fn replace_entry_start_body(
    ast_nodes: &mut [AstNode],
    entry_start_index: usize,
    body: Vec<AstNode>,
) -> Result<(), CompilerError> {
    let Some(entry_start_node) = ast_nodes.get_mut(entry_start_index) else {
        return Err(CompilerError::compiler_error(
            "Entry start function index is out of bounds while rewriting captured declarations.",
        ));
    };

    let NodeKind::Function(_, _, start_body) = &mut entry_start_node.kind else {
        return Err(CompilerError::compiler_error(
            "Entry start function node is not a function while rewriting captured declarations.",
        ));
    };

    *start_body = body;
    Ok(())
}

fn prune_template_only_captured_declarations(
    start_body: Vec<AstNode>,
    captured_symbols: &FxHashSet<InternedPath>,
) -> Vec<AstNode> {
    if captured_symbols.is_empty() {
        return start_body;
    }

    let prunable_symbols = start_body
        .iter()
        .filter_map(|statement| {
            let NodeKind::VariableDeclaration(declaration) = &statement.kind else {
                return None;
            };

            captured_symbols
                .contains(&declaration.id)
                .then_some(declaration.id.to_owned())
        })
        .collect::<FxHashSet<_>>();

    if prunable_symbols.is_empty() {
        return start_body;
    }

    let declaration_values = start_body
        .iter()
        .filter_map(|statement| {
            let NodeKind::VariableDeclaration(declaration) = &statement.kind else {
                return None;
            };

            prunable_symbols
                .contains(&declaration.id)
                .then_some((declaration.id.to_owned(), declaration.value.to_owned()))
        })
        .collect::<FxHashMap<_, _>>();

    // Keep declarations that feed non-template start semantics, then keep their
    // transitive declaration dependencies inside the same prunable set.
    let mut required_symbols = FxHashSet::default();
    for statement in &start_body {
        if let NodeKind::VariableDeclaration(declaration) = &statement.kind
            && prunable_symbols.contains(&declaration.id)
        {
            continue;
        }

        collect_references_from_ast_node(statement, &mut required_symbols);
    }

    let mut kept_symbols = FxHashSet::default();
    let mut pending = required_symbols
        .into_iter()
        .filter(|symbol| prunable_symbols.contains(symbol))
        .collect::<Vec<_>>();

    while let Some(symbol) = pending.pop() {
        if !kept_symbols.insert(symbol.to_owned()) {
            continue;
        }

        let Some(value) = declaration_values.get(&symbol) else {
            continue;
        };

        let mut dependencies = FxHashSet::default();
        collect_references_from_expression(value, &mut dependencies);

        for dependency in dependencies {
            if prunable_symbols.contains(&dependency) && !kept_symbols.contains(&dependency) {
                pending.push(dependency);
            }
        }
    }

    let pruned_symbols = prunable_symbols
        .difference(&kept_symbols)
        .cloned()
        .collect::<FxHashSet<_>>();

    if pruned_symbols.is_empty() {
        return start_body;
    }

    start_body
        .into_iter()
        .filter(|statement| {
            !matches!(
                &statement.kind,
                NodeKind::VariableDeclaration(declaration)
                    if pruned_symbols.contains(&declaration.id)
            )
        })
        .collect()
}

fn as_top_level_template_declaration<'a>(
    node: &'a AstNode,
    string_table: &StringTable,
) -> Option<&'a Declaration> {
    let NodeKind::VariableDeclaration(declaration) = &node.kind else {
        return None;
    };

    let is_template_name = declaration
        .id
        .name_str(string_table)
        .is_some_and(|name| name == TOP_LEVEL_TEMPLATE_NAME);

    if !is_template_name {
        return None;
    }

    if !matches!(declaration.value.kind, ExpressionKind::Template(_)) {
        return None;
    }

    Some(declaration)
}

fn as_top_level_template_return_expression(node: &AstNode) -> Option<&Expression> {
    let NodeKind::Return(values) = &node.kind else {
        return None;
    };

    (values.len() == 1).then_some(())?;
    let expression = &values[0];
    matches!(expression.kind, ExpressionKind::Template(_)).then_some(expression)
}

fn build_runtime_fragment_capture_plan(
    candidate: &RuntimeTemplateCandidate,
) -> Result<RuntimeFragmentCapturePlan, CompilerError> {
    // 1) Collect all referenced symbols in the template expression.
    // 2) Pull in only the declaration statements needed to evaluate that expression.
    let mut required_symbols = FxHashSet::default();
    collect_references_from_expression(&candidate.template_expression, &mut required_symbols);

    let declaration_lookup = candidate
        .preceding_statements
        .iter()
        .enumerate()
        .filter_map(|(index, node)| {
            let NodeKind::VariableDeclaration(declaration) = &node.kind else {
                return None;
            };

            Some((declaration.id.to_owned(), (index, declaration)))
        })
        .collect::<FxHashMap<_, _>>();

    let mut included_declarations = FxHashSet::default();
    let mut visiting = FxHashSet::default();
    for symbol in required_symbols {
        include_declaration_dependencies(
            &symbol,
            &declaration_lookup,
            &mut included_declarations,
            &mut visiting,
        )?;
    }

    include_mutation_dependencies(
        &declaration_lookup,
        &mut included_declarations,
        &mut visiting,
    )?;

    let included_symbols = included_declarations
        .iter()
        .filter_map(|index| {
            let NodeKind::VariableDeclaration(declaration) =
                &candidate.preceding_statements.get(*index)?.kind
            else {
                return None;
            };
            Some(declaration.id.to_owned())
        })
        .collect::<FxHashSet<_>>();

    // Reassignments to captured symbols would require mutable capture semantics.
    // Keep this strict until the runtime fragment model supports mutable state capture.
    for statement in &candidate.preceding_statements {
        let NodeKind::Assignment { target, .. } = &statement.kind else {
            continue;
        };

        let mut assignment_targets = FxHashSet::default();
        collect_references_from_ast_node(target, &mut assignment_targets);

        // Runtime fragments currently capture declaration snapshots, not full mutable state.
        // Reject assignments that would require mutable capture semantics.
        if assignment_targets
            .into_iter()
            .any(|symbol| included_symbols.contains(&symbol))
        {
            return Err(CompilerError::new_rule_error(
                "Runtime start-fragment captures currently do not support mutable reassignments before template evaluation.",
                statement.location.clone(),
            ));
        }
    }

    let mut ordered_indices = included_declarations.into_iter().collect::<Vec<_>>();
    ordered_indices.sort_unstable();

    let mut fragment_body = Vec::with_capacity(ordered_indices.len());
    for index in ordered_indices {
        let Some(statement) = candidate
            .preceding_statements
            .get(index)
            .map(ToOwned::to_owned)
        else {
            return Err(CompilerError::compiler_error(
                "Fragment dependency index was out of bounds.",
            ));
        };
        fragment_body.push(statement);
    }

    Ok(RuntimeFragmentCapturePlan {
        fragment_body,
        captured_symbols: included_symbols,
    })
}

fn include_mutation_dependencies(
    declaration_lookup: &FxHashMap<InternedPath, (usize, &Declaration)>,
    included_declarations: &mut FxHashSet<usize>,
    visiting: &mut FxHashSet<InternedPath>,
) -> Result<(), CompilerError> {
    loop {
        let tracked_symbols = declaration_lookup
            .iter()
            .filter_map(|(symbol, (index, _))| {
                included_declarations
                    .contains(index)
                    .then_some(symbol.to_owned())
            })
            .collect::<FxHashSet<_>>();

        if tracked_symbols.is_empty() {
            break;
        }

        let mut symbols_to_include = Vec::new();
        for (symbol, (index, declaration)) in declaration_lookup {
            if included_declarations.contains(index) {
                continue;
            }

            if expression_may_mutate_tracked_symbols(&declaration.value, &tracked_symbols) {
                symbols_to_include.push(symbol.to_owned());
            }
        }

        if symbols_to_include.is_empty() {
            break;
        }

        let previous_len = included_declarations.len();
        for symbol in symbols_to_include {
            include_declaration_dependencies(
                &symbol,
                declaration_lookup,
                included_declarations,
                visiting,
            )?;
        }

        if included_declarations.len() == previous_len {
            break;
        }
    }

    Ok(())
}

fn expression_may_mutate_tracked_symbols(
    expression: &Expression,
    tracked_symbols: &FxHashSet<InternedPath>,
) -> bool {
    match &expression.kind {
        ExpressionKind::FunctionCall(_, args)
        | ExpressionKind::HostFunctionCall(_, args)
        | ExpressionKind::Collection(args) => {
            call_arguments_mutate_tracked_symbols(args, tracked_symbols)
                || args.iter().any(|argument| {
                    expression_may_mutate_tracked_symbols(argument, tracked_symbols)
                })
        }

        ExpressionKind::ResultHandledFunctionCall { args, handling, .. } => {
            if call_arguments_mutate_tracked_symbols(args, tracked_symbols) {
                return true;
            }

            if args
                .iter()
                .any(|argument| expression_may_mutate_tracked_symbols(argument, tracked_symbols))
            {
                return true;
            }

            match handling {
                ResultCallHandling::Fallback(fallback_values) => {
                    fallback_values.iter().any(|fallback| {
                        expression_may_mutate_tracked_symbols(fallback, tracked_symbols)
                    })
                }
                ResultCallHandling::Handler { fallback, body, .. } => {
                    fallback.as_ref().is_some_and(|fallback_values| {
                        fallback_values.iter().any(|fallback| {
                            expression_may_mutate_tracked_symbols(fallback, tracked_symbols)
                        })
                    }) || body
                        .iter()
                        .any(|node| ast_node_may_mutate_tracked_symbols(node, tracked_symbols))
                }
                ResultCallHandling::Propagate => false,
            }
        }

        ExpressionKind::Runtime(nodes) => nodes
            .iter()
            .any(|node| ast_node_may_mutate_tracked_symbols(node, tracked_symbols)),

        ExpressionKind::Template(template) => template
            .content
            .flatten_expressions()
            .into_iter()
            .any(|value| expression_may_mutate_tracked_symbols(&value, tracked_symbols)),

        ExpressionKind::StructDefinition(arguments) | ExpressionKind::StructInstance(arguments) => {
            arguments.iter().any(|argument| {
                expression_may_mutate_tracked_symbols(&argument.value, tracked_symbols)
            })
        }

        ExpressionKind::Range(lower, upper) => {
            expression_may_mutate_tracked_symbols(lower, tracked_symbols)
                || expression_may_mutate_tracked_symbols(upper, tracked_symbols)
        }

        ExpressionKind::Function(_, body) => body
            .iter()
            .any(|node| ast_node_may_mutate_tracked_symbols(node, tracked_symbols)),

        ExpressionKind::BuiltinCast { value, .. }
        | ExpressionKind::ResultConstruct { value, .. }
        | ExpressionKind::Coerced { value, .. } => {
            expression_may_mutate_tracked_symbols(value, tracked_symbols)
        }

        ExpressionKind::HandledResult { value, handling } => {
            if expression_may_mutate_tracked_symbols(value, tracked_symbols) {
                return true;
            }

            match handling {
                ResultCallHandling::Fallback(fallback_values) => {
                    fallback_values.iter().any(|fallback| {
                        expression_may_mutate_tracked_symbols(fallback, tracked_symbols)
                    })
                }
                ResultCallHandling::Handler { fallback, body, .. } => {
                    fallback.as_ref().is_some_and(|fallback_values| {
                        fallback_values.iter().any(|fallback| {
                            expression_may_mutate_tracked_symbols(fallback, tracked_symbols)
                        })
                    }) || body
                        .iter()
                        .any(|node| ast_node_may_mutate_tracked_symbols(node, tracked_symbols))
                }
                ResultCallHandling::Propagate => false,
            }
        }

        ExpressionKind::Copy(place) => ast_node_may_mutate_tracked_symbols(place, tracked_symbols),

        ExpressionKind::Reference(_)
        | ExpressionKind::NoValue
        | ExpressionKind::OptionNone
        | ExpressionKind::Int(_)
        | ExpressionKind::Float(_)
        | ExpressionKind::StringSlice(_)
        | ExpressionKind::Bool(_)
        | ExpressionKind::Char(_)
        | ExpressionKind::Path(_) => false,
    }
}

fn ast_node_may_mutate_tracked_symbols(
    node: &AstNode,
    tracked_symbols: &FxHashSet<InternedPath>,
) -> bool {
    match &node.kind {
        NodeKind::VariableDeclaration(declaration) => {
            expression_may_mutate_tracked_symbols(&declaration.value, tracked_symbols)
        }

        NodeKind::Assignment { target, value } => {
            ast_node_references_tracked_symbols(target, tracked_symbols)
                || expression_may_mutate_tracked_symbols(value, tracked_symbols)
        }

        NodeKind::MethodCall { receiver, args, .. } => {
            ast_node_references_tracked_symbols(receiver, tracked_symbols)
                || call_named_arguments_mutate_tracked_symbols(args, tracked_symbols)
                || args.iter().any(|argument| {
                    expression_may_mutate_tracked_symbols(&argument.value, tracked_symbols)
                })
        }

        NodeKind::FunctionCall { args, .. } | NodeKind::HostFunctionCall { args, .. } => {
            call_named_arguments_mutate_tracked_symbols(args, tracked_symbols)
                || args.iter().any(|argument| {
                    expression_may_mutate_tracked_symbols(&argument.value, tracked_symbols)
                })
        }

        NodeKind::ResultHandledFunctionCall { args, handling, .. } => {
            if call_named_arguments_mutate_tracked_symbols(args, tracked_symbols) {
                return true;
            }

            if args.iter().any(|argument| {
                expression_may_mutate_tracked_symbols(&argument.value, tracked_symbols)
            }) {
                return true;
            }

            match handling {
                ResultCallHandling::Fallback(fallback_values) => {
                    fallback_values.iter().any(|fallback| {
                        expression_may_mutate_tracked_symbols(fallback, tracked_symbols)
                    })
                }
                ResultCallHandling::Handler { fallback, body, .. } => {
                    fallback.as_ref().is_some_and(|fallback_values| {
                        fallback_values.iter().any(|fallback| {
                            expression_may_mutate_tracked_symbols(fallback, tracked_symbols)
                        })
                    }) || body.iter().any(|statement| {
                        ast_node_may_mutate_tracked_symbols(statement, tracked_symbols)
                    })
                }
                ResultCallHandling::Propagate => false,
            }
        }

        NodeKind::Rvalue(expression) => {
            expression_may_mutate_tracked_symbols(expression, tracked_symbols)
        }

        NodeKind::Return(values) => values
            .iter()
            .any(|value| expression_may_mutate_tracked_symbols(value, tracked_symbols)),

        NodeKind::ReturnError(value) => {
            expression_may_mutate_tracked_symbols(value, tracked_symbols)
        }

        NodeKind::If(condition, then_body, else_body) => {
            expression_may_mutate_tracked_symbols(condition, tracked_symbols)
                || then_body.iter().any(|statement| {
                    ast_node_may_mutate_tracked_symbols(statement, tracked_symbols)
                })
                || else_body.as_ref().is_some_and(|body| {
                    body.iter().any(|statement| {
                        ast_node_may_mutate_tracked_symbols(statement, tracked_symbols)
                    })
                })
        }

        NodeKind::Match(scrutinee, arms, default) => {
            if expression_may_mutate_tracked_symbols(scrutinee, tracked_symbols) {
                return true;
            }

            if arms.iter().any(|arm| {
                expression_may_mutate_tracked_symbols(&arm.condition, tracked_symbols)
                    || arm.body.iter().any(|statement| {
                        ast_node_may_mutate_tracked_symbols(statement, tracked_symbols)
                    })
            }) {
                return true;
            }

            default.as_ref().is_some_and(|body| {
                body.iter().any(|statement| {
                    ast_node_may_mutate_tracked_symbols(statement, tracked_symbols)
                })
            })
        }

        NodeKind::RangeLoop {
            bindings,
            range,
            body,
        } => {
            bindings.item.as_ref().is_some_and(|binding| {
                expression_may_mutate_tracked_symbols(&binding.value, tracked_symbols)
            }) || bindings.index.as_ref().is_some_and(|binding| {
                expression_may_mutate_tracked_symbols(&binding.value, tracked_symbols)
            }) || expression_may_mutate_tracked_symbols(&range.start, tracked_symbols)
                || expression_may_mutate_tracked_symbols(&range.end, tracked_symbols)
                || range.step.as_ref().is_some_and(|step| {
                    expression_may_mutate_tracked_symbols(step, tracked_symbols)
                })
                || body.iter().any(|statement| {
                    ast_node_may_mutate_tracked_symbols(statement, tracked_symbols)
                })
        }

        NodeKind::CollectionLoop {
            bindings,
            iterable,
            body,
        } => {
            bindings.item.as_ref().is_some_and(|binding| {
                expression_may_mutate_tracked_symbols(&binding.value, tracked_symbols)
            }) || bindings.index.as_ref().is_some_and(|binding| {
                expression_may_mutate_tracked_symbols(&binding.value, tracked_symbols)
            }) || expression_may_mutate_tracked_symbols(iterable, tracked_symbols)
                || body.iter().any(|statement| {
                    ast_node_may_mutate_tracked_symbols(statement, tracked_symbols)
                })
        }

        NodeKind::WhileLoop(condition, body) => {
            expression_may_mutate_tracked_symbols(condition, tracked_symbols)
                || body.iter().any(|statement| {
                    ast_node_may_mutate_tracked_symbols(statement, tracked_symbols)
                })
        }

        NodeKind::FieldAccess { base, .. } => {
            ast_node_may_mutate_tracked_symbols(base, tracked_symbols)
        }

        NodeKind::MultiBind { value, .. } => {
            expression_may_mutate_tracked_symbols(value, tracked_symbols)
        }

        NodeKind::StructDefinition(_, fields) => fields
            .iter()
            .any(|field| expression_may_mutate_tracked_symbols(&field.value, tracked_symbols)),

        NodeKind::Function(_, _, body) => body
            .iter()
            .any(|statement| ast_node_may_mutate_tracked_symbols(statement, tracked_symbols)),

        NodeKind::Break | NodeKind::Continue | NodeKind::Operator(_) => false,
    }
}

fn call_arguments_mutate_tracked_symbols(
    args: &[Expression],
    tracked_symbols: &FxHashSet<InternedPath>,
) -> bool {
    args.iter().any(|argument| {
        argument.ownership == Ownership::MutableOwned
            && expression_references_tracked_symbols(argument, tracked_symbols)
    })
}

fn call_named_arguments_mutate_tracked_symbols(
    args: &[CallArgument],
    tracked_symbols: &FxHashSet<InternedPath>,
) -> bool {
    args.iter().any(|argument| {
        argument.access_mode == CallAccessMode::Mutable
            && expression_references_tracked_symbols(&argument.value, tracked_symbols)
    })
}

fn ast_node_references_tracked_symbols(
    node: &AstNode,
    tracked_symbols: &FxHashSet<InternedPath>,
) -> bool {
    let mut references = FxHashSet::default();
    collect_references_from_ast_node(node, &mut references);
    references
        .into_iter()
        .any(|symbol| tracked_symbols.contains(&symbol))
}

fn expression_references_tracked_symbols(
    expression: &Expression,
    tracked_symbols: &FxHashSet<InternedPath>,
) -> bool {
    let mut references = FxHashSet::default();
    collect_references_from_expression(expression, &mut references);
    references
        .into_iter()
        .any(|symbol| tracked_symbols.contains(&symbol))
}

fn include_declaration_dependencies(
    symbol: &InternedPath,
    declaration_lookup: &FxHashMap<InternedPath, (usize, &Declaration)>,
    included_declarations: &mut FxHashSet<usize>,
    visiting: &mut FxHashSet<InternedPath>,
) -> Result<(), CompilerError> {
    let Some((index, declaration)) = declaration_lookup.get(symbol) else {
        return Ok(());
    };

    if included_declarations.contains(index) {
        return Ok(());
    }

    if !visiting.insert(symbol.to_owned()) {
        return Err(CompilerError::compiler_error(
            "Cyclic declaration capture detected while synthesizing runtime fragment.",
        ));
    }

    let mut nested_symbols = FxHashSet::default();
    collect_references_from_expression(&declaration.value, &mut nested_symbols);
    for dependency in nested_symbols {
        if dependency != *symbol {
            include_declaration_dependencies(
                &dependency,
                declaration_lookup,
                included_declarations,
                visiting,
            )?;
        }
    }

    visiting.remove(symbol);
    included_declarations.insert(*index);
    Ok(())
}

fn collect_references_from_expression(
    expression: &Expression,
    references: &mut FxHashSet<InternedPath>,
) {
    match &expression.kind {
        ExpressionKind::Reference(name) => {
            references.insert(name.to_owned());
        }

        ExpressionKind::Copy(place) => {
            collect_references_from_ast_node(place, references);
        }

        ExpressionKind::Runtime(nodes) => {
            for node in nodes {
                collect_references_from_ast_node(node, references);
            }
        }

        ExpressionKind::FunctionCall(_, args)
        | ExpressionKind::HostFunctionCall(_, args)
        | ExpressionKind::Collection(args) => {
            for argument in args {
                collect_references_from_expression(argument, references);
            }
        }

        ExpressionKind::ResultHandledFunctionCall { args, handling, .. } => {
            for argument in args {
                collect_references_from_expression(argument, references);
            }

            match handling {
                ResultCallHandling::Fallback(fallback_values) => {
                    for fallback in fallback_values {
                        collect_references_from_expression(fallback, references);
                    }
                }
                ResultCallHandling::Handler { fallback, body, .. } => {
                    if let Some(fallback_values) = fallback {
                        for fallback in fallback_values {
                            collect_references_from_expression(fallback, references);
                        }
                    }

                    for node in body {
                        collect_references_from_ast_node(node, references);
                    }
                }
                ResultCallHandling::Propagate => {}
            }
        }

        ExpressionKind::BuiltinCast { value, .. } => {
            collect_references_from_expression(value, references);
        }

        ExpressionKind::ResultConstruct { value, .. } => {
            collect_references_from_expression(value, references);
        }

        ExpressionKind::HandledResult { value, handling } => {
            collect_references_from_expression(value, references);

            match handling {
                ResultCallHandling::Fallback(fallback_values) => {
                    for fallback in fallback_values {
                        collect_references_from_expression(fallback, references);
                    }
                }
                ResultCallHandling::Handler { fallback, body, .. } => {
                    if let Some(fallback_values) = fallback {
                        for fallback in fallback_values {
                            collect_references_from_expression(fallback, references);
                        }
                    }

                    for node in body {
                        collect_references_from_ast_node(node, references);
                    }
                }
                ResultCallHandling::Propagate => {}
            }
        }

        ExpressionKind::Template(template) => {
            for value in template.content.flatten_expressions() {
                collect_references_from_expression(&value, references);
            }
        }

        ExpressionKind::StructDefinition(arguments) | ExpressionKind::StructInstance(arguments) => {
            for argument in arguments {
                collect_references_from_expression(&argument.value, references);
            }
        }

        ExpressionKind::Range(lower, upper) => {
            collect_references_from_expression(lower, references);
            collect_references_from_expression(upper, references);
        }

        ExpressionKind::Function(_, body) => {
            for node in body {
                collect_references_from_ast_node(node, references);
            }
        }

        ExpressionKind::Coerced { value, .. } => {
            collect_references_from_expression(value, references);
        }

        ExpressionKind::NoValue
        | ExpressionKind::OptionNone
        | ExpressionKind::Int(_)
        | ExpressionKind::Float(_)
        | ExpressionKind::StringSlice(_)
        | ExpressionKind::Bool(_)
        | ExpressionKind::Char(_)
        | ExpressionKind::Path(_) => {}
    }
}

fn collect_references_from_ast_node(node: &AstNode, references: &mut FxHashSet<InternedPath>) {
    match &node.kind {
        NodeKind::VariableDeclaration(declaration) => {
            collect_references_from_expression(&declaration.value, references);
        }

        NodeKind::Assignment { target, value } => {
            collect_references_from_ast_node(target, references);
            collect_references_from_expression(value, references);
        }

        NodeKind::FieldAccess { base, .. } => {
            collect_references_from_ast_node(base, references);
        }

        NodeKind::MethodCall { receiver, args, .. } => {
            collect_references_from_ast_node(receiver, references);
            for argument in args {
                collect_references_from_expression(&argument.value, references);
            }
        }

        NodeKind::FunctionCall { args, .. } | NodeKind::HostFunctionCall { args, .. } => {
            for argument in args {
                collect_references_from_expression(&argument.value, references);
            }
        }

        NodeKind::ResultHandledFunctionCall { args, handling, .. } => {
            for argument in args {
                collect_references_from_expression(&argument.value, references);
            }

            match handling {
                ResultCallHandling::Fallback(fallback_values) => {
                    for fallback in fallback_values {
                        collect_references_from_expression(fallback, references);
                    }
                }
                ResultCallHandling::Handler { fallback, body, .. } => {
                    if let Some(fallback_values) = fallback {
                        for fallback in fallback_values {
                            collect_references_from_expression(fallback, references);
                        }
                    }

                    for node in body {
                        collect_references_from_ast_node(node, references);
                    }
                }
                ResultCallHandling::Propagate => {}
            }
        }

        NodeKind::MultiBind { targets: _, value } => {
            collect_references_from_expression(value, references);
        }

        NodeKind::StructDefinition(_, fields) => {
            for field in fields {
                collect_references_from_expression(&field.value, references);
            }
        }

        NodeKind::Function(_, _, body) => {
            for statement in body {
                collect_references_from_ast_node(statement, references);
            }
        }

        NodeKind::Rvalue(expression) => {
            collect_references_from_expression(expression, references);
        }

        NodeKind::Return(values) => {
            for value in values {
                collect_references_from_expression(value, references);
            }
        }

        NodeKind::ReturnError(value) => {
            collect_references_from_expression(value, references);
        }

        NodeKind::If(condition, then_body, else_body) => {
            collect_references_from_expression(condition, references);
            for statement in then_body {
                collect_references_from_ast_node(statement, references);
            }
            if let Some(else_body) = else_body {
                for statement in else_body {
                    collect_references_from_ast_node(statement, references);
                }
            }
        }

        NodeKind::Match(scrutinee, arms, default) => {
            collect_references_from_expression(scrutinee, references);
            for arm in arms {
                collect_references_from_expression(&arm.condition, references);
                for statement in &arm.body {
                    collect_references_from_ast_node(statement, references);
                }
            }
            if let Some(default_body) = default {
                for statement in default_body {
                    collect_references_from_ast_node(statement, references);
                }
            }
        }

        NodeKind::RangeLoop {
            bindings,
            range,
            body,
        } => {
            if let Some(item_binding) = &bindings.item {
                collect_references_from_expression(&item_binding.value, references);
            }
            if let Some(index_binding) = &bindings.index {
                collect_references_from_expression(&index_binding.value, references);
            }
            collect_references_from_expression(&range.start, references);
            collect_references_from_expression(&range.end, references);
            if let Some(step) = &range.step {
                collect_references_from_expression(step, references);
            }
            for statement in body {
                collect_references_from_ast_node(statement, references);
            }
        }

        NodeKind::CollectionLoop {
            bindings,
            iterable,
            body,
        } => {
            if let Some(item_binding) = &bindings.item {
                collect_references_from_expression(&item_binding.value, references);
            }
            if let Some(index_binding) = &bindings.index {
                collect_references_from_expression(&index_binding.value, references);
            }
            collect_references_from_expression(iterable, references);
            for statement in body {
                collect_references_from_ast_node(statement, references);
            }
        }

        NodeKind::WhileLoop(condition, body) => {
            collect_references_from_expression(condition, references);
            for statement in body {
                collect_references_from_ast_node(statement, references);
            }
        }

        NodeKind::Break | NodeKind::Continue | NodeKind::Operator(_) => {}
    }
}

#[cfg(test)]
#[path = "tests/template_tests.rs"]
mod template_tests;
