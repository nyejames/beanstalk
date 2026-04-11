//! Tests for SSE payload formatting and disconnected-client pruning.

use super::{broadcast_reload, format_reload_event, handle_sse_connection_with_timeouts};
use crate::projects::dev_server::state::{DevServerState, SseClient};
use std::io::Read;
use std::net::{Shutdown, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

fn bind_loopback_listener() -> Option<TcpListener> {
    match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => Some(listener),
        Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => None,
        Err(error) => panic!("should bind test listener: {error}"),
    }
}

#[test]
fn reload_event_uses_expected_sse_format() {
    let formatted = format_reload_event(42);
    assert_eq!(formatted, "event: reload\ndata: 42\n\n");
}

#[test]
fn broadcast_prunes_disconnected_clients() {
    let state = Arc::new(DevServerState::new(PathBuf::from("dev")));

    let (sender_ok, receiver_ok) = mpsc::sync_channel::<String>(1);
    let client_id_ok = state.next_client_id.fetch_add(1, Ordering::Relaxed);
    state
        .clients
        .lock()
        .expect("clients mutex should not be poisoned")
        .push(SseClient {
            id: client_id_ok,
            sender: sender_ok,
        });

    let (sender_dead, receiver_dead) = mpsc::sync_channel::<String>(1);
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

#[test]
fn loopback_disconnect_prunes_sse_client_promptly() {
    let Some(listener) = bind_loopback_listener() else {
        return;
    };
    let address = listener
        .local_addr()
        .expect("listener should report bound address");
    let state = Arc::new(DevServerState::new(PathBuf::from("dev")));
    let (done_sender, done_receiver) = mpsc::channel();

    let server_state = Arc::clone(&state);
    thread::spawn(move || {
        let (stream, _) = listener.accept().expect("should accept client");
        handle_sse_connection_with_timeouts(
            stream,
            server_state,
            Duration::from_millis(50),
            Duration::from_millis(50),
        )
        .expect("sse handler should exit cleanly");
        done_sender
            .send(())
            .expect("server thread should signal completion");
    });

    let mut client = TcpStream::connect(address).expect("client should connect");
    let mut buffer = [0_u8; 256];
    let bytes_read = client
        .read(&mut buffer)
        .expect("client should read initial sse headers");
    assert!(bytes_read > 0);

    for _ in 0..20 {
        if state
            .clients
            .lock()
            .expect("clients mutex should not be poisoned")
            .len()
            == 1
        {
            break;
        }
        thread::sleep(Duration::from_millis(10));
    }

    client
        .shutdown(Shutdown::Both)
        .expect("client should close the SSE connection");
    drop(client);

    let notified = broadcast_reload(&state, 3);
    assert_eq!(notified, 1);
    done_receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("disconnected SSE client should be pruned promptly");
    assert!(
        state
            .clients
            .lock()
            .expect("clients mutex should not be poisoned")
            .is_empty()
    );
}
