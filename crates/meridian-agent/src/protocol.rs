use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Minimal JSON-RPC-ish protocol message used over Codex app-server stdio.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest<'a> {
    pub jsonrpc: &'static str,
    pub id: u64,
    pub method: &'a str,
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification<'a> {
    pub jsonrpc: &'static str,
    pub method: &'a str,
    pub params: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    #[serde(default)]
    pub jsonrpc: Option<String>,
    pub id: Option<Value>,
    #[serde(default)]
    pub method: Option<String>,
    #[serde(default)]
    pub params: Option<Value>,
    #[serde(default)]
    pub result: Option<Value>,
    #[serde(default)]
    pub error: Option<Value>,
}

pub fn request(id: u64, method: &str, params: Value) -> String {
    let req = JsonRpcRequest {
        jsonrpc: "2.0",
        id,
        method,
        params,
    };
    serde_json::to_string(&req).expect("serialize")
}

pub fn notification(method: &str, params: Value) -> String {
    let n = JsonRpcNotification {
        jsonrpc: "2.0",
        method,
        params,
    };
    serde_json::to_string(&n).expect("serialize")
}
