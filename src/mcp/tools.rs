//! MCP Tool Definitions and Dispatch Handlers.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use serde_json::{json, Value};
use serde_llsd::LLSDValue;

use crate::client::LeapClient;
use crate::codec::{get_bool, get_str, parse_vec3};
use crate::mcp::types::{json_to_llsd, llsd_to_json, McpCallResult, McpToolDefinition};
use crate::scripting::engine::{create_script_engine, ScriptContext, ScriptEvent};
use crate::ui::tab_camera::SavedLocation;

/// Return the full list of tool definitions registered with this MCP server.
pub fn get_tool_definitions() -> Vec<McpToolDefinition> {
    vec![
        McpToolDefinition {
            name: "aurora_connect".to_string(),
            description: "Spawn and connect to the Aurora-Viewer subprocess over LEAP standard I/O pipes. If viewer_path is not specified, auto-discovers the executable.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "viewer_path": {
                        "type": "string",
                        "description": "Optional file path to aurora-viewer.exe. If omitted, default build directories are searched."
                    },
                    "extra_args": {
                        "type": "string",
                        "description": "Optional CLI arguments to pass to the viewer process (e.g. '--channel LEAP-AI')."
                    }
                }
            }),
        },
        McpToolDefinition {
            name: "aurora_disconnect".to_string(),
            description: "Gracefully disconnect from Aurora-Viewer and terminate background LEAP reader/writer threads.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        McpToolDefinition {
            name: "aurora_login".to_string(),
            description: "Initiate authentication and login sequence for an avatar onto a specified grid.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "username": {
                        "type": "string",
                        "description": "Avatar username in 'Firstname Lastname' format (or single name for modern SL accounts)."
                    },
                    "password": {
                        "type": "string",
                        "description": "Account password."
                    },
                    "start_location": {
                        "type": "string",
                        "description": "Optional start location: 'home', 'last', or an SLurl (e.g. 'secondlife://Horizons/128/128/25')."
                    },
                    "grid_uri": {
                        "type": "string",
                        "description": "Optional login URI (e.g. 'http://login.osgrid.org/' or SL Agni/Aditi)."
                    }
                },
                "required": ["username", "password"]
            }),
        },
        McpToolDefinition {
            name: "aurora_get_telemetry".to_string(),
            description: "Query current real-time viewer and avatar telemetry: spatial coordinates, heading yaw, region name, sim FPS, time dilation, and camera parameters.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        McpToolDefinition {
            name: "aurora_move".to_string(),
            description: "Command avatar movement in a specified direction for a set duration. Optionally captures a viewport snapshot upon completion for visual perception.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "direction": {
                        "type": "string",
                        "enum": ["forward", "backward", "left", "right", "up", "down", "turn_left", "turn_right"],
                        "description": "Movement direction."
                    },
                    "duration_sec": {
                        "type": "number",
                        "description": "Duration in seconds to hold movement key (default 1.0, max 10.0)."
                    },
                    "capture_snapshot": {
                        "type": "boolean",
                        "description": "If true, automatically captures a high-resolution snapshot after moving and returns image data."
                    }
                },
                "required": ["direction"]
            }),
        },
        McpToolDefinition {
            name: "aurora_stop_moving".to_string(),
            description: "Immediately release all active movement keys and halt avatar locomotion.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        McpToolDefinition {
            name: "aurora_teleport".to_string(),
            description: "Teleport the avatar to a specific region and coordinate, or SLurl. Optionally captures a snapshot upon arrival.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "region": {
                        "type": "string",
                        "description": "Destination region simulator name."
                    },
                    "x": { "type": "number", "description": "X coordinate (0-256)." },
                    "y": { "type": "number", "description": "Y coordinate (0-256)." },
                    "z": { "type": "number", "description": "Z altitude in meters." },
                    "slurl": {
                        "type": "string",
                        "description": "Optional direct SLurl string (e.g. 'secondlife://Region/128/128/25')."
                    },
                    "capture_snapshot": {
                        "type": "boolean",
                        "description": "If true, captures a viewport snapshot upon teleport completion."
                    }
                }
            }),
        },
        McpToolDefinition {
            name: "aurora_camera_control".to_string(),
            description: "Precision 3D camera control: set exact eye position and focus target, orbit around target, pan, or reset to default trailing avatar view.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "action": {
                        "type": "string",
                        "enum": ["set", "orbit", "pan", "reset", "focus_avatar"],
                        "description": "Camera control operation."
                    },
                    "x": { "type": "number", "description": "Eye X coordinate (for 'set')." },
                    "y": { "type": "number", "description": "Eye Y coordinate (for 'set')." },
                    "z": { "type": "number", "description": "Eye Z coordinate (for 'set')." },
                    "target_x": { "type": "number", "description": "Target/Focus X coordinate (for 'set')." },
                    "target_y": { "type": "number", "description": "Target/Focus Y coordinate (for 'set')." },
                    "target_z": { "type": "number", "description": "Target/Focus Z coordinate (for 'set')." },
                    "fov": { "type": "number", "description": "Field of view in degrees (30.0 - 120.0)." },
                    "yaw_deg": { "type": "number", "description": "Orbit horizontal rotation in degrees (for 'orbit')." },
                    "pitch_deg": { "type": "number", "description": "Orbit elevation pitch in degrees (for 'orbit')." },
                    "forward": { "type": "number", "description": "Forward/back distance (for 'pan')." },
                    "strafe": { "type": "number", "description": "Left/right distance (for 'pan')." },
                    "elevation": { "type": "number", "description": "Up/down distance (for 'pan')." },
                    "capture_snapshot": {
                        "type": "boolean",
                        "description": "If true, captures a viewport snapshot after adjusting camera."
                    }
                },
                "required": ["action"]
            }),
        },
        McpToolDefinition {
            name: "aurora_take_snapshot".to_string(),
            description: "Capture an in-world high-resolution PNG screenshot via LLCamera.takePicture. Returns file path and base64 image data for multimodal inspection.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "width": { "type": "integer", "description": "Width in pixels (default: 1024)." },
                    "height": { "type": "integer", "description": "Height in pixels (default: 768)." },
                    "save_to": { "type": "string", "description": "Optional local file path to save image." },
                    "include_ui": { "type": "boolean", "description": "Whether to include 2D UI overlay (default: false)." }
                }
            }),
        },
        McpToolDefinition {
            name: "aurora_bookmark_location".to_string(),
            description: "Save current avatar and camera position into saved_locations.json for instant recall.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Descriptive name for the bookmark." },
                    "notes": { "type": "string", "description": "Optional contextual notes." }
                },
                "required": ["name"]
            }),
        },
        McpToolDefinition {
            name: "aurora_list_bookmarks".to_string(),
            description: "Retrieve all saved location bookmarks from saved_locations.json.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        McpToolDefinition {
            name: "aurora_goto_bookmark".to_string(),
            description: "Restore avatar location and/or camera viewpoint to a previously saved bookmark.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "name": { "type": "string", "description": "Name or substring of the saved bookmark." },
                    "mode": {
                        "type": "string",
                        "enum": ["teleport", "camera", "both"],
                        "description": "Restoration mode (default: 'both')."
                    }
                },
                "required": ["name"]
            }),
        },
        McpToolDefinition {
            name: "aurora_get_radar".to_string(),
            description: "Scan nearby avatars within draw distance using LLNearbyAvatars: names, distances, 3D offsets, and active animation states.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        McpToolDefinition {
            name: "aurora_get_objects".to_string(),
            description: "Inspect 3D primitives and linksets in simulator proximity using LLObjects.getNearbyObjects.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "max_distance": {
                        "type": "number",
                        "description": "Maximum search radius in meters (default 64.0)."
                    }
                }
            }),
        },
        McpToolDefinition {
            name: "aurora_send_chat".to_string(),
            description: "Send an in-world local chat message via LLChat.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "message": { "type": "string", "description": "Message text to speak." },
                    "channel": { "type": "integer", "description": "Chat channel (default 0 for public local chat)." },
                    "chat_type": {
                        "type": "string",
                        "enum": ["whisper", "say", "shout"],
                        "description": "Chat volume range (default 'say')."
                    }
                },
                "required": ["message"]
            }),
        },
        McpToolDefinition {
            name: "aurora_get_recent_chat".to_string(),
            description: "Retrieve recent in-world chat messages heard on the chat pump.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "limit": { "type": "integer", "description": "Maximum number of messages to return (default 25)." }
                }
            }),
        },
        McpToolDefinition {
            name: "aurora_eval_script".to_string(),
            description: "Execute an arbitrary Rhai automation script against the running viewer. Returns execution logs, assertion results, and error details.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "code": {
                        "type": "string",
                        "description": "Rhai script source code."
                    }
                },
                "required": ["code"]
            }),
        },
        McpToolDefinition {
            name: "aurora_raw_llsd".to_string(),
            description: "Send a raw LLSD JSON request to any viewer event pump and receive correlated reply.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "pump": { "type": "string", "description": "Target LEAP event pump name." },
                    "data": { "type": "object", "description": "LLSD message payload dictionary." },
                    "timeout_sec": { "type": "number", "description": "Request timeout in seconds (default 5.0)." }
                },
                "required": ["pump", "data"]
            }),
        },
        McpToolDefinition {
            name: "aurora_get_notifications".to_string(),
            description: "Query all active pending notifications, alerts, group invites, and script dialogs from Aurora-Viewer.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {}
            }),
        },
        McpToolDefinition {
            name: "aurora_respond_notification".to_string(),
            description: "Respond to, accept, decline, or select a button on an active notification or script dialog.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "id": {
                        "type": "number",
                        "description": "Unique notification ID to respond to."
                    },
                    "action": {
                        "type": "string",
                        "description": "Response action: 'accept', 'decline', 'ignore', 'button', or 'cancel'."
                    },
                    "button_index": {
                        "type": "integer",
                        "description": "Optional 0-based button index if clicking a numbered dialog button."
                    },
                    "text": {
                        "type": "string",
                        "description": "Optional text response (e.g. for text input dialogs)."
                    }
                },
                "required": ["id"]
            }),
        },
        McpToolDefinition {
            name: "aurora_create_object".to_string(),
            description: "Rez a new 3D primitive (cube, sphere, cylinder, prism, pyramid, cone, torus, etc.) inworld near the avatar or at a specified coordinate.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "shape": {
                        "type": "string",
                        "description": "Primitive shape name ('cube'/'box', 'sphere', 'cylinder', 'prism', 'pyramid', 'cone', 'torus', 'tube', 'ring'). Default is 'cube'."
                    },
                    "position": {
                        "type": "array",
                        "items": { "type": "number" },
                        "description": "Optional [x, y, z] in-world position. If omitted, rezzes 2 meters in front of avatar."
                    },
                    "scale": {
                        "type": "array",
                        "items": { "type": "number" },
                        "description": "Optional [scale_x, scale_y, scale_z] dimensions in meters. Default is [0.5, 0.5, 0.5]."
                    },
                    "material": {
                        "type": "integer",
                        "description": "Optional material code (1 for metal, 0 for stone, etc.). Default is 1."
                    }
                }
            }),
        },
        McpToolDefinition {
            name: "aurora_delete_object".to_string(),
            description: "Delete (derez) an inworld primitive or object into the avatar's trash folder.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "local_id": {
                        "type": "integer",
                        "description": "Simulator local ID of the object to delete."
                    },
                    "uuid": {
                        "type": "string",
                        "description": "Full UUID of the object to delete."
                    }
                }
            }),
        },
        McpToolDefinition {
            name: "aurora_set_object_transform".to_string(),
            description: "Modify an inworld object's position, scale, or rotation in the simulator.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "local_id": {
                        "type": "integer",
                        "description": "Simulator local ID of the target object."
                    },
                    "uuid": {
                        "type": "string",
                        "description": "Full UUID of the target object."
                    },
                    "position": {
                        "type": "array",
                        "items": { "type": "number" },
                        "description": "Optional new [x, y, z] in-world position."
                    },
                    "scale": {
                        "type": "array",
                        "items": { "type": "number" },
                        "description": "Optional new [scale_x, scale_y, scale_z] dimensions."
                    },
                    "rotation": {
                        "type": "array",
                        "items": { "type": "number" },
                        "description": "Optional [x, y, z, w] quaternion or [roll, pitch, yaw] euler rotation."
                    }
                }
            }),
        },
        McpToolDefinition {
            name: "aurora_select_object".to_string(),
            description: "Select an object in-world by local_id or uuid, causing the simulator to return its properties and highlight it.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "local_id": { "type": "integer", "description": "Local ID of object to select." },
                    "uuid": { "type": "string", "description": "UUID of object to select." }
                }
            }),
        },
        McpToolDefinition {
            name: "aurora_deselect_object".to_string(),
            description: "Deselect active selection or a specific object inworld.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "local_id": { "type": "integer", "description": "Optional local ID to deselect." },
                    "uuid": { "type": "string", "description": "Optional UUID to deselect." }
                }
            }),
        },
        McpToolDefinition {
            name: "aurora_set_object_name".to_string(),
            description: "Set the display name of an inworld object.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "local_id": { "type": "integer", "description": "Local ID of target object." },
                    "uuid": { "type": "string", "description": "UUID of target object." },
                    "name": { "type": "string", "description": "New name for the object." }
                },
                "required": ["name"]
            }),
        },
        McpToolDefinition {
            name: "aurora_duplicate_object".to_string(),
            description: "Duplicate an inworld primitive or object with an offset.".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "local_id": { "type": "integer", "description": "Local ID of source object." },
                    "uuid": { "type": "string", "description": "UUID of source object." },
                    "offset": {
                        "type": "array",
                        "items": { "type": "number" },
                        "description": "Optional [x, y, z] duplication offset in meters (default [0, 0, 1])."
                    }
                }
            }),
        },
    ]
}

/// Locate the viewer executable if not explicitly given.
fn resolve_viewer_path(custom: Option<&str>) -> Result<PathBuf, String> {
    if let Some(p) = custom {
        let path = PathBuf::from(p);
        if path.exists() {
            return Ok(path);
        }
        return Err(format!("Specified viewer executable does not exist: {p}"));
    }

    if let Ok(env_path) = std::env::var("AURORA_VIEWER_PATH") {
        let path = PathBuf::from(env_path);
        if path.exists() {
            return Ok(path);
        }
    }

    let candidates = [
        "Aurora-Viewer/target/debug/aurora-viewer.exe",
        "../Aurora-Viewer/target/debug/aurora-viewer.exe",
        "Aurora-Viewer/target/release/aurora-viewer.exe",
        "../Aurora-Viewer/target/release/aurora-viewer.exe",
        "aurora-viewer.exe",
        "target/debug/aurora-viewer.exe",
    ];

    for c in candidates {
        let p = PathBuf::from(c);
        if p.exists() {
            if let Ok(canon) = p.canonicalize() {
                return Ok(canon);
            }
            return Ok(p);
        }
    }

    Err("Could not locate aurora-viewer.exe automatically. Please specify 'viewer_path' or set AURORA_VIEWER_PATH.".to_string())
}

/// Helper to trigger a snapshot capture and return (path, base64_png, width, height).
fn capture_snapshot_internal(
    client: &LeapClient,
    width: u32,
    height: u32,
    save_to: Option<&str>,
    include_ui: bool,
) -> Result<(String, String, u32, u32), String> {
    let mut args = HashMap::new();
    args.insert("op".to_string(), LLSDValue::String("takePicture".to_string()));
    args.insert("width".to_string(), LLSDValue::Integer(width as i32));
    args.insert("height".to_string(), LLSDValue::Integer(height as i32));
    args.insert("ui".to_string(), LLSDValue::Boolean(include_ui));

    if let Some(path) = save_to {
        args.insert("save_to".to_string(), LLSDValue::String(path.to_string()));
    }

    let reply = client.request("LLCamera", args, Duration::from_secs(12))?;
    let status = get_bool(&reply, "status").unwrap_or(false);
    if !status {
        let err = get_str(&reply, "error").unwrap_or("takePicture failed in viewer");
        return Err(err.to_string());
    }

    let w = crate::codec::get_i32(&reply, "width").unwrap_or(width as i32) as u32;
    let h = crate::codec::get_i32(&reply, "height").unwrap_or(height as i32) as u32;
    let file_path = get_str(&reply, "file_path").unwrap_or("").to_string();
    let b64 = get_str(&reply, "png_base64").unwrap_or("").to_string();

    Ok((file_path, b64, w, h))
}

/// Execute an MCP tool call and return structured McpCallResult.
pub fn dispatch_tool_call(client: &mut LeapClient, name: &str, args: &Value) -> McpCallResult {
    match name {
        "aurora_connect" => {
            if client.is_connected() {
                return McpCallResult::success("Aurora-Viewer is already connected.");
            }
            let custom_path = args.get("viewer_path").and_then(|v| v.as_str());
            let exe_path = match resolve_viewer_path(custom_path) {
                Ok(p) => p,
                Err(e) => return McpCallResult::error(e),
            };

            let extra_args = args.get("extra_args").and_then(|v| v.as_str()).unwrap_or("");
            match client.spawn_and_connect(&exe_path, extra_args) {
                Ok(_) => {
                    let pid = client.child_pid().unwrap_or(0);
                    McpCallResult::success(format!(
                        "Successfully connected to Aurora-Viewer (PID: {pid}) via LEAP pipes at '{}'.",
                        exe_path.display()
                    ))
                }
                Err(e) => McpCallResult::error(format!("Failed to connect to viewer: {e}")),
            }
        }

        "aurora_disconnect" => {
            if !client.is_connected() {
                return McpCallResult::success("Aurora-Viewer is not currently connected.");
            }
            client.disconnect();
            McpCallResult::success("Disconnected from Aurora-Viewer and terminated LEAP session.")
        }

        "aurora_login" => {
            if !client.is_connected() {
                return McpCallResult::error("Aurora-Viewer is not connected. Call aurora_connect first.");
            }
            let username = match args.get("username").and_then(|v| v.as_str()) {
                Some(u) => u,
                None => return McpCallResult::error("Missing required parameter: 'username'"),
            };
            let password = match args.get("password").and_then(|v| v.as_str()) {
                Some(p) => p,
                None => return McpCallResult::error("Missing required parameter: 'password'"),
            };
            let start_location = args.get("start_location").and_then(|v| v.as_str()).unwrap_or("");
            let grid_uri = args.get("grid_uri").and_then(|v| v.as_str()).unwrap_or("");

            let mut req = HashMap::new();
            req.insert("op".to_string(), LLSDValue::String("login".to_string()));
            req.insert("username".to_string(), LLSDValue::String(username.to_string()));
            req.insert("password".to_string(), LLSDValue::String(password.to_string()));
            if !start_location.is_empty() {
                req.insert("start".to_string(), LLSDValue::String(start_location.to_string()));
            }
            if !grid_uri.is_empty() {
                req.insert("uri".to_string(), LLSDValue::String(grid_uri.to_string()));
            }

            match client.request("LLLogin", req, Duration::from_secs(10)) {
                Ok(reply) => {
                    let json_rep = llsd_to_json(&reply);
                    McpCallResult::success_json(&json!({
                        "status": "Login request initiated",
                        "username": username,
                        "reply": json_rep
                    }))
                }
                Err(e) => McpCallResult::error(format!("Login request failed: {e}")),
            }
        }

        "aurora_get_telemetry" => {
            let telem = client.get_telemetry();
            let pid = client.child_pid();
            let is_connected = client.is_connected();

            let payload = json!({
                "connected": is_connected,
                "process_pid": pid,
                "startup_state": telem.startup_state,
                "login_state": telem.login_state,
                "agent_id": telem.agent_id,
                "region_name": telem.region_name,
                "parcel_name": telem.parcel_name,
                "parcel_area": telem.parcel_area,
                "sim_fps": telem.sim_fps,
                "slurl": telem.slurl,
                "flying": telem.flying,
                "agent_pos": telem.agent_pos.map(|(x, y, z)| json!({"x": x, "y": y, "z": z})),
                "agent_yaw": telem.agent_yaw,
                "camera_pos": telem.camera_pos.map(|(x, y, z)| json!({"x": x, "y": y, "z": z})),
                "camera_target": telem.camera_target.map(|(x, y, z)| json!({"x": x, "y": y, "z": z})),
                "camera_fov": telem.camera_fov,
            });

            McpCallResult::success_json(&payload)
        }

        "aurora_move" => {
            if !client.is_connected() {
                return McpCallResult::error("Aurora-Viewer is not connected.");
            }
            let dir = match args.get("direction").and_then(|v| v.as_str()) {
                Some(d) => d,
                None => return McpCallResult::error("Missing parameter 'direction'"),
            };
            let duration = args.get("duration_sec").and_then(|v| v.as_f64()).unwrap_or(1.0).clamp(0.1, 10.0);
            let capture_snap = args.get("capture_snapshot").and_then(|v| v.as_bool()).unwrap_or(false);

            // Start move
            let mut start_args = HashMap::new();
            start_args.insert("op".to_string(), LLSDValue::String("start_move".to_string()));
            start_args.insert("direction".to_string(), LLSDValue::String(dir.to_string()));
            let _ = client.send("LLAgent", start_args);

            std::thread::sleep(Duration::from_secs_f64(duration));

            // Stop move
            let mut stop_args = HashMap::new();
            stop_args.insert("op".to_string(), LLSDValue::String("stop_move".to_string()));
            stop_args.insert("direction".to_string(), LLSDValue::String(dir.to_string()));
            let _ = client.send("LLAgent", stop_args);

            let telem = client.get_telemetry();
            let summary = format!("Avatar moved '{dir}' for {duration:.1}s. Current Pos: {:?}", telem.agent_pos);

            if capture_snap {
                match capture_snapshot_internal(client, 800, 600, None, false) {
                    Ok((path, b64, w, h)) => {
                        let text = format!("{summary}\nSnapshot captured: {path} ({w}x{h})");
                        McpCallResult::success_with_image(text, b64)
                    }
                    Err(e) => McpCallResult::success(format!("{summary}\n(Snapshot capture failed: {e})")),
                }
            } else {
                McpCallResult::success(summary)
            }
        }

        "aurora_stop_moving" => {
            if !client.is_connected() {
                return McpCallResult::error("Aurora-Viewer is not connected.");
            }
            let mut args = HashMap::new();
            args.insert("op".to_string(), LLSDValue::String("stop".to_string()));
            let _ = client.send("LLAgent", args);
            McpCallResult::success("Halted all avatar movement.")
        }

        "aurora_teleport" => {
            if !client.is_connected() {
                return McpCallResult::error("Aurora-Viewer is not connected.");
            }
            let mut tp_args = HashMap::new();
            tp_args.insert("op".to_string(), LLSDValue::String("teleport".to_string()));

            if let Some(slurl) = args.get("slurl").and_then(|v| v.as_str()) {
                tp_args.insert("slurl".to_string(), LLSDValue::String(slurl.to_string()));
            }
            if let Some(reg) = args.get("region").and_then(|v| v.as_str()) {
                tp_args.insert("region".to_string(), LLSDValue::String(reg.to_string()));
            }
            if let Some(x) = args.get("x").and_then(|v| v.as_f64()) {
                tp_args.insert("x".to_string(), LLSDValue::Real(x));
            }
            if let Some(y) = args.get("y").and_then(|v| v.as_f64()) {
                tp_args.insert("y".to_string(), LLSDValue::Real(y));
            }
            if let Some(z) = args.get("z").and_then(|v| v.as_f64()) {
                tp_args.insert("z".to_string(), LLSDValue::Real(z));
            }

            match client.request("LLAgent", tp_args, Duration::from_secs(8)) {
                Ok(reply) => {
                    let capture_snap = args.get("capture_snapshot").and_then(|v| v.as_bool()).unwrap_or(false);
                    let rep_json = llsd_to_json(&reply);
                    let summary = format!("Teleport initiated: {rep_json}");

                    if capture_snap {
                        std::thread::sleep(Duration::from_millis(1500));
                        match capture_snapshot_internal(client, 800, 600, None, false) {
                            Ok((path, b64, w, h)) => {
                                let text = format!("{summary}\nArrival snapshot: {path} ({w}x{h})");
                                McpCallResult::success_with_image(text, b64)
                            }
                            Err(e) => McpCallResult::success(format!("{summary}\n(Snapshot failed: {e})")),
                        }
                    } else {
                        McpCallResult::success(summary)
                    }
                }
                Err(e) => McpCallResult::error(format!("Teleport command failed: {e}")),
            }
        }

        "aurora_camera_control" => {
            if !client.is_connected() {
                return McpCallResult::error("Aurora-Viewer is not connected.");
            }
            let action = match args.get("action").and_then(|v| v.as_str()) {
                Some(a) => a,
                None => return McpCallResult::error("Missing parameter 'action'"),
            };

            let res = match action {
                "set" => {
                    let mut req = HashMap::new();
                    req.insert("op".to_string(), LLSDValue::String("set".to_string()));
                    if let Some(x) = args.get("x").and_then(|v| v.as_f64()) {
                        req.insert("x".to_string(), LLSDValue::Real(x));
                    }
                    if let Some(y) = args.get("y").and_then(|v| v.as_f64()) {
                        req.insert("y".to_string(), LLSDValue::Real(y));
                    }
                    if let Some(z) = args.get("z").and_then(|v| v.as_f64()) {
                        req.insert("z".to_string(), LLSDValue::Real(z));
                    }
                    if let Some(tx) = args.get("target_x").and_then(|v| v.as_f64()) {
                        req.insert("target_x".to_string(), LLSDValue::Real(tx));
                    }
                    if let Some(ty) = args.get("target_y").and_then(|v| v.as_f64()) {
                        req.insert("target_y".to_string(), LLSDValue::Real(ty));
                    }
                    if let Some(tz) = args.get("target_z").and_then(|v| v.as_f64()) {
                        req.insert("target_z".to_string(), LLSDValue::Real(tz));
                    }
                    if let Some(fov) = args.get("fov").and_then(|v| v.as_f64()) {
                        req.insert("fov".to_string(), LLSDValue::Real(fov));
                    }
                    client.request("LLCamera", req, Duration::from_secs(3))
                }
                "reset" => {
                    let mut req = HashMap::new();
                    req.insert("op".to_string(), LLSDValue::String("reset".to_string()));
                    client.request("LLCamera", req, Duration::from_secs(3))
                }
                "orbit" => {
                    let yaw = args.get("yaw_deg").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let pitch = args.get("pitch_deg").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let mut get_args = HashMap::new();
                    get_args.insert("op".to_string(), LLSDValue::String("get".to_string()));
                    match client.request("LLCamera", get_args, Duration::from_secs(3)) {
                        Ok(rep) => {
                            let (cx, cy, cz) = parse_vec3(&rep, Some("position")).unwrap_or((124.0, 128.0, 26.5));
                            let (tx, ty, tz) = parse_vec3(&rep, Some("target")).unwrap_or((128.0, 128.0, 26.0));
                            let rx = cx - tx;
                            let ry = cy - ty;
                            let rz = cz - tz;
                            let dist = (rx * rx + ry * ry + rz * rz).sqrt().max(0.1);
                            let current_yaw = ry.atan2(rx);
                            let current_pitch = (rz / dist).asin();
                            let new_yaw = current_yaw + yaw.to_radians();
                            let new_pitch = (current_pitch + pitch.to_radians()).clamp(-1.45, 1.45);
                            let new_rx = dist * new_pitch.cos() * new_yaw.cos();
                            let new_ry = dist * new_pitch.cos() * new_yaw.sin();
                            let new_rz = dist * new_pitch.sin();

                            let mut set_args = HashMap::new();
                            set_args.insert("op".to_string(), LLSDValue::String("set".to_string()));
                            set_args.insert("x".to_string(), LLSDValue::Real(tx + new_rx));
                            set_args.insert("y".to_string(), LLSDValue::Real(ty + new_ry));
                            set_args.insert("z".to_string(), LLSDValue::Real(tz + new_rz));
                            set_args.insert("target_x".to_string(), LLSDValue::Real(tx));
                            set_args.insert("target_y".to_string(), LLSDValue::Real(ty));
                            set_args.insert("target_z".to_string(), LLSDValue::Real(tz));
                            client.request("LLCamera", set_args, Duration::from_secs(3))
                        }
                        Err(e) => Err(e),
                    }
                }
                "pan" => {
                    let fwd = args.get("forward").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let strafe = args.get("strafe").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let elev = args.get("elevation").and_then(|v| v.as_f64()).unwrap_or(0.0);
                    let mut get_args = HashMap::new();
                    get_args.insert("op".to_string(), LLSDValue::String("get".to_string()));
                    match client.request("LLCamera", get_args, Duration::from_secs(3)) {
                        Ok(rep) => {
                            let (cx, cy, cz) = parse_vec3(&rep, Some("position")).unwrap_or((128.0, 128.0, 25.0));
                            let (tx, ty, tz) = parse_vec3(&rep, Some("target")).unwrap_or((128.0, 128.0, 26.0));
                            let mut dx = tx - cx;
                            let mut dy = ty - cy;
                            let mut dz = tz - cz;
                            let len = (dx * dx + dy * dy + dz * dz).sqrt().max(0.001);
                            dx /= len;
                            dy /= len;
                            dz /= len;
                            let rx = dy;
                            let ry = -dx;
                            let r_len = (rx * rx + ry * ry).sqrt().max(0.001);
                            let rx = rx / r_len;
                            let ry = ry / r_len;

                            let delta_x = dx * fwd + rx * strafe;
                            let delta_y = dy * fwd + ry * strafe;
                            let delta_z = dz * fwd + elev;

                            let mut set_args = HashMap::new();
                            set_args.insert("op".to_string(), LLSDValue::String("set".to_string()));
                            set_args.insert("x".to_string(), LLSDValue::Real(cx + delta_x));
                            set_args.insert("y".to_string(), LLSDValue::Real(cy + delta_y));
                            set_args.insert("z".to_string(), LLSDValue::Real(cz + delta_z));
                            set_args.insert("target_x".to_string(), LLSDValue::Real(tx + delta_x));
                            set_args.insert("target_y".to_string(), LLSDValue::Real(ty + delta_y));
                            set_args.insert("target_z".to_string(), LLSDValue::Real(tz + delta_z));
                            client.request("LLCamera", set_args, Duration::from_secs(3))
                        }
                        Err(e) => Err(e),
                    }
                }
                "focus_avatar" | "focus_front" | "front_view" => {
                    let telem = client.get_telemetry();
                    if let Some((ax, ay, az)) = telem.agent_pos {
                        let yaw = telem.agent_yaw.unwrap_or(0.0);
                        let cam_dist = 2.5;
                        let cx = ax + cam_dist * yaw.cos();
                        let cy = ay + cam_dist * yaw.sin();
                        let cz = az + 1.1;

                        let mut set_args = HashMap::new();
                        set_args.insert("op".to_string(), LLSDValue::String("set".to_string()));
                        set_args.insert("x".to_string(), LLSDValue::Real(cx));
                        set_args.insert("y".to_string(), LLSDValue::Real(cy));
                        set_args.insert("z".to_string(), LLSDValue::Real(cz));
                        set_args.insert("target_x".to_string(), LLSDValue::Real(ax));
                        set_args.insert("target_y".to_string(), LLSDValue::Real(ay));
                        set_args.insert("target_z".to_string(), LLSDValue::Real(az + 1.0));
                        client.request("LLCamera", set_args, Duration::from_secs(3))
                    } else {
                        let mut req = HashMap::new();
                        req.insert("op".to_string(), LLSDValue::String("preset".to_string()));
                        req.insert("preset".to_string(), LLSDValue::String("front_portrait".to_string()));
                        client.request("LLCamera", req, Duration::from_secs(3))
                    }
                }
                "preset" => {
                    let preset_name = args.get("preset").and_then(|v| v.as_str()).unwrap_or("front_portrait");
                    let mut req = HashMap::new();
                    req.insert("op".to_string(), LLSDValue::String("preset".to_string()));
                    req.insert("preset".to_string(), LLSDValue::String(preset_name.to_string()));
                    client.request("LLCamera", req, Duration::from_secs(3))
                }
                other => Err(format!("Unknown camera action: '{other}'")),
            };

            match res {
                Ok(reply) => {
                    let capture_snap = args.get("capture_snapshot").and_then(|v| v.as_bool()).unwrap_or(false);
                    let rep_json = llsd_to_json(&reply);
                    let summary = format!("Camera operation '{action}' executed: {rep_json}");

                    if capture_snap {
                        match capture_snapshot_internal(client, 800, 600, None, false) {
                            Ok((path, b64, w, h)) => {
                                let text = format!("{summary}\nSnapshot view: {path} ({w}x{h})");
                                McpCallResult::success_with_image(text, b64)
                            }
                            Err(e) => McpCallResult::success(format!("{summary}\n(Snapshot failed: {e})")),
                        }
                    } else {
                        McpCallResult::success(summary)
                    }
                }
                Err(e) => McpCallResult::error(format!("Camera control failed: {e}")),
            }
        }

        "aurora_take_snapshot" => {
            if !client.is_connected() {
                return McpCallResult::error("Aurora-Viewer is not connected.");
            }
            let width = args.get("width").and_then(|v| v.as_u64()).unwrap_or(1024) as u32;
            let height = args.get("height").and_then(|v| v.as_u64()).unwrap_or(768) as u32;
            let save_to = args.get("save_to").and_then(|v| v.as_str());
            let ui = args.get("include_ui").and_then(|v| v.as_bool()).unwrap_or(false);

            match capture_snapshot_internal(client, width, height, save_to, ui) {
                Ok((path, b64, w, h)) => {
                    let text = format!("Snapshot captured successfully!\nSaved to: {path}\nResolution: {w}x{h}");
                    McpCallResult::success_with_image(text, b64)
                }
                Err(e) => McpCallResult::error(format!("Snapshot capture failed: {e}")),
            }
        }

        "aurora_bookmark_location" => {
            let name = match args.get("name").and_then(|v| v.as_str()) {
                Some(n) => n,
                None => return McpCallResult::error("Missing parameter 'name'"),
            };
            let notes = args.get("notes").and_then(|v| v.as_str()).unwrap_or("");
            let telem = client.get_telemetry();

            let loc = SavedLocation {
                id: uuid::Uuid::new_v4().to_string(),
                name: name.to_string(),
                region: telem.region_name.clone().unwrap_or_else(|| "Unknown".to_string()),
                created_at: chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string(),
                avatar_pos: telem.agent_pos.unwrap_or((128.0, 128.0, 25.0)),
                camera_pos: telem.camera_pos.unwrap_or((124.0, 128.0, 26.5)),
                camera_target: telem.camera_target.unwrap_or((128.0, 128.0, 26.0)),
                camera_fov: telem.camera_fov.unwrap_or(60.0),
                notes: notes.to_string(),
            };

            let storage_path = PathBuf::from("saved_locations.json");
            let mut list: Vec<SavedLocation> = if storage_path.exists() {
                std::fs::read_to_string(&storage_path)
                    .ok()
                    .and_then(|d| serde_json::from_str(&d).ok())
                    .unwrap_or_default()
            } else {
                Vec::new()
            };

            list.push(loc.clone());
            if let Ok(json_str) = serde_json::to_string_pretty(&list) {
                let _ = std::fs::write(&storage_path, json_str);
            }

            McpCallResult::success_json(&json!({
                "status": "Location bookmarked",
                "bookmark": loc
            }))
        }

        "aurora_list_bookmarks" => {
            let storage_path = PathBuf::from("saved_locations.json");
            if storage_path.exists() {
                if let Ok(data) = std::fs::read_to_string(&storage_path) {
                    if let Ok(list) = serde_json::from_str::<Vec<SavedLocation>>(&data) {
                        return McpCallResult::success_json(&json!({ "bookmarks": list }));
                    }
                }
            }
            McpCallResult::success_json(&json!({ "bookmarks": [] }))
        }

        "aurora_goto_bookmark" => {
            if !client.is_connected() {
                return McpCallResult::error("Aurora-Viewer is not connected.");
            }
            let query = match args.get("name").and_then(|v| v.as_str()) {
                Some(q) => q.to_lowercase(),
                None => return McpCallResult::error("Missing parameter 'name'"),
            };
            let mode = args.get("mode").and_then(|v| v.as_str()).unwrap_or("both");

            let storage_path = PathBuf::from("saved_locations.json");
            let list: Vec<SavedLocation> = if storage_path.exists() {
                std::fs::read_to_string(&storage_path)
                    .ok()
                    .and_then(|d| serde_json::from_str(&d).ok())
                    .unwrap_or_default()
            } else {
                Vec::new()
            };

            let matched = list.iter().find(|loc| loc.name.to_lowercase().contains(&query));
            match matched {
                Some(loc) => {
                    let mut actions_done = Vec::new();

                    if mode == "teleport" || mode == "both" {
                        let mut tp_args = HashMap::new();
                        tp_args.insert("op".to_string(), LLSDValue::String("teleport".to_string()));
                        tp_args.insert("region".to_string(), LLSDValue::String(loc.region.clone()));
                        tp_args.insert("x".to_string(), LLSDValue::Real(loc.avatar_pos.0));
                        tp_args.insert("y".to_string(), LLSDValue::Real(loc.avatar_pos.1));
                        tp_args.insert("z".to_string(), LLSDValue::Real(loc.avatar_pos.2));
                        let _ = client.send("LLAgent", tp_args);
                        actions_done.push(format!("Teleport to ({:.1}, {:.1}, {:.1}) in {}", loc.avatar_pos.0, loc.avatar_pos.1, loc.avatar_pos.2, loc.region));
                    }

                    if mode == "camera" || mode == "both" {
                        let mut cam_args = HashMap::new();
                        cam_args.insert("op".to_string(), LLSDValue::String("set".to_string()));
                        cam_args.insert("x".to_string(), LLSDValue::Real(loc.camera_pos.0));
                        cam_args.insert("y".to_string(), LLSDValue::Real(loc.camera_pos.1));
                        cam_args.insert("z".to_string(), LLSDValue::Real(loc.camera_pos.2));
                        cam_args.insert("target_x".to_string(), LLSDValue::Real(loc.camera_target.0));
                        cam_args.insert("target_y".to_string(), LLSDValue::Real(loc.camera_target.1));
                        cam_args.insert("target_z".to_string(), LLSDValue::Real(loc.camera_target.2));
                        cam_args.insert("fov".to_string(), LLSDValue::Real(loc.camera_fov));
                        let _ = client.send("LLCamera", cam_args);
                        actions_done.push(format!("Camera restored to FOV {:.1}°", loc.camera_fov));
                    }

                    McpCallResult::success(format!("Restored bookmark '{}': {}", loc.name, actions_done.join(", ")))
                }
                None => McpCallResult::error(format!("No saved location found matching '{query}'.")),
            }
        }

        "aurora_get_radar" => {
            if !client.is_connected() {
                return McpCallResult::error("Aurora-Viewer is not connected.");
            }
            let mut args_map = HashMap::new();
            args_map.insert("op".to_string(), LLSDValue::String("getNearby".to_string()));
            match client.request("LLNearbyAvatars", args_map, Duration::from_secs(5)) {
                Ok(reply) => {
                    let json_rep = llsd_to_json(&reply);
                    McpCallResult::success_json(&json_rep)
                }
                Err(e) => McpCallResult::error(format!("Failed to retrieve radar avatars: {e}")),
            }
        }

        "aurora_get_objects" => {
            if !client.is_connected() {
                return McpCallResult::error("Aurora-Viewer is not connected.");
            }
            let max_dist = args.get("max_distance").and_then(|v| v.as_f64()).unwrap_or(64.0);
            let mut req = HashMap::new();
            req.insert("op".to_string(), LLSDValue::String("getNearbyObjects".to_string()));
            req.insert("max_distance".to_string(), LLSDValue::Real(max_dist));

            match client.request("LLObjects", req, Duration::from_secs(5)) {
                Ok(reply) => {
                    let json_rep = llsd_to_json(&reply);
                    McpCallResult::success_json(&json_rep)
                }
                Err(e) => McpCallResult::error(format!("Failed to query nearby objects: {e}")),
            }
        }

        "aurora_send_chat" => {
            if !client.is_connected() {
                return McpCallResult::error("Aurora-Viewer is not connected.");
            }
            let msg = match args.get("message").and_then(|v| v.as_str()) {
                Some(m) => m,
                None => return McpCallResult::error("Missing parameter 'message'"),
            };
            let channel = args.get("channel").and_then(|v| v.as_i64()).unwrap_or(0) as i32;
            let chat_type = args.get("chat_type").and_then(|v| v.as_str()).unwrap_or("say");

            let mut req = HashMap::new();
            req.insert("op".to_string(), LLSDValue::String("chat".to_string()));
            req.insert("message".to_string(), LLSDValue::String(msg.to_string()));
            req.insert("channel".to_string(), LLSDValue::Integer(channel));
            let type_str = match chat_type {
                "whisper" => "whisper",
                "shout" => "shout",
                _ => "normal",
            };
            req.insert("type".to_string(), LLSDValue::String(type_str.to_string()));

            match client.request("LLChat", req, Duration::from_secs(4)) {
                Ok(_) => McpCallResult::success(format!("Chat transmitted on channel {channel} ({chat_type}): \"{msg}\"")),
                Err(e) => McpCallResult::error(format!("Failed to send chat: {e}")),
            }
        }

        "aurora_get_recent_chat" => {
            let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(25) as usize;
            let events = client.get_events();
            let mut chat_lines = Vec::new();

            for ev in events {
                if ev.pump == "chat" {
                    if let LLSDValue::Map(_) = ev.data {
                        let from = get_str(&ev.data, "from").unwrap_or("Simulator").to_string();
                        let message = get_str(&ev.data, "message").unwrap_or("").to_string();
                        let channel = crate::codec::get_i32(&ev.data, "channel").unwrap_or(0);
                        if !message.is_empty() {
                            chat_lines.push(json!({
                                "timestamp": ev.timestamp,
                                "from": from,
                                "channel": channel,
                                "message": message,
                            }));
                        }
                    }
                }
            }

            let start_idx = chat_lines.len().saturating_sub(limit);
            let slice = &chat_lines[start_idx..];
            McpCallResult::success_json(&json!({ "chat": slice }))
        }

        "aurora_eval_script" => {
            let code = match args.get("code").and_then(|v| v.as_str()) {
                Some(c) => c,
                None => return McpCallResult::error("Missing parameter 'code'"),
            };

            let (event_tx, event_rx) = crossbeam_channel::unbounded();
            let cancel_flag = Arc::new(AtomicBool::new(false));
            let ctx = ScriptContext::new(client.clone(), event_tx, cancel_flag);
            let engine = create_script_engine(ctx);

            let mut scope = rhai::Scope::new();
            let start = std::time::Instant::now();
            let eval_res = engine.run_with_scope(&mut scope, code);
            let duration_ms = start.elapsed().as_millis() as u64;

            let mut logs = Vec::new();
            let mut total_assertions = 0;
            let mut passed_assertions = 0;
            let mut failed_assertions = 0;

            while let Ok(ev) = event_rx.try_recv() {
                match ev {
                    ScriptEvent::Log(msg) => {
                        logs.push(format!("[{:?}] {}", msg.level, msg.message));
                    }
                    ScriptEvent::Assertion(ast) => {
                        total_assertions += 1;
                        if ast.passed {
                            passed_assertions += 1;
                        } else {
                            failed_assertions += 1;
                        }
                    }
                    _ => {}
                }
            }

            match eval_res {
                Ok(_) => McpCallResult::success_json(&json!({
                    "status": "Script executed successfully",
                    "duration_ms": duration_ms,
                    "total_assertions": total_assertions,
                    "passed": passed_assertions,
                    "failed": failed_assertions,
                    "logs": logs
                })),
                Err(err) => McpCallResult::error(format!(
                    "Script execution error: {err}\nDuration: {duration_ms}ms\nLogs:\n{}",
                    logs.join("\n")
                )),
            }
        }

        "aurora_raw_llsd" => {
            if !client.is_connected() {
                return McpCallResult::error("Aurora-Viewer is not connected.");
            }
            let pump = match args.get("pump").and_then(|v| v.as_str()) {
                Some(p) => p,
                None => return McpCallResult::error("Missing parameter 'pump'"),
            };
            let data_val = match args.get("data") {
                Some(d) => d,
                None => return McpCallResult::error("Missing parameter 'data'"),
            };
            let timeout_sec = args.get("timeout_sec").and_then(|v| v.as_f64()).unwrap_or(5.0);

            let llsd_data = json_to_llsd(data_val);
            let map = match llsd_data {
                LLSDValue::Map(m) => m,
                other => {
                    let mut m = HashMap::new();
                    m.insert("value".to_string(), other);
                    m
                }
            };

            match client.request(pump, map, Duration::from_secs_f64(timeout_sec)) {
                Ok(reply) => {
                    let json_rep = llsd_to_json(&reply);
                    McpCallResult::success_json(&json_rep)
                }
                Err(e) => McpCallResult::error(format!("Raw LLSD request failed: {e}")),
            }
        }

        "aurora_get_notifications" => {
            if !client.is_connected() {
                return McpCallResult::error("Aurora-Viewer is not connected.");
            }
            match client.get_notifications() {
                Ok(notifs) => {
                    let json_val = serde_json::to_value(&notifs).unwrap_or_else(|_| json!([]));
                    McpCallResult::success_json(&json!({ "notifications": json_val }))
                }
                Err(e) => McpCallResult::error(format!("Failed to retrieve notifications: {e}")),
            }
        }

        "aurora_respond_notification" => {
            if !client.is_connected() {
                return McpCallResult::error("Aurora-Viewer is not connected.");
            }
            let id = match args.get("id").and_then(|v| v.as_f64()) {
                Some(i) => i,
                None => return McpCallResult::error("Missing parameter 'id'"),
            };
            let action = args.get("action").and_then(|v| v.as_str()).unwrap_or("accept");
            let button_index = args.get("button_index").and_then(|v| v.as_u64()).map(|b| b as usize);
            let text = args.get("text").and_then(|v| v.as_str());

            match client.respond_notification(id, action, button_index, text) {
                Ok(_) => McpCallResult::success(format!("Notification {id} responded with action '{action}'")),
                Err(e) => McpCallResult::error(format!("Failed to respond to notification {id}: {e}")),
            }
        }

        "aurora_create_object" => {
            if !client.is_connected() {
                return McpCallResult::error("Aurora-Viewer is not connected.");
            }
            let mut req = HashMap::new();
            req.insert("op".to_string(), LLSDValue::String("createObject".to_string()));
            if let Some(shape) = args.get("shape").and_then(|v| v.as_str()) {
                req.insert("shape".to_string(), LLSDValue::String(shape.to_string()));
            }
            if let Some(pos) = args.get("position").and_then(|v| v.as_array()) {
                let arr = pos.iter().filter_map(|n| n.as_f64()).map(LLSDValue::Real).collect();
                req.insert("position".to_string(), LLSDValue::Array(arr));
            }
            if let Some(scale) = args.get("scale").and_then(|v| v.as_array()) {
                let arr = scale.iter().filter_map(|n| n.as_f64()).map(LLSDValue::Real).collect();
                req.insert("scale".to_string(), LLSDValue::Array(arr));
            } else if let Some(s) = args.get("scale").and_then(|v| v.as_f64()) {
                req.insert("scale".to_string(), LLSDValue::Real(s));
            }
            if let Some(mat) = args.get("material").and_then(|v| v.as_i64()) {
                req.insert("material".to_string(), LLSDValue::Integer(mat as i32));
            }

            match client.request("LLObjects", req, Duration::from_secs(5)) {
                Ok(reply) => McpCallResult::success_json(&llsd_to_json(&reply)),
                Err(e) => McpCallResult::error(format!("Failed to create object: {e}")),
            }
        }

        "aurora_delete_object" => {
            if !client.is_connected() {
                return McpCallResult::error("Aurora-Viewer is not connected.");
            }
            let mut req = HashMap::new();
            req.insert("op".to_string(), LLSDValue::String("deleteObject".to_string()));
            if let Some(id) = args.get("local_id").and_then(|v| v.as_i64()) {
                req.insert("local_id".to_string(), LLSDValue::Integer(id as i32));
            }
            if let Some(uuid) = args.get("uuid").and_then(|v| v.as_str()) {
                req.insert("uuid".to_string(), LLSDValue::String(uuid.to_string()));
            }

            match client.request("LLObjects", req, Duration::from_secs(5)) {
                Ok(reply) => McpCallResult::success_json(&llsd_to_json(&reply)),
                Err(e) => McpCallResult::error(format!("Failed to delete object: {e}")),
            }
        }

        "aurora_set_object_transform" => {
            if !client.is_connected() {
                return McpCallResult::error("Aurora-Viewer is not connected.");
            }
            let mut req = HashMap::new();
            req.insert("op".to_string(), LLSDValue::String("setTransform".to_string()));
            if let Some(id) = args.get("local_id").and_then(|v| v.as_i64()) {
                req.insert("local_id".to_string(), LLSDValue::Integer(id as i32));
            }
            if let Some(uuid) = args.get("uuid").and_then(|v| v.as_str()) {
                req.insert("uuid".to_string(), LLSDValue::String(uuid.to_string()));
            }
            if let Some(pos) = args.get("position").and_then(|v| v.as_array()) {
                let arr = pos.iter().filter_map(|n| n.as_f64()).map(LLSDValue::Real).collect();
                req.insert("position".to_string(), LLSDValue::Array(arr));
            }
            if let Some(scale) = args.get("scale").and_then(|v| v.as_array()) {
                let arr = scale.iter().filter_map(|n| n.as_f64()).map(LLSDValue::Real).collect();
                req.insert("scale".to_string(), LLSDValue::Array(arr));
            }
            if let Some(rot) = args.get("rotation").and_then(|v| v.as_array()) {
                let arr = rot.iter().filter_map(|n| n.as_f64()).map(LLSDValue::Real).collect();
                req.insert("rotation".to_string(), LLSDValue::Array(arr));
            }

            match client.request("LLObjects", req, Duration::from_secs(5)) {
                Ok(reply) => McpCallResult::success_json(&llsd_to_json(&reply)),
                Err(e) => McpCallResult::error(format!("Failed to transform object: {e}")),
            }
        }

        "aurora_select_object" => {
            if !client.is_connected() {
                return McpCallResult::error("Aurora-Viewer is not connected.");
            }
            let mut req = HashMap::new();
            req.insert("op".to_string(), LLSDValue::String("selectObject".to_string()));
            if let Some(id) = args.get("local_id").and_then(|v| v.as_i64()) {
                req.insert("local_id".to_string(), LLSDValue::Integer(id as i32));
            }
            if let Some(uuid) = args.get("uuid").and_then(|v| v.as_str()) {
                req.insert("uuid".to_string(), LLSDValue::String(uuid.to_string()));
            }

            match client.request("LLObjects", req, Duration::from_secs(5)) {
                Ok(reply) => McpCallResult::success_json(&llsd_to_json(&reply)),
                Err(e) => McpCallResult::error(format!("Failed to select object: {e}")),
            }
        }

        "aurora_deselect_object" => {
            if !client.is_connected() {
                return McpCallResult::error("Aurora-Viewer is not connected.");
            }
            let mut req = HashMap::new();
            req.insert("op".to_string(), LLSDValue::String("deselect".to_string()));
            if let Some(id) = args.get("local_id").and_then(|v| v.as_i64()) {
                req.insert("local_id".to_string(), LLSDValue::Integer(id as i32));
            }
            if let Some(uuid) = args.get("uuid").and_then(|v| v.as_str()) {
                req.insert("uuid".to_string(), LLSDValue::String(uuid.to_string()));
            }

            match client.request("LLObjects", req, Duration::from_secs(5)) {
                Ok(reply) => McpCallResult::success_json(&llsd_to_json(&reply)),
                Err(e) => McpCallResult::error(format!("Failed to deselect: {e}")),
            }
        }

        "aurora_set_object_name" => {
            if !client.is_connected() {
                return McpCallResult::error("Aurora-Viewer is not connected.");
            }
            let name_str = match args.get("name").and_then(|v| v.as_str()) {
                Some(n) => n,
                None => return McpCallResult::error("Missing parameter 'name'"),
            };
            let mut req = HashMap::new();
            req.insert("op".to_string(), LLSDValue::String("setName".to_string()));
            req.insert("name".to_string(), LLSDValue::String(name_str.to_string()));
            if let Some(id) = args.get("local_id").and_then(|v| v.as_i64()) {
                req.insert("local_id".to_string(), LLSDValue::Integer(id as i32));
            }
            if let Some(uuid) = args.get("uuid").and_then(|v| v.as_str()) {
                req.insert("uuid".to_string(), LLSDValue::String(uuid.to_string()));
            }

            match client.request("LLObjects", req, Duration::from_secs(5)) {
                Ok(reply) => McpCallResult::success_json(&llsd_to_json(&reply)),
                Err(e) => McpCallResult::error(format!("Failed to set object name: {e}")),
            }
        }

        "aurora_duplicate_object" => {
            if !client.is_connected() {
                return McpCallResult::error("Aurora-Viewer is not connected.");
            }
            let mut req = HashMap::new();
            req.insert("op".to_string(), LLSDValue::String("duplicate".to_string()));
            if let Some(id) = args.get("local_id").and_then(|v| v.as_i64()) {
                req.insert("local_id".to_string(), LLSDValue::Integer(id as i32));
            }
            if let Some(uuid) = args.get("uuid").and_then(|v| v.as_str()) {
                req.insert("uuid".to_string(), LLSDValue::String(uuid.to_string()));
            }
            if let Some(off) = args.get("offset").and_then(|v| v.as_array()) {
                let arr = off.iter().filter_map(|n| n.as_f64()).map(LLSDValue::Real).collect();
                req.insert("offset".to_string(), LLSDValue::Array(arr));
            }

            match client.request("LLObjects", req, Duration::from_secs(5)) {
                Ok(reply) => McpCallResult::success_json(&llsd_to_json(&reply)),
                Err(e) => McpCallResult::error(format!("Failed to duplicate object: {e}")),
            }
        }

        unknown => McpCallResult::error(format!("Unknown tool: '{unknown}'")),
    }
}
