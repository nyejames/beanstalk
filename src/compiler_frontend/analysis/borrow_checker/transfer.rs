use crate::backends::function_registry::{
    CallTarget, HostAccessKind, HostFunctionDef, HostRegistry, HostReturnAlias,
};
use crate::compiler_frontend::analysis::borrow_checker::diagnostics::BorrowDiagnostics;
use crate::compiler_frontend::analysis::borrow_checker::state::{
    BorrowState, FunctionLayout, LocalState, RootSet,
};
use crate::compiler_frontend::analysis::borrow_checker::types::{
    AccessKind, FunctionReturnAliasSummary, LocalMode, StatementBorrowFact, TerminatorBorrowFact,
    ValueAccessClassification, ValueBorrowFact,
};
use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorLocation};
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FunctionId, HirExpression, HirExpressionKind, HirMatchArm, HirNodeId, HirPattern,
    HirPlace, HirStatement, HirStatementKind, HirTerminator, HirValueId, OptionVariant,
};
use crate::compiler_frontend::hir::hir_nodes::{HirBlock, HirModule};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::return_borrow_checker_error;
use rustc_hash::FxHashMap;

pub(super) struct BorrowTransferContext<'a> {
    pub module: &'a HirModule,
    pub string_table: &'a StringTable,
    pub host_registry: &'a HostRegistry,
    pub function_by_path: &'a FxHashMap<InternedPath, FunctionId>,
    pub function_param_mutability: &'a FxHashMap<FunctionId, Vec<bool>>,
    pub function_return_alias: &'a FxHashMap<FunctionId, FunctionReturnAliasSummary>,
    pub diagnostics: BorrowDiagnostics<'a>,
}

#[derive(Debug, Clone, Default)]
pub(super) struct BlockTransferStats {
    pub statements_analyzed: usize,
    pub terminators_analyzed: usize,
    pub conflicts_checked: usize,
    pub mutable_call_sites: usize,
    pub statement_facts: Vec<(HirNodeId, StatementBorrowFact)>,
    pub terminator_fact: Option<(BlockId, TerminatorBorrowFact)>,
    pub value_facts: Vec<(HirValueId, ValueBorrowFact)>,
}

pub(super) fn transfer_block(
    context: &BorrowTransferContext<'_>,
    layout: &FunctionLayout,
    block: &HirBlock,
    state: &mut BorrowState,
) -> Result<BlockTransferStats, CompilerError> {
    let mut stats = BlockTransferStats::default();
    let mut value_fact_buffer = ValueFactBuffer::new(layout.local_count());

    for statement in &block.statements {
        transfer_statement(
            context,
            layout,
            state,
            statement,
            &mut stats,
            &mut value_fact_buffer,
        )?;
        stats.statements_analyzed += 1;
    }

    transfer_terminator(
        context,
        layout,
        state,
        block.id,
        &block.terminator,
        &mut stats,
        &mut value_fact_buffer,
    )?;
    stats.terminators_analyzed += 1;

    stats.value_facts = value_fact_buffer.into_serialized(layout);
    Ok(stats)
}

fn transfer_statement(
    context: &BorrowTransferContext<'_>,
    layout: &FunctionLayout,
    state: &mut BorrowState,
    statement: &HirStatement,
    stats: &mut BlockTransferStats,
    value_fact_buffer: &mut ValueFactBuffer,
) -> Result<(), CompilerError> {
    let mut tracker = StatementAccessTracker::new(layout.local_count());
    let conflicts_before = stats.conflicts_checked;

    match &statement.kind {
        HirStatementKind::Assign { target, value } => {
            let location = context.diagnostics.statement_error_location(statement);

            record_shared_reads_in_place_indices(
                context,
                layout,
                state,
                target,
                &mut tracker,
                location.clone(),
                stats,
                value_fact_buffer,
            )?;
            record_shared_reads_in_expression(
                context,
                layout,
                state,
                value,
                &mut tracker,
                location.clone(),
                stats,
                value_fact_buffer,
            )?;

            transfer_assign_target(
                context,
                layout,
                state,
                target,
                value,
                &mut tracker,
                location,
                stats,
            )?;
        }

        HirStatementKind::Call {
            target,
            args,
            result,
        } => {
            let location = context.diagnostics.statement_error_location(statement);
            let semantics = resolve_call_semantics(context, target, args.len(), location.clone())?;
            if semantics
                .arg_mutability
                .iter()
                .any(|is_mutable| *is_mutable)
            {
                stats.mutable_call_sites += 1;
            }

            let mut arg_roots = vec![RootSet::empty(layout.local_count()); args.len()];

            for (arg_index, argument) in args.iter().enumerate() {
                record_shared_reads_in_expression(
                    context,
                    layout,
                    state,
                    argument,
                    &mut tracker,
                    location.clone(),
                    stats,
                    value_fact_buffer,
                )?;

                let mut roots = RootSet::empty(layout.local_count());
                collect_expression_roots(
                    layout,
                    state,
                    argument,
                    &mut roots,
                    context
                        .diagnostics
                        .value_error_location(argument.id, location.clone()),
                )?;
                arg_roots[arg_index] = roots.clone();

                if semantics.arg_mutability[arg_index] {
                    let mutable_roots = mutable_argument_roots(
                        layout,
                        state,
                        argument,
                        context
                            .diagnostics
                            .value_error_location(argument.id, location.clone()),
                    )?;
                    if !mutable_roots.is_empty() {
                        check_mutable_access(
                            context,
                            layout,
                            state,
                            &mutable_roots,
                            None,
                            &mut tracker,
                            context
                                .diagnostics
                                .value_error_location(argument.id, location.clone()),
                            stats,
                        )?;
                    }

                    value_fact_buffer.record(
                        argument.id,
                        ValueAccessClassification::MutableArgument,
                        &mutable_roots,
                    );
                }
            }

            if let Some(result_local) = result {
                let Some(local_index) = layout.index_of(*result_local) else {
                    return_borrow_checker_error!(
                        format!(
                            "Call result local '{}' is not in the active function layout",
                            context.diagnostics.local_name(*result_local)
                        ),
                        location,
                        {
                            CompilationStage => "Borrow Checking",
                        }
                    );
                };

                let alias_roots = match semantics.return_alias {
                    CallResultAlias::Fresh => None,
                    CallResultAlias::AliasArgs(ref arg_indices) => {
                        let mut roots = RootSet::empty(layout.local_count());
                        for arg_index in arg_indices {
                            if let Some(arg_root_set) = arg_roots.get(*arg_index) {
                                roots.union_with(arg_root_set);
                            }
                        }
                        Some(roots)
                    }
                    CallResultAlias::Unknown => {
                        let mut roots = RootSet::empty(layout.local_count());
                        for arg_root_set in &arg_roots {
                            roots.union_with(arg_root_set);
                        }
                        Some(roots)
                    }
                };

                let new_local_state = match alias_roots {
                    Some(roots) if !roots.is_empty() => LocalState::alias(roots),
                    _ => LocalState::slot(layout.local_count()),
                };
                state.update_local_state(local_index, new_local_state);
            }
        }

        HirStatementKind::Expr(expression) => {
            let location = context.diagnostics.statement_error_location(statement);
            record_shared_reads_in_expression(
                context,
                layout,
                state,
                expression,
                &mut tracker,
                location,
                stats,
                value_fact_buffer,
            )?;
        }

        HirStatementKind::Drop(_local) => {
            // Ownership and drop-elision are handled by later phases.
        }
    }

    let statement_fact = StatementBorrowFact {
        shared_roots: roots_to_local_ids(layout, &tracker.shared_roots),
        mutable_roots: roots_to_local_ids(layout, &tracker.mutable_roots),
        conflicts_checked: stats.conflicts_checked - conflicts_before,
    };
    stats.statement_facts.push((statement.id, statement_fact));

    Ok(())
}

fn transfer_terminator(
    context: &BorrowTransferContext<'_>,
    layout: &FunctionLayout,
    state: &BorrowState,
    block_id: BlockId,
    terminator: &HirTerminator,
    stats: &mut BlockTransferStats,
    value_fact_buffer: &mut ValueFactBuffer,
) -> Result<(), CompilerError> {
    let mut tracker = StatementAccessTracker::new(layout.local_count());
    let location = context
        .diagnostics
        .terminator_error_location(block_id, terminator);
    let conflicts_before = stats.conflicts_checked;

    match terminator {
        HirTerminator::Jump { args, .. } => {
            for local in args {
                let Some(local_index) = layout.index_of(*local) else {
                    return_borrow_checker_error!(
                        format!(
                            "Jump argument local '{}' is not in the active function layout",
                            context.diagnostics.local_name(*local)
                        ),
                        location.clone(),
                        {
                            CompilationStage => "Borrow Checking",
                        }
                    );
                };

                let roots = state.effective_roots(local_index);
                check_shared_access(
                    context,
                    layout,
                    &roots,
                    &mut tracker,
                    location.clone(),
                    stats,
                )?;
            }
        }

        HirTerminator::If { condition, .. } => {
            record_shared_reads_in_expression(
                context,
                layout,
                state,
                condition,
                &mut tracker,
                location,
                stats,
                value_fact_buffer,
            )?;
        }

        HirTerminator::Match { scrutinee, arms } => {
            record_shared_reads_in_expression(
                context,
                layout,
                state,
                scrutinee,
                &mut tracker,
                location.clone(),
                stats,
                value_fact_buffer,
            )?;

            for arm in arms {
                record_shared_reads_in_pattern(
                    context,
                    layout,
                    state,
                    arm,
                    &mut tracker,
                    location.clone(),
                    stats,
                    value_fact_buffer,
                )?;
            }
        }

        HirTerminator::Return(value) => {
            record_shared_reads_in_expression(
                context,
                layout,
                state,
                value,
                &mut tracker,
                location,
                stats,
                value_fact_buffer,
            )?;
        }

        HirTerminator::Panic { message } => {
            if let Some(message) = message {
                record_shared_reads_in_expression(
                    context,
                    layout,
                    state,
                    message,
                    &mut tracker,
                    location,
                    stats,
                    value_fact_buffer,
                )?;
            }
        }

        HirTerminator::Loop { .. }
        | HirTerminator::Break { .. }
        | HirTerminator::Continue { .. } => {}
    }

    stats.terminator_fact = Some((
        block_id,
        TerminatorBorrowFact {
            shared_roots: roots_to_local_ids(layout, &tracker.shared_roots),
            mutable_roots: roots_to_local_ids(layout, &tracker.mutable_roots),
            conflicts_checked: stats.conflicts_checked - conflicts_before,
        },
    ));

    Ok(())
}

fn record_shared_reads_in_pattern(
    context: &BorrowTransferContext<'_>,
    layout: &FunctionLayout,
    state: &BorrowState,
    arm: &HirMatchArm,
    tracker: &mut StatementAccessTracker,
    location: ErrorLocation,
    stats: &mut BlockTransferStats,
    value_fact_buffer: &mut ValueFactBuffer,
) -> Result<(), CompilerError> {
    if let HirPattern::Literal(expression) = &arm.pattern {
        record_shared_reads_in_expression(
            context,
            layout,
            state,
            expression,
            tracker,
            location.clone(),
            stats,
            value_fact_buffer,
        )?;
    }

    if let Some(guard) = &arm.guard {
        record_shared_reads_in_expression(
            context,
            layout,
            state,
            guard,
            tracker,
            location,
            stats,
            value_fact_buffer,
        )?;
    }

    Ok(())
}

#[derive(Debug, Clone)]
struct CallSemantics {
    arg_mutability: Vec<bool>,
    return_alias: CallResultAlias,
}

#[derive(Debug, Clone)]
enum CallResultAlias {
    Fresh,
    AliasArgs(Vec<usize>),
    Unknown,
}

fn resolve_call_semantics(
    context: &BorrowTransferContext<'_>,
    target: &CallTarget,
    arg_len: usize,
    location: ErrorLocation,
) -> Result<CallSemantics, CompilerError> {
    match target {
        CallTarget::UserFunction(path) => {
            let Some(function_id) = context.function_by_path.get(path).copied() else {
                return_borrow_checker_error!(
                    format!(
                        "Borrow checker could not resolve user call target '{}'",
                        context.diagnostics.path_name(path)
                    ),
                    location,
                    {
                        CompilationStage => "Borrow Checking",
                        PrimarySuggestion => "Ensure the called function is declared in the module before use",
                    }
                );
            };

            let Some(param_mutability) = context.function_param_mutability.get(&function_id) else {
                return_borrow_checker_error!(
                    format!(
                        "Borrow checker is missing parameter mutability metadata for function '{}'",
                        context.diagnostics.function_name(function_id)
                    ),
                    context.diagnostics.function_error_location(function_id),
                    {
                        CompilationStage => "Borrow Checking",
                    }
                );
            };

            if param_mutability.len() != arg_len {
                return_borrow_checker_error!(
                    format!(
                        "Borrow checker found argument count mismatch for function '{}': expected {}, got {}",
                        context.diagnostics.function_name(function_id),
                        param_mutability.len(),
                        arg_len
                    ),
                    location,
                    {
                        CompilationStage => "Borrow Checking",
                        PrimarySuggestion => "Ensure call argument count matches the function signature",
                    }
                );
            }

            let return_alias = match context.function_return_alias.get(&function_id) {
                Some(FunctionReturnAliasSummary::Fresh) => CallResultAlias::Fresh,
                Some(FunctionReturnAliasSummary::AliasParams(indices)) => {
                    CallResultAlias::AliasArgs(indices.clone())
                }
                Some(FunctionReturnAliasSummary::Unknown) | None => CallResultAlias::Unknown,
            };

            Ok(CallSemantics {
                arg_mutability: param_mutability.clone(),
                return_alias,
            })
        }

        CallTarget::HostFunction(path) => {
            let host_def = resolve_host_definition(context, path, location.clone())?;
            if host_def.parameters.len() != arg_len {
                return_borrow_checker_error!(
                    format!(
                        "Borrow checker found argument count mismatch for host function '{}': expected {}, got {}",
                        host_def.name,
                        host_def.parameters.len(),
                        arg_len
                    ),
                    location,
                    {
                        CompilationStage => "Borrow Checking",
                        PrimarySuggestion => "Ensure call argument count matches host function signature",
                    }
                );
            }

            let arg_mutability = host_def
                .parameters
                .iter()
                .map(|param| matches!(param.access_kind, HostAccessKind::Mutable))
                .collect::<Vec<_>>();

            let return_alias = match host_def.return_alias {
                HostReturnAlias::Fresh => CallResultAlias::Fresh,
                HostReturnAlias::AliasAnyArg => CallResultAlias::AliasArgs((0..arg_len).collect()),
                HostReturnAlias::AliasMutableArgs => CallResultAlias::AliasArgs(
                    arg_mutability
                        .iter()
                        .enumerate()
                        .filter_map(
                            |(index, is_mutable)| if *is_mutable { Some(index) } else { None },
                        )
                        .collect(),
                ),
            };

            Ok(CallSemantics {
                arg_mutability,
                return_alias,
            })
        }
    }
}

fn resolve_host_definition<'a>(
    context: &'a BorrowTransferContext<'_>,
    path: &InternedPath,
    location: ErrorLocation,
) -> Result<&'a HostFunctionDef, CompilerError> {
    if let Some(name) = path.name_str(context.string_table) {
        if let Some(definition) = context.host_registry.get_function(name) {
            return Ok(definition);
        }
    }

    let full = path.to_string(context.string_table);
    if let Some(definition) = context.host_registry.get_function(&full) {
        return Ok(definition);
    }

    return_borrow_checker_error!(
        format!(
            "Borrow checker could not resolve host call target '{}'",
            context.diagnostics.path_name(path)
        ),
        location,
        {
            CompilationStage => "Borrow Checking",
            PrimarySuggestion => "Ensure host registry metadata includes this host function",
        }
    )
}

fn transfer_assign_target(
    context: &BorrowTransferContext<'_>,
    layout: &FunctionLayout,
    state: &mut BorrowState,
    target: &HirPlace,
    value: &HirExpression,
    tracker: &mut StatementAccessTracker,
    location: ErrorLocation,
    stats: &mut BlockTransferStats,
) -> Result<(), CompilerError> {
    match target {
        HirPlace::Local(local_id) => {
            let Some(local_index) = layout.index_of(*local_id) else {
                return_borrow_checker_error!(
                    format!(
                        "Assignment target local '{}' is not in the active function layout",
                        context.diagnostics.local_name(*local_id)
                    ),
                    location,
                    {
                        CompilationStage => "Borrow Checking",
                    }
                );
            };

            let local_state = state.local_state(local_index).clone();
            let rhs_alias_roots =
                direct_place_roots_from_expression(layout, state, value, location.clone())?;

            if local_state.mode.is_definitely_uninit() {
                apply_slot_rebinding(state, layout.local_count(), local_index, rhs_alias_roots);
                return Ok(());
            }

            let mut write_roots = RootSet::empty(layout.local_count());
            if local_state.mode.contains(LocalMode::SLOT) {
                write_roots.insert(local_index);
            }
            if local_state.mode.contains(LocalMode::ALIAS) {
                write_roots.union_with(&local_state.alias_roots);
            }

            check_mutable_access(
                context,
                layout,
                state,
                &write_roots,
                Some(local_index),
                tracker,
                location.clone(),
                stats,
            )?;

            match (
                local_state.mode.contains(LocalMode::SLOT),
                local_state.mode.contains(LocalMode::ALIAS),
            ) {
                (false, true) => {
                    // Alias-view writes through to referent and does not rebind.
                }

                (true, false) => {
                    apply_slot_rebinding(state, layout.local_count(), local_index, rhs_alias_roots);
                }

                (true, true) => {
                    let mut alias_roots = local_state.alias_roots;
                    if let Some(rhs_roots) = rhs_alias_roots {
                        alias_roots.union_with(&rhs_roots);
                    }

                    state.update_local_state(
                        local_index,
                        LocalState {
                            mode: LocalMode::SLOT.union(LocalMode::ALIAS),
                            alias_roots,
                        },
                    );
                }

                (false, false) => {
                    state.update_local_state(local_index, LocalState::slot(layout.local_count()));
                }
            }
        }

        _ => {
            let roots = roots_for_place(layout, state, target, location.clone())?;
            check_mutable_access(
                context, layout, state, &roots, None, tracker, location, stats,
            )?;
        }
    }

    Ok(())
}

fn apply_slot_rebinding(
    state: &mut BorrowState,
    local_count: usize,
    local_index: usize,
    rhs_alias_roots: Option<RootSet>,
) {
    match rhs_alias_roots {
        Some(roots) => state.update_local_state(local_index, LocalState::alias(roots)),
        None => state.update_local_state(local_index, LocalState::slot(local_count)),
    }
}

fn check_shared_access(
    context: &BorrowTransferContext<'_>,
    layout: &FunctionLayout,
    roots: &RootSet,
    tracker: &mut StatementAccessTracker,
    location: ErrorLocation,
    stats: &mut BlockTransferStats,
) -> Result<(), CompilerError> {
    for root_index in roots.iter_ones() {
        stats.conflicts_checked += 1;

        if let Some(existing) = tracker.conflict(root_index, AccessKind::Shared) {
            let root_name = context.diagnostics.local_name(layout.local_ids[root_index]);

            return_borrow_checker_error!(
                format!(
                    "Cannot read '{}' as shared after a mutable access in the same evaluation sequence ({:?} -> Shared)",
                    root_name,
                    existing
                ),
                location,
                {
                    CompilationStage => "Borrow Checking",
                    BorrowKind => "Shared",
                    PrimarySuggestion => "Split the expression into separate statements to avoid overlapping access modes",
                }
            );
        }

        tracker.record(root_index, AccessKind::Shared);
    }

    Ok(())
}

fn check_mutable_access(
    context: &BorrowTransferContext<'_>,
    layout: &FunctionLayout,
    state: &BorrowState,
    roots: &RootSet,
    actor_index_hint: Option<usize>,
    tracker: &mut StatementAccessTracker,
    location: ErrorLocation,
    stats: &mut BlockTransferStats,
) -> Result<(), CompilerError> {
    for root_index in roots.iter_ones() {
        stats.conflicts_checked += 1;

        if let Some(existing) = tracker.conflict(root_index, AccessKind::Mutable) {
            let root_name = context.diagnostics.local_name(layout.local_ids[root_index]);

            return_borrow_checker_error!(
                format!(
                    "Cannot mutably access '{}' due to overlapping {:?} access in the same evaluation sequence",
                    root_name,
                    existing
                ),
                location.clone(),
                {
                    CompilationStage => "Borrow Checking",
                    BorrowKind => "Mutable",
                    PrimarySuggestion => "Split mutable and shared accesses into separate statements",
                }
            );
        }

        if !layout.local_mutable[root_index] {
            let root_name = context.diagnostics.local_name(layout.local_ids[root_index]);
            return_borrow_checker_error!(
                format!("Cannot mutably access immutable local '{}'", root_name),
                location.clone(),
                {
                    CompilationStage => "Borrow Checking",
                    BorrowKind => "Mutable",
                    PrimarySuggestion => "Declare the variable as mutable with '~=' before mutating it",
                }
            );
        }

        let alias_count = state.alias_count_for_root(root_index);
        if alias_count > 1 {
            let actor_index = actor_index_hint.unwrap_or(root_index);
            let actor_name = context
                .diagnostics
                .local_name(layout.local_ids[actor_index]);
            let conflicting_local = context
                .diagnostics
                .conflicting_local_for_root(layout, state, actor_index, root_index)
                .map(|local| context.diagnostics.local_name(local))
                .unwrap_or_else(|| String::from("<unknown>"));

            return_borrow_checker_error!(
                format!(
                    "Cannot mutably access '{}' because '{}' may alias the same value",
                    actor_name, conflicting_local
                ),
                location.clone(),
                {
                    CompilationStage => "Borrow Checking",
                    BorrowKind => "Mutable",
                    LifetimeHint => "Mutable access requires exclusive aliasing",
                }
            );
        }

        tracker.record(root_index, AccessKind::Mutable);
    }

    Ok(())
}

fn mutable_argument_roots(
    layout: &FunctionLayout,
    state: &BorrowState,
    expression: &HirExpression,
    location: ErrorLocation,
) -> Result<RootSet, CompilerError> {
    if let HirExpressionKind::Load(place) = &expression.kind {
        return roots_for_place(layout, state, place, location);
    }

    let mut roots = RootSet::empty(layout.local_count());
    collect_expression_roots(layout, state, expression, &mut roots, location)?;
    Ok(roots)
}

fn direct_place_roots_from_expression(
    layout: &FunctionLayout,
    state: &BorrowState,
    expression: &HirExpression,
    location: ErrorLocation,
) -> Result<Option<RootSet>, CompilerError> {
    if let HirExpressionKind::Load(place) = &expression.kind {
        return Ok(Some(roots_for_place(layout, state, place, location)?));
    }

    Ok(None)
}

fn roots_for_place(
    layout: &FunctionLayout,
    state: &BorrowState,
    place: &HirPlace,
    location: ErrorLocation,
) -> Result<RootSet, CompilerError> {
    match place {
        HirPlace::Local(local_id) => {
            let Some(local_index) = layout.index_of(*local_id) else {
                return_borrow_checker_error!(
                    format!(
                        "Borrow checker could not resolve place local '{}' in the current function",
                        local_id
                    ),
                    location,
                    {
                        CompilationStage => "Borrow Checking",
                    }
                );
            };

            let local_state = state.local_state(local_index);
            if local_state.mode.is_definitely_uninit() {
                return_borrow_checker_error!(
                    format!(
                        "Borrow checker encountered use of local '{}' before initialization or after scope end",
                        local_id
                    ),
                    location,
                    {
                        CompilationStage => "Borrow Checking",
                        PrimarySuggestion => "Initialize the local before use and avoid using branch-local locals outside their region",
                    }
                );
            }

            Ok(state.effective_roots(local_index))
        }

        HirPlace::Field { base, .. } => roots_for_place(layout, state, base, location),

        HirPlace::Index { base, .. } => roots_for_place(layout, state, base, location),
    }
}

fn record_shared_reads_in_place_indices(
    context: &BorrowTransferContext<'_>,
    layout: &FunctionLayout,
    state: &BorrowState,
    place: &HirPlace,
    tracker: &mut StatementAccessTracker,
    location: ErrorLocation,
    stats: &mut BlockTransferStats,
    value_fact_buffer: &mut ValueFactBuffer,
) -> Result<(), CompilerError> {
    match place {
        HirPlace::Local(_) => Ok(()),

        HirPlace::Field { base, .. } => record_shared_reads_in_place_indices(
            context,
            layout,
            state,
            base,
            tracker,
            location,
            stats,
            value_fact_buffer,
        ),

        HirPlace::Index { base, index } => {
            record_shared_reads_in_place_indices(
                context,
                layout,
                state,
                base,
                tracker,
                location.clone(),
                stats,
                value_fact_buffer,
            )?;

            record_shared_reads_in_expression(
                context,
                layout,
                state,
                index,
                tracker,
                location,
                stats,
                value_fact_buffer,
            )
        }
    }
}

fn record_shared_reads_in_expression(
    context: &BorrowTransferContext<'_>,
    layout: &FunctionLayout,
    state: &BorrowState,
    expression: &HirExpression,
    tracker: &mut StatementAccessTracker,
    location: ErrorLocation,
    stats: &mut BlockTransferStats,
    value_fact_buffer: &mut ValueFactBuffer,
) -> Result<(), CompilerError> {
    match &expression.kind {
        HirExpressionKind::Int(_)
        | HirExpressionKind::Float(_)
        | HirExpressionKind::Bool(_)
        | HirExpressionKind::Char(_)
        | HirExpressionKind::StringLiteral(_) => {}

        HirExpressionKind::Load(place) => {
            let value_location = context
                .diagnostics
                .value_error_location(expression.id, location.clone());
            record_shared_reads_in_place_indices(
                context,
                layout,
                state,
                place,
                tracker,
                value_location.clone(),
                stats,
                value_fact_buffer,
            )?;

            let roots = roots_for_place(layout, state, place, value_location.clone())?;
            check_shared_access(context, layout, &roots, tracker, value_location, stats)?;
        }

        HirExpressionKind::BinOp { left, right, .. } => {
            record_shared_reads_in_expression(
                context,
                layout,
                state,
                left,
                tracker,
                location.clone(),
                stats,
                value_fact_buffer,
            )?;
            record_shared_reads_in_expression(
                context,
                layout,
                state,
                right,
                tracker,
                location.clone(),
                stats,
                value_fact_buffer,
            )?;
        }

        HirExpressionKind::UnaryOp { operand, .. } => {
            record_shared_reads_in_expression(
                context,
                layout,
                state,
                operand,
                tracker,
                location.clone(),
                stats,
                value_fact_buffer,
            )?;
        }

        HirExpressionKind::StructConstruct { fields, .. } => {
            for (_, value) in fields {
                record_shared_reads_in_expression(
                    context,
                    layout,
                    state,
                    value,
                    tracker,
                    location.clone(),
                    stats,
                    value_fact_buffer,
                )?;
            }
        }

        HirExpressionKind::Collection(elements)
        | HirExpressionKind::TupleConstruct { elements } => {
            for element in elements {
                record_shared_reads_in_expression(
                    context,
                    layout,
                    state,
                    element,
                    tracker,
                    location.clone(),
                    stats,
                    value_fact_buffer,
                )?;
            }
        }

        HirExpressionKind::Range { start, end } => {
            record_shared_reads_in_expression(
                context,
                layout,
                state,
                start,
                tracker,
                location.clone(),
                stats,
                value_fact_buffer,
            )?;
            record_shared_reads_in_expression(
                context,
                layout,
                state,
                end,
                tracker,
                location.clone(),
                stats,
                value_fact_buffer,
            )?;
        }

        HirExpressionKind::OptionConstruct { variant, value } => {
            if matches!(variant, OptionVariant::Some) {
                if let Some(inner) = value {
                    record_shared_reads_in_expression(
                        context,
                        layout,
                        state,
                        inner,
                        tracker,
                        location.clone(),
                        stats,
                        value_fact_buffer,
                    )?;
                }
            }
        }

        HirExpressionKind::ResultConstruct { value, .. } => {
            record_shared_reads_in_expression(
                context,
                layout,
                state,
                value,
                tracker,
                location.clone(),
                stats,
                value_fact_buffer,
            )?;
        }
    }

    let mut expression_roots = RootSet::empty(layout.local_count());
    collect_expression_roots(
        layout,
        state,
        expression,
        &mut expression_roots,
        context
            .diagnostics
            .value_error_location(expression.id, location.clone()),
    )?;
    let classification = if expression_roots.is_empty() {
        ValueAccessClassification::None
    } else {
        ValueAccessClassification::SharedRead
    };
    value_fact_buffer.record(expression.id, classification, &expression_roots);

    Ok(())
}

fn collect_expression_roots(
    layout: &FunctionLayout,
    state: &BorrowState,
    expression: &HirExpression,
    out: &mut RootSet,
    location: ErrorLocation,
) -> Result<(), CompilerError> {
    match &expression.kind {
        HirExpressionKind::Load(place) => {
            let roots = roots_for_place(layout, state, place, location.clone())?;
            out.union_with(&roots);

            if let HirPlace::Index { index, .. } = place {
                collect_expression_roots(layout, state, index, out, location)?;
            }
        }

        HirExpressionKind::BinOp { left, right, .. } => {
            collect_expression_roots(layout, state, left, out, location.clone())?;
            collect_expression_roots(layout, state, right, out, location)?;
        }

        HirExpressionKind::UnaryOp { operand, .. } => {
            collect_expression_roots(layout, state, operand, out, location)?;
        }

        HirExpressionKind::StructConstruct { fields, .. } => {
            for (_, value) in fields {
                collect_expression_roots(layout, state, value, out, location.clone())?;
            }
        }

        HirExpressionKind::Collection(elements)
        | HirExpressionKind::TupleConstruct { elements } => {
            for element in elements {
                collect_expression_roots(layout, state, element, out, location.clone())?;
            }
        }

        HirExpressionKind::Range { start, end } => {
            collect_expression_roots(layout, state, start, out, location.clone())?;
            collect_expression_roots(layout, state, end, out, location)?;
        }

        HirExpressionKind::OptionConstruct { value, .. } => {
            if let Some(inner) = value {
                collect_expression_roots(layout, state, inner, out, location)?;
            }
        }

        HirExpressionKind::ResultConstruct { value, .. } => {
            collect_expression_roots(layout, state, value, out, location)?;
        }

        HirExpressionKind::Int(_)
        | HirExpressionKind::Float(_)
        | HirExpressionKind::Bool(_)
        | HirExpressionKind::Char(_)
        | HirExpressionKind::StringLiteral(_) => {}
    }

    Ok(())
}

fn roots_to_local_ids(
    layout: &FunctionLayout,
    roots: &RootSet,
) -> Vec<crate::compiler_frontend::hir::hir_nodes::LocalId> {
    roots
        .iter_ones()
        .map(|index| layout.local_ids[index])
        .collect::<Vec<_>>()
}

#[derive(Debug, Clone)]
struct StatementAccessTracker {
    root_access: Vec<Option<AccessKind>>,
    shared_roots: RootSet,
    mutable_roots: RootSet,
}

impl StatementAccessTracker {
    fn new(root_count: usize) -> Self {
        Self {
            root_access: vec![None; root_count],
            shared_roots: RootSet::empty(root_count),
            mutable_roots: RootSet::empty(root_count),
        }
    }

    fn conflict(&self, root_index: usize, new_access: AccessKind) -> Option<AccessKind> {
        let existing = self.root_access[root_index]?;

        match (existing, new_access) {
            (AccessKind::Shared, AccessKind::Shared) => None,
            (AccessKind::Shared, AccessKind::Mutable)
            | (AccessKind::Mutable, AccessKind::Shared)
            | (AccessKind::Mutable, AccessKind::Mutable) => Some(existing),
        }
    }

    fn record(&mut self, root_index: usize, access: AccessKind) {
        match access {
            AccessKind::Shared => self.shared_roots.insert(root_index),
            AccessKind::Mutable => self.mutable_roots.insert(root_index),
        }

        let entry = &mut self.root_access[root_index];
        match (*entry, access) {
            (Some(AccessKind::Mutable), _) => {}
            (_, AccessKind::Mutable) => *entry = Some(AccessKind::Mutable),
            (None, AccessKind::Shared) => *entry = Some(AccessKind::Shared),
            (Some(AccessKind::Shared), AccessKind::Shared) => {}
        }
    }
}

#[derive(Debug, Clone)]
struct ValueFactBuffer {
    local_count: usize,
    facts: FxHashMap<HirValueId, (ValueAccessClassification, RootSet)>,
}

impl ValueFactBuffer {
    fn new(local_count: usize) -> Self {
        Self {
            local_count,
            facts: FxHashMap::default(),
        }
    }

    fn record(
        &mut self,
        value_id: HirValueId,
        classification: ValueAccessClassification,
        roots: &RootSet,
    ) {
        let entry = self.facts.entry(value_id).or_insert_with(|| {
            (
                ValueAccessClassification::None,
                RootSet::empty(self.local_count),
            )
        });

        entry.0 = entry.0.merge(classification);
        entry.1.union_with(roots);
    }

    fn into_serialized(self, layout: &FunctionLayout) -> Vec<(HirValueId, ValueBorrowFact)> {
        self.facts
            .into_iter()
            .map(|(value_id, (classification, roots))| {
                (
                    value_id,
                    ValueBorrowFact {
                        classification,
                        roots: roots_to_local_ids(layout, &roots),
                    },
                )
            })
            .collect::<Vec<_>>()
    }
}
