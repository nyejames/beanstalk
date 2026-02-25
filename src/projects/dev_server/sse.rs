//! Server-Sent Events helpers for hot reload.
//!
//! Each connected client receives reload events over a channel-backed writer loop, and failed
//! clients are pruned from shared state during broadcast.

use crate::projects::dev_server::state::{DevServerState, SseClient};
use std::io::{self, Write};
use std::net::TcpStream;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::Duration;

const KEEP_ALIVE_INTERVAL: Duration = Duration::from_secs(15);

pub fn format_reload_event(version: u64) -> String {
    format!("event: reload\ndata: {version}\n\n")
}

pub fn broadcast_reload(state: &Arc<DevServerState>, version: u64) -> usize {
    let event = format_reload_event(version);
    let mut clients = match state.clients.lock() {
        Ok(guard) => guard,
        Err(_) => return 0,
    };

    let mut notified_count = 0usize;
    // Broadcast and prune disconnected clients in one pass.
    clients.retain(|client| {
        let sent = client.sender.send(event.clone()).is_ok();
        if sent {
            notified_count += 1;
        }
        sent
    });

    notified_count
}

fn register_client(state: &Arc<DevServerState>, sender: mpsc::Sender<String>) -> Option<u64> {
    let client_id = state.next_client_id.fetch_add(1, Ordering::Relaxed);
    let mut clients = state.clients.lock().ok()?;
    clients.push(SseClient {
        id: client_id,
        sender,
    });
    Some(client_id)
}

pub fn remove_client(state: &Arc<DevServerState>, client_id: u64) {
    if let Ok(mut clients) = state.clients.lock() {
        clients.retain(|client| client.id != client_id);
    }
}

pub fn handle_sse_connection(mut stream: TcpStream, state: Arc<DevServerState>) -> io::Result<()> {
    let headers = concat!(
        "HTTP/1.1 200 OK\r\n",
        "Content-Type: text/event-stream\r\n",
        "Cache-Control: no-cache\r\n",
        "Connection: keep-alive\r\n",
        "Access-Control-Allow-Origin: *\r\n\r\n"
    );
    stream.write_all(headers.as_bytes())?;
    stream.write_all(b": connected\n\n")?;
    stream.flush()?;

    let (sender, receiver) = mpsc::channel::<String>();
    let Some(client_id) = register_client(&state, sender) else {
        return Err(io::Error::other(
            "Failed to register SSE client due to state lock poisoning",
        ));
    };

    loop {
        match receiver.recv_timeout(KEEP_ALIVE_INTERVAL) {
            Ok(event_payload) => {
                if stream.write_all(event_payload.as_bytes()).is_err() || stream.flush().is_err() {
                    break;
                }
            }
            Err(RecvTimeoutError::Timeout) => {
                // Keep-alive comments prevent proxies and browsers from considering the stream idle.
                if stream.write_all(b": keep-alive\n\n").is_err() || stream.flush().is_err() {
                    break;
                }
            }
            Err(RecvTimeoutError::Disconnected) => break,
        }
    }

    remove_client(&state, client_id);
    Ok(())
}

#[cfg(test)]
#[path = "tests/sse_tests.rs"]
mod tests;
