//! Conflict detection helpers for borrow-transfer access checks.
//!
//! WHAT: classifies when a requested access overlaps an active borrow state.
//! WHY: transfer logic needs one place to keep borrow-conflict rules deterministic and testable.

use crate::compiler_frontend::analysis::borrow_checker::BorrowCheckError;
use crate::compiler_frontend::analysis::borrow_checker::state::{
    BorrowState, FunctionLayout, RootSet,
};
use crate::compiler_frontend::analysis::borrow_checker::types::{AccessKind, LocalMode};
use crate::compiler_frontend::compiler_messages::{
    BorrowAccessKind, DiagnosticPlace, InvalidMutableAccessReason,
};
use crate::compiler_frontend::hir::hir_side_table::HirLocalOriginKind;

use super::{AccessCheckContext, BorrowTransferContext, MutableAccessPolicy};

// WHAT: Validates shared-root reads against statement-local and active mutable conflicts.
// WHY: Shared reads must be blocked when the same root is mutably active.
pub(super) fn check_shared_access(
    check: &mut AccessCheckContext<'_, '_>,
    roots: &RootSet,
) -> Result<(), BorrowCheckError> {
    for root_index in roots.iter_ones() {
        check.stats.conflicts_checked += 1;

        if let Some(existing) = check.tracker.conflict(root_index, AccessKind::Shared) {
            let place = check
                .context
                .diagnostics
                .local_place(check.layout.local_ids[root_index]);
            return Err(check.context.diagnostics.shared_mutable_conflict(
                place,
                borrow_access_kind(existing),
                BorrowAccessKind::Shared,
                None,
                check.location.clone(),
            ));
        }

        if let Some(conflicting_index) = active_mutable_alias_for_root(
            check.context,
            check.layout,
            check.state,
            root_index,
            check.actor_index_hint,
            check.current_order,
        ) {
            let place = check
                .context
                .diagnostics
                .local_place(check.layout.local_ids[root_index]);
            let conflicting_place = check
                .context
                .diagnostics
                .local_place(check.layout.local_ids[conflicting_index]);
            return Err(check.context.diagnostics.shared_mutable_conflict(
                place,
                BorrowAccessKind::Mutable,
                BorrowAccessKind::Shared,
                Some(conflicting_place),
                check.location.clone(),
            ));
        }

        check.tracker.record(root_index, AccessKind::Shared);
    }

    Ok(())
}

// WHAT: Validates mutable accesses against overlap, mutability, and alias exclusivity.
// WHY: Mutable access is only valid when the acting root has exclusive active ownership view.
pub(super) fn check_mutable_access(
    check: &mut AccessCheckContext<'_, '_>,
    roots: &RootSet,
    policy: MutableAccessPolicy,
) -> Result<(), BorrowCheckError> {
    for root_index in roots.iter_ones() {
        check.stats.conflicts_checked += 1;

        if let Some(existing) = check.tracker.conflict(root_index, AccessKind::Mutable)
            && !(policy.allow_prior_shared && existing == AccessKind::Shared)
        {
            let place = check
                .context
                .diagnostics
                .local_place(check.layout.local_ids[root_index]);
            if existing == AccessKind::Mutable {
                return Err(check
                    .context
                    .diagnostics
                    .multiple_mutable_borrows(place, check.location.clone()));
            }

            return Err(check.context.diagnostics.shared_mutable_conflict(
                place,
                borrow_access_kind(existing),
                BorrowAccessKind::Mutable,
                None,
                check.location.clone(),
            ));
        }

        if policy.require_root_mutable && !check.layout.local_mutable[root_index] {
            let place = check
                .context
                .diagnostics
                .local_place(check.layout.local_ids[root_index]);
            return Err(check.context.diagnostics.invalid_mutable_access(
                place,
                InvalidMutableAccessReason::ImmutablePlace,
                None,
                check.location.clone(),
            ));
        }

        let actor_index = check.actor_index_hint.unwrap_or(root_index);
        let alias_count = active_alias_count_for_root(
            check.layout,
            check.state,
            root_index,
            actor_index,
            check.current_order,
            policy.strict_move_exclusivity,
        );
        if alias_count > 1 {
            let conflicting_local = conflicting_active_local_for_root(
                check.layout,
                check.state,
                root_index,
                actor_index,
                check.current_order,
                policy.strict_move_exclusivity,
            )
            .map(|index| {
                check
                    .context
                    .diagnostics
                    .local_place(check.layout.local_ids[index])
            });
            let place = check
                .context
                .diagnostics
                .local_place(check.layout.local_ids[actor_index]);

            return Err(check.context.diagnostics.invalid_mutable_access(
                place,
                InvalidMutableAccessReason::AliasedValueRequiresExclusiveAccess,
                conflicting_local.or(Some(DiagnosticPlace::Unknown)),
                check.location.clone(),
            ));
        }

        check.tracker.record(root_index, AccessKind::Mutable);
    }

    Ok(())
}

fn borrow_access_kind(access: AccessKind) -> BorrowAccessKind {
    match access {
        AccessKind::Shared => BorrowAccessKind::Shared,
        AccessKind::Mutable => BorrowAccessKind::Mutable,
    }
}

fn active_alias_count_for_root(
    layout: &FunctionLayout,
    state: &BorrowState,
    root_index: usize,
    actor_index: usize,
    current_order: i32,
    strict_move_exclusivity: bool,
) -> u32 {
    let mut count = 0u32;
    let actor_state = state.local_state(actor_index);
    let actor_is_alias_for_root = actor_index != root_index
        && actor_state.mode.contains(LocalMode::ALIAS)
        && actor_state.alias_roots.contains(root_index);

    for candidate_index in 0..layout.local_count() {
        if actor_is_alias_for_root && candidate_index == root_index {
            continue;
        }

        let roots = state.effective_roots(candidate_index);
        if !roots.contains(root_index) {
            continue;
        }

        if candidate_index == actor_index {
            count += 1;
            continue;
        }

        if !is_local_active_for_alias_conflict(
            layout,
            state,
            root_index,
            candidate_index,
            current_order,
            strict_move_exclusivity,
        ) {
            continue;
        }

        count += 1;
    }

    count
}

fn active_mutable_alias_for_root(
    context: &BorrowTransferContext<'_>,
    layout: &FunctionLayout,
    state: &BorrowState,
    root_index: usize,
    actor_index_hint: Option<usize>,
    current_order: i32,
) -> Option<usize> {
    for candidate_index in 0..layout.local_count() {
        if Some(candidate_index) == actor_index_hint {
            continue;
        }

        if !layout.local_mutable[candidate_index] {
            continue;
        }

        if matches!(
            context
                .diagnostics
                .local_origin_kind(layout.local_ids[candidate_index]),
            Some(kind) if kind != HirLocalOriginKind::User
        ) {
            continue;
        }

        let candidate_state = state.local_state(candidate_index);
        if !candidate_state.mode.contains(LocalMode::ALIAS) {
            continue;
        }

        if layout.local_is_expired(candidate_index, current_order)
            && !local_alias_never_read(layout, state, candidate_index)
        {
            continue;
        }

        let roots = state.effective_roots(candidate_index);
        if roots.contains(root_index) {
            return Some(candidate_index);
        }
    }

    None
}

fn conflicting_active_local_for_root(
    layout: &FunctionLayout,
    state: &BorrowState,
    root_index: usize,
    actor_index: usize,
    current_order: i32,
    strict_move_exclusivity: bool,
) -> Option<usize> {
    let actor_state = state.local_state(actor_index);
    let actor_is_alias_for_root = actor_index != root_index
        && actor_state.mode.contains(LocalMode::ALIAS)
        && actor_state.alias_roots.contains(root_index);

    for candidate_index in 0..layout.local_count() {
        if actor_is_alias_for_root && candidate_index == root_index {
            continue;
        }

        if candidate_index == actor_index {
            continue;
        }

        if !is_local_active_for_alias_conflict(
            layout,
            state,
            root_index,
            candidate_index,
            current_order,
            strict_move_exclusivity,
        ) {
            continue;
        }

        let roots = state.effective_roots(candidate_index);
        if roots.contains(root_index) {
            return Some(candidate_index);
        }
    }

    None
}

// WHAT: Determines whether a candidate alias can still participate in exclusivity conflicts.
// WHY: Expired alias views should stop blocking mutable access unless strict move rules apply.
fn is_local_active_for_alias_conflict(
    layout: &FunctionLayout,
    state: &BorrowState,
    root_index: usize,
    local_index: usize,
    current_order: i32,
    strict_move_exclusivity: bool,
) -> bool {
    let last_use = layout.local_last_use_order[local_index];
    if last_use >= 0 {
        if last_use >= current_order {
            return true;
        }

        let local_state = state.local_state(local_index);
        if !local_state.mode.contains(LocalMode::ALIAS) {
            return false;
        }

        if strict_move_exclusivity {
            return local_state.direct_alias_roots.contains(root_index)
                || layout.local_mutable[local_index];
        }

        return false;
    }

    let local_state = state.local_state(local_index);
    if !local_state.mode.contains(LocalMode::ALIAS) {
        return false;
    }

    if strict_move_exclusivity {
        return local_state.direct_alias_roots.contains(root_index)
            || layout.local_mutable[local_index];
    }

    // Unused mutable aliases remain active until the end of the scope.
    layout.local_mutable[local_index]
}

fn local_alias_never_read(
    layout: &FunctionLayout,
    state: &BorrowState,
    local_index: usize,
) -> bool {
    let local_state = state.local_state(local_index);
    if !local_state.mode.contains(LocalMode::ALIAS) {
        return false;
    }

    let first_write = layout.local_first_write_order[local_index];
    let last_use = layout.local_last_use_order[local_index];
    first_write >= 0 && first_write == last_use
}
