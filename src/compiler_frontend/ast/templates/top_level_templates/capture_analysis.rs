//! Runtime-fragment capture dependency analysis and start-body pruning.
//!
//! WHAT: computes fragment-local setup statements required to evaluate runtime
//! templates and prunes template-only setup from the entry start body.
//! WHY: runtime fragments execute before `start()`, so setup must be replayed
//! deterministically without coupling capture policy to orchestration code.

use super::RuntimeTemplateCandidate;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::symbol_analysis::{
    ast_node_may_mutate_tracked_symbols, collect_references_from_ast_node,
    collect_references_from_expression,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::interned_path::InternedPath;
use rustc_hash::{FxHashMap, FxHashSet};

pub(super) struct RuntimeFragmentCapturePlan {
    pub(super) fragment_body: Vec<AstNode>,
    pub(super) captured_symbols: FxHashSet<InternedPath>,
}

/// WHAT: determines which preceding setup statements must be replayed in a
/// runtime fragment function so the template can evaluate correctly.
///
/// WHY: runtime templates lower to generated standalone functions, so they
/// cannot rely on entry-start local state unless we explicitly replay the
/// relevant declaration/assignment setup in fragment order.
pub(super) fn build_runtime_fragment_capture_plan(
    candidate: &RuntimeTemplateCandidate,
) -> Result<RuntimeFragmentCapturePlan, CompilerError> {
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
    let mut required_symbols = FxHashSet::default();
    collect_references_from_expression(&candidate.template_expression, &mut required_symbols);
    for symbol in required_symbols {
        include_declaration_dependencies(
            &symbol,
            &declaration_lookup,
            &mut included_declarations,
            &mut visiting,
        )?;
    }

    let mut included_statements = included_declarations.clone();
    loop {
        let tracked_symbols = declaration_symbols_for_indices(
            &included_declarations,
            &candidate.preceding_statements,
        );
        if tracked_symbols.is_empty() {
            break;
        }

        let previous_statement_len = included_statements.len();
        let previous_declaration_len = included_declarations.len();

        for (index, statement) in candidate.preceding_statements.iter().enumerate() {
            if included_statements.contains(&index) {
                continue;
            }

            if !ast_node_may_mutate_tracked_symbols(statement, &tracked_symbols) {
                continue;
            }

            // Replay mutable setup that can influence already-captured symbols.
            // This replaces the old rejection rule with deterministic setup replay.
            included_statements.insert(index);

            // Declarations that mutate captured symbols are part of replay setup too.
            // Mark the declaration itself as captured so start-body pruning can
            // safely remove template-only declarations like short-circuit temps.
            if let NodeKind::VariableDeclaration(declaration) = &statement.kind {
                include_declaration_dependencies(
                    &declaration.id,
                    &declaration_lookup,
                    &mut included_declarations,
                    &mut visiting,
                )?;
            }

            // If the mutation statement depends on declarations, capture those too.
            let mut dependencies = FxHashSet::default();
            collect_references_from_ast_node(statement, &mut dependencies);
            for dependency in dependencies {
                include_declaration_dependencies(
                    &dependency,
                    &declaration_lookup,
                    &mut included_declarations,
                    &mut visiting,
                )?;
            }
        }

        // All included declarations must execute in the fragment setup body.
        included_statements.extend(included_declarations.iter().copied());

        if included_statements.len() == previous_statement_len
            && included_declarations.len() == previous_declaration_len
        {
            break;
        }
    }

    let captured_symbols =
        declaration_symbols_for_indices(&included_declarations, &candidate.preceding_statements);

    let mut ordered_indices = included_statements.into_iter().collect::<Vec<_>>();
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
        captured_symbols,
    })
}

/// WHAT: prunes start-body setup that exists only to feed runtime fragment
/// captures, while preserving setup still required by non-template start code.
///
/// WHY: runtime fragments replay template-only setup in generated functions; the
/// entry start body should keep only behavior needed after fragment hydration.
pub(super) fn prune_template_only_captured_setup(
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

    let template_only_assignment_indices =
        find_template_only_assignment_indices(&start_body, captured_symbols);

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
    for (index, statement) in start_body.iter().enumerate() {
        if let NodeKind::VariableDeclaration(declaration) = &statement.kind
            && prunable_symbols.contains(&declaration.id)
        {
            continue;
        }

        // Template-only assignments that target captured symbols are replayed in
        // runtime fragment setup and do not force start-body declaration retention.
        if template_only_assignment_indices.contains(&index) {
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
        .enumerate()
        .filter_map(|(index, statement)| {
            if matches!(
                &statement.kind,
                NodeKind::VariableDeclaration(declaration)
                    if pruned_symbols.contains(&declaration.id)
            ) {
                return None;
            }

            if template_only_assignment_indices.contains(&index)
                && assignment_targets_only_symbols(&statement, &pruned_symbols)
            {
                return None;
            }

            Some(statement)
        })
        .collect()
}

fn find_template_only_assignment_indices(
    start_body: &[AstNode],
    symbols: &FxHashSet<InternedPath>,
) -> FxHashSet<usize> {
    start_body
        .iter()
        .enumerate()
        .filter_map(|(index, statement)| {
            assignment_targets_only_symbols(statement, symbols).then_some(index)
        })
        .collect()
}

fn assignment_targets_only_symbols(statement: &AstNode, symbols: &FxHashSet<InternedPath>) -> bool {
    let NodeKind::Assignment { target, .. } = &statement.kind else {
        return false;
    };

    let mut assignment_targets = FxHashSet::default();
    collect_references_from_ast_node(target, &mut assignment_targets);
    !assignment_targets.is_empty()
        && assignment_targets
            .iter()
            .all(|target| symbols.contains(target))
}

fn declaration_symbols_for_indices(
    indices: &FxHashSet<usize>,
    statements: &[AstNode],
) -> FxHashSet<InternedPath> {
    indices
        .iter()
        .filter_map(|index| {
            let NodeKind::VariableDeclaration(declaration) = &statements.get(*index)?.kind else {
                return None;
            };
            Some(declaration.id.to_owned())
        })
        .collect()
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
