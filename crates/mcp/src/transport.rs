//! Transport layer: Stdio, HTTP, and in-memory Channel transports.
//!
//! All transports expose the same `send()` / `notify()` interface so the
//! `McpClient` is agnostic to the underlying connection.
//!
//! ## Stdio transport
//!
//! Spawns a subprocess and communicates over stdin/stdout.  A background
//! reader task correlates responses to pending requests using a
//! `HashMap<i64, oneshot::Sender<Response>>` protected by a `tokio::sync::Mutex`.
//!
//! ## HTTP transport
//!
//! POSTs JSON-RPC requests to an endpoint and reads the JSON-RPC response
//! from the HTTP body.  Suitable for streamable-HTTP MCP servers.
//!
//! ## Channel transport
//!
//! In-memory pipe for unit tests.  Pair one `ChannelTransport` with a
//! corresponding `McpServer::serve_channel()` call.
//!
//! **Note:** `ChannelTransport` serialises concurrent requests through a single
//! Mutex-protected receiver.  It is designed for sequential test scenarios only.

use std::collections::HashMap;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

use tokio::sync::{oneshot, Mutex};
use tracing::{debug, warn};

use crate::jsonrpc::{Notification, Request, RequestId, Response};

// ─────────────────────────────────────────────────────────────────────────────
// Transport enum
// ─────────────────────────────────────────────────────────────────────────────

/// Unified transport type.  Pick the variant that matches your server.
pub enum Transport {
    Stdio(StdioTransport),
    Http(HttpTransport),
    Channel(ChannelTransport),
}

impl Transport {
    /// Send a request and await the response.
    pub async fn send(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> vanswarm_core::Result<serde_json::Value> {
        match self {
            Transport::Stdio(t) => t.send_request(method, params).await,
            Transport::Http(t) => t.send_request(method, params).await,
            Transport::Channel(t) => t.send_request(method, params).await,
        }
    }

    /// Send a notification (no response expected).
    pub async fn notify(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> vanswarm_core::Result<()> {
        match self {
            Transport::Stdio(t) => t.send_notification(method, params).await,
            Transport::Http(_) => Ok(()), // HTTP MCP uses polling, no notifications
            Transport::Channel(t) => t.send_notification(method, params).await,
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// StdioTransport
// ─────────────────────────────────────────────────────────────────────────────

type PendingMap = Arc<Mutex<HashMap<i64, oneshot::Sender<Response>>>>;

/// Communicates with a subprocess via stdin/stdout (newline-delimited JSON).
pub struct StdioTransport {
    write_tx: tokio::sync::mpsc::UnboundedSender<String>,
    pending: PendingMap,
    next_id: AtomicI64,
}

impl StdioTransport {
    /// Spawn `command args...` and set up the background reader/writer tasks.
    pub async fn spawn(
        command: &str,
        args: &[impl AsRef<str>],
    ) -> vanswarm_core::Result<Self> {
        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
        use tokio::process::Command;

        let args: Vec<&str> = args.iter().map(|s| s.as_ref()).collect();

        let mut child = Command::new(command)
            .args(&args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| {
                vanswarm_core::FrameworkError::Config(format!(
                    "Failed to spawn MCP subprocess '{command}': {e}"
                ))
            })?;

        let mut stdin = child.stdin.take().expect("stdin piped");
        let stdout = child.stdout.take().expect("stdout piped");

        let pending: PendingMap = Arc::new(Mutex::new(HashMap::new()));
        let pending_reader = Arc::clone(&pending);

        let (write_tx, mut write_rx) = tokio::sync::mpsc::unbounded_channel::<String>();

        // Writer task — forwards serialised request lines to subprocess stdin.
        tokio::spawn(async move {
            while let Some(line) = write_rx.recv().await {
                debug!(line, "→ MCP subprocess");
                if let Err(e) = stdin.write_all(line.as_bytes()).await {
                    warn!("MCP stdin write error: {e}");
                    break;
                }
                if let Err(e) = stdin.write_all(b"\n").await {
                    warn!("MCP stdin newline error: {e}");
                    break;
                }
                let _ = stdin.flush().await;
            }
        });

        // Reader task — reads lines from subprocess stdout and dispatches.
        tokio::spawn(async move {
            let reader = BufReader::new(stdout);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                debug!(line, "← MCP subprocess");
                match serde_json::from_str::<Response>(&line) {
                    Ok(response) => {
                        if let RequestId::Number(id) = &response.id {
                            let mut map = pending_reader.lock().await;
                            if let Some(tx) = map.remove(id) {
                                let _ = tx.send(response);
                            }
                        }
                    }
                    Err(e) => warn!("Failed to parse MCP response: {e} — line: {line}"),
                }
            }
        });

        Ok(Self { write_tx, pending, next_id: AtomicI64::new(1) })
    }

    async fn send_request(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> vanswarm_core::Result<serde_json::Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = Request::new(id, method, params);
        let json = serde_json::to_string(&request)
            .map_err(|e| vanswarm_core::FrameworkError::Serialization(e.into()))?;

        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id, tx);

        self.write_tx.send(json).map_err(|e| {
            vanswarm_core::FrameworkError::Config(format!("MCP write channel closed: {e}"))
        })?;

        let response = rx.await.map_err(|_| {
            vanswarm_core::FrameworkError::Config(
                "MCP response channel closed before reply arrived".into(),
            )
        })?;

        response.into_result()
    }

    async fn send_notification(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> vanswarm_core::Result<()> {
        let n = Notification::new(method, params);
        let json = serde_json::to_string(&n)
            .map_err(|e| vanswarm_core::FrameworkError::Serialization(e.into()))?;
        self.write_tx.send(json).map_err(|e| {
            vanswarm_core::FrameworkError::Config(format!("MCP write channel closed: {e}"))
        })
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// HttpTransport
// ─────────────────────────────────────────────────────────────────────────────

/// Communicates via HTTP POST (Streamable HTTP MCP transport).
pub struct HttpTransport {
    client: reqwest::Client,
    endpoint: String,
    next_id: AtomicI64,
}

impl HttpTransport {
    pub fn new(endpoint: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            endpoint: endpoint.into(),
            next_id: AtomicI64::new(1),
        }
    }

    async fn send_request(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> vanswarm_core::Result<serde_json::Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = Request::new(id, method, params);

        let http_response = self
            .client
            .post(&self.endpoint)
            .json(&request)
            .send()
            .await
            .map_err(vanswarm_core::FrameworkError::Http)?;

        let rpc_response: Response = http_response
            .json()
            .await
            .map_err(vanswarm_core::FrameworkError::Http)?;

        rpc_response.into_result()
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// ChannelTransport — in-memory transport for unit tests
// ─────────────────────────────────────────────────────────────────────────────

/// In-memory transport backed by `tokio::sync::mpsc` channels.
///
/// Pair with `McpServer::serve_channel()` for unit tests that don't need a
/// real subprocess or network connection.
pub struct ChannelTransport {
    /// Send serialised JSON-RPC lines to the server.
    request_tx: tokio::sync::mpsc::Sender<String>,
    /// Receive serialised JSON-RPC response lines from the server.
    response_rx: Arc<Mutex<tokio::sync::mpsc::Receiver<String>>>,
    next_id: AtomicI64,
}

impl ChannelTransport {
    /// Create a paired (client-side, server-side) channel transport.
    ///
    /// Returns `(client_transport, server_rx, server_tx)`.
    pub fn pair() -> (
        Self,
        tokio::sync::mpsc::Receiver<String>, // server reads from here
        tokio::sync::mpsc::Sender<String>,   // server writes to here
    ) {
        let (req_tx, req_rx) = tokio::sync::mpsc::channel(128);
        let (resp_tx, resp_rx) = tokio::sync::mpsc::channel(128);

        let transport = Self {
            request_tx: req_tx,
            response_rx: Arc::new(Mutex::new(resp_rx)),
            next_id: AtomicI64::new(1),
        };

        (transport, req_rx, resp_tx)
    }

    async fn send_request(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> vanswarm_core::Result<serde_json::Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let request = Request::new(id, method, params);
        let json = serde_json::to_string(&request)
            .map_err(|e| vanswarm_core::FrameworkError::Serialization(e.into()))?;

        self.request_tx.send(json).await.map_err(|e| {
            vanswarm_core::FrameworkError::Config(format!(
                "Channel transport closed: {e}"
            ))
        })?;

        // Drain the response channel until we find a response matching our ID.
        // Note: ChannelTransport serialises concurrent requests through the
        // response_rx lock; it is designed for sequential test scenarios only.
        let mut rx_guard = self.response_rx.lock().await;
        while let Some(line) = rx_guard.recv().await {
            if let Ok(resp) = serde_json::from_str::<Response>(&line) {
                if let RequestId::Number(n) = &resp.id {
                    if *n == id {
                        return resp.into_result();
                    }
                }
            }
        }

        Err(vanswarm_core::FrameworkError::Config(
            "Channel transport closed before response".into(),
        ))
    }

    async fn send_notification(
        &self,
        method: &str,
        params: Option<serde_json::Value>,
    ) -> vanswarm_core::Result<()> {
        let n = Notification::new(method, params);
        let json = serde_json::to_string(&n)
            .map_err(|e| vanswarm_core::FrameworkError::Serialization(e.into()))?;
        self.request_tx.send(json).await.map_err(|e| {
            vanswarm_core::FrameworkError::Config(format!("Channel transport closed: {e}"))
        })
    }
}
