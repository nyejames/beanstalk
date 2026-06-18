//! Unit tests for scope-frame arena capacity behavior.

use super::ScopeArena;

#[test]
fn with_capacity_seeds_frame_storage() {
    let arena = ScopeArena::with_capacity(16);

    assert!(
        arena.frames.capacity() >= 16,
        "ScopeArena::with_capacity must reserve initial frame storage"
    );
}

#[test]
fn with_capacity_preserves_root_frame_ids() {
    let mut arena = ScopeArena::with_capacity(4);
    let root_id = arena.alloc_root_frame_with_capacity(0);
    let child_id = arena.alloc_child_frame(root_id);

    assert_eq!(root_id.as_usize(), 0);
    assert_eq!(child_id.as_usize(), 1);
}
