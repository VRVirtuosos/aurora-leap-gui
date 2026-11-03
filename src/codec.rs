//! LEAP Length-Prefixed Stream Codec powered by `serde-llsd`.
//!
//! Framing specification: `<byte_count>:<llsd_notation_bytes>`
//!
//! Encodes and decodes LEAP envelopes:
//! `{"pump": "<pump_name>", "data": <llsd_value>}`

use std::collections::HashMap;
use serde_llsd::LLSDValue;

/// Maximum payload size protection (16 MB).
pub const MAX_PAYLOAD_SIZE: usize = 16 * 1024 * 1024;
pub const MAX_PREFIX_LEN: usize = 20;
pub const COLON: u8 = b':';

/// A structured LEAP wire message consisting of a pump name and payload data.
#[derive(Debug, Clone, PartialEq)]
pub struct LeapMessage {
    pub pump: String,
    pub data: LLSDValue,
}

impl LeapMessage {
    pub fn new(pump: impl Into<String>, data: LLSDValue) -> Self {
        Self {
            pump: pump.into(),
            data,
        }
    }

    /// Convert this message into an envelope LLSDValue Map.
    pub fn to_llsd(&self) -> LLSDValue {
        let mut map = HashMap::new();
        map.insert("pump".to_string(), LLSDValue::String(self.pump.clone()));
        map.insert("data".to_string(), self.data.clone());
        LLSDValue::Map(map)
    }

    /// Parse a LeapMessage from an envelope LLSDValue Map.
    pub fn from_llsd(val: LLSDValue) -> Result<Self, String> {
        match val {
            LLSDValue::Map(mut map) => {
                let pump = match map.remove("pump") {
                    Some(LLSDValue::String(s)) => s,
                    Some(other) => format!("{other:?}"),
                    None => return Err("LEAP envelope missing 'pump' key".to_string()),
                };
                let data = match map.remove("data") {
                    Some(d) => d,
                    None => return Err("LEAP envelope missing 'data' key".to_string()),
                };
                Ok(Self { pump, data })
            }
            other => Err(format!("Expected LLSD Map for LEAP envelope, got {other:?}")),
        }
    }
}

/// Helper functions to build LLSD Maps easily.
#[allow(dead_code)]
pub fn llsd_map(entries: Vec<(&str, LLSDValue)>) -> LLSDValue {
    let mut map = HashMap::new();
    for (k, v) in entries {
        map.insert(k.to_string(), v);
    }
    LLSDValue::Map(map)
}

/// Extract string field from an LLSD Map.
pub fn get_str<'a>(val: &'a LLSDValue, key: &str) -> Option<&'a str> {
    match val {
        LLSDValue::Map(map) => match map.get(key) {
            Some(LLSDValue::String(s)) => Some(s.as_str()),
            _ => None,
        },
        _ => None,
    }
}

/// Extract int field from an LLSD Map.
pub fn get_i32(val: &LLSDValue, key: &str) -> Option<i32> {
    match val {
        LLSDValue::Map(map) => match map.get(key) {
            Some(LLSDValue::Integer(i)) => Some(*i),
            _ => None,
        },
        _ => None,
    }
}

/// Extract bool field from an LLSD Map.
pub fn get_bool(val: &LLSDValue, key: &str) -> Option<bool> {
    match val {
        LLSDValue::Map(map) => match map.get(key) {
            Some(LLSDValue::Boolean(b)) => Some(*b),
            _ => None,
        },
        _ => None,
    }
}

/// Extract real (f64) field from an LLSD Map.
pub fn get_f64(val: &LLSDValue, key: &str) -> Option<f64> {
    match val {
        LLSDValue::Map(map) => match map.get(key) {
            Some(LLSDValue::Real(r)) => Some(*r),
            Some(LLSDValue::Integer(i)) => Some(*i as f64),
            _ => None,
        },
        _ => None,
    }
}

/// Extract sub-map from an LLSD Map.
pub fn get_map<'a>(val: &'a LLSDValue, key: &str) -> Option<&'a HashMap<String, LLSDValue>> {
    match val {
        LLSDValue::Map(map) => match map.get(key) {
            Some(LLSDValue::Map(sub)) => Some(sub),
            _ => None,
        },
        _ => None,
    }
}

/// Extract array from an LLSD Map.
pub fn get_array<'a>(val: &'a LLSDValue, key: &str) -> Option<&'a Vec<LLSDValue>> {
    match val {
        LLSDValue::Map(map) => match map.get(key) {
            Some(LLSDValue::Array(arr)) => Some(arr),
            _ => None,
        },
        _ => None,
    }
}

/// Convert any LLSD numeric value (Real or Integer) to f64.
pub fn val_to_f64(val: &LLSDValue) -> Option<f64> {
    match val {
        LLSDValue::Real(r) => Some(*r),
        LLSDValue::Integer(i) => Some(*i as f64),
        _ => None,
    }
}

/// Extract a 3D vector (x, y, z) from an LLSD value.
/// Supports:
/// 1. Direct Array: `[x, y, z]`
/// 2. Direct Map: `{"x": x, "y": y, "z": z}`
/// 3. Sub-key inside a Map (e.g. `val["position"]` as Array or Map)
/// 4. Flat coordinates in `val` (`x`, `y`, `z`) or prefixed (e.g. `target_x`, `target_y`, `target_z`)
pub fn parse_vec3(val: &LLSDValue, key: Option<&str>) -> Option<(f64, f64, f64)> {
    if let Some(k) = key {
        if let LLSDValue::Map(map) = val {
            if let Some(sub) = map.get(k) {
                if let Some(v) = parse_vec3(sub, None) {
                    return Some(v);
                }
            }
            // Fallback: check flat keys based on key name
            if k == "position" || k == "agent_pos" || k == "camera_pos" {
                if let (Some(x), Some(y), Some(z)) = (
                    map.get("x").and_then(val_to_f64),
                    map.get("y").and_then(val_to_f64),
                    map.get("z").and_then(val_to_f64),
                ) {
                    return Some((x, y, z));
                }
            }
            if k == "target" || k == "camera_target" || k == "focus" {
                if let (Some(tx), Some(ty), Some(tz)) = (
                    map.get("target_x").and_then(val_to_f64),
                    map.get("target_y").and_then(val_to_f64),
                    map.get("target_z").and_then(val_to_f64),
                ) {
                    return Some((tx, ty, tz));
                }
            }
        }
    }

    match val {
        LLSDValue::Array(arr) => {
            if arr.len() >= 3 {
                let x = val_to_f64(&arr[0])?;
                let y = val_to_f64(&arr[1])?;
                let z = val_to_f64(&arr[2])?;
                Some((x, y, z))
            } else {
                None
            }
        }
        LLSDValue::Map(map) => {
            let x = map.get("x").and_then(val_to_f64)?;
            let y = map.get("y").and_then(val_to_f64)?;
            let z = map.get("z").and_then(val_to_f64)?;
            Some((x, y, z))
        }
        _ => None,
    }
}

/// Encode an LLSDValue into a clean notation string without the XML header.
pub fn to_clean_notation(val: &LLSDValue) -> Result<String, String> {
    let raw = serde_llsd::notation_to_string(val).map_err(|e| format!("Serialization error: {e}"))?;
    let trimmed = raw.trim_start();
    if let Some(stripped) = trimmed.strip_prefix("<? llsd/notation ?>") {
        Ok(stripped.trim_start().to_string())
    } else {
        Ok(trimmed.to_string())
    }
}

/// Parse an LLSDValue from a notation or auto-detected string.
pub fn from_notation_str(s: &str) -> Result<LLSDValue, String> {
    let trimmed = s.trim_start();
    if trimmed.starts_with("<? llsd/notation ?>") || trimmed.starts_with("<?llsd/notation?>") {
        serde_llsd::auto_from_str(trimmed).map_err(|e| format!("Auto parse error: {e}"))
    } else {
        serde_llsd::notation_from_str(trimmed).map_err(|e| format!("Notation parse error: {e}"))
    }
}

/// Encode a LeapMessage into length-prefixed bytes: `<len>:<notation_bytes>`.
pub fn encode_frame(msg: &LeapMessage) -> Result<Vec<u8>, String> {
    let envelope = msg.to_llsd();
    let notation = to_clean_notation(&envelope)?;
    let payload = notation.as_bytes();
    let len_str = payload.len().to_string();

    let mut frame = Vec::with_capacity(len_str.len() + 1 + payload.len());
    frame.extend_from_slice(len_str.as_bytes());
    frame.push(COLON);
    frame.extend_from_slice(payload);
    Ok(frame)
}

/// Stateful stream decoder that accumulates chunks and yields parsed LeapMessages.
#[derive(Debug, Default)]
pub struct LeapDecoder {
    buffer: Vec<u8>,
}

impl LeapDecoder {
    pub fn new() -> Self {
        Self { buffer: Vec::new() }
    }

    /// Feed incoming bytes into the decoder buffer.
    pub fn feed(&mut self, chunk: &[u8]) {
        self.buffer.extend_from_slice(chunk);
    }

    /// Attempt to decode the next frame from the buffer.
    pub fn decode_next(&mut self) -> Result<Option<LeapMessage>, String> {
        // Find colon delimiter
        let colon_idx = match self.buffer.iter().position(|&b| b == COLON) {
            Some(idx) => idx,
            None => {
                if self.buffer.len() > MAX_PREFIX_LEN {
                    return Err(format!(
                        "Length prefix exceeded limit without colon (buffer len: {})",
                        self.buffer.len()
                    ));
                }
                return Ok(None);
            }
        };

        let prefix_bytes = &self.buffer[..colon_idx];
        let prefix_str = std::str::from_utf8(prefix_bytes)
            .map_err(|_| "Non-UTF8 length prefix".to_string())?
            .trim();

        let length: usize = prefix_str
            .parse()
            .map_err(|_| format!("Invalid length prefix integer: '{prefix_str}'"))?;

        if length > MAX_PAYLOAD_SIZE {
            return Err(format!("Payload length {length} exceeds maximum allowed size"));
        }

        let payload_start = colon_idx + 1;
        let payload_end = payload_start + length;

        if self.buffer.len() < payload_end {
            // Incomplete payload, wait for more data
            return Ok(None);
        }

        let payload_bytes = &self.buffer[payload_start..payload_end];
        let payload_str = std::str::from_utf8(payload_bytes)
            .map_err(|e| format!("Malformed UTF-8 in LEAP payload: {e}"))?;

        let parsed_val = from_notation_str(payload_str)?;
        let msg = LeapMessage::from_llsd(parsed_val)?;

        // Drain the consumed frame from the buffer
        self.buffer.drain(..payload_end);

        Ok(Some(msg))
    }

    /// Check if buffer currently has any buffered unparsed bytes.
    #[allow(dead_code)]
    pub fn buffered_len(&self) -> usize {
        self.buffer.len()
    }

    /// Clear buffer on fatal stream error.
    pub fn clear(&mut self) {
        self.buffer.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_codec_roundtrip() {
        let mut data = HashMap::new();
        data.insert("op".to_string(), LLSDValue::String("ping".to_string()));
        data.insert("reqid".to_string(), LLSDValue::Integer(101));
        let msg = LeapMessage::new("LLAppViewer", LLSDValue::Map(data));

        let frame = encode_frame(&msg).expect("Failed to encode frame");
        assert!(frame.contains(&COLON));

        let mut decoder = LeapDecoder::new();
        decoder.feed(&frame);

        let decoded = decoder.decode_next().expect("Decode error").expect("Expected decoded message");
        assert_eq!(decoded.pump, "LLAppViewer");
        assert_eq!(get_str(&decoded.data, "op"), Some("ping"));
        assert_eq!(get_i32(&decoded.data, "reqid"), Some(101));
    }

    #[test]
    fn test_fragmented_frame() {
        let mut data = HashMap::new();
        data.insert("msg".to_string(), LLSDValue::String("fragmentation test".to_string()));
        let msg = LeapMessage::new("chat", LLSDValue::Map(data));
        let frame = encode_frame(&msg).expect("Failed to encode frame");

        let mut decoder = LeapDecoder::new();
        // Feed byte by byte
        for (i, &b) in frame.iter().enumerate() {
            decoder.feed(&[b]);
            let res = decoder.decode_next().expect("Decode error");
            if i < frame.len() - 1 {
                assert!(res.is_none());
            } else {
                let decoded = res.expect("Expected frame at the end");
                assert_eq!(decoded.pump, "chat");
                assert_eq!(get_str(&decoded.data, "msg"), Some("fragmentation test"));
            }
        }
    }

    #[test]
    fn test_parse_vec3_formats() {
        // Array format (as returned by LLCamera)
        let mut map = HashMap::new();
        map.insert(
            "position".to_string(),
            LLSDValue::Array(vec![
                LLSDValue::Real(128.5),
                LLSDValue::Real(64.25),
                LLSDValue::Real(25.0),
            ]),
        );
        map.insert(
            "target".to_string(),
            LLSDValue::Array(vec![
                LLSDValue::Real(130.0),
                LLSDValue::Real(64.25),
                LLSDValue::Real(25.0),
            ]),
        );
        let val = LLSDValue::Map(map);

        assert_eq!(parse_vec3(&val, Some("position")), Some((128.5, 64.25, 25.0)));
        assert_eq!(parse_vec3(&val, Some("target")), Some((130.0, 64.25, 25.0)));

        // Map format
        let mut submap = HashMap::new();
        submap.insert("x".to_string(), LLSDValue::Real(10.0));
        submap.insert("y".to_string(), LLSDValue::Real(20.0));
        submap.insert("z".to_string(), LLSDValue::Real(30.0));
        let mut parent = HashMap::new();
        parent.insert("position".to_string(), LLSDValue::Map(submap));
        let val_submap = LLSDValue::Map(parent);

        assert_eq!(parse_vec3(&val_submap, Some("position")), Some((10.0, 20.0, 30.0)));

        // Flat format
        let mut flat = HashMap::new();
        flat.insert("x".to_string(), LLSDValue::Real(5.0));
        flat.insert("y".to_string(), LLSDValue::Real(6.0));
        flat.insert("z".to_string(), LLSDValue::Real(7.0));
        let val_flat = LLSDValue::Map(flat);
        assert_eq!(parse_vec3(&val_flat, Some("position")), Some((5.0, 6.0, 7.0)));
    }
}
