//! Unit tests for MCP JSON-RPC protocol, tool registry, and tool dispatch.

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::client::LeapClient;
    use crate::mcp::types::{json_to_llsd, llsd_to_json};
    use serde_json::json;
    use serde_llsd::LLSDValue;

    #[test]
    fn test_mcp_tool_definitions_registry() {
        let tools = get_tool_definitions();
        assert!(tools.len() >= 16, "Expected at least 16 MCP tools registered, got {}", tools.len());

        let names: Vec<&str> = tools.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"aurora_connect"));
        assert!(names.contains(&"aurora_disconnect"));
        assert!(names.contains(&"aurora_login"));
        assert!(names.contains(&"aurora_get_telemetry"));
        assert!(names.contains(&"aurora_move"));
        assert!(names.contains(&"aurora_stop_moving"));
        assert!(names.contains(&"aurora_teleport"));
        assert!(names.contains(&"aurora_camera_control"));
        assert!(names.contains(&"aurora_take_snapshot"));
        assert!(names.contains(&"aurora_bookmark_location"));
        assert!(names.contains(&"aurora_list_bookmarks"));
        assert!(names.contains(&"aurora_goto_bookmark"));
        assert!(names.contains(&"aurora_get_radar"));
        assert!(names.contains(&"aurora_get_objects"));
        assert!(names.contains(&"aurora_send_chat"));
        assert!(names.contains(&"aurora_get_recent_chat"));
        assert!(names.contains(&"aurora_eval_script"));
        assert!(names.contains(&"aurora_raw_llsd"));

        // Validate that each tool has valid JSON schema
        for tool in &tools {
            assert!(!tool.description.is_empty(), "Tool {} missing description", tool.name);
            assert_eq!(tool.input_schema["type"], "object", "Tool {} schema must be object", tool.name);
        }
    }

    #[test]
    fn test_json_to_llsd_and_back() {
        let original_json = json!({
            "name": "Test Avatar",
            "active": true,
            "count": 42,
            "coords": [128.5, 64.0, 25.25],
            "sub": {
                "key": "val"
            }
        });

        let llsd = json_to_llsd(&original_json);
        match &llsd {
            LLSDValue::Map(m) => {
                assert_eq!(m.get("name"), Some(&LLSDValue::String("Test Avatar".to_string())));
                assert_eq!(m.get("active"), Some(&LLSDValue::Boolean(true)));
                assert_eq!(m.get("count"), Some(&LLSDValue::Integer(42)));
            }
            _ => panic!("Expected LLSD map"),
        }

        let back_json = llsd_to_json(&llsd);
        assert_eq!(back_json["name"], "Test Avatar");
        assert_eq!(back_json["active"], true);
        assert_eq!(back_json["count"], 42);
        assert_eq!(back_json["coords"][0], 128.5);
    }

    #[test]
    fn test_mcp_eval_script_tool() {
        let mut client = LeapClient::new();
        let code = r#"
            let a = 15;
            let b = 25;
            let sum = a + b;
            assert_eq(sum, 40);
        "#;
        let args = json!({ "code": code });
        let result = dispatch_tool_call(&mut client, "aurora_eval_script", &args);
        assert!(!result.is_error, "eval script failed: {:?}", result.content);

        let text = match &result.content[0] {
            McpContent::Text { text } => text,
            _ => panic!("Expected text content"),
        };
        let parsed: serde_json::Value = serde_json::from_str(text).expect("Valid JSON report");
        assert_eq!(parsed["status"], "Script executed successfully");
        assert_eq!(parsed["total_assertions"], 1);
        assert_eq!(parsed["passed"], 1);
        assert_eq!(parsed["failed"], 0);
    }

    #[test]
    fn test_mcp_bookmark_tools_roundtrip() {
        let mut client = LeapClient::new();
        let bookmark_args = json!({
            "name": "MCP Test Sanctuary",
            "notes": "Automated MCP unit test bookmark"
        });
        let res = dispatch_tool_call(&mut client, "aurora_bookmark_location", &bookmark_args);
        assert!(!res.is_error);

        let list_res = dispatch_tool_call(&mut client, "aurora_list_bookmarks", &json!({}));
        assert!(!list_res.is_error);
        let text = match &list_res.content[0] {
            McpContent::Text { text } => text,
            _ => panic!("Expected text content"),
        };
        let parsed: serde_json::Value = serde_json::from_str(text).expect("Valid JSON");
        let bookmarks = parsed["bookmarks"].as_array().expect("Array of bookmarks");
        assert!(bookmarks.iter().any(|b| b["name"] == "MCP Test Sanctuary"));
    }

    #[test]
    fn test_mcp_telemetry_query_offline() {
        let mut client = LeapClient::new();
        let res = dispatch_tool_call(&mut client, "aurora_get_telemetry", &json!({}));
        assert!(!res.is_error);
        let text = match &res.content[0] {
            McpContent::Text { text } => text,
            _ => panic!("Expected text content"),
        };
        let parsed: serde_json::Value = serde_json::from_str(text).expect("Valid JSON");
        assert_eq!(parsed["connected"], false);
    }
}
