//! LEAP Client Manager for Aurora-Viewer.
//!
//! Spawns and manages the `aurora-viewer.exe` process, coordinates background I/O
//! threads over stdio pipes, correlates request/response messages by `reqid`,
//! and buffers live stderr output and LEAP event streams.

use std::collections::{HashMap, VecDeque};
use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use chrono::Local;
use crossbeam_channel::{bounded, unbounded, Sender};
use serde_llsd::LLSDValue;

use crate::codec::{encode_frame, LeapDecoder, LeapMessage};

/// Maximum number of LEAP protocol frames retained in the in-memory blackbox ring buffer.
pub const BLACKBOX_CAPACITY: usize = 250;

/// A recorded blackbox frame of LEAP protocol traffic.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BlackboxFrame {
    pub timestamp: String,
    pub direction: String, // "outbound" or "inbound"
    pub pump: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub latency_us: Option<u64>,
    pub data: serde_json::Value,
}

/// Recorded macro action.
#[derive(Debug, Clone)]
pub struct MacroRecord {
    pub timestamp: Instant,
    pub code_line: String,
}

/// Macro recorder state for generating Rhai automated test scripts from user GUI interactions.
pub struct MacroRecorderState {
    pub is_recording: AtomicBool,
    pub start_time: Mutex<Option<Instant>>,
    pub last_event_time: Mutex<Option<Instant>>,
    pub records: Mutex<Vec<MacroRecord>>,
}

impl Default for MacroRecorderState {
    fn default() -> Self {
        Self {
            is_recording: AtomicBool::new(false),
            start_time: Mutex::new(None),
            last_event_time: Mutex::new(None),
            records: Mutex::new(Vec::new()),
        }
    }
}

/// A timestamped log entry from viewer stderr.
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: String,
    pub text: String,
}

/// A timestamped event received from Aurora-Viewer over LEAP.
#[derive(Debug, Clone)]
pub struct EventEntry {
    pub id: u64,
    pub timestamp: String,
    pub pump: String,
    pub data: LLSDValue,
    pub notation_preview: String,
}

/// Information obtained during the LEAP handshake.
#[derive(Debug, Clone, Default)]
pub struct HandshakeInfo {
    pub cmd_uuid: String,
    pub reply_uuid: String,
    pub features: HashMap<String, LLSDValue>,
    pub received_at: Option<String>,
}

/// Reactive telemetry state updated automatically by inbound LEAP events.
#[derive(Debug, Clone, Default)]
pub struct CachedTelemetry {
    pub startup_state: Option<String>,
    pub login_state: Option<String>,
    pub login_progress: Option<(String, f32)>,
    pub agent_id: Option<String>,
    pub agent_pos: Option<(f64, f64, f64)>,
    pub agent_yaw: Option<f64>,
    pub region_name: Option<String>,
    pub region_id: Option<String>,
    pub water_height: Option<f64>,
    pub parcel_name: Option<String>,
    pub parcel_area: Option<i32>,
    pub sim_fps: Option<f64>,
    pub nearby_avatars_count: Option<usize>,
    pub slurl: Option<String>,
    pub flying: Option<bool>,
    pub camera_pos: Option<(f64, f64, f64)>,
    pub camera_target: Option<(f64, f64, f64)>,
    pub camera_fov: Option<f64>,
}

/// Status of the viewer process connection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProcessStatus {
    Disconnected,
    Starting,
    Connected { pid: u32 },
    Terminated { exit_code: Option<i32> },
}

/// Thread-safe state shared between background threads and the GUI.
struct ClientInner {
    status: Mutex<ProcessStatus>,
    handshake: Mutex<Option<HandshakeInfo>>,
    telemetry: Mutex<CachedTelemetry>,
    pending_replies: Mutex<HashMap<String, Sender<LLSDValue>>>,
    events: Mutex<VecDeque<EventEntry>>,
    logs: Mutex<VecDeque<LogEntry>>,
    subscriptions: Mutex<HashMap<String, String>>, // pump -> listener_name
    sent_counter: AtomicU64,
    recv_counter: AtomicU64,
    event_id_counter: AtomicU64,
    reqid_counter: AtomicU64,

    // Milestone 6.5 & 6.7: Blackbox Flight Recorder & Macro Recorder
    blackbox: Mutex<VecDeque<BlackboxFrame>>,
    outbound_req_times: Mutex<HashMap<String, Instant>>,
    macro_recorder: MacroRecorderState,
}

impl ClientInner {
    fn record_blackbox(&self, direction: &str, pump: &str, latency_us: Option<u64>, data: &LLSDValue) {
        let timestamp = Local::now().format("%Y-%m-%dT%H:%M:%S%.3f").to_string();
        let json_val = crate::mcp::types::llsd_to_json(data);
        let mut bb = self.blackbox.lock().unwrap();
        if bb.len() >= BLACKBOX_CAPACITY {
            bb.pop_front();
        }
        bb.push_back(BlackboxFrame {
            timestamp,
            direction: direction.to_string(),
            pump: pump.to_string(),
            latency_us,
            data: json_val,
        });
    }

    fn record_macro(&self, pump: &str, data: &LLSDValue) {
        if !self.macro_recorder.is_recording.load(Ordering::Relaxed) {
            return;
        }

        let code = format_macro_action(pump, data);
        let now = Instant::now();
        let mut last_lock = self.macro_recorder.last_event_time.lock().unwrap();
        let delta_ms = if let Some(last) = *last_lock {
            now.duration_since(last).as_millis() as u64
        } else {
            0
        };
        *last_lock = Some(now);
        drop(last_lock);

        let mut recs = self.macro_recorder.records.lock().unwrap();
        if delta_ms >= 120 {
            recs.push(MacroRecord {
                timestamp: now,
                code_line: format!("sleep({});", delta_ms),
            });
        }
        recs.push(MacroRecord {
            timestamp: now,
            code_line: code,
        });
    }
}

fn format_macro_action(pump: &str, data: &LLSDValue) -> String {
    if let LLSDValue::Map(ref map) = data {
        if pump == "LLMovement" {
            let mut parts = Vec::new();
            for (k, v) in map {
                if k == "reply" || k == "reqid" {
                    continue;
                }
                parts.push(format!("\"{}\": {}", k, format_llsd_val_simple(v)));
            }
            return format!("leap_send(\"LLMovement\", #{{ {} }});", parts.join(", "));
        }

        if pump == "LLChat" {
            if let Some(msg) = map.get("message").and_then(|v| v.as_string()) {
                let channel = map.get("channel").and_then(|v| v.as_integer()).copied().unwrap_or(0);
                return format!("chat_say(\"{}\", {});", msg.replace('\\', "\\\\").replace('"', "\\\""), channel);
            }
        }

        if pump == "LLCamera" {
            if let Some(op) = map.get("op").and_then(|v| v.as_string()) {
                if op == "look_at" {
                    let x = crate::codec::get_f64(data, "x").unwrap_or(128.0);
                    let y = crate::codec::get_f64(data, "y").unwrap_or(128.0);
                    let z = crate::codec::get_f64(data, "z").unwrap_or(30.0);
                    return format!("camera_look_at({x:.2}, {y:.2}, {z:.2});");
                } else if op == "set" {
                    let cx = crate::codec::get_f64(data, "x").unwrap_or(124.0);
                    let cy = crate::codec::get_f64(data, "y").unwrap_or(128.0);
                    let cz = crate::codec::get_f64(data, "z").unwrap_or(28.0);
                    let tx = crate::codec::get_f64(data, "target_x").unwrap_or(128.0);
                    let ty = crate::codec::get_f64(data, "target_y").unwrap_or(128.0);
                    let tz = crate::codec::get_f64(data, "target_z").unwrap_or(26.0);
                    let fov = crate::codec::get_f64(data, "fov").unwrap_or(60.0);
                    return format!("camera_set({cx:.2}, {cy:.2}, {cz:.2}, {tx:.2}, {ty:.2}, {tz:.2}, {fov:.1});");
                }
            }
        }

        if pump == "LLAgent" {
            if let Some(op) = map.get("op").and_then(|v| v.as_string()) {
                if op == "teleport" {
                    let region = map.get("region").and_then(|v| v.as_string()).map(|s| s.as_str()).unwrap_or("");
                    let x = crate::codec::get_f64(data, "x").unwrap_or(128.0);
                    let y = crate::codec::get_f64(data, "y").unwrap_or(128.0);
                    let z = crate::codec::get_f64(data, "z").unwrap_or(30.0);
                    return format!("agent_teleport(\"{region}\", {x:.1}, {y:.1}, {z:.1});");
                }
            }
        }

        // Generic pump fallback
        let mut parts = Vec::new();
        for (k, v) in map {
            if k == "reply" || k == "reqid" {
                continue;
            }
            parts.push(format!("\"{}\": {}", k, format_llsd_val_simple(v)));
        }
        format!("leap_send(\"{pump}\", #{{ {} }});", parts.join(", "))
    } else {
        format!("leap_send(\"{pump}\", #{{}});")
    }
}

fn format_llsd_val_simple(val: &LLSDValue) -> String {
    match val {
        LLSDValue::Undefined => "()".to_string(),
        LLSDValue::Boolean(b) => b.to_string(),
        LLSDValue::Integer(i) => i.to_string(),
        LLSDValue::Real(r) => format!("{r:.3}"),
        LLSDValue::String(s) => format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\"")),
        LLSDValue::UUID(u) => format!("\"{}\"", u),
        LLSDValue::Map(m) => {
            let mut parts = Vec::new();
            for (k, v) in m {
                if k == "reply" || k == "reqid" {
                    continue;
                }
                parts.push(format!("\"{}\": {}", k, format_llsd_val_simple(v)));
            }
            format!("#{{ {} }}", parts.join(", "))
        }
        LLSDValue::Array(a) => {
            let parts: Vec<String> = a.iter().map(format_llsd_val_simple).collect();
            format!("[{}]", parts.join(", "))
        }
        _ => "()".to_string(),
    }
}

/// A handle to an in-flight asynchronous LEAP request that can be polled without blocking the UI thread.
pub struct AsyncRequest<T> {
    rx: crossbeam_channel::Receiver<Result<T, String>>,
}

impl<T> AsyncRequest<T> {
    /// Poll for completion without blocking the UI render thread.
    /// Returns Some(Ok(T)) if completed successfully, Some(Err(e)) on error/timeout,
    /// or None if the background request is still in progress.
    pub fn poll(&self) -> Option<Result<T, String>> {
        match self.rx.try_recv() {
            Ok(res) => Some(res),
            Err(crossbeam_channel::TryRecvError::Empty) => None,
            Err(crossbeam_channel::TryRecvError::Disconnected) => Some(Err("Request channel closed".to_string())),
        }
    }
}

/// Notification item received from LLNotifications subsystem.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct NotificationItem {
    pub id: f64,
    pub id_str: String,
    pub kind: String,
    pub title: String,
    pub body: String,
    pub interactive: bool,
    pub stays_until_answered: bool,
    pub notif_type: Option<String>,
    pub buttons: Vec<String>,
    pub channel: Option<i32>,
    pub questions: Option<i32>,
    pub permission_names: Vec<String>,
    pub url: Option<String>,
}

impl NotificationItem {
    pub fn list_from_llsd(reply: &LLSDValue) -> Vec<Self> {
        let mut items = Vec::new();
        if let Some(list) = reply.as_map().and_then(|m| m.get("notifications")).and_then(|v| v.as_array()) {
            for v in list {
                if let Some(m) = v.as_map() {
                    let id = m.get("id").and_then(crate::codec::val_to_f64).unwrap_or(0.0);
                    let id_str = m.get("id_str").and_then(|s| s.as_string()).cloned().unwrap_or_else(|| id.to_string());
                    let kind = m.get("kind").and_then(|s| s.as_string()).cloned().unwrap_or_default();
                    let title = m.get("title").and_then(|s| s.as_string()).cloned().unwrap_or_default();
                    let body = m.get("body").and_then(|s| s.as_string()).cloned().unwrap_or_default();
                    let interactive = m.get("interactive").and_then(|b| b.as_boolean()).copied().unwrap_or(false);
                    let stays_until_answered = m.get("stays_until_answered").and_then(|b| b.as_boolean()).copied().unwrap_or(false);
                    let notif_type = m.get("type").and_then(|s| s.as_string()).cloned();
                    let buttons = m.get("buttons").and_then(|b| b.as_array()).map(|arr| {
                        arr.iter().filter_map(|x| x.as_string().cloned()).collect()
                    }).unwrap_or_default();
                    let channel = m.get("channel").and_then(|c| c.as_integer()).copied();
                    let questions = m.get("questions").and_then(|q| q.as_integer()).copied();
                    let permission_names = m.get("permission_names").and_then(|p| p.as_array()).map(|arr| {
                        arr.iter().filter_map(|x| x.as_string().cloned()).collect()
                    }).unwrap_or_default();
                    let url = m.get("url").and_then(|u| u.as_string()).cloned();

                    items.push(NotificationItem {
                        id,
                        id_str,
                        kind,
                        title,
                        body,
                        interactive,
                        stays_until_answered,
                        notif_type,
                        buttons,
                        channel,
                        questions,
                        permission_names,
                        url,
                    });
                }
            }
        }
        items
    }
}

/// Main LEAP client controller.
#[derive(Clone)]
pub struct LeapClient {
    inner: Arc<ClientInner>,
    outbound_tx: Sender<Vec<u8>>,
    stop_signal: Arc<AtomicBool>,
    child_handle: Arc<Mutex<Option<Child>>>,
    _threads: Arc<Mutex<Vec<JoinHandle<()>>>>,
}

impl LeapClient {
    pub fn new() -> Self {
        let (outbound_tx, _) = unbounded();
        Self {
            inner: Arc::new(ClientInner {
                status: Mutex::new(ProcessStatus::Disconnected),
                handshake: Mutex::new(None),
                telemetry: Mutex::new(CachedTelemetry::default()),
                pending_replies: Mutex::new(HashMap::new()),
                events: Mutex::new(VecDeque::with_capacity(1000)),
                logs: Mutex::new(VecDeque::with_capacity(2000)),
                subscriptions: Mutex::new(HashMap::new()),
                sent_counter: AtomicU64::new(0),
                recv_counter: AtomicU64::new(0),
                event_id_counter: AtomicU64::new(0),
                reqid_counter: AtomicU64::new(1000),
                blackbox: Mutex::new(VecDeque::with_capacity(BLACKBOX_CAPACITY)),
                outbound_req_times: Mutex::new(HashMap::new()),
                macro_recorder: MacroRecorderState::default(),
            }),
            outbound_tx,
            stop_signal: Arc::new(AtomicBool::new(false)),
            child_handle: Arc::new(Mutex::new(None)),
            _threads: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Check if currently connected.
    pub fn is_connected(&self) -> bool {
        matches!(*self.inner.status.lock().unwrap(), ProcessStatus::Connected { .. })
    }

    /// Get PID of child viewer process if connected.
    pub fn child_pid(&self) -> Option<u32> {
        match self.status() {
            ProcessStatus::Connected { pid } => Some(pid),
            _ => None,
        }
    }

    /// Get current status.
    pub fn status(&self) -> ProcessStatus {
        self.inner.status.lock().unwrap().clone()
    }

    /// Get handshake info if established.
    pub fn handshake(&self) -> Option<HandshakeInfo> {
        self.inner.handshake.lock().unwrap().clone()
    }

    /// Get reactive cached telemetry (updated live from inbound LEAP packets with zero wait).
    pub fn cached_telemetry(&self) -> CachedTelemetry {
        self.inner.telemetry.lock().unwrap().clone()
    }

    /// Alias for cached_telemetry.
    pub fn get_telemetry(&self) -> CachedTelemetry {
        self.cached_telemetry()
    }

    /// Get total sent messages count.
    pub fn sent_count(&self) -> u64 {
        self.inner.sent_counter.load(Ordering::Relaxed)
    }

    /// Get total received messages count.
    pub fn recv_count(&self) -> u64 {
        self.inner.recv_counter.load(Ordering::Relaxed)
    }

    /// Get recent logs (up to max items).
    pub fn get_logs(&self) -> Vec<LogEntry> {
        self.inner.logs.lock().unwrap().iter().cloned().collect()
    }

    /// Clear logs.
    pub fn clear_logs(&self) {
        self.inner.logs.lock().unwrap().clear();
    }

    /// Get recent events (up to max items).
    pub fn get_events(&self) -> Vec<EventEntry> {
        self.inner.events.lock().unwrap().iter().cloned().collect()
    }

    /// Clear events.
    pub fn clear_events(&self) {
        self.inner.events.lock().unwrap().clear();
    }

    /// Get active pump subscriptions.
    pub fn get_subscriptions(&self) -> Vec<(String, String)> {
        self.inner.subscriptions.lock().unwrap().iter().map(|(k, v)| (k.clone(), v.clone())).collect()
    }

    /// Allocate next incremental request ID.
    pub fn next_reqid(&self) -> i32 {
        self.inner.reqid_counter.fetch_add(1, Ordering::SeqCst) as i32
    }

    /// Convenience method to spawn and connect using space-separated argument string.
    pub fn spawn_and_connect(&mut self, exe_path: &Path, extra_args_str: &str) -> Result<(), String> {
        let args: Vec<String> = extra_args_str
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();
        self.connect(exe_path, &args, None)
    }

    /// Launch `aurora-viewer.exe` as a subprocess and establish LEAP stdio communication.
    pub fn connect(
        &mut self,
        exe_path: &Path,
        extra_args: &[String],
        cwd: Option<&Path>,
    ) -> Result<(), String> {
        self.disconnect();

        if !exe_path.exists() {
            return Err(format!("Executable does not exist at: {}", exe_path.display()));
        }

        *self.inner.status.lock().unwrap() = ProcessStatus::Starting;
        self.stop_signal.store(false, Ordering::SeqCst);

        let mut cmd = Command::new(exe_path);
        cmd.arg("--leap-stdio");
        cmd.env("AURORA_LEAP_GUI", "1");
        cmd.env("AURORA_NO_CRASH_DIALOG", "1");
        for arg in extra_args {
            cmd.arg(arg);
        }

        if let Some(dir) = cwd {
            cmd.current_dir(dir);
        } else if let Some(parent) = exe_path.parent() {
            // Default working directory to repo root if possible
            if let Some(grandparent) = parent.parent().and_then(|p| p.parent()) {
                cmd.current_dir(grandparent);
            } else {
                cmd.current_dir(parent);
            }
        }

        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| format!("Failed to spawn {}: {e}", exe_path.display()))?;
        let pid = child.id();

        let stdin = child.stdin.take().ok_or("Failed to open child stdin")?;
        let stdout = child.stdout.take().ok_or("Failed to open child stdout")?;
        let stderr = child.stderr.take().ok_or("Failed to open child stderr")?;

        *self.child_handle.lock().unwrap() = Some(child);

        let (outbound_tx, outbound_rx) = unbounded::<Vec<u8>>();
        self.outbound_tx = outbound_tx;

        let stop_signal = self.stop_signal.clone();
        let inner = self.inner.clone();

        // 1. Stdin Writer Thread
        let stop_writer = stop_signal.clone();
        let mut stdin_writer = stdin;
        let writer_handle = thread::Builder::new()
            .name("leap-writer".to_string())
            .spawn(move || {
                while !stop_writer.load(Ordering::Relaxed) {
                    match outbound_rx.recv_timeout(Duration::from_millis(50)) {
                        Ok(frame) => {
                            if stdin_writer.write_all(&frame).is_err() || stdin_writer.flush().is_err() {
                                break;
                            }
                        }
                        Err(crossbeam_channel::RecvTimeoutError::Timeout) => {}
                        Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                    }
                }
            })
            .map_err(|e| format!("Failed to spawn writer thread: {e}"))?;

        // 2. Stderr Reader Thread
        let stop_stderr = stop_signal.clone();
        let inner_stderr = inner.clone();
        let stderr_reader = std::io::BufReader::new(stderr);
        let stderr_handle = thread::Builder::new()
            .name("leap-stderr".to_string())
            .spawn(move || {
                use std::io::BufRead;
                for line in stderr_reader.lines() {
                    if stop_stderr.load(Ordering::Relaxed) {
                        break;
                    }
                    if let Ok(l) = line {
                        let timestamp = Local::now().format("%H:%M:%S%.3f").to_string();
                        let mut logs = inner_stderr.logs.lock().unwrap();
                        if logs.len() >= 2000 {
                            logs.pop_front();
                        }
                        logs.push_back(LogEntry { timestamp, text: l });
                    }
                }
            })
            .map_err(|e| format!("Failed to spawn stderr thread: {e}"))?;

        // 3. Stdout Reader Thread
        let stop_reader = stop_signal.clone();
        let inner_reader = inner.clone();
        let mut stdout_reader = stdout;
        let reader_handle = thread::Builder::new()
            .name("leap-reader".to_string())
            .spawn(move || {
                let mut decoder = LeapDecoder::new();
                let mut chunk_buf = [0u8; 8192];

                while !stop_reader.load(Ordering::Relaxed) {
                    match stdout_reader.read(&mut chunk_buf) {
                        Ok(0) => break, // EOF
                        Ok(n) => {
                            decoder.feed(&chunk_buf[..n]);
                            loop {
                                match decoder.decode_next() {
                                    Ok(Some(msg)) => {
                                        inner_reader.recv_counter.fetch_add(1, Ordering::Relaxed);
                                        Self::dispatch_inbound(&inner_reader, msg);
                                    }
                                    Ok(None) => break,
                                    Err(err) => {
                                        let timestamp = Local::now().format("%H:%M:%S%.3f").to_string();
                                        let mut logs = inner_reader.logs.lock().unwrap();
                                        logs.push_back(LogEntry {
                                            timestamp,
                                            text: format!("[LEAP Decoder Error] {err}"),
                                        });
                                        decoder.clear();
                                        break;
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            let timestamp = Local::now().format("%H:%M:%S%.3f").to_string();
                            let mut logs = inner_reader.logs.lock().unwrap();
                            logs.push_back(LogEntry {
                                timestamp,
                                text: format!("[LEAP Read Error] {e}"),
                            });
                            break;
                        }
                    }
                }

                *inner_reader.status.lock().unwrap() = ProcessStatus::Terminated { exit_code: None };
            })
            .map_err(|e| format!("Failed to spawn stdout reader thread: {e}"))?;

        let mut threads = self._threads.lock().unwrap();
        threads.push(writer_handle);
        threads.push(stderr_handle);
        threads.push(reader_handle);

        *self.inner.status.lock().unwrap() = ProcessStatus::Connected { pid };

        // Wait up to 10 seconds for initial handshake frame
        let t0 = Instant::now();
        while t0.elapsed() < Duration::from_secs(10) {
            if self.inner.handshake.lock().unwrap().is_some() {
                return Ok(());
            }
            thread::sleep(Duration::from_millis(50));
        }

        Err("Timed out waiting for LEAP initial handshake from Aurora-Viewer".to_string())
    }

    /// Dispatch inbound LEAP messages to pending replies and event buffers.
    fn dispatch_inbound(inner: &Arc<ClientInner>, msg: LeapMessage) {
        let timestamp = Local::now().format("%H:%M:%S%.3f").to_string();
        let preview = crate::codec::to_clean_notation(&msg.data).unwrap_or_else(|_| format!("{:?}", msg.data));

        // 1. Check for initial handshake packet:
        // Envelope: {"pump": reply_uuid, "data": {"command": cmd_uuid, "features": {}}}
        {
            let mut handshake_lock = inner.handshake.lock().unwrap();
            if handshake_lock.is_none() {
                if let LLSDValue::Map(ref map) = msg.data {
                    if let Some(LLSDValue::String(cmd_uuid)) = map.get("command") {
                        let features = match map.get("features") {
                            Some(LLSDValue::Map(f)) => f.clone(),
                            _ => HashMap::new(),
                        };
                        *handshake_lock = Some(HandshakeInfo {
                            cmd_uuid: cmd_uuid.clone(),
                            reply_uuid: msg.pump.clone(),
                            features,
                            received_at: Some(timestamp.clone()),
                        });
                    }
                }
            }
        }

        // 2. Correlate reqid if response has one
        let mut latency_us = None;
        if let LLSDValue::Map(ref map) = msg.data {
            if let Some(reqid_val) = map.get("reqid") {
                let reqid_key = match reqid_val {
                    LLSDValue::Integer(i) => i.to_string(),
                    LLSDValue::String(s) => s.clone(),
                    other => format!("{other:?}"),
                };

                let mut pending = inner.pending_replies.lock().unwrap();
                if let Some(tx) = pending.remove(&reqid_key) {
                    let _ = tx.send(msg.data.clone());
                }

                let mut req_times = inner.outbound_req_times.lock().unwrap();
                if let Some(t0) = req_times.remove(&reqid_key) {
                    latency_us = Some(t0.elapsed().as_micros() as u64);
                }
            }
        }

        // Record inbound frame in Blackbox Flight Recorder
        inner.record_blackbox("inbound", &msg.pump, latency_us, &msg.data);

        // 3. Buffer event into recent events list
        let event_id = inner.event_id_counter.fetch_add(1, Ordering::Relaxed);
        let mut events = inner.events.lock().unwrap();
        if events.len() >= 1000 {
            events.pop_front();
        }
        events.push_back(EventEntry {
            id: event_id,
            timestamp,
            pump: msg.pump.clone(),
            data: msg.data.clone(),
            notation_preview: preview,
        });

        // 4. Update reactive cached telemetry
        {
            let mut telem = inner.telemetry.lock().unwrap();
            match msg.pump.as_str() {
                "LLStartUp" | "StartupState" => {
                    if let Some(s) = crate::codec::get_str(&msg.data, "state") {
                        telem.startup_state = Some(s.to_string());
                    }
                }
                "LLLogin" | "login" => {
                    if let Some(ev) = crate::codec::get_str(&msg.data, "event") {
                        match ev {
                            "LoggedIn" => {
                                telem.startup_state = Some("STATE_STARTED".to_string());
                                telem.login_state = Some("LoggedIn".to_string());
                                if let Some(id) = crate::codec::get_str(&msg.data, "agent_id") {
                                    telem.agent_id = Some(id.to_string());
                                }
                            }
                            "LoginProgress" => {
                                let msg_str = crate::codec::get_str(&msg.data, "message").unwrap_or("").to_string();
                                let frac = crate::codec::get_f64(&msg.data, "fraction").unwrap_or(0.0) as f32;
                                telem.login_progress = Some((msg_str, frac));
                            }
                            "LoginFailed" => {
                                telem.startup_state = Some("STATE_LOGIN_FAILED".to_string());
                                telem.login_state = Some("LoginFailed".to_string());
                            }
                            _ => {}
                        }
                    }
                }
                "teleport" | "LLTeleport" => {
                    if let Some(ev) = crate::codec::get_str(&msg.data, "event") {
                        if ev == "TeleportFinished" {
                            if let Some(fl) = crate::codec::get_bool(&msg.data, "flying") {
                                telem.flying = Some(fl);
                            }
                        }
                    }
                }
                "camera" | "LLCamera" => {
                    if let Some(pos) = crate::codec::parse_vec3(&msg.data, Some("position")) {
                        telem.camera_pos = Some(pos);
                    }
                    if let Some(tgt) = crate::codec::parse_vec3(&msg.data, Some("target")) {
                        telem.camera_target = Some(tgt);
                    }
                    if let Some(fov) = crate::codec::get_f64(&msg.data, "fov") {
                        let deg = if fov > 0.0 && fov < 3.2 { fov.to_degrees() } else { fov };
                        telem.camera_fov = Some(deg);
                    }
                }
                _ => {}
            }

            // Also inspect correlated responses for agent / camera / region telemetry
            if let LLSDValue::Map(ref map) = msg.data {
                if let Some(reg) = map.get("region").or_else(|| map.get("region_name")).and_then(|v| v.as_string()) {
                    if !reg.is_empty() {
                        telem.region_name = Some(reg.to_string());
                    }
                }
                if let Some(sl) = map.get("slurl").and_then(|v| v.as_string()) {
                    if !sl.is_empty() {
                        telem.slurl = Some(sl.to_string());
                    }
                }
                // Agent coordinates: inspect responses from LLAgent or containing agent_id/yaw/pos_global/in_world
                let is_agent_msg = map.contains_key("agent_id") || map.contains_key("pos_global") || map.contains_key("yaw") || map.contains_key("in_world") || map.contains_key("logged_in") || msg.pump == "agent" || msg.pump == "LLAgent";
                if is_agent_msg {
                    if let Some(in_world) = map.get("in_world").and_then(|v| v.as_boolean()).or_else(|| map.get("logged_in").and_then(|v| v.as_boolean())) {
                        if !*in_world {
                            telem.agent_pos = None;
                        }
                    }
                    if let Some(pos) = crate::codec::parse_vec3(&msg.data, Some("position")) {
                        if pos.0 != 0.0 || pos.1 != 0.0 || pos.2 != 0.0 {
                            telem.agent_pos = Some(pos);
                        }
                    } else if let (Some(x), Some(y), Some(z)) = (
                        map.get("x").and_then(crate::codec::val_to_f64),
                        map.get("y").and_then(crate::codec::val_to_f64),
                        map.get("z").and_then(crate::codec::val_to_f64),
                    ) {
                        if x != 0.0 || y != 0.0 || z != 0.0 {
                            telem.agent_pos = Some((x, y, z));
                        }
                    }
                }

                // Camera coordinates: inspect responses from LLCamera or containing camera_pos/target/target_x/fov
                let is_cam_msg = map.contains_key("camera_pos") || map.contains_key("camera_target") || map.contains_key("target") || map.contains_key("target_x") || map.contains_key("fov") || msg.pump == "camera" || msg.pump == "LLCamera";
                if is_cam_msg {
                    if let Some(pos) = crate::codec::parse_vec3(&msg.data, Some("camera_pos"))
                        .or_else(|| crate::codec::parse_vec3(&msg.data, Some("position")))
                    {
                        telem.camera_pos = Some(pos);
                    }
                    if let Some(tgt) = crate::codec::parse_vec3(&msg.data, Some("camera_target"))
                        .or_else(|| crate::codec::parse_vec3(&msg.data, Some("target")))
                    {
                        telem.camera_target = Some(tgt);
                    }
                    if let Some(fov) = crate::codec::get_f64(&msg.data, "fov") {
                        let deg = if fov > 0.0 && fov < 3.2 { fov.to_degrees() } else { fov };
                        telem.camera_fov = Some(deg);
                    }
                }
                if let Some(yaw) = map.get("yaw").and_then(crate::codec::val_to_f64) {
                    telem.agent_yaw = Some(yaw);
                }
                if let Some(fl) = map.get("flying").and_then(|v| v.as_boolean()) {
                    telem.flying = Some(*fl);
                }
                if let Some(id) = map.get("agent_id").and_then(|v| v.as_string()) {
                    telem.agent_id = Some(id.to_string());
                }
                if let Some(r_id) = map.get("region_id").and_then(|v| v.as_string()) {
                    if !r_id.is_empty() {
                        telem.region_id = Some(r_id.to_string());
                    }
                }
                if let Some(wh) = map.get("water_height").and_then(crate::codec::val_to_f64) {
                    telem.water_height = Some(wh);
                }
                if let Some(p_name) = map.get("parcel_name").and_then(|v| v.as_string()) {
                    if !p_name.is_empty() {
                        telem.parcel_name = Some(p_name.to_string());
                    }
                }
                if let Some(area) = map.get("area").and_then(|v| v.as_integer()) {
                    telem.parcel_area = Some(*area);
                }
                if let Some(fps) = map.get("fps").and_then(crate::codec::val_to_f64) {
                    telem.sim_fps = Some(fps);
                }
                if let Some(cnt) = map.get("count").and_then(|v| v.as_integer()) {
                    if msg.pump.contains("atar") || msg.pump.contains("radar") {
                        telem.nearby_avatars_count = Some(*cnt as usize);
                    }
                }
                if let Some(st) = map.get("state").and_then(|v| v.as_string()) {
                    telem.startup_state = Some(st.to_string());
                }
            }
        }
    }

    /// Send a raw message without waiting for reply.
    pub fn send_message(&self, pump: impl Into<String>, data: LLSDValue) -> Result<(), String> {
        let pump_str = pump.into();
        let msg = LeapMessage::new(pump_str.clone(), data.clone());
        let frame = encode_frame(&msg)?;

        // Record in Blackbox Flight Recorder and Macro Recorder
        self.inner.record_blackbox("outbound", &pump_str, None, &data);
        self.inner.record_macro(&pump_str, &data);

        let _ = self.outbound_tx.send(frame);
        self.inner.sent_counter.fetch_add(1, Ordering::Relaxed);

        Ok(())
    }

    /// Asynchronously send a LEAP message map without waiting for a reply.
    pub fn send(&self, pump: impl Into<String>, data: HashMap<String, LLSDValue>) -> Result<(), String> {
        self.send_message(pump, LLSDValue::Map(data))
    }

    /// Send a request and wait synchronously for the correlated reply by reqid.
    pub fn request(
        &self,
        pump: impl Into<String>,
        mut data: HashMap<String, LLSDValue>,
        timeout: Duration,
    ) -> Result<LLSDValue, String> {
        let reqid = self.next_reqid();
        let reqid_str = reqid.to_string();

        let reply_pump = self.handshake().map(|h| h.reply_uuid).unwrap_or_else(|| "reply".to_string());
        data.insert("reply".to_string(), LLSDValue::String(reply_pump));
        data.insert("reqid".to_string(), LLSDValue::Integer(reqid));

        let (reply_tx, reply_rx) = bounded::<LLSDValue>(1);
        {
            let mut pending = self.inner.pending_replies.lock().unwrap();
            pending.insert(reqid_str.clone(), reply_tx);
            let mut req_times = self.inner.outbound_req_times.lock().unwrap();
            req_times.insert(reqid_str.clone(), Instant::now());
        }

        let send_res = self.send_message(pump, LLSDValue::Map(data));
        if let Err(e) = send_res {
            self.inner.pending_replies.lock().unwrap().remove(&reqid_str);
            self.inner.outbound_req_times.lock().unwrap().remove(&reqid_str);
            return Err(e);
        }

        match reply_rx.recv_timeout(timeout) {
            Ok(reply) => Ok(reply),
            Err(crossbeam_channel::RecvTimeoutError::Timeout) => {
                self.inner.pending_replies.lock().unwrap().remove(&reqid_str);
                self.inner.outbound_req_times.lock().unwrap().remove(&reqid_str);
                Err(format!("Request timed out after {:?}", timeout))
            }
            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => {
                self.inner.pending_replies.lock().unwrap().remove(&reqid_str);
                self.inner.outbound_req_times.lock().unwrap().remove(&reqid_str);
                Err("LEAP client disconnected while waiting for reply".to_string())
            }
        }
    }

    /// Asynchronously send a request in a background thread and return an AsyncRequest handle.
    /// The calling GUI thread can poll this handle on every frame with zero blocking.
    pub fn request_async<F, T>(
        &self,
        pump: impl Into<String>,
        data: HashMap<String, LLSDValue>,
        timeout: Duration,
        mapper: F,
    ) -> AsyncRequest<T>
    where
        F: FnOnce(LLSDValue) -> Result<T, String> + Send + 'static,
        T: Send + 'static,
    {
        let client = self.clone();
        let pump = pump.into();
        let (tx, rx) = bounded(1);

        thread::Builder::new()
            .name("leap-async-req".to_string())
            .spawn(move || {
                let res = client.request(pump, data, timeout).and_then(mapper);
                let _ = tx.send(res);
            })
            .ok();

        AsyncRequest { rx }
    }

    /// Convenience async request returning raw LLSDValue without custom mapping.
    pub fn request_raw_async(
        &self,
        pump: impl Into<String>,
        data: HashMap<String, LLSDValue>,
        timeout: Duration,
    ) -> AsyncRequest<LLSDValue> {
        self.request_async(pump, data, timeout, Ok)
    }

    /// Send a command to the core LLLeapListener command pump.
    pub fn command(
        &self,
        op: &str,
        mut extra_args: HashMap<String, LLSDValue>,
        timeout: Duration,
    ) -> Result<LLSDValue, String> {
        let cmd_pump = self.handshake().map(|h| h.cmd_uuid).ok_or("No LEAP command pump UUID available")?;
        extra_args.insert("op".to_string(), LLSDValue::String(op.to_string()));
        self.request(cmd_pump, extra_args, timeout)
    }

    /// Send ping to command pump and verify roundtrip.
    pub fn ping(&self, timeout: Duration) -> Result<LLSDValue, String> {
        self.command("ping", HashMap::new(), timeout)
    }

    /// Register a listener on an event pump via `listen`.
    pub fn listen(&self, source_pump: &str, listener_name: &str) -> Result<bool, String> {
        let mut args = HashMap::new();
        args.insert("source".to_string(), LLSDValue::String(source_pump.to_string()));
        args.insert("listener".to_string(), LLSDValue::String(listener_name.to_string()));

        let reply = self.command("listen", args, Duration::from_secs(5))?;
        let status = crate::codec::get_bool(&reply, "status").unwrap_or(false);
        if status {
            self.inner.subscriptions.lock().unwrap().insert(source_pump.to_string(), listener_name.to_string());
        }
        Ok(status)
    }

    /// Unregister a listener via `stoplistening`.
    pub fn stop_listening(&self, source_pump: &str, listener_name: &str) -> Result<bool, String> {
        let mut args = HashMap::new();
        args.insert("source".to_string(), LLSDValue::String(source_pump.to_string()));
        args.insert("listener".to_string(), LLSDValue::String(listener_name.to_string()));

        let reply = self.command("stoplistening", args, Duration::from_secs(5))?;
        let status = crate::codec::get_bool(&reply, "status").unwrap_or(false);
        self.inner.subscriptions.lock().unwrap().remove(source_pump);
        Ok(status)
    }

    /// Terminate viewer and clean up background threads.
    pub fn disconnect(&mut self) {
        self.stop_signal.store(true, Ordering::SeqCst);

        // Try graceful quit first
        if self.is_connected() {
            let mut quit_args = HashMap::new();
            quit_args.insert("op".to_string(), LLSDValue::String("requestQuit".to_string()));
            let _ = self.send_message("LLAppViewer", LLSDValue::Map(quit_args));
            
            // Poll for graceful exit up to 2.5 seconds before forceful kill
            let t0 = Instant::now();
            while t0.elapsed() < Duration::from_millis(2500) {
                if let Some(ref mut child) = *self.child_handle.lock().unwrap() {
                    if let Ok(Some(_)) = child.try_wait() {
                        break;
                    }
                } else {
                    break;
                }
                thread::sleep(Duration::from_millis(50));
            }
        }

        // Kill process if still alive
        if let Some(mut child) = self.child_handle.lock().unwrap().take() {
            if child.try_wait().ok().flatten().is_none() {
                let _ = child.kill();
            }
            let exit_code = child.wait().ok().and_then(|st| st.code());
            *self.inner.status.lock().unwrap() = ProcessStatus::Terminated { exit_code };
        } else {
            *self.inner.status.lock().unwrap() = ProcessStatus::Disconnected;
        }

        *self.inner.handshake.lock().unwrap() = None;
        self.inner.pending_replies.lock().unwrap().clear();
        self.inner.subscriptions.lock().unwrap().clear();
    }

    // ── Milestone 6.5: Blackbox Flight Recorder API ────────────────────────
    /// Retrieve all recorded blackbox frames from the in-memory circular ring buffer.
    pub fn get_blackbox_frames(&self) -> Vec<BlackboxFrame> {
        self.inner.blackbox.lock().unwrap().iter().cloned().collect()
    }

    /// Clear all recorded blackbox frames.
    pub fn clear_blackbox(&self) {
        self.inner.blackbox.lock().unwrap().clear();
    }

    /// Export a full Blackbox Incident Bundle (.zip) for diagnostic capture.
    /// The archive contains:
    /// - `leap_traffic.jsonl`: chronological raw protocol traces with latency
    /// - `viewer_output.log`: captured stderr/stdout console lines
    /// - `system_info.json`: client telemetry, OS environment, PID, and counters
    pub fn export_incident_bundle(&self, output_path: &Path) -> Result<(), String> {
        use zip::write::SimpleFileOptions;
        use zip::ZipWriter;

        let file = std::fs::File::create(output_path)
            .map_err(|e| format!("Failed to create bundle file at '{}': {e}", output_path.display()))?;
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated);

        // 1. leap_traffic.jsonl
        zip.start_file("leap_traffic.jsonl", options)
            .map_err(|e| format!("Failed to create 'leap_traffic.jsonl' in zip: {e}"))?;
        {
            let frames = self.get_blackbox_frames();
            for frame in frames {
                if let Ok(line) = serde_json::to_string(&frame) {
                    zip.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
                    zip.write_all(b"\n").map_err(|e| e.to_string())?;
                }
            }
        }

        // 2. viewer_output.log
        zip.start_file("viewer_output.log", options)
            .map_err(|e| format!("Failed to create 'viewer_output.log' in zip: {e}"))?;
        {
            let logs = self.get_logs();
            for entry in logs {
                let line = format!("[{}] {}\n", entry.timestamp, entry.text);
                zip.write_all(line.as_bytes()).map_err(|e| e.to_string())?;
            }
        }

        // 3. system_info.json
        zip.start_file("system_info.json", options)
            .map_err(|e| format!("Failed to create 'system_info.json' in zip: {e}"))?;
        {
            let telem = self.cached_telemetry();
            let handshake = self.handshake();
            let info = serde_json::json!({
                "bundle_timestamp": Local::now().to_rfc3339(),
                "os": std::env::consts::OS,
                "arch": std::env::consts::ARCH,
                "family": std::env::consts::FAMILY,
                "app_version": env!("CARGO_PKG_VERSION"),
                "child_pid": self.child_pid(),
                "process_status": format!("{:?}", self.status()),
                "frames_sent": self.sent_count(),
                "frames_received": self.recv_count(),
                "handshake": handshake.map(|h| serde_json::json!({
                    "cmd_uuid": h.cmd_uuid,
                    "reply_uuid": h.reply_uuid,
                    "received_at": h.received_at,
                })),
                "telemetry": serde_json::json!({
                    "agent_id": telem.agent_id,
                    "agent_pos": telem.agent_pos,
                    "agent_yaw": telem.agent_yaw,
                    "flying": telem.flying,
                    "region_name": telem.region_name,
                    "region_id": telem.region_id,
                    "sim_fps": telem.sim_fps,
                    "nearby_avatars_count": telem.nearby_avatars_count,
                    "camera_pos": telem.camera_pos,
                    "camera_target": telem.camera_target,
                    "camera_fov": telem.camera_fov,
                })
            });
            let info_bytes = serde_json::to_vec_pretty(&info).map_err(|e| e.to_string())?;
            zip.write_all(&info_bytes).map_err(|e| e.to_string())?;
        }

        zip.finish().map_err(|e| format!("Failed to finalize zip bundle: {e}"))?;
        Ok(())
    }

    // ── Milestone 8.2: Dialog Notifications & Permissions Subsystem ──────────
    /// Query all active pending notifications, alerts, and script dialogs from viewer.
    pub fn get_notifications(&self) -> Result<Vec<NotificationItem>, String> {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("getNotifications".to_string()));
        let reply = self.request("LLNotifications", args, Duration::from_secs(3))?;
        Ok(NotificationItem::list_from_llsd(&reply))
    }

    /// Respond to or answer a notification by ID.
    pub fn respond_notification(&self, id: f64, action: &str, button_index: Option<usize>, text: Option<&str>) -> Result<(), String> {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("respond".to_string()));
        args.insert("id".to_string(), LLSDValue::Real(id));
        args.insert("response".to_string(), LLSDValue::String(action.to_string()));
        if let Some(idx) = button_index {
            args.insert("button_index".to_string(), LLSDValue::Integer(idx as i32));
        }
        if let Some(t) = text {
            args.insert("text".to_string(), LLSDValue::String(t.to_string()));
        }
        let reply = self.request("LLNotifications", args, Duration::from_secs(3))?;
        if crate::codec::get_bool(&reply, "status").unwrap_or(false) {
            Ok(())
        } else {
            Err(crate::codec::get_str(&reply, "error").unwrap_or("Failed to respond to notification").to_string())
        }
    }

    /// Dismiss or cancel a notification by ID.
    pub fn dismiss_notification(&self, id: f64) -> Result<(), String> {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("dismiss".to_string()));
        args.insert("id".to_string(), LLSDValue::Real(id));
        let reply = self.request("LLNotifications", args, Duration::from_secs(3))?;
        if crate::codec::get_bool(&reply, "status").unwrap_or(false) {
            Ok(())
        } else {
            Err(crate::codec::get_str(&reply, "error").unwrap_or("Failed to dismiss notification").to_string())
        }
    }

    // ── Milestone 6.7: Rhai Script Macro Recorder API ───────────────────────
    /// Check if macro recording is currently running.
    pub fn is_macro_recording(&self) -> bool {
        self.inner.macro_recorder.is_recording.load(Ordering::Relaxed)
    }

    /// Number of actions captured in the current macro session.
    pub fn macro_action_count(&self) -> usize {
        self.inner.macro_recorder.records.lock().unwrap().len()
    }

    /// Start recording manual interactions as an automated Rhai test script.
    pub fn start_macro_recording(&self) {
        self.inner.macro_recorder.is_recording.store(true, Ordering::SeqCst);
        let now = Instant::now();
        *self.inner.macro_recorder.start_time.lock().unwrap() = Some(now);
        *self.inner.macro_recorder.last_event_time.lock().unwrap() = Some(now);
        self.inner.macro_recorder.records.lock().unwrap().clear();
    }

    /// Stop macro recording and generate a formatted Rhai script draft.
    pub fn stop_macro_recording(&self) -> String {
        self.inner.macro_recorder.is_recording.store(false, Ordering::SeqCst);
        let records = self.inner.macro_recorder.records.lock().unwrap();
        let count = records.len();
        let total_secs = self.inner.macro_recorder.start_time.lock().unwrap()
            .map(|t| t.elapsed().as_secs_f64()).unwrap_or(0.0);

        let mut script = String::new();
        script.push_str("// ── Aurora LEAP Recorded Macro ───────────────────────────────\n");
        script.push_str(&format!("// Recorded: {}\n", Local::now().format("%Y-%m-%d %H:%M:%S")));
        script.push_str(&format!("// Actions: {} | Total Duration: {:.1}s\n\n", count, total_secs));
        script.push_str("print(\"▶ Replaying recorded macro...\");\n\n");

        for r in records.iter() {
            script.push_str(&r.code_line);
            script.push('\n');
        }

        script.push_str("\nprint(\"✔ Recorded macro replay finished successfully.\");\n");
        script
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_blackbox_circular_buffer_capacity() {
        let client = LeapClient::new();
        for i in 0..300 {
            let mut data = HashMap::new();
            data.insert("index".to_string(), LLSDValue::Integer(i));
            let _ = client.send_message("TestPump", LLSDValue::Map(data));
        }

        let frames = client.get_blackbox_frames();
        assert_eq!(frames.len(), BLACKBOX_CAPACITY);
        // Latest frame should have index 299
        let last_val = &frames.last().unwrap().data["index"];
        assert_eq!(last_val, &serde_json::json!(299));
    }

    #[test]
    fn test_macro_recorder_lifecycle_and_generation() {
        let client = LeapClient::new();
        assert!(!client.is_macro_recording());

        client.start_macro_recording();
        assert!(client.is_macro_recording());

        // Send simulated movements & camera actions
        let mut move_data = HashMap::new();
        move_data.insert("forward".to_string(), LLSDValue::Integer(1));
        let _ = client.send("LLMovement", move_data);

        let mut cam_data = HashMap::new();
        cam_data.insert("op".to_string(), LLSDValue::String("look_at".to_string()));
        cam_data.insert("x".to_string(), LLSDValue::Real(130.0));
        cam_data.insert("y".to_string(), LLSDValue::Real(135.0));
        cam_data.insert("z".to_string(), LLSDValue::Real(32.0));
        let _ = client.send("LLCamera", cam_data);

        assert_eq!(client.macro_action_count(), 2);

        let script = client.stop_macro_recording();
        assert!(!client.is_macro_recording());

        assert!(script.contains("leap_send(\"LLMovement\""));
        assert!(script.contains("camera_look_at(130.00, 135.00, 32.00);"));
        assert!(script.contains("Recorded Macro"));
    }

    #[test]
    fn test_export_incident_bundle_zip() {
        let client = LeapClient::new();

        // Put simulated data in blackbox
        let mut data = HashMap::new();
        data.insert("op".to_string(), LLSDValue::String("ping".to_string()));
        let _ = client.send_message("LLWorld", LLSDValue::Map(data));

        // Create temp zip path
        let temp_dir = std::env::temp_dir();
        let zip_path = temp_dir.join(format!("test_incident_bundle_{}.zip", uuid::Uuid::new_v4()));

        let res = client.export_incident_bundle(&zip_path);
        assert!(res.is_ok(), "Export bundle failed: {:?}", res.err());
        assert!(zip_path.exists());

        // Verify zip contents
        let file = std::fs::File::open(&zip_path).unwrap();
        let mut archive = zip::ZipArchive::new(file).unwrap();
        assert!(archive.by_name("leap_traffic.jsonl").is_ok());
        assert!(archive.by_name("viewer_output.log").is_ok());
        assert!(archive.by_name("system_info.json").is_ok());

        let _ = std::fs::remove_file(zip_path);
    }

    #[test]
    fn test_live_viewer_camera_and_agent_telemetry() {
        use std::path::PathBuf;

        let candidate_paths = [
            PathBuf::from("../Aurora-Viewer/target/debug/aurora-viewer.exe"),
            PathBuf::from("Aurora-Viewer/target/debug/aurora-viewer.exe"),
            PathBuf::from("../../Aurora-Viewer/target/debug/aurora-viewer.exe"),
        ];

        let mut exe_path = None;
        for p in candidate_paths {
            if p.exists() {
                exe_path = Some(p.canonicalize().unwrap());
                break;
            }
        }

        let exe = match exe_path {
            Some(e) => e,
            None => return,
        };

        let mut client = LeapClient::new();
        client.spawn_and_connect(&exe, "").expect("Failed to connect to aurora-viewer");

        // 1. Test LLCamera.set
        let mut set_args = HashMap::new();
        set_args.insert("op".to_string(), LLSDValue::String("set".to_string()));
        set_args.insert("x".to_string(), LLSDValue::Real(99.0));
        set_args.insert("y".to_string(), LLSDValue::Real(88.0));
        set_args.insert("z".to_string(), LLSDValue::Real(77.0));
        set_args.insert("target_x".to_string(), LLSDValue::Real(100.0));
        set_args.insert("target_y".to_string(), LLSDValue::Real(100.0));
        set_args.insert("target_z".to_string(), LLSDValue::Real(50.0));
        set_args.insert("fov".to_string(), LLSDValue::Real(45.0));

        let set_reply = client.request("LLCamera", set_args, Duration::from_secs(15)).expect("LLCamera.set failed");
        assert_eq!(crate::codec::get_bool(&set_reply, "status"), Some(true));

        // 2. Test LLCamera.get: verify manual_view is preserved and reported
        let mut get_args = HashMap::new();
        get_args.insert("op".to_string(), LLSDValue::String("get".to_string()));
        let get_reply = client.request("LLCamera", get_args, Duration::from_secs(10)).expect("LLCamera.get failed");
        assert_eq!(crate::codec::get_bool(&get_reply, "status"), Some(true));
        assert_eq!(crate::codec::get_bool(&get_reply, "is_manual"), Some(true));
        let pos = crate::codec::parse_vec3(&get_reply, Some("position")).expect("position in get reply");
        assert!((pos.0 - 99.0).abs() < 0.01 && (pos.1 - 88.0).abs() < 0.01 && (pos.2 - 77.0).abs() < 0.01);

        // 3. Test LLAgent.get
        let mut agent_args = HashMap::new();
        agent_args.insert("op".to_string(), LLSDValue::String("get".to_string()));
        let agent_reply = client.request("LLAgent", agent_args, Duration::from_secs(5)).expect("LLAgent.get failed");
        assert!(agent_reply.as_map().is_some());

        client.disconnect();
    }
}
