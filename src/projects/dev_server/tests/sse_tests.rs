//! Tests for SSE payload formatting and disconnected-client pruning.

use super::{broadcast_reload, format_reload_event};
use crate::projects::dev_server::state::{DevServerState, SseClient};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc;

#[test]
fn reload_event_uses_expected_sse_format() {
    let formatted = format_reload_event(42);
    assert_eq!(formatted, "event: reload\ndata: 42\n\n");
}

#[test]
fn broadcast_prunes_disconnected_clients() {
    let state = Arc::new(DevServerState::new(PathBuf::from("dev")));

    let (sender_ok, receiver_ok) = mpsc::channel::<String>();
    let client_id_ok = state.next_client_id.fetch_add(1, Ordering::Relaxed);
    state
        .clients
        .lock()
        .expect("clients mutex should not be poisoned")
        .push(SseClient {
            id: client_id_ok,
            sender: sender_ok,
        });

    let (sender_dead, receiver_dead) = mpsc::channel::<String>();
    drop(receiver_dead);
    let client_id_dead = state.next_client_id.fetch_add(1, Ordering::Relaxed);
    state
        .clients
        .lock()
        .expect("clients mutex should not be poisoned")
        .push(SseClient {
            id: client_id_dead,
            sender: sender_dead,
        });

    let notified = broadcast_reload(&state, 7);
    assert_eq!(notified, 1);

    let remaining = state
        .clients
        .lock()
        .expect("clients mutex should not be poisoned");
    assert_eq!(remaining.len(), 1);
    assert_eq!(remaining[0].id, client_id_ok);
    assert_eq!(
        receiver_ok
            .recv()
            .expect("connected client should receive event"),
        "event: reload\ndata: 7\n\n"
    );
}
