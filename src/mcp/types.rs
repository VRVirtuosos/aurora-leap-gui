//! MCP (Model Context Protocol) Data Types & JSON-RPC 2.0 Structures.

use std::collections::HashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use serde_llsd::LLSDValue;

/// Standard JSON-RPC 2.0 Request envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    #[serde(default)]
    pub id: Option<Value>,
    pub method: String,
    #[serde(default)]
    pub params: Option<Value>,
}

/// Standard JSON-RPC 2.0 Response envelope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

impl JsonRpcResponse {
    pub fn ok(id: Option<Value>, result: Value) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn error(id: Option<Value>, code: i64, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".to_string(),
            id,
            result: None,
            error: Some(JsonRpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

/// Standard JSON-RPC 2.0 Error details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcError {
    pub code: i64,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

/// MCP Tool schema declaration for `tools/list`.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
}

/// Individual content block in an MCP tool result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum McpContent {
    #[serde(rename = "text")]
    Text { text: String },
    #[serde(rename = "image")]
    Image {
        data: String,
        #[serde(rename = "mimeType")]
        mime_type: String,
    },
}

impl McpContent {
    pub fn text(t: impl Into<String>) -> Self {
        McpContent::Text { text: t.into() }
    }

    pub fn image_base64(png_base64: impl Into<String>, mime_type: Option<&str>) -> Self {
        McpContent::Image {
            data: png_base64.into(),
            mime_type: mime_type.unwrap_or("image/png").to_string(),
        }
    }
}

/// Standard MCP `tools/call` response payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct McpCallResult {
    pub content: Vec<McpContent>,
    #[serde(default)]
    pub is_error: bool,
}

impl McpCallResult {
    pub fn success(text: impl Into<String>) -> Self {
        Self {
            content: vec![McpContent::text(text)],
            is_error: false,
        }
    }

    pub fn success_json(val: &Value) -> Self {
        let text = serde_json::to_string_pretty(val).unwrap_or_else(|_| val.to_string());
        Self::success(text)
    }

    pub fn success_with_image(text: impl Into<String>, png_base64: String) -> Self {
        Self {
            content: vec![
                McpContent::text(text),
                McpContent::image_base64(png_base64, Some("image/png")),
            ],
            is_error: false,
        }
    }

    pub fn error(msg: impl Into<String>) -> Self {
        Self {
            content: vec![McpContent::text(format!("Error: {}", msg.into()))],
            is_error: true,
        }
    }
}

/// Convert a `serde_json::Value` into an `LLSDValue`.
pub fn json_to_llsd(val: &Value) -> LLSDValue {
    match val {
        Value::Null => LLSDValue::Undefined,
        Value::Bool(b) => LLSDValue::Boolean(*b),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                if i >= i32::MIN as i64 && i <= i32::MAX as i64 {
                    LLSDValue::Integer(i as i32)
                } else {
                    LLSDValue::Real(i as f64)
                }
            } else if let Some(f) = n.as_f64() {
                LLSDValue::Real(f)
            } else {
                LLSDValue::Undefined
            }
        }
        Value::String(s) => {
            // Check if string matches standard UUID format
            if let Ok(u) = uuid::Uuid::parse_str(s) {
                LLSDValue::UUID(u)
            } else {
                LLSDValue::String(s.clone())
            }
        }
        Value::Array(arr) => {
            let mut list = Vec::with_capacity(arr.len());
            for item in arr {
                list.push(json_to_llsd(item));
            }
            LLSDValue::Array(list)
        }
        Value::Object(map) => {
            let mut llsd_map = HashMap::with_capacity(map.len());
            for (k, v) in map {
                llsd_map.insert(k.clone(), json_to_llsd(v));
            }
            LLSDValue::Map(llsd_map)
        }
    }
}

/// Convert an `LLSDValue` into a `serde_json::Value`.
pub fn llsd_to_json(val: &LLSDValue) -> Value {
    match val {
        LLSDValue::Undefined => Value::Null,
        LLSDValue::Boolean(b) => Value::Bool(*b),
        LLSDValue::Integer(i) => Value::Number((*i).into()),
        LLSDValue::Real(r) => {
            serde_json::Number::from_f64(*r)
                .map(Value::Number)
                .unwrap_or(Value::Null)
        }
        LLSDValue::String(s) => Value::String(s.clone()),
        LLSDValue::UUID(u) => Value::String(u.to_string()),
        LLSDValue::Date(d) => {
            if let Some(dt) = chrono::DateTime::from_timestamp(*d, 0) {
                Value::String(dt.to_rfc3339())
            } else {
                Value::Number((*d).into())
            }
        }
        LLSDValue::URI(u) => Value::String(u.clone()),
        LLSDValue::Binary(bytes) => {
            use base64::Engine;
            Value::String(base64::engine::general_purpose::STANDARD.encode(bytes))
        }
        LLSDValue::Map(map) => {
            let mut obj = serde_json::Map::new();
            for (k, v) in map {
                obj.insert(k.clone(), llsd_to_json(v));
            }
            Value::Object(obj)
        }
        LLSDValue::Array(arr) => {
            let list: Vec<Value> = arr.iter().map(llsd_to_json).collect();
            Value::Array(list)
        }
    }
}
