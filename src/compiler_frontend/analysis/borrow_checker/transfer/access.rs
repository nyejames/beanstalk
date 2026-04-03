//! Statement/terminator transfer rules.
//!
//! This file contains the forward transfer logic for borrow checking.
//! It classifies shared vs mutable access, checks exclusivity constraints,
//! and emits statement/terminator/value facts.

use crate::compiler_frontend::analysis::borrow_checker::state::{
    BorrowState, FunctionLayout, LocalState, RootSet,
};
use crate::compiler_frontend::analysis::borrow_checker::types::{
    LocalMode, StatementBorrowFact, TerminatorBorrowFact, ValueAccessClassification,
};
use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, HirExpression, HirExpressionKind, HirMatchArm, HirPattern, HirPlace, HirStatement,
    HirStatementKind, HirTerminator, OptionVariant, ValueKind,
};
use crate::return_borrow_checker_error;

use super::call_semantics::{ArgEffect, CallResultAlias, resolve_call_semantics};
use super::facts::{StatementAccessTracker, ValueFactBuffer, roots_to_local_ids};
use super::{BlockTransferStats, BorrowTransferContext};

mod conflicts;
mod move_decision;

use conflicts::{check_mutable_access, check_shared_access};
use move_decision::{MoveDecision, classify_move_decision};

// WHAT: These helper contexts split statement transfer into two concerns:
// shared-read collection and access-conflict validation.
// WHY: transfer threads the same diagnostics/layout/state bundle through many helpers, so keeping
// those bundles explicit avoids wide argument lists without cloning the same context structs.
// WHAT: Shared-read traversal context used while scanning one statement/terminator.
// WHY: Keeps helper signatures compact while threading diagnostics and fact sinks.
struct SharedReadEnv<'a, 'module> {
    context: &'a BorrowTransferContext<'module>,
    layout: &'a FunctionLayout,
    state: &'a BorrowState,
    tracker: &'a mut StatementAccessTracker,
    location: SourceLocation,
    current_order: i32,
    stats: &'a mut BlockTransferStats,
    value_fact_buffer: &'a mut ValueFactBuffer,
}

// WHAT: Shared and mutable conflict checks both inspect the same transfer bundle.
// WHY: Keeping one access-check context avoids duplicating the same layout/state/tracker fields.
struct AccessCheckContext<'a, 'module> {
    context: &'a BorrowTransferContext<'module>,
    layout: &'a FunctionLayout,
    state: &'a BorrowState,
    tracker: &'a mut StatementAccessTracker,
    location: SourceLocation,
    stats: &'a mut BlockTransferStats,
    actor_index_hint: Option<usize>,
    current_order: i32,
}

#[derive(Clone, Copy)]
struct MutableAccessPolicy {
    allow_prior_shared: bool,
    require_root_mutable: bool,
    strict_move_exclusivity: bool,
}

pub(super) fn transfer_statement(
    context: &BorrowTransferContext<'_>,
    layout: &FunctionLayout,
    state: &mut BorrowState,
    block_id: BlockId,
    statement: &HirStatement,
    stats: &mut BlockTransferStats,
    value_fact_buffer: &mut ValueFactBuffer,
) -> Result<(), CompilerError> {
    // WHAT: transfer one statement at the block frontier.
    // WHY: the fixed-point driver merges only block states, so statement effects must be exact.
    let mut tracker = StatementAccessTracker::new(layout.local_count());
    let conflicts_before = stats.conflicts_checked;
    let statement_order = layout.statement_order_or_unknown(statement.id);

    match &statement.kind {
        HirStatementKind::Assign { target, value } => {
            let location = context.diagnostics.statement_error_location(statement);

            {
                let mut read_env = SharedReadEnv {
                    context,
                    layout,
                    state,
                    tracker: &mut tracker,
                    location: location.clone(),
                    current_order: statement_order,
                    stats,
                    value_fact_buffer,
                };
                record_shared_reads_in_place_indices(&mut read_env, target, location.clone())?;
            }
            {
                let mut read_env = SharedReadEnv {
                    context,
                    layout,
                    state,
                    tracker: &mut tracker,
                    location: location.clone(),
                    current_order: statement_order,
                    stats,
                    value_fact_buffer,
                };
                record_shared_reads_in_expression(&mut read_env, value, location.clone())?;
            }

            transfer_assign_target(
                context,
                layout,
                state,
                block_id,
                statement_order,
                &mut tracker,
                location,
                stats,
                target,
                value,
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
                .arg_effects
                .iter()
                .any(|effect| !matches!(effect, ArgEffect::SharedBorrow))
            {
                stats.mutable_call_sites += 1;
            }

            let mut arg_roots = vec![RootSet::empty(layout.local_count()); args.len()];

            for (arg_index, argument) in args.iter().enumerate() {
                let argument_location = context
                    .diagnostics
                    .value_error_location(argument.id, location.clone());
                let arg_effect = semantics.arg_effects[arg_index];

                if matches!(arg_effect, ArgEffect::MutableBorrow | ArgEffect::MayConsume) {
                    // For mutable arguments, the argument root itself should be treated as
                    // mutable access, not an initial shared load. We still record any shared
                    // reads needed to evaluate projections (for example index expressions).
                    match &argument.kind {
                        HirExpressionKind::Load(place) => {
                            let mut read_env = SharedReadEnv {
                                context,
                                layout,
                                state,
                                tracker: &mut tracker,
                                location: argument_location.clone(),
                                current_order: statement_order,
                                stats,
                                value_fact_buffer,
                            };
                            record_shared_reads_in_place_indices(
                                &mut read_env,
                                place,
                                argument_location.clone(),
                            )?;
                        }
                        _ => {
                            let mut read_env = SharedReadEnv {
                                context,
                                layout,
                                state,
                                tracker: &mut tracker,
                                location: argument_location.clone(),
                                current_order: statement_order,
                                stats,
                                value_fact_buffer,
                            };
                            record_shared_reads_in_expression(
                                &mut read_env,
                                argument,
                                argument_location.clone(),
                            )?;
                        }
                    }
                } else {
                    let mut read_env = SharedReadEnv {
                        context,
                        layout,
                        state,
                        tracker: &mut tracker,
                        location: argument_location.clone(),
                        current_order: statement_order,
                        stats,
                        value_fact_buffer,
                    };
                    record_shared_reads_in_expression(
                        &mut read_env,
                        argument,
                        argument_location.clone(),
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

                match arg_effect {
                    ArgEffect::SharedBorrow => {}
                    ArgEffect::MutableBorrow => {
                        let mutable_roots = mutable_argument_roots(
                            layout,
                            state,
                            argument,
                            argument_location.clone(),
                        )?;
                        if !mutable_roots.is_empty() {
                            let mut check = AccessCheckContext {
                                context,
                                layout,
                                state,
                                tracker: &mut tracker,
                                location: argument_location.clone(),
                                stats,
                                actor_index_hint: None,
                                current_order: statement_order,
                            };
                            check_mutable_access(
                                &mut check,
                                &mutable_roots,
                                MutableAccessPolicy {
                                    allow_prior_shared: false,
                                    require_root_mutable: true,
                                    strict_move_exclusivity: false,
                                },
                            )?;
                        }

                        value_fact_buffer.record(
                            argument.id,
                            ValueAccessClassification::MutableArgument,
                            &mutable_roots,
                        );
                    }
                    ArgEffect::MayConsume => {
                        let mutable_roots = mutable_argument_roots(
                            layout,
                            state,
                            argument,
                            argument_location.clone(),
                        )?;
                        if !mutable_roots.is_empty() {
                            // WHAT: choose borrow vs move at the call site from future-use facts.
                            // WHY: user mutable params can either borrow or consume through one ABI.
                            match classify_move_decision(
                                layout,
                                block_id,
                                &mutable_roots,
                                statement_order,
                            ) {
                                MoveDecision::Borrow => {
                                    let mut check = AccessCheckContext {
                                        context,
                                        layout,
                                        state,
                                        tracker: &mut tracker,
                                        location: argument_location.clone(),
                                        stats,
                                        actor_index_hint: None,
                                        current_order: statement_order,
                                    };
                                    check_mutable_access(
                                        &mut check,
                                        &mutable_roots,
                                        MutableAccessPolicy {
                                            allow_prior_shared: false,
                                            require_root_mutable: true,
                                            strict_move_exclusivity: false,
                                        },
                                    )?;
                                }
                                MoveDecision::Move => {
                                    let mut check = AccessCheckContext {
                                        context,
                                        layout,
                                        state,
                                        tracker: &mut tracker,
                                        location: argument_location.clone(),
                                        stats,
                                        actor_index_hint: None,
                                        current_order: statement_order,
                                    };
                                    check_mutable_access(
                                        &mut check,
                                        &mutable_roots,
                                        MutableAccessPolicy {
                                            allow_prior_shared: false,
                                            require_root_mutable: false,
                                            strict_move_exclusivity: true,
                                        },
                                    )?;

                                    for root_index in mutable_roots.iter_ones() {
                                        state.invalidate_root(root_index);
                                    }
                                }
                                MoveDecision::Inconsistent(root_index) => {
                                    return_borrow_checker_error!(
                                        format!(
                                            "Inconsistent ownership outcome for '{}' across control-flow paths",
                                            context.diagnostics.local_name(layout.local_ids[root_index])
                                        ),
                                        argument_location.clone(),
                                        {
                                            CompilationStage => "Borrow Checking",
                                            LifetimeHint => "A mutable call argument cannot be moved on one path and borrowed on another",
                                            PrimarySuggestion => "Make ownership outcomes consistent before passing to a mutable parameter",
                                        }
                                    );
                                }
                            }
                        }

                        value_fact_buffer.record(
                            argument.id,
                            ValueAccessClassification::MutableArgument,
                            &mutable_roots,
                        );
                    }
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
                            let Some(arg_root_set) = arg_roots.get(*arg_index) else {
                                return_borrow_checker_error!(
                                    format!(
                                        "Borrow checker found out-of-range return-alias index {} at call site",
                                        arg_index
                                    ),
                                    location.clone(),
                                    {
                                        CompilationStage => "Borrow Checking",
                                        PrimarySuggestion => "Ensure call alias metadata only references existing arguments",
                                    }
                                );
                            };
                            roots.union_with(arg_root_set);
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
            let mut read_env = SharedReadEnv {
                context,
                layout,
                state,
                tracker: &mut tracker,
                location: location.clone(),
                current_order: statement_order,
                stats,
                value_fact_buffer,
            };
            record_shared_reads_in_expression(&mut read_env, expression, location.clone())?;
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
    let terminator_order = layout.terminator_order_or_unknown(block_id);

    match terminator {
        // Jump argument passing is CFG plumbing, not a semantic read.
        HirTerminator::Jump { .. } => {}

        HirTerminator::If { condition, .. } => {
            let mut read_env = SharedReadEnv {
                context,
                layout,
                state,
                tracker: &mut tracker,
                location: location.clone(),
                current_order: terminator_order,
                stats,
                value_fact_buffer,
            };
            record_shared_reads_in_expression(&mut read_env, condition, location.clone())?;
        }

        HirTerminator::Match { scrutinee, arms } => {
            {
                let mut read_env = SharedReadEnv {
                    context,
                    layout,
                    state,
                    tracker: &mut tracker,
                    location: location.clone(),
                    current_order: terminator_order,
                    stats,
                    value_fact_buffer,
                };
                record_shared_reads_in_expression(&mut read_env, scrutinee, location.clone())?;
            }

            for arm in arms {
                let mut read_env = SharedReadEnv {
                    context,
                    layout,
                    state,
                    tracker: &mut tracker,
                    location: location.clone(),
                    current_order: terminator_order,
                    stats,
                    value_fact_buffer,
                };
                record_shared_reads_in_pattern(&mut read_env, arm)?;
            }
        }

        HirTerminator::Return(value) => {
            let mut read_env = SharedReadEnv {
                context,
                layout,
                state,
                tracker: &mut tracker,
                location: location.clone(),
                current_order: terminator_order,
                stats,
                value_fact_buffer,
            };
            record_shared_reads_in_expression(&mut read_env, value, location.clone())?;
        }

        HirTerminator::Panic { message } => {
            if let Some(message) = message {
                let mut read_env = SharedReadEnv {
                    context,
                    layout,
                    state,
                    tracker: &mut tracker,
                    location: location.clone(),
                    current_order: terminator_order,
                    stats,
                    value_fact_buffer,
                };
                record_shared_reads_in_expression(&mut read_env, message, location.clone())?;
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
    env: &mut SharedReadEnv<'_, '_>,
    arm: &HirMatchArm,
) -> Result<(), CompilerError> {
    if let HirPattern::Literal(expression) = &arm.pattern {
        let location = env.location.clone();
        record_shared_reads_in_expression(env, expression, location)?;
    }

    if let Some(guard) = &arm.guard {
        let location = env.location.clone();
        record_shared_reads_in_expression(env, guard, location)?;
    }

    Ok(())
}

fn transfer_assign_target(
    context: &BorrowTransferContext<'_>,
    layout: &FunctionLayout,
    state: &mut BorrowState,
    block_id: BlockId,
    current_order: i32,
    tracker: &mut StatementAccessTracker,
    location: SourceLocation,
    stats: &mut BlockTransferStats,
    target: &HirPlace,
    value: &HirExpression,
) -> Result<(), CompilerError> {
    match target {
        HirPlace::Local(local_id) => {
            let Some(local_index) = layout.index_of(*local_id) else {
                return_borrow_checker_error!(
                    format!(
                        "Assignment target local '{}' is not in the active function layout",
                        context.diagnostics.local_name(*local_id)
                    ),
                    location.clone(),
                    {
                        CompilationStage => "Borrow Checking",
                    }
                );
            };

            let local_state = state.local_state(local_index).clone();
            let mut rhs_alias_roots =
                direct_place_roots_from_expression(layout, state, value, location.clone())?;
            let mut rhs_direct_alias_roots = rhs_alias_roots.as_ref().map(|rhs_roots| {
                direct_root_aliases_from_expression(layout, state, value, rhs_roots)
            });

            if let Some(rhs_roots) = rhs_alias_roots.as_ref().filter(|roots| !roots.is_empty()) {
                let can_attempt_move = local_state.mode.is_definitely_uninit()
                    && layout.local_mutable[local_index]
                    && rhs_roots
                        .iter_ones()
                        .all(|root_index| layout.local_mutable[root_index]);

                if can_attempt_move {
                    // WHAT: assignments can consume source ownership when target takes fresh slot ownership.
                    // WHY: this keeps move propagation aligned with call-site move behavior.
                    match classify_move_decision(layout, block_id, rhs_roots, current_order) {
                        MoveDecision::Borrow | MoveDecision::Inconsistent(_) => {
                            // `May` means path-dependent usage. For assignments we conservatively
                            // keep borrow semantics and let branch joins validate consistency.
                        }
                        MoveDecision::Move => {
                            for root_index in rhs_roots.iter_ones() {
                                state.invalidate_root(root_index);
                            }
                            rhs_alias_roots = None;
                            rhs_direct_alias_roots = None;
                        }
                    }
                }
            }

            if local_state.mode.is_definitely_uninit() {
                match rhs_alias_roots {
                    Some(rhs_roots) => {
                        let target_is_mutable = layout.local_mutable[local_index];
                        if target_is_mutable && !rhs_roots.is_empty() {
                            let mut check = AccessCheckContext {
                                context,
                                layout,
                                state: &*state,
                                tracker,
                                location: location.clone(),
                                stats,
                                actor_index_hint: Some(local_index),
                                current_order,
                            };
                            check_mutable_access(
                                &mut check,
                                &rhs_roots,
                                MutableAccessPolicy {
                                    allow_prior_shared: true,
                                    require_root_mutable: false,
                                    strict_move_exclusivity: false,
                                },
                            )?;
                        }

                        let direct_roots = rhs_direct_alias_roots
                            .unwrap_or_else(|| RootSet::empty(layout.local_count()));
                        state.update_local_state(
                            local_index,
                            LocalState::alias_with_direct(rhs_roots, direct_roots),
                        );
                    }
                    None => {
                        state.update_local_state(
                            local_index,
                            LocalState::slot(layout.local_count()),
                        );
                    }
                }
                return Ok(());
            }

            let mut write_roots = RootSet::empty(layout.local_count());
            if local_state.mode.contains(LocalMode::SLOT) {
                write_roots.insert(local_index);
            }
            if local_state.mode.contains(LocalMode::ALIAS) {
                write_roots.union_with(&local_state.alias_roots);
            }

            let mut check = AccessCheckContext {
                context,
                layout,
                state: &*state,
                tracker,
                location: location.clone(),
                stats,
                actor_index_hint: Some(local_index),
                current_order,
            };
            check_mutable_access(
                &mut check,
                &write_roots,
                MutableAccessPolicy {
                    allow_prior_shared: true,
                    require_root_mutable: true,
                    strict_move_exclusivity: false,
                },
            )?;

            match (
                local_state.mode.contains(LocalMode::SLOT),
                local_state.mode.contains(LocalMode::ALIAS),
            ) {
                (false, true) => {
                    // Alias-view writes through to referent and does not rebind.
                }

                (true, false) => {
                    apply_slot_rebinding(
                        state,
                        layout.local_count(),
                        local_index,
                        rhs_alias_roots,
                        rhs_direct_alias_roots,
                    );
                }

                (true, true) => {
                    let mut alias_roots = local_state.alias_roots;
                    let mut direct_alias_roots = local_state.direct_alias_roots;
                    if let Some(rhs_roots) = rhs_alias_roots {
                        alias_roots.union_with(&rhs_roots);
                    }
                    if let Some(rhs_direct_roots) = rhs_direct_alias_roots {
                        direct_alias_roots.union_with(&rhs_direct_roots);
                    }

                    state.update_local_state(
                        local_index,
                        LocalState {
                            mode: LocalMode::SLOT.union(LocalMode::ALIAS),
                            alias_roots,
                            direct_alias_roots,
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
            let mut check = AccessCheckContext {
                context,
                layout,
                state: &*state,
                tracker,
                location: location.clone(),
                stats,
                actor_index_hint: None,
                current_order,
            };
            check_mutable_access(
                &mut check,
                &roots,
                MutableAccessPolicy {
                    allow_prior_shared: true,
                    require_root_mutable: true,
                    strict_move_exclusivity: false,
                },
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
    rhs_direct_alias_roots: Option<RootSet>,
) {
    match rhs_alias_roots {
        Some(roots) => {
            let direct_roots =
                rhs_direct_alias_roots.unwrap_or_else(|| RootSet::empty(local_count));
            state.update_local_state(
                local_index,
                LocalState::alias_with_direct(roots, direct_roots),
            )
        }
        None => state.update_local_state(local_index, LocalState::slot(local_count)),
    }
}

fn place_root_local_index(layout: &FunctionLayout, place: &HirPlace) -> Option<usize> {
    match place {
        HirPlace::Local(local_id) => layout.index_of(*local_id),
        HirPlace::Field { base, .. } | HirPlace::Index { base, .. } => {
            place_root_local_index(layout, base)
        }
    }
}

fn mutable_argument_roots(
    layout: &FunctionLayout,
    state: &BorrowState,
    expression: &HirExpression,
    location: SourceLocation,
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
    location: SourceLocation,
) -> Result<Option<RootSet>, CompilerError> {
    let HirExpressionKind::Load(place) = &expression.kind else {
        return Ok(None);
    };

    if expression.value_kind == ValueKind::Place {
        return Ok(Some(roots_for_place(layout, state, place, location)?));
    }

    if let HirPlace::Local(local_id) = place
        && let Some(local_index) = layout.index_of(*local_id)
        && state
            .local_state(local_index)
            .mode
            .contains(LocalMode::ALIAS)
    {
        return Ok(Some(roots_for_place(layout, state, place, location)?));
    }

    Ok(None)
}

fn direct_root_aliases_from_expression(
    layout: &FunctionLayout,
    state: &BorrowState,
    expression: &HirExpression,
    rhs_roots: &RootSet,
) -> RootSet {
    let mut direct_roots = RootSet::empty(layout.local_count());

    if expression.value_kind != ValueKind::Place {
        return direct_roots;
    }

    let HirExpressionKind::Load(place) = &expression.kind else {
        return direct_roots;
    };

    let HirPlace::Local(source_local_id) = place else {
        return direct_roots;
    };

    let Some(source_index) = layout.index_of(*source_local_id) else {
        return direct_roots;
    };

    let source_state = state.local_state(source_index);
    if source_state.mode.contains(LocalMode::SLOT) && rhs_roots.contains(source_index) {
        direct_roots.insert(source_index);
    }

    direct_roots
}

fn roots_for_place(
    layout: &FunctionLayout,
    state: &BorrowState,
    place: &HirPlace,
    location: SourceLocation,
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
    env: &mut SharedReadEnv<'_, '_>,
    place: &HirPlace,
    location: SourceLocation,
) -> Result<(), CompilerError> {
    match place {
        HirPlace::Local(_) => Ok(()),

        HirPlace::Field { base, .. } => record_shared_reads_in_place_indices(env, base, location),

        HirPlace::Index { base, index } => {
            record_shared_reads_in_place_indices(env, base, location.clone())?;
            record_shared_reads_in_expression(env, index, location)
        }
    }
}

fn record_shared_reads_in_expression(
    env: &mut SharedReadEnv<'_, '_>,
    expression: &HirExpression,
    location: SourceLocation,
) -> Result<(), CompilerError> {
    match &expression.kind {
        HirExpressionKind::Int(_)
        | HirExpressionKind::Float(_)
        | HirExpressionKind::Bool(_)
        | HirExpressionKind::Char(_)
        | HirExpressionKind::StringLiteral(_) => {}

        HirExpressionKind::Load(place) => {
            let value_location = env
                .context
                .diagnostics
                .value_error_location(expression.id, location.clone());
            record_shared_reads_in_place_indices(env, place, value_location.clone())?;

            let roots = roots_for_place(env.layout, env.state, place, value_location.clone())?;
            let actor_index_hint = place_root_local_index(env.layout, place);
            let mut check = AccessCheckContext {
                context: env.context,
                layout: env.layout,
                state: env.state,
                tracker: env.tracker,
                location: value_location,
                stats: env.stats,
                actor_index_hint,
                current_order: env.current_order,
            };
            check_shared_access(&mut check, &roots)?;
        }

        HirExpressionKind::Copy(place) => {
            let value_location = env
                .context
                .diagnostics
                .value_error_location(expression.id, location.clone());
            record_shared_reads_in_place_indices(env, place, value_location.clone())?;

            let roots = roots_for_place(env.layout, env.state, place, value_location.clone())?;
            let actor_index_hint = place_root_local_index(env.layout, place);
            let mut check = AccessCheckContext {
                context: env.context,
                layout: env.layout,
                state: env.state,
                tracker: env.tracker,
                location: value_location.clone(),
                stats: env.stats,
                actor_index_hint,
                current_order: env.current_order,
            };
            check_shared_access(&mut check, &roots)?;

            env.value_fact_buffer.record(
                expression.id,
                ValueAccessClassification::SharedRead,
                &roots,
            );
            return Ok(());
        }

        HirExpressionKind::BinOp { left, right, .. } => {
            record_shared_reads_in_expression(env, left, location.clone())?;
            record_shared_reads_in_expression(env, right, location.clone())?;
        }

        HirExpressionKind::UnaryOp { operand, .. } => {
            record_shared_reads_in_expression(env, operand, location.clone())?;
        }

        HirExpressionKind::StructConstruct { fields, .. } => {
            for (_, value) in fields {
                record_shared_reads_in_expression(env, value, location.clone())?;
            }
        }

        HirExpressionKind::Collection(elements)
        | HirExpressionKind::TupleConstruct { elements } => {
            for element in elements {
                record_shared_reads_in_expression(env, element, location.clone())?;
            }
        }
        HirExpressionKind::TupleGet { tuple, .. } => {
            record_shared_reads_in_expression(env, tuple, location.clone())?;
        }

        HirExpressionKind::Range { start, end } => {
            record_shared_reads_in_expression(env, start, location.clone())?;
            record_shared_reads_in_expression(env, end, location.clone())?;
        }

        HirExpressionKind::OptionConstruct { variant, value } => {
            if matches!(variant, OptionVariant::Some)
                && let Some(inner) = value
            {
                record_shared_reads_in_expression(env, inner, location.clone())?;
            }
        }

        HirExpressionKind::ResultConstruct { value, .. } => {
            record_shared_reads_in_expression(env, value, location.clone())?;
        }

        HirExpressionKind::ResultPropagate { result } => {
            record_shared_reads_in_expression(env, result, location.clone())?;
        }

        HirExpressionKind::ResultIsOk { result }
        | HirExpressionKind::ResultUnwrapOk { result }
        | HirExpressionKind::ResultUnwrapErr { result } => {
            record_shared_reads_in_expression(env, result, location.clone())?;
        }
    }

    let mut expression_roots = RootSet::empty(env.layout.local_count());
    collect_expression_roots(
        env.layout,
        env.state,
        expression,
        &mut expression_roots,
        env.context
            .diagnostics
            .value_error_location(expression.id, location.clone()),
    )?;
    let classification = if expression_roots.is_empty() {
        ValueAccessClassification::None
    } else {
        ValueAccessClassification::SharedRead
    };
    env.value_fact_buffer
        .record(expression.id, classification, &expression_roots);

    Ok(())
}

fn collect_expression_roots(
    layout: &FunctionLayout,
    state: &BorrowState,
    expression: &HirExpression,
    out: &mut RootSet,
    location: SourceLocation,
) -> Result<(), CompilerError> {
    match &expression.kind {
        HirExpressionKind::Load(place) => {
            let roots = roots_for_place(layout, state, place, location.clone())?;
            out.union_with(&roots);

            if let HirPlace::Index { index, .. } = place {
                collect_expression_roots(layout, state, index, out, location)?;
            }
        }

        HirExpressionKind::Copy(place) => {
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
        HirExpressionKind::TupleGet { tuple, .. } => {
            collect_expression_roots(layout, state, tuple, out, location.clone())?;
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

        HirExpressionKind::ResultPropagate { result } => {
            collect_expression_roots(layout, state, result, out, location)?;
        }

        HirExpressionKind::ResultIsOk { result }
        | HirExpressionKind::ResultUnwrapOk { result }
        | HirExpressionKind::ResultUnwrapErr { result } => {
            collect_expression_roots(layout, state, result, out, location)?;
        }

        HirExpressionKind::Int(_)
        | HirExpressionKind::Float(_)
        | HirExpressionKind::Bool(_)
        | HirExpressionKind::Char(_)
        | HirExpressionKind::StringLiteral(_) => {}
    }

    Ok(())
}
