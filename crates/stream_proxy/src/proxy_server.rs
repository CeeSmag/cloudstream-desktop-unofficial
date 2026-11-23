//! The listening socket: accepts the player's connections and serves each on its own thread.

use std::io::ErrorKind;
use std::net::{Ipv4Addr, Shutdown, SocketAddr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use crate::connection_gate::{ConnectionGate, Permit};
use crate::http_request::{read_request, RequestError};
use crate::http_response::write_message;
use crate::proxy_config::ProxyConfig;
use crate::proxy_error::ProxyError;
use crate::request_handler::Handler;

/// How long one read on a connection may block before the overall deadline is looked at again.
const READ_SLICE: Duration = Duration::from_secs(1);

/// How long a refusal because the proxy is busy may take to write.
const BUSY_WRITE_TIMEOUT: Duration = Duration::from_secs(1);

/// How long a refusal waits for the request it refuses (see `refuse_busy`).
pub(crate) const BUSY_READ_TIMEOUT: Duration = Duration::from_millis(200);

/// How long waking the accept thread at shutdown may take.
const WAKE_TIMEOUT: Duration = Duration::from_secs(1);

/// What every connection thread shares.
struct Shared {
    handler: Handler,
    gate: Arc<ConnectionGate>,
    stopping: AtomicBool,
    config: ProxyConfig,
}

/// The running server.
pub(crate) struct ProxyServer {
    port: u16,
    shared: Arc<Shared>,
    accept: Mutex<Option<JoinHandle<()>>>,
}

impl ProxyServer {
    /// Opens a port on this PC's loopback address and starts accepting.
    pub(crate) fn start(
        build_handler: impl FnOnce(u16) -> Handler,
        config: ProxyConfig,
    ) -> Result<Self, ProxyError> {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).map_err(ProxyError::Start)?;
        let port = listener.local_addr().map_err(ProxyError::Start)?.port();
        let shared = Arc::new(Shared {
            handler: build_handler(port),
            gate: ConnectionGate::new(config.max_connections),
            stopping: AtomicBool::new(false),
            config,
        });
        let for_thread = Arc::clone(&shared);
        let accept = std::thread::Builder::new()
            .name("stream-proxy-accept".to_owned())
            .spawn(move || accept_loop(&listener, &for_thread))
            .map_err(ProxyError::Start)?;
        tracing::info!(port, "the stream proxy is listening");
        Ok(Self {
            port,
            shared,
            accept: Mutex::new(Some(accept)),
        })
    }

    pub(crate) fn port(&self) -> u16 {
        self.port
    }

    /// How many connections are being served.
    #[cfg(test)]
    pub(crate) fn open_connections(&self) -> usize {
        self.shared.gate.open_count()
    }

    pub(crate) fn is_stopped(&self) -> bool {
        self.shared.stopping.load(Ordering::SeqCst)
    }

    /// Stops accepting, then waits (for a limited time) for the connections being served. Safe to
    /// call more than once. The caller revokes the streams first so that what is being sent stops.
    pub(crate) fn shutdown(&self) {
        if self.shared.stopping.swap(true, Ordering::SeqCst) {
            return;
        }
        // The accept thread is blocked in `accept`; a connection to ourselves wakes it.
        let address = SocketAddr::from((Ipv4Addr::LOCALHOST, self.port));
        let _ = TcpStream::connect_timeout(&address, WAKE_TIMEOUT);
        let accept = self
            .accept
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(handle) = accept {
            if handle.join().is_err() {
                tracing::warn!("the stream proxy's accept thread had panicked");
            }
        }
        if !self.shared.gate.wait_idle(self.shared.config.shutdown_wait) {
            let open = self.shared.gate.open_count();
            tracing::warn!(
                open,
                "the stream proxy stopped while connections were still open"
            );
        }
        tracing::info!(port = self.port, "the stream proxy stopped");
    }
}

fn accept_loop(listener: &TcpListener, shared: &Arc<Shared>) {
    for incoming in listener.incoming() {
        if shared.stopping.load(Ordering::SeqCst) {
            return;
        }
        let stream = match incoming {
            Ok(stream) => stream,
            Err(error) => {
                tracing::warn!(%error, "a connection to the stream proxy failed to open");
                std::thread::sleep(Duration::from_millis(100));
                continue;
            }
        };
        match shared.gate.try_enter() {
            Some(permit) => start_connection(stream, shared, permit),
            None => refuse_busy(stream),
        }
    }
}

fn start_connection(stream: TcpStream, shared: &Arc<Shared>, permit: Permit) {
    let shared = Arc::clone(shared);
    let spawned = std::thread::Builder::new()
        .name("stream-proxy-connection".to_owned())
        .spawn(move || serve_connection(stream, &shared, permit));
    if let Err(error) = spawned {
        tracing::warn!(%error, "no thread for a connection to the stream proxy");
    }
}

/// Tells a player the proxy is busy. Its request is read first: closing a connection that still
/// has unread data makes TCP reset it, and the reset can destroy the answer before the player
/// reads it. This runs on the accepting thread, so the wait is short; a player on this PC has
/// sent its request by the time the connection is accepted.
fn refuse_busy(mut stream: TcpStream) {
    let _ = stream.set_read_timeout(Some(BUSY_READ_TIMEOUT));
    let _ = stream.set_write_timeout(Some(BUSY_WRITE_TIMEOUT));
    let _ = read_request(&mut stream, Instant::now() + BUSY_READ_TIMEOUT);
    let _ = write_message(&mut stream, 503, "The stream proxy is busy; try again.");
    let _ = stream.shutdown(Shutdown::Write);
    tracing::debug!("a connection was refused: the stream proxy is busy");
}

/// Serves the one request of a connection. The place at the gate is given back *before* the
/// socket is closed: a player that has read its answer to the end and at once asks again must find
/// the place free, not be told the proxy is busy.
fn serve_connection(mut stream: TcpStream, shared: &Shared, permit: Permit) {
    let _ = stream.set_nodelay(true);
    let _ = stream.set_read_timeout(Some(READ_SLICE));
    let _ = stream.set_write_timeout(Some(shared.config.write_timeout));
    let deadline = Instant::now() + shared.config.request_timeout;
    let result = match read_request(&mut stream, deadline) {
        Ok(request) => shared.handler.serve(&request, &mut stream),
        Err(error) => reply_to_bad_request(&mut stream, &error),
    };
    if let Err(error) = result {
        log_write_failure(&error);
    }
    permit.release_then(|| {
        let _ = stream.shutdown(Shutdown::Both);
    });
}

fn reply_to_bad_request(stream: &mut TcpStream, error: &RequestError) -> std::io::Result<()> {
    tracing::debug!(%error, "a request to the stream proxy was refused");
    match error {
        RequestError::TooLarge => write_message(stream, 431, "The request is too large."),
        RequestError::Malformed => write_message(stream, 400, "The request is not understood."),
        RequestError::Incomplete => {
            write_message(stream, 408, "The request did not arrive in time.")
        }
        RequestError::Io(_) => Ok(()),
    }
}

/// The player closing a connection (it switched stream, it seeked) is normal; other failures are not.
fn log_write_failure(error: &std::io::Error) {
    if matches!(
        error.kind(),
        ErrorKind::BrokenPipe
            | ErrorKind::ConnectionReset
            | ErrorKind::ConnectionAborted
            | ErrorKind::TimedOut
            | ErrorKind::WouldBlock
    ) {
        tracing::debug!(%error, "the player closed a connection to the stream proxy");
    } else {
        tracing::warn!(%error, "an answer of the stream proxy could not be written");
    }
}
