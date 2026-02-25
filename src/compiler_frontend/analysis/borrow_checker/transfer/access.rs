//! Statement/terminator transfer rules.
//!
//! This file contains the forward transfer logic for borrow checking.
//! It classifies shared vs mutable access, checks exclusivity constraints,
//! and emits statement/terminator/value facts.

use crate::compiler_frontend::analysis::borrow_checker::state::{
    BorrowState, FunctionLayout, LocalState, RootSet,
};
use crate::compiler_frontend::analysis::borrow_checker::types::{
    AccessKind, LocalMode, StatementBorrowFact, TerminatorBorrowFact, ValueAccessClassification,
};
use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorLocation};
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, HirExpression, HirExpressionKind, HirMatchArm, HirPattern, HirPlace, HirStatement,
    HirStatementKind, HirTerminator, OptionVariant,
};
use crate::return_borrow_checker_error;

use super::call_semantics::{CallResultAlias, resolve_call_semantics};
use super::facts::{StatementAccessTracker, ValueFactBuffer, roots_to_local_ids};
use super::{BlockTransferStats, BorrowTransferContext};

pub(super) fn transfer_statement(
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
                let argument_location = context
                    .diagnostics
                    .value_error_location(argument.id, location.clone());

                if semantics.arg_mutability[arg_index] {
                    // For mutable arguments, the argument root itself should be treated as
                    // mutable access, not an initial shared load. We still record any shared
                    // reads needed to evaluate projections (for example index expressions).
                    match &argument.kind {
                        HirExpressionKind::Load(place) => {
                            record_shared_reads_in_place_indices(
                                context,
                                layout,
                                state,
                                place,
                                &mut tracker,
                                argument_location.clone(),
                                stats,
                                value_fact_buffer,
                            )?;
                        }
                        _ => {
                            record_shared_reads_in_expression(
                                context,
                                layout,
                                state,
                                argument,
                                &mut tracker,
                                argument_location.clone(),
                                stats,
                                value_fact_buffer,
                            )?;
                        }
                    }
                } else {
                    record_shared_reads_in_expression(
                        context,
                        layout,
                        state,
                        argument,
                        &mut tracker,
                        argument_location.clone(),
                        stats,
                        value_fact_buffer,
                    )?;
                }

                let mut roots = RootSet::empty(layout.local_count());
                collect_expression_roots(
                    layout,
                    state,
                    argument,
                    &mut roots,
                    argument_location.clone(),
                )?;
                arg_roots[arg_index] = roots;

                if semantics.arg_mutability[arg_index] {
                    let mutable_roots =
                        mutable_argument_roots(layout, state, argument, argument_location.clone())?;
                    if !mutable_roots.is_empty() {
                        check_mutable_access(
                            context,
                            layout,
                            state,
                            &mutable_roots,
                            false,
                            None,
                            &mut tracker,
                            argument_location,
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
            // Ownership/drop semantics are handled by later analyses.
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

pub(super) fn transfer_terminator(
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
                true,
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
                context, layout, state, &roots, true, None, tracker, location, stats,
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
    allow_prior_shared: bool,
    actor_index_hint: Option<usize>,
    tracker: &mut StatementAccessTracker,
    location: ErrorLocation,
    stats: &mut BlockTransferStats,
) -> Result<(), CompilerError> {
    for root_index in roots.iter_ones() {
        stats.conflicts_checked += 1;

        if let Some(existing) = tracker.conflict(root_index, AccessKind::Mutable) {
            if allow_prior_shared && existing == AccessKind::Shared {
                tracker.record(root_index, AccessKind::Mutable);
                continue;
            }

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
