//! Runtime/const start-fragment synthesis orchestration.
//!
//! WHAT: orders const/runtime fragment sources and emits generated runtime
//! fragment wrapper functions.
//! WHY: keeping this pass focused on synthesis/orchestration makes capture and
//! extraction logic independently testable.

use super::capture_analysis::{
    build_runtime_fragment_capture_plan, prune_template_only_captured_setup,
};
use super::fragment_extraction::{extract_runtime_template_candidates, replace_entry_start_body};
use super::{AstStartTemplateItem, RuntimeTemplateCandidate, fold_template_with_context};
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::statements::functions::{
    FunctionReturn, FunctionSignature, ReturnSlot,
};
use crate::compiler_frontend::ast::templates::template::TemplateConstValueKind;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::headers::parse_file_headers::{
    TopLevelTemplateItem, TopLevelTemplateKind,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::projects::settings::IMPLICIT_START_FUNC_NAME;
use rustc_hash::{FxHashMap, FxHashSet};

#[allow(clippy::large_enum_variant)]
#[derive(Clone)]
enum OrderedFragmentSource {
    Const {
        value: StringId,
        location: SourceLocation,
    },
    Runtime(RuntimeTemplateCandidate),
}

pub(super) fn synthesize_start_template_items(
    ast_nodes: &mut Vec<AstNode>,
    entry_dir: &InternedPath,
    top_level_template_items: &[TopLevelTemplateItem],
    const_templates_by_path: &FxHashMap<InternedPath, StringId>,
    project_path_resolver: &ProjectPathResolver,
    path_format_config: &PathStringFormatConfig,
    string_table: &mut StringTable,
) -> Result<Vec<AstStartTemplateItem>, CompilerError> {
    // 1) Locate the entry start function and extract runtime top-level templates.
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

    let extraction =
        extract_runtime_template_candidates(ast_nodes, entry_start_index, string_table)?;

    // 2) Merge entry-file const-template headers with extracted runtime templates in source order.
    let mut ordered_header_items = top_level_template_items.to_owned();
    ordered_header_items.sort_by_key(|item| item.file_order);

    let mut ordered_fragment_sources: Vec<OrderedFragmentSource> =
        Vec::with_capacity(ordered_header_items.len() + extraction.runtime_candidates.len());
    for template_item in ordered_header_items {
        let TopLevelTemplateKind::ConstTemplate { header_path } = template_item.kind;
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

    // Runtime fragment ordering comes from extracted top-level template declarations.
    // This avoids treating templates nested inside unrelated expressions as start fragments.
    for candidate in extraction.runtime_candidates {
        ordered_fragment_sources.push(OrderedFragmentSource::Runtime(candidate));
    }
    ordered_fragment_sources.sort_by(compare_fragment_locations);

    // 3) Emit ordered fragment items and generated runtime wrapper functions.
    let mut next_fragment_index = 0usize;
    let mut captured_runtime_symbols = FxHashSet::default();
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

                // Wrapper-shaped does not imply compile-time-final.
                // Only fully renderable final templates become const start fragments.
                // Runtime templates remain runtime fragment entries.
                if matches!(
                    template.const_value_kind(),
                    TemplateConstValueKind::RenderableString
                ) {
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

                // Runtime fragments are generated functions so builders can keep a
                // stable "mount slot -> callable fragment" contract while `start()`
                // remains the lifecycle entrypoint after hydration.
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
                    scope: extraction.entry_scope.to_owned(),
                });

                start_template_items.push(AstStartTemplateItem::RuntimeStringFunction {
                    function: fragment_name,
                    location: candidate.location,
                });
            }
        }
    }

    // 4) Keep start() focused on non-template behavior; prune setup now replayed
    // only for runtime-fragment hydration.
    let pruned_start_body =
        prune_template_only_captured_setup(extraction.non_template_body, &captured_runtime_symbols);
    replace_entry_start_body(ast_nodes, entry_start_index, pruned_start_body)?;

    Ok(start_template_items)
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
