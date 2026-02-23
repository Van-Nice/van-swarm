//! JSON-RPC 2.0 wire types used throughout the MCP protocol.

use serde::{Deserialize, Serialize};

// ─────────────────────────────────────────────────────────────────────────────
// RequestId
// ─────────────────────────────────────────────────────────────────────────────

/// JSON-RPC 2.0 request / response identifier.
///
/// MCP commonly uses integer IDs, but the spec permits strings too.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RequestId {
    Number(i64),
    String(String),
}

impl From<i64> for RequestId {
    fn from(n: i64) -> Self {
        RequestId::Number(n)
    }
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequestId::Number(n) => write!(f, "{n}"),
            RequestId::String(s) => write!(f, "{s}"),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Request / Notification
// ─────────────────────────────────────────────────────────────────────────────

/// A JSON-RPC 2.0 method call (expects a response).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub jsonrpc: String,
    pub id: RequestId,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl Request {
    pub fn new(
        id: impl Into<RequestId>,
        method: impl Into<String>,
        params: Option<serde_json::Value>,
    ) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id: id.into(),
            method: method.into(),
            params,
        }
    }
}

/// A JSON-RPC 2.0 notification (no response expected).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Notification {
    pub jsonrpc: String,
    pub method: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<serde_json::Value>,
}

impl Notification {
    pub fn new(method: impl Into<String>, params: Option<serde_json::Value>) -> Self {
        Self { jsonrpc: "2.0".into(), method: method.into(), params }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Response
// ─────────────────────────────────────────────────────────────────────────────

/// A JSON-RPC 2.0 response (success or error).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub jsonrpc: String,
    pub id: RequestId,
    #[serde(flatten)]
    pub body: ResponseBody,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ResponseBody {
    Ok { result: serde_json::Value },
    Err { error: RpcError },
}

impl Response {
    pub fn ok(id: impl Into<RequestId>, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id: id.into(),
            body: ResponseBody::Ok { result },
        }
    }

    pub fn err(id: impl Into<RequestId>, error: RpcError) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id: id.into(),
            body: ResponseBody::Err { error },
        }
    }

    /// Unwrap to the result value or map the RPC error to a `FrameworkError`.
    pub fn into_result(
        self,
    ) -> Result<serde_json::Value, rustmastra_core::FrameworkError> {
        match self.body {
            ResponseBody::Ok { result } => Ok(result),
            ResponseBody::Err { error } => Err(rustmastra_core::FrameworkError::Config(
                format!("RPC {} error {}: {}", self.id, error.code, error.message),
            )),
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// RpcError
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

impl RpcError {
    pub fn new(code: i32, message: impl Into<String>) -> Self {
        Self { code, message: message.into(), data: None }
    }
}

impl std::fmt::Display for RpcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "JSON-RPC error {}: {}", self.code, self.message)
    }
}

/// Standard JSON-RPC 2.0 error codes.
pub mod codes {
    pub const PARSE_ERROR: i32 = -32700;
    pub const INVALID_REQUEST: i32 = -32600;
    pub const METHOD_NOT_FOUND: i32 = -32601;
    pub const INVALID_PARAMS: i32 = -32602;
    pub const INTERNAL_ERROR: i32 = -32603;
}

// ─────────────────────────────────────────────────────────────────────────────
// Incoming message (server-side parser)
// ─────────────────────────────────────────────────────────────────────────────

/// Either a JSON-RPC request or notification.
///
/// The server calls `parse_incoming()` on each received line to determine
/// whether it needs to send a response.
#[derive(Debug)]
pub enum IncomingMessage {
    Request(Request),
    Notification(Notification),
}

/// Parse a raw JSON line into either a request (has `id`) or notification.
pub fn parse_incoming(
    line: &str,
) -> Result<IncomingMessage, rustmastra_core::FrameworkError> {
    let v: serde_json::Value = serde_json::from_str(line).map_err(|e| {
        rustmastra_core::FrameworkError::Config(format!("JSON-RPC parse error: {e}"))
    })?;

    if v.get("id").is_some() {
        let req: Request = serde_json::from_value(v).map_err(|e| {
            rustmastra_core::FrameworkError::Config(format!("Invalid request: {e}"))
        })?;
        Ok(IncomingMessage::Request(req))
    } else {
        let n: Notification = serde_json::from_value(v).map_err(|e| {
            rustmastra_core::FrameworkError::Config(format!("Invalid notification: {e}"))
        })?;
        Ok(IncomingMessage::Notification(n))
    }
}
