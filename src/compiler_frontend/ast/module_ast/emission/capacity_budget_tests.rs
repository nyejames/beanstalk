//! Tests for the AST-local scope-frame capacity budget policy.

use super::ScopeFrameCapacityBudget;

#[test]
fn scope_frame_capacity_budget_spends_module_estimate_once() {
    let mut budget = ScopeFrameCapacityBudget::new(10, 3);

    assert_eq!(budget.next_root_capacity(), 4);
    assert_eq!(budget.next_root_capacity(), 3);
    assert_eq!(budget.next_root_capacity(), 3);
    assert_eq!(budget.next_root_capacity(), 0);
}

#[test]
fn scope_frame_capacity_budget_handles_even_distribution() {
    let mut budget = ScopeFrameCapacityBudget::new(12, 4);

    assert_eq!(budget.next_root_capacity(), 3);
    assert_eq!(budget.next_root_capacity(), 3);
    assert_eq!(budget.next_root_capacity(), 3);
    assert_eq!(budget.next_root_capacity(), 3);
}

#[test]
fn scope_frame_capacity_budget_returns_zero_without_roots_or_frames() {
    let mut no_roots = ScopeFrameCapacityBudget::new(8, 0);
    assert_eq!(no_roots.next_root_capacity(), 0);

    let mut no_frames = ScopeFrameCapacityBudget::new(0, 4);
    assert_eq!(no_frames.next_root_capacity(), 0);
    assert_eq!(no_frames.next_root_capacity(), 0);
}
