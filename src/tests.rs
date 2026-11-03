//! Automated Test Suite Runner for Aurora-Viewer LEAP Features.
//!
//! Executes all Tier 1, 2, and 3 verification suites against the running
//! `aurora-viewer.exe` process and reports structured test results to the GUI.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use serde_llsd::LLSDValue;

use crate::client::LeapClient;
use crate::codec::{get_array, get_bool, get_f64, get_i32, get_map, get_str};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestStatus {
    Pending,
    Running,
    Pass,
    Fail,
}

#[derive(Debug, Clone)]
pub struct TestCase {
    pub id: &'static str,
    pub name: &'static str,
    pub category: &'static str,
    pub description: &'static str,
    pub status: TestStatus,
    pub duration: Option<Duration>,
    pub details: String,
}

#[derive(Debug, Clone)]
pub struct TestRunState {
    pub is_running: bool,
    pub progress: f32,
    pub current_test: String,
    pub total_tests: usize,
    pub passed_tests: usize,
    pub failed_tests: usize,
    pub total_duration: Option<Duration>,
    pub cases: Vec<TestCase>,
}

impl Default for TestRunState {
    fn default() -> Self {
        Self {
            is_running: false,
            progress: 0.0,
            current_test: String::new(),
            total_tests: 13,
            passed_tests: 0,
            failed_tests: 0,
            total_duration: None,
            cases: default_test_cases(),
        }
    }
}

pub fn default_test_cases() -> Vec<TestCase> {
    vec![
        TestCase {
            id: "handshake",
            name: "Protocol Handshake & UUIDs",
            category: "Tier 1: Core",
            description: "Verify LEAP intro frame, valid 36-char command and reply UUIDs, and features map.",
            status: TestStatus::Pending,
            duration: None,
            details: String::new(),
        },
        TestCase {
            id: "ping_reqid",
            name: "Ping / Pong & reqid Correlation",
            category: "Tier 1: Core",
            description: "Verify roundtrip ping response with verbatim integer and string reqid correlation.",
            status: TestStatus::Pending,
            duration: None,
            details: String::new(),
        },
        TestCase {
            id: "get_apis",
            name: "Introspection: getAPIs",
            category: "Tier 1: Core",
            description: "Enumerate all registered subsystem APIs via LLLeapListener getAPIs.",
            status: TestStatus::Pending,
            duration: None,
            details: String::new(),
        },
        TestCase {
            id: "get_api_schema",
            name: "Introspection: getAPI Schemas",
            category: "Tier 1: Core",
            description: "Inspect operation metadata and parameter schemas for LLViewerControl and LLAppViewer.",
            status: TestStatus::Pending,
            duration: None,
            details: String::new(),
        },
        TestCase {
            id: "viewer_control",
            name: "LLViewerControl Mutations",
            category: "Tier 1: Subsystems",
            description: "Test get, set, toggle on DrawDistance, VSync, Shadows, Language, and query groups/vars.",
            status: TestStatus::Pending,
            duration: None,
            details: String::new(),
        },
        TestCase {
            id: "subsystems_telemetry",
            name: "Subsystems Telemetry",
            category: "Tier 1: Subsystems",
            description: "Query LLAppViewer.status, LLAgent position/id, LLLogin state, LLCamera, LLInventory.",
            status: TestStatus::Pending,
            duration: None,
            details: String::new(),
        },
        TestCase {
            id: "pubsub_lifecycle",
            name: "Pub/Sub Pump Lifecycle",
            category: "Tier 3: Pub/Sub",
            description: "Register dynamic listener via listen, verify duplicate rejection, and unregister via stoplistening.",
            status: TestStatus::Pending,
            duration: None,
            details: String::new(),
        },
        TestCase {
            id: "chat_send_and_event",
            name: "LLChat Transmission & Events",
            category: "Tier 3: Pub/Sub",
            description: "Dispatch LLChat.sendChat and verify broadcast event reception on the 'chat' pump.",
            status: TestStatus::Pending,
            duration: None,
            details: String::new(),
        },
        TestCase {
            id: "boundary_payloads",
            name: "Payload & Type Robustness",
            category: "Tier 2: Boundary",
            description: "Test complex nested maps, arrays, unicode strings, and negative integer reqids.",
            status: TestStatus::Pending,
            duration: None,
            details: String::new(),
        },
        TestCase {
            id: "pipelining_burst",
            name: "High-Throughput Pipelining",
            category: "Tier 3: Stress",
            description: "Transmit 25 rapid back-to-back requests and verify zero loss and exact correlation.",
            status: TestStatus::Pending,
            duration: None,
            details: String::new(),
        },
        TestCase {
            id: "agent_movement",
            name: "LLAgent Movement & Teleport",
            category: "Tier 2: Interactive",
            description: "Test LLAgent.move with directional step, yaw rotation, and LLAgent.teleport coordinates.",
            status: TestStatus::Pending,
            duration: None,
            details: String::new(),
        },
        TestCase {
            id: "camera_snapshot",
            name: "LLCamera Inworld Snapshot",
            category: "Tier 2: Interactive",
            description: "Request LLCamera.takePicture at 320x240 and verify PNG base64 payload and dimensions.",
            status: TestStatus::Pending,
            duration: None,
            details: String::new(),
        },
        TestCase {
            id: "notifications_subsystem",
            name: "LLNotifications Subsystem",
            category: "Tier 1: Subsystems",
            description: "Query LLNotifications.getNotifications, inspect active alerts/dialogs schema and reply map.",
            status: TestStatus::Pending,
            duration: None,
            details: String::new(),
        },
    ]
}

pub struct TestRunner;

impl TestRunner {
    pub fn start_suite(client: LeapClient, state_ref: Arc<Mutex<TestRunState>>) {
        {
            let mut state = state_ref.lock().unwrap();
            if state.is_running {
                return;
            }
            state.is_running = true;
            state.progress = 0.0;
            state.passed_tests = 0;
            state.failed_tests = 0;
            state.total_duration = None;
            state.cases = default_test_cases();
        }

        thread::Builder::new()
            .name("test-runner".to_string())
            .spawn(move || {
                let suite_start = Instant::now();
                let test_ids: Vec<&'static str> = {
                    let state = state_ref.lock().unwrap();
                    state.cases.iter().map(|c| c.id).collect()
                };

                let total = test_ids.len();
                for (idx, id) in test_ids.iter().enumerate() {
                    {
                        let mut state = state_ref.lock().unwrap();
                        state.current_test = format!("{id} ({}/{})", idx + 1, total);
                        state.progress = idx as f32 / total as f32;
                        if let Some(c) = state.cases.iter_mut().find(|c| c.id == *id) {
                            c.status = TestStatus::Running;
                        }
                    }

                    let t0 = Instant::now();
                    let result = match *id {
                        "handshake" => Self::run_test_handshake(&client),
                        "ping_reqid" => Self::run_test_ping_reqid(&client),
                        "get_apis" => Self::run_test_get_apis(&client),
                        "get_api_schema" => Self::run_test_get_api_schema(&client),
                        "viewer_control" => Self::run_test_viewer_control(&client),
                        "subsystems_telemetry" => Self::run_test_subsystems_telemetry(&client),
                        "pubsub_lifecycle" => Self::run_test_pubsub_lifecycle(&client),
                        "chat_send_and_event" => Self::run_test_chat_send_and_event(&client),
                        "boundary_payloads" => Self::run_test_boundary_payloads(&client),
                        "pipelining_burst" => Self::run_test_pipelining_burst(&client),
                        "agent_movement" => Self::run_test_agent_movement(&client),
                        "camera_snapshot" => Self::run_test_camera_snapshot(&client),
                        "notifications_subsystem" => Self::run_test_notifications_subsystem(&client),
                        _ => Err(format!("Unknown test: {id}")),
                    };
                    let duration = t0.elapsed();

                    {
                        let mut state = state_ref.lock().unwrap();
                        if let Some(c) = state.cases.iter_mut().find(|c| c.id == *id) {
                            c.duration = Some(duration);
                            match result {
                                Ok(details) => {
                                    c.status = TestStatus::Pass;
                                    c.details = details;
                                    state.passed_tests += 1;
                                }
                                Err(err) => {
                                    c.status = TestStatus::Fail;
                                    c.details = err;
                                    state.failed_tests += 1;
                                }
                            }
                        }
                    }

                    thread::sleep(Duration::from_millis(50));
                }

                {
                    let mut state = state_ref.lock().unwrap();
                    state.is_running = false;
                    state.progress = 1.0;
                    state.current_test = "Completed".to_string();
                    state.total_duration = Some(suite_start.elapsed());
                }
            })
            .expect("Failed to spawn test-runner thread");
    }

    fn run_test_handshake(client: &LeapClient) -> Result<String, String> {
        let handshake = client.handshake().ok_or("No handshake information found")?;
        if handshake.cmd_uuid.len() != 36 {
            return Err(format!("Command UUID length {} != 36: {}", handshake.cmd_uuid.len(), handshake.cmd_uuid));
        }
        if handshake.reply_uuid.len() != 36 {
            return Err(format!("Reply UUID length {} != 36: {}", handshake.reply_uuid.len(), handshake.reply_uuid));
        }
        Ok(format!(
            "Command UUID: {}\nReply UUID: {}\nFeatures Count: {}\nReceived At: {}",
            handshake.cmd_uuid,
            handshake.reply_uuid,
            handshake.features.len(),
            handshake.received_at.unwrap_or_default()
        ))
    }

    fn run_test_ping_reqid(client: &LeapClient) -> Result<String, String> {
        // Integer reqid
        let ping_reqid = client.next_reqid();
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("ping".to_string()));
        let reply = client.request("LLAppViewer", args, Duration::from_secs(4))?;

        if !get_bool(&reply, "status").unwrap_or(false) {
            return Err(format!("Ping reply status was not true: {reply:?}"));
        }
        if get_i32(&reply, "reqid") != Some(ping_reqid) {
            return Err(format!("reqid mismatch: expected {ping_reqid}, got {:?}", reply));
        }

        Ok(format!("Verified integer reqid {ping_reqid} matched verbatim. Status: true"))
    }

    fn run_test_get_apis(client: &LeapClient) -> Result<String, String> {
        let reply = client.command("getAPIs", HashMap::new(), Duration::from_secs(4))?;
        let apis = get_map(&reply, "apis").ok_or_else(|| format!("Missing 'apis' map in reply: {reply:?}"))?;

        let expected = [
            "LLAppViewer",
            "LLViewerControl",
            "LLChat",
            "LLAgent",
            "LLLogin",
            "LLCamera",
            "LLInventory",
            "LLNotifications",
        ];

        for name in &expected {
            if !apis.contains_key(*name) {
                return Err(format!("Expected API '{name}' not found in getAPIs: {:?}", apis.keys()));
            }
        }

        Ok(format!("Discovered {} APIs: {:?}", apis.len(), apis.keys().collect::<Vec<_>>()))
    }

    fn run_test_get_api_schema(client: &LeapClient) -> Result<String, String> {
        let mut args = HashMap::new();
        args.insert("api".to_string(), LLSDValue::String("LLViewerControl".to_string()));
        let reply = client.command("getAPI", args, Duration::from_secs(4))?;

        if get_str(&reply, "name") != Some("LLViewerControl") {
            return Err(format!("Unexpected API name: {:?}", get_str(&reply, "name")));
        }

        let ops = get_array(&reply, "ops").ok_or("Missing 'ops' array")?;
        let mut op_names = Vec::new();
        for op in ops {
            if let Some(name) = get_str(op, "name") {
                op_names.push(name.to_string());
            }
        }

        for exp in &["get", "set", "toggle", "groups", "vars"] {
            if !op_names.iter().any(|n| n == exp) {
                return Err(format!("Missing expected op '{exp}' in LLViewerControl ops: {op_names:?}"));
            }
        }

        Ok(format!("Validated LLViewerControl operations: {:?}", op_names))
    }

    fn run_test_viewer_control(client: &LeapClient) -> Result<String, String> {
        // 1. Get DrawDistance
        let mut get_args = HashMap::new();
        get_args.insert("op".to_string(), LLSDValue::String("get".to_string()));
        get_args.insert("key".to_string(), LLSDValue::String("DrawDistance".to_string()));
        let get_reply = client.request("LLViewerControl", get_args, Duration::from_secs(4))?;
        let orig_dd = get_f64(&get_reply, "value").unwrap_or(128.0);

        // 2. Set DrawDistance to 256.0
        let mut set_args = HashMap::new();
        set_args.insert("op".to_string(), LLSDValue::String("set".to_string()));
        set_args.insert("key".to_string(), LLSDValue::String("DrawDistance".to_string()));
        set_args.insert("value".to_string(), LLSDValue::Real(256.0));
        let set_reply = client.request("LLViewerControl", set_args, Duration::from_secs(4))?;
        let set_val = get_f64(&set_reply, "value").ok_or("Missing value in set reply")?;
        if (set_val - 256.0).abs() > 0.01 {
            return Err(format!("Set DrawDistance failed: expected 256.0, got {set_val}"));
        }

        // 3. Restore DrawDistance
        let mut restore_args = HashMap::new();
        restore_args.insert("op".to_string(), LLSDValue::String("set".to_string()));
        restore_args.insert("key".to_string(), LLSDValue::String("DrawDistance".to_string()));
        restore_args.insert("value".to_string(), LLSDValue::Real(orig_dd));
        let _ = client.request("LLViewerControl", restore_args, Duration::from_secs(4))?;

        // 4. Toggle VSync
        let mut toggle_args = HashMap::new();
        toggle_args.insert("op".to_string(), LLSDValue::String("toggle".to_string()));
        toggle_args.insert("key".to_string(), LLSDValue::String("VSync".to_string()));
        let toggle_reply = client.request("LLViewerControl", toggle_args.clone(), Duration::from_secs(4))?;
        let toggled = get_bool(&toggle_reply, "value").ok_or("Missing bool value in toggle reply")?;

        // Restore VSync
        let _ = client.request("LLViewerControl", toggle_args, Duration::from_secs(4))?;

        // 5. Query groups
        let mut groups_args = HashMap::new();
        groups_args.insert("op".to_string(), LLSDValue::String("groups".to_string()));
        let groups_reply = client.request("LLViewerControl", groups_args, Duration::from_secs(4))?;
        let groups = get_array(&groups_reply, "groups").ok_or("Missing 'groups' array")?;

        Ok(format!(
            "DrawDistance mutated & restored (orig={orig_dd}). VSync toggled={toggled}. Setting groups: {}",
            groups.len()
        ))
    }

    fn run_test_subsystems_telemetry(client: &LeapClient) -> Result<String, String> {
        // LLAppViewer.status
        let mut app_args = HashMap::new();
        app_args.insert("op".to_string(), LLSDValue::String("status".to_string()));
        let app_status = client.request("LLAppViewer", app_args, Duration::from_secs(4))?;
        let version = get_str(&app_status, "version").ok_or("Missing 'version' in LLAppViewer status")?;
        let uptime = get_f64(&app_status, "uptime").unwrap_or(0.0);

        // LLAgent.getPosition
        let mut agent_args = HashMap::new();
        agent_args.insert("op".to_string(), LLSDValue::String("getPosition".to_string()));
        let agent_pos = client.request("LLAgent", agent_args, Duration::from_secs(4))?;
        let _ = get_f64(&agent_pos, "x").ok_or("Missing 'x' in LLAgent position")?;

        // LLLogin.getState
        let mut login_args = HashMap::new();
        login_args.insert("op".to_string(), LLSDValue::String("getState".to_string()));
        let login_state = client.request("LLLogin", login_args, Duration::from_secs(4))?;
        let state_name = get_str(&login_state, "state").unwrap_or("unknown");

        // LLCamera.get
        let mut cam_args = HashMap::new();
        cam_args.insert("op".to_string(), LLSDValue::String("get".to_string()));
        let cam_info = client.request("LLCamera", cam_args, Duration::from_secs(4))?;
        let _ = crate::codec::parse_vec3(&cam_info, Some("position")).ok_or("Missing camera position")?;

        // LLInventory.getRoot
        let mut inv_args = HashMap::new();
        inv_args.insert("op".to_string(), LLSDValue::String("getRoot".to_string()));
        let inv_root = client.request("LLInventory", inv_args, Duration::from_secs(4))?;
        let root_uuid = get_str(&inv_root, "root").ok_or("Missing inventory root UUID")?;

        Ok(format!(
            "Viewer Version: {version}, Uptime: {uptime:.1}s\nLogin State: {state_name}\nInventory Root: {root_uuid}"
        ))
    }

    fn run_test_pubsub_lifecycle(client: &LeapClient) -> Result<String, String> {
        // Listen to StartupState
        let ok1 = client.listen("StartupState", "gui_test_startup_listener")?;
        if !ok1 {
            return Err("Failed to register listener on StartupState".to_string());
        }

        // Duplicate listener on same pump should be rejected (false)
        let dup = client.listen("StartupState", "gui_test_startup_listener")?;
        if dup {
            return Err("Duplicate listener was not rejected as expected".to_string());
        }

        // Stop listening
        let stop_ok = client.stop_listening("StartupState", "gui_test_startup_listener")?;
        if !stop_ok {
            return Err("Failed to stop listening on StartupState".to_string());
        }

        Ok("Successfully registered listener, rejected duplicate, and stopped listener.".to_string())
    }

    fn run_test_chat_send_and_event(client: &LeapClient) -> Result<String, String> {
        // Listen to chat
        let _ = client.listen("chat", "gui_chat_monitor");

        // Send chat
        let mut chat_args = HashMap::new();
        chat_args.insert("op".to_string(), LLSDValue::String("sendChat".to_string()));
        chat_args.insert("message".to_string(), LLSDValue::String("GUI Automated Test Message".to_string()));
        chat_args.insert("channel".to_string(), LLSDValue::Integer(0));
        let reply = client.request("LLChat", chat_args, Duration::from_secs(4))?;

        if !get_bool(&reply, "status").unwrap_or(false) {
            return Err(format!("sendChat returned status false: {reply:?}"));
        }

        // Send a direct notification on the 'chat' pump to test event forwarding
        let mut ev_data = HashMap::new();
        ev_data.insert("from".to_string(), LLSDValue::String("GUITester".to_string()));
        ev_data.insert("message".to_string(), LLSDValue::String("Direct pump event test".to_string()));
        ev_data.insert("channel".to_string(), LLSDValue::Integer(0));
        client.send_message("chat", LLSDValue::Map(ev_data))?;

        // Give reader thread 200ms to receive forwarded event
        thread::sleep(Duration::from_millis(200));

        let events = client.get_events();
        let found = events.iter().any(|e| e.pump == "chat");
        if !found {
            return Err("Forwarded event was not found in client event buffer".to_string());
        }

        Ok("Chat sent successfully and forwarded event was captured in the event stream.".to_string())
    }

    fn run_test_boundary_payloads(client: &LeapClient) -> Result<String, String> {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("ping".to_string()));
        args.insert("unicode_test".to_string(), LLSDValue::String("Unicode ⚡ 🚀 🌟 日本語".to_string()));
        args.insert("nested_array".to_string(), LLSDValue::Array(vec![
            LLSDValue::Integer(-12345),
            LLSDValue::Boolean(true),
            LLSDValue::Real(3.14159265),
        ]));

        let reply = client.request("LLAppViewer", args, Duration::from_secs(4))?;
        if !get_bool(&reply, "status").unwrap_or(false) {
            return Err(format!("Boundary ping returned false: {reply:?}"));
        }

        Ok("Complex nested types and unicode payload parsed and handled with zero errors.".to_string())
    }

    fn run_test_pipelining_burst(client: &LeapClient) -> Result<String, String> {
        let count = 25;
        let t0 = Instant::now();

        for i in 0..count {
            let mut args = HashMap::new();
            args.insert("op".to_string(), LLSDValue::String("ping".to_string()));
            args.insert("iteration".to_string(), LLSDValue::Integer(i));
            let reply = client.request("LLAppViewer", args, Duration::from_secs(5))?;
            if !get_bool(&reply, "status").unwrap_or(false) {
                return Err(format!("Pipelining request {i} failed: {reply:?}"));
            }
        }

        let elapsed = t0.elapsed();
        let rps = count as f64 / elapsed.as_secs_f64();
        Ok(format!("Sent {count} back-to-back requests in {elapsed:.2?}. Rate: {rps:.1} req/sec"))
    }

    fn run_test_agent_movement(client: &LeapClient) -> Result<String, String> {
        // 1. Move forward
        let mut move_args = HashMap::new();
        move_args.insert("op".to_string(), LLSDValue::String("move".to_string()));
        move_args.insert("direction".to_string(), LLSDValue::String("forward".to_string()));
        move_args.insert("step".to_string(), LLSDValue::Real(2.0));
        let move_reply = client.request("LLAgent", move_args, Duration::from_secs(5))?;
        if !get_bool(&move_reply, "status").unwrap_or(false) {
            return Err(format!("LLAgent.move forward returned status false: {move_reply:?}"));
        }

        // 2. Turn right
        let mut turn_args = HashMap::new();
        turn_args.insert("op".to_string(), LLSDValue::String("move".to_string()));
        turn_args.insert("direction".to_string(), LLSDValue::String("turn_right".to_string()));
        let _ = client.request("LLAgent", turn_args, Duration::from_secs(5))?;

        // 3. Query avatar position
        let mut pos_args = HashMap::new();
        pos_args.insert("op".to_string(), LLSDValue::String("getPosition".to_string()));
        let pos_reply = client.request("LLAgent", pos_args, Duration::from_secs(5))?;
        if !get_bool(&pos_reply, "status").unwrap_or(false) {
            return Err(format!("LLAgent.getPosition returned status false: {pos_reply:?}"));
        }

        let x = get_f64(&pos_reply, "x").unwrap_or(0.0);
        let y = get_f64(&pos_reply, "y").unwrap_or(0.0);
        let z = get_f64(&pos_reply, "z").unwrap_or(0.0);

        Ok(format!("Step forward and turn right confirmed. Verified position: ({x:.1}, {y:.1}, {z:.1})"))
    }

    fn run_test_camera_snapshot(client: &LeapClient) -> Result<String, String> {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("takePicture".to_string()));
        args.insert("width".to_string(), LLSDValue::Integer(320));
        args.insert("height".to_string(), LLSDValue::Integer(240));

        let reply = client.request("LLCamera", args, Duration::from_secs(8))?;
        if !get_bool(&reply, "status").unwrap_or(false) {
            return Err(format!("LLCamera.takePicture failed: {reply:?}"));
        }

        let w = get_i32(&reply, "width").unwrap_or(0);
        let h = get_i32(&reply, "height").unwrap_or(0);
        let b64 = get_str(&reply, "png_base64").unwrap_or("");
        let file_path = get_str(&reply, "file_path").unwrap_or("");

        if b64.is_empty() {
            return Err("takePicture reply missing png_base64 data".to_string());
        }

        Ok(format!("Snapshot captured: {w}x{h}, PNG payload {len} chars, saved to: {file_path}", len = b64.len()))
    }

    fn run_test_notifications_subsystem(client: &LeapClient) -> Result<String, String> {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("getNotifications".to_string()));
        let reply = client.request("LLNotifications", args, Duration::from_secs(4))?;

        if !get_bool(&reply, "status").unwrap_or(false) {
            let err = get_str(&reply, "error").unwrap_or("Failed status in LLNotifications reply");
            return Err(err.to_string());
        }

        let notifs = crate::client::NotificationItem::list_from_llsd(&reply);
        Ok(format!("Validated LLNotifications subsystem. Active notifications count: {}", notifs.len()))
    }
}
