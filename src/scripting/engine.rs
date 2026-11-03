//! Rhai engine configuration and LEAP bindings.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crossbeam_channel::Sender;
use rhai::{Array, Dynamic, Engine, Map, FLOAT, INT};
use serde_llsd::LLSDValue;

use crate::client::LeapClient;
use crate::codec::{get_bool, get_f64, get_str, parse_vec3};
use crate::scripting::models::{AssertionRecord, LogLevel, ScriptLogMessage};

/// Events emitted during script execution to update the UI in real time.
#[derive(Debug, Clone)]
pub enum ScriptEvent {
    Started { script_name: String },
    Log(ScriptLogMessage),
    Assertion(AssertionRecord),
    Finished { total_assertions: usize, passed: usize, failed: usize, duration_ms: u64 },
    Error { message: String },
}

/// Shared execution context passed into Rhai native functions.
#[derive(Clone)]
pub struct ScriptContext {
    pub client: LeapClient,
    pub event_tx: Sender<ScriptEvent>,
    pub cancel_flag: Arc<AtomicBool>,
    pub assertions: Arc<Mutex<Vec<AssertionRecord>>>,
}

impl ScriptContext {
    pub fn new(client: LeapClient, event_tx: Sender<ScriptEvent>, cancel_flag: Arc<AtomicBool>) -> Self {
        Self {
            client,
            event_tx,
            cancel_flag,
            assertions: Arc::new(Mutex::new(Vec::new())),
        }
    }

    pub fn log(&self, level: LogLevel, msg: impl Into<String>) {
        let msg = msg.into();
        let _ = self.event_tx.send(ScriptEvent::Log(ScriptLogMessage::new(level, msg)));
    }

    pub fn record_assertion(&self, passed: bool, description: &str, actual: &str, expected: &str) {
        let rec = AssertionRecord {
            passed,
            description: description.to_string(),
            actual: actual.to_string(),
            expected: expected.to_string(),
        };
        if passed {
            self.log(LogLevel::Pass, format!("ASSERT PASS: {} (value: {})", description, actual));
        } else {
            self.log(LogLevel::Fail, format!("ASSERT FAIL: {} (actual: '{}', expected: '{}')", description, actual, expected));
        }
        self.assertions.lock().unwrap().push(rec.clone());
        let _ = self.event_tx.send(ScriptEvent::Assertion(rec));
    }
}

/// Convert an `LLSDValue` into a Rhai `Dynamic`.
pub fn llsd_to_dynamic(val: &LLSDValue) -> Dynamic {
    match val {
        LLSDValue::Undefined => Dynamic::UNIT,
        LLSDValue::Boolean(b) => Dynamic::from_bool(*b),
        LLSDValue::Integer(i) => Dynamic::from_int(*i as INT),
        LLSDValue::Real(r) => Dynamic::from_float(*r),
        LLSDValue::String(s) => Dynamic::from(s.clone()),
        LLSDValue::UUID(u) => Dynamic::from(u.to_string()),
        LLSDValue::Date(d) => Dynamic::from(d.to_string()),
        LLSDValue::URI(u) => Dynamic::from(u.clone()),
        LLSDValue::Binary(b) => {
            let mut arr = Array::new();
            for byte in b {
                arr.push(Dynamic::from_int(*byte as INT));
            }
            Dynamic::from_array(arr)
        }
        LLSDValue::Map(m) => {
            let mut map = Map::new();
            for (k, v) in m {
                map.insert(k.clone().into(), llsd_to_dynamic(v));
            }
            Dynamic::from_map(map)
        }
        LLSDValue::Array(a) => {
            let mut arr = Array::new();
            for item in a {
                arr.push(llsd_to_dynamic(item));
            }
            Dynamic::from_array(arr)
        }
    }
}

/// Convert a Rhai `Dynamic` into an `LLSDValue`.
pub fn dynamic_to_llsd(dyn_val: &Dynamic) -> LLSDValue {
    if dyn_val.is_unit() {
        LLSDValue::Undefined
    } else if let Some(b) = dyn_val.as_bool().ok() {
        LLSDValue::Boolean(b)
    } else if let Some(i) = dyn_val.as_int().ok() {
        LLSDValue::Integer(i as i32)
    } else if let Some(f) = dyn_val.as_float().ok() {
        LLSDValue::Real(f)
    } else if let Some(s) = dyn_val.clone().try_cast::<String>() {
        LLSDValue::String(s)
    } else if let Some(m) = dyn_val.clone().try_cast::<Map>() {
        let mut map = HashMap::new();
        for (k, v) in m {
            map.insert(k.to_string(), dynamic_to_llsd(&v));
        }
        LLSDValue::Map(map)
    } else if let Some(arr) = dyn_val.clone().try_cast::<Array>() {
        let mut vec = Vec::new();
        for item in arr {
            vec.push(dynamic_to_llsd(&item));
        }
        LLSDValue::Array(vec)
    } else {
        LLSDValue::String(dyn_val.to_string())
    }
}

/// Create and configure the Rhai Engine with LEAP functions.
pub fn create_script_engine(ctx: ScriptContext) -> Engine {
    let mut engine = Engine::new();

    // ── Logging & Output ──────────────────────────────────────────────────────
    let c = ctx.clone();
    engine.on_print(move |msg: &str| {
        c.log(LogLevel::Info, msg);
    });

    let c = ctx.clone();
    engine.on_debug(move |msg: &str, _, _| {
        c.log(LogLevel::Info, msg);
    });

    let c = ctx.clone();
    engine.register_fn("log_info", move |msg: &str| {
        c.log(LogLevel::Info, msg);
    });

    let c = ctx.clone();
    engine.register_fn("log_warn", move |msg: &str| {
        c.log(LogLevel::Warn, msg);
    });

    let c = ctx.clone();
    engine.register_fn("log_leap", move |msg: &str| {
        c.log(LogLevel::Leap, msg);
    });

    // ── Timing & Sleep ────────────────────────────────────────────────────────
    let c = ctx.clone();
    engine.register_fn("sleep", move |millis: INT| {
        let target_ms = millis.max(0) as u64;
        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(target_ms) {
            if c.cancel_flag.load(Ordering::Relaxed) {
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    });

    let c = ctx.clone();
    engine.register_fn("sleep_ms", move |millis: INT| {
        let target_ms = millis.max(0) as u64;
        let start = Instant::now();
        while start.elapsed() < Duration::from_millis(target_ms) {
            if c.cancel_flag.load(Ordering::Relaxed) {
                break;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    });

    // ── Assertions ────────────────────────────────────────────────────────────
    let c = ctx.clone();
    engine.register_fn("assert_true", move |cond: bool| {
        c.record_assertion(cond, "Condition is true", &cond.to_string(), "true");
    });

    let c = ctx.clone();
    engine.register_fn("assert_false", move |cond: bool| {
        c.record_assertion(!cond, "Condition is false", &cond.to_string(), "false");
    });

    let c = ctx.clone();
    engine.register_fn("assert_eq", move |actual: Dynamic, expected: Dynamic| {
        let act_str = actual.to_string();
        let exp_str = expected.to_string();
        let passed = act_str == exp_str;
        c.record_assertion(passed, "Values are equal", &act_str, &exp_str);
    });

    let c = ctx.clone();
    engine.register_fn("assert_ne", move |actual: Dynamic, unexpected: Dynamic| {
        let act_str = actual.to_string();
        let unexp_str = unexpected.to_string();
        let passed = act_str != unexp_str;
        c.record_assertion(passed, "Values are not equal", &act_str, &format!("not {unexp_str}"));
    });

    let c = ctx.clone();
    engine.register_fn("assert_true", move |cond: bool, desc: &str| {
        c.record_assertion(cond, desc, &cond.to_string(), "true");
    });

    let c = ctx.clone();
    engine.register_fn("assert_false", move |cond: bool, desc: &str| {
        c.record_assertion(!cond, desc, &cond.to_string(), "false");
    });

    let c = ctx.clone();
    engine.register_fn("assert_eq", move |actual: Dynamic, expected: Dynamic, desc: &str| {
        let act_str = actual.to_string();
        let exp_str = expected.to_string();
        let passed = act_str == exp_str;
        c.record_assertion(passed, desc, &act_str, &exp_str);
    });

    let c = ctx.clone();
    engine.register_fn("assert_ne", move |actual: Dynamic, unexpected: Dynamic, desc: &str| {
        let act_str = actual.to_string();
        let unexp_str = unexpected.to_string();
        let passed = act_str != unexp_str;
        c.record_assertion(passed, desc, &act_str, &format!("not {unexp_str}"));
    });

    let c = ctx.clone();
    engine.register_fn("assert_near", move |val1: f64, val2: f64, eps: f64, desc: &str| {
        let diff = (val1 - val2).abs();
        let passed = diff <= eps;
        c.record_assertion(passed, desc, &format!("{val1:.3}"), &format!("{val2:.3} (±{eps:.3})"));
    });

    let c = ctx.clone();
    engine.register_fn("assert_sim_fps_min", move |min_fps: FLOAT| {
        let fps = c.client.cached_telemetry().sim_fps.unwrap_or(0.0);
        let passed = fps >= min_fps as f64;
        c.record_assertion(passed, "Simulator FPS is above threshold", &format!("{fps:.1} FPS"), &format!(">= {min_fps:.1} FPS"));
    });

    let c = ctx.clone();
    engine.register_fn("assert_sim_fps_min", move |min_fps: FLOAT, desc: &str| {
        let fps = c.client.cached_telemetry().sim_fps.unwrap_or(0.0);
        let passed = fps >= min_fps as f64;
        c.record_assertion(passed, desc, &format!("{fps:.1} FPS"), &format!(">= {min_fps:.1} FPS"));
    });

    // ── LEAP Core Generic Request / Send ─────────────────────────────────────
    let c = ctx.clone();
    engine.register_fn("leap_request", move |pump: &str, data: Map| -> Dynamic {
        if c.cancel_flag.load(Ordering::Relaxed) {
            return Dynamic::UNIT;
        }
        let mut map_args = HashMap::new();
        for (k, v) in data {
            map_args.insert(k.to_string(), dynamic_to_llsd(&v));
        }
        c.log(LogLevel::Leap, format!("REQ -> [{pump}] op: {:?}", map_args.get("op")));
        match c.client.request(pump, map_args, Duration::from_secs(10)) {
            Ok(reply) => {
                c.log(LogLevel::Leap, format!("REP <- [{pump}] OK"));
                llsd_to_dynamic(&reply)
            }
            Err(e) => {
                c.log(LogLevel::Fail, format!("REP <- [{pump}] ERROR: {e}"));
                Dynamic::UNIT
            }
        }
    });

    let c = ctx.clone();
    engine.register_fn("leap_send", move |pump: &str, data: Map| {
        if c.cancel_flag.load(Ordering::Relaxed) {
            return;
        }
        let mut map_args = HashMap::new();
        for (k, v) in data {
            map_args.insert(k.to_string(), dynamic_to_llsd(&v));
        }
        c.log(LogLevel::Leap, format!("SEND -> [{pump}]"));
        let _ = c.client.send(pump, map_args);
    });

    // ── Agent Movement & Position Helpers ─────────────────────────────────────
    let c = ctx.clone();
    engine.register_fn("agent_get_pos", move || -> Map {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("getPosition".to_string()));
        let mut out = Map::new();
        if let Ok(reply) = c.client.request("LLAgent", args, Duration::from_secs(5)) {
            out.insert("x".into(), Dynamic::from_float(get_f64(&reply, "x").unwrap_or(0.0)));
            out.insert("y".into(), Dynamic::from_float(get_f64(&reply, "y").unwrap_or(0.0)));
            out.insert("z".into(), Dynamic::from_float(get_f64(&reply, "z").unwrap_or(0.0)));
            out.insert("yaw".into(), Dynamic::from_float(get_f64(&reply, "yaw").unwrap_or(0.0)));
            out.insert("flying".into(), Dynamic::from_bool(get_bool(&reply, "flying").unwrap_or(false)));
            out.insert("region".into(), Dynamic::from(get_str(&reply, "region_name").unwrap_or("").to_string()));
            out.insert("slurl".into(), Dynamic::from(get_str(&reply, "slurl").unwrap_or("").to_string()));
        }
        out
    });

    let c = ctx.clone();
    engine.register_fn("agent_step", move |direction: &str, step_meters: f64| -> bool {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("move".to_string()));
        args.insert("direction".to_string(), LLSDValue::String(direction.to_string()));
        args.insert("step".to_string(), LLSDValue::Real(step_meters));
        match c.client.request("LLAgent", args, Duration::from_secs(5)) {
            Ok(_) => true,
            Err(_) => false,
        }
    });

    let c = ctx.clone();
    engine.register_fn("agent_teleport", move |region: &str, x: f64, y: f64, z: f64| -> bool {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("teleport".to_string()));
        args.insert("region".to_string(), LLSDValue::String(region.to_string()));
        args.insert("x".to_string(), LLSDValue::Real(x));
        args.insert("y".to_string(), LLSDValue::Real(y));
        args.insert("z".to_string(), LLSDValue::Real(z));
        c.log(LogLevel::Info, format!("Teleporting to {region} ({x:.0}, {y:.0}, {z:.0})..."));
        c.client.request("LLAgent", args, Duration::from_secs(10)).is_ok()
    });

    let c = ctx.clone();
    engine.register_fn("agent_teleport_slurl", move |slurl: &str| -> bool {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("teleport".to_string()));
        args.insert("slurl".to_string(), LLSDValue::String(slurl.to_string()));
        c.log(LogLevel::Info, format!("Teleporting via SLURL: {slurl}..."));
        c.client.request("LLAgent", args, Duration::from_secs(10)).is_ok()
    });

    let c = ctx.clone();
    engine.register_fn("agent_stop", move || -> bool {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("stop".to_string()));
        c.client.request("LLAgent", args, Duration::from_secs(3)).is_ok()
    });

    let c = ctx.clone();
    engine.register_fn("agent_set_flying", move |_flying: bool| -> bool {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("move".to_string()));
        args.insert("direction".to_string(), LLSDValue::String("fly".to_string()));
        c.client.request("LLAgent", args, Duration::from_secs(3)).is_ok()
    });

    // ── Login Helpers ─────────────────────────────────────────────────────────
    let c = ctx.clone();
    engine.register_fn("login_get_state", move || -> String {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("getState".to_string()));
        if let Ok(reply) = c.client.request("LLLogin", args, Duration::from_secs(5)) {
            get_str(&reply, "state").unwrap_or("STATE_UNKNOWN").to_string()
        } else {
            "DISCONNECTED".to_string()
        }
    });

    let c = ctx.clone();
    engine.register_fn("login_wait_until_inworld", move |timeout_secs: INT| -> bool {
        let max_duration = Duration::from_secs(timeout_secs.max(1) as u64);
        let start = Instant::now();
        c.log(LogLevel::Info, format!("Waiting up to {timeout_secs}s for STATE_STARTED..."));
        while start.elapsed() < max_duration {
            if c.cancel_flag.load(Ordering::Relaxed) {
                return false;
            }
            let mut args = HashMap::new();
            args.insert("op".to_string(), LLSDValue::String("getState".to_string()));
            if let Ok(reply) = c.client.request("LLLogin", args, Duration::from_secs(2)) {
                if get_str(&reply, "state") == Some("STATE_STARTED") || get_bool(&reply, "logged_in") == Some(true) {
                    c.log(LogLevel::Pass, "Viewer is now logged inworld (STATE_STARTED)!");
                    return true;
                }
            }
            std::thread::sleep(Duration::from_millis(500));
        }
        c.log(LogLevel::Warn, "Timed out waiting for STATE_STARTED");
        false
    });

    let c = ctx.clone();
    engine.register_fn("login_authenticate", move |username: &str, pass: &str, uri: &str, start_loc: &str| -> bool {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("login".to_string()));
        args.insert("username".to_string(), LLSDValue::String(username.to_string()));
        args.insert("password".to_string(), LLSDValue::String(pass.to_string()));
        if !uri.is_empty() {
            args.insert("uri".to_string(), LLSDValue::String(uri.to_string()));
        }
        if !start_loc.is_empty() {
            args.insert("start".to_string(), LLSDValue::String(start_loc.to_string()));
        }
        c.log(LogLevel::Info, format!("Sending login request for '{username}'..."));
        c.client.request("LLLogin", args, Duration::from_secs(10)).is_ok()
    });

    let c = ctx.clone();
    engine.register_fn("login_logout", move || -> bool {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("logout".to_string()));
        c.log(LogLevel::Info, "Sending logout request...");
        c.client.request("LLLogin", args, Duration::from_secs(5)).is_ok()
    });

    // ── Chat Helpers ──────────────────────────────────────────────────────────
    let c = ctx.clone();
    engine.register_fn("chat_say", move |msg: &str, channel: INT| -> bool {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("chat".to_string()));
        args.insert("message".to_string(), LLSDValue::String(msg.to_string()));
        args.insert("channel".to_string(), LLSDValue::Integer(channel as i32));
        args.insert("type".to_string(), LLSDValue::String("normal".to_string()));
        c.client.request("LLChat", args, Duration::from_secs(5)).is_ok()
    });

    let c = ctx.clone();
    engine.register_fn("chat_shout", move |msg: &str, channel: INT| -> bool {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("chat".to_string()));
        args.insert("message".to_string(), LLSDValue::String(msg.to_string()));
        args.insert("channel".to_string(), LLSDValue::Integer(channel as i32));
        args.insert("type".to_string(), LLSDValue::String("shout".to_string()));
        c.client.request("LLChat", args, Duration::from_secs(5)).is_ok()
    });

    let c = ctx.clone();
    engine.register_fn("chat_whisper", move |msg: &str, channel: INT| -> bool {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("chat".to_string()));
        args.insert("message".to_string(), LLSDValue::String(msg.to_string()));
        args.insert("channel".to_string(), LLSDValue::Integer(channel as i32));
        args.insert("type".to_string(), LLSDValue::String("whisper".to_string()));
        c.client.request("LLChat", args, Duration::from_secs(5)).is_ok()
    });

    // ── Camera Helpers ────────────────────────────────────────────────────────
    let c = ctx.clone();
    engine.register_fn("camera_get", move || -> Map {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("get".to_string()));
        let mut out = Map::new();
        if let Ok(reply) = c.client.request("LLCamera", args, Duration::from_secs(4)) {
            if let Some((x, y, z)) = parse_vec3(&reply, Some("position")) {
                out.insert("x".into(), Dynamic::from(x));
                out.insert("y".into(), Dynamic::from(y));
                out.insert("z".into(), Dynamic::from(z));
            }
            if let Some((tx, ty, tz)) = parse_vec3(&reply, Some("target")) {
                out.insert("target_x".into(), Dynamic::from(tx));
                out.insert("target_y".into(), Dynamic::from(ty));
                out.insert("target_z".into(), Dynamic::from(tz));
            }
            if let Some(fov) = get_f64(&reply, "fov") {
                let deg = if fov > 0.0 && fov < 3.2 { fov.to_degrees() } else { fov };
                out.insert("fov".into(), Dynamic::from(deg));
            }
        }
        out
    });

    let c = ctx.clone();
    engine.register_fn("camera_reset", move || -> bool {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("reset".to_string()));
        c.client.request("LLCamera", args, Duration::from_secs(3)).is_ok()
    });

    let c = ctx.clone();
    engine.register_fn("camera_set_position", move |x: FLOAT, y: FLOAT, z: FLOAT| -> bool {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("set".to_string()));
        args.insert("x".to_string(), LLSDValue::Real(x));
        args.insert("y".to_string(), LLSDValue::Real(y));
        args.insert("z".to_string(), LLSDValue::Real(z));
        c.client.request("LLCamera", args, Duration::from_secs(3)).is_ok()
    });

    let c = ctx.clone();
    engine.register_fn("camera_look_at", move |x: FLOAT, y: FLOAT, z: FLOAT| -> bool {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("look_at".to_string()));
        args.insert("x".to_string(), LLSDValue::Real(x));
        args.insert("y".to_string(), LLSDValue::Real(y));
        args.insert("z".to_string(), LLSDValue::Real(z));
        c.client.request("LLCamera", args, Duration::from_secs(3)).is_ok()
    });

    let c = ctx.clone();
    engine.register_fn("camera_set", move |cx: FLOAT, cy: FLOAT, cz: FLOAT, tx: FLOAT, ty: FLOAT, tz: FLOAT, fov: FLOAT| -> bool {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("set".to_string()));
        args.insert("x".to_string(), LLSDValue::Real(cx));
        args.insert("y".to_string(), LLSDValue::Real(cy));
        args.insert("z".to_string(), LLSDValue::Real(cz));
        args.insert("target_x".to_string(), LLSDValue::Real(tx));
        args.insert("target_y".to_string(), LLSDValue::Real(ty));
        args.insert("target_z".to_string(), LLSDValue::Real(tz));
        args.insert("fov".to_string(), LLSDValue::Real(fov));
        c.client.request("LLCamera", args, Duration::from_secs(3)).is_ok()
    });

    let c = ctx.clone();
    engine.register_fn("camera_set_fov", move |fov: FLOAT| -> bool {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("set".to_string()));
        args.insert("fov".to_string(), LLSDValue::Real(fov));
        c.client.request("LLCamera", args, Duration::from_secs(3)).is_ok()
    });

    let c = ctx.clone();
    engine.register_fn("camera_preset", move |preset: &str| -> bool {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("preset".to_string()));
        args.insert("preset".to_string(), LLSDValue::String(preset.to_string()));
        c.client.request("LLCamera", args, Duration::from_secs(3)).is_ok()
    });

    let c = ctx.clone();
    engine.register_fn("camera_pan", move |fwd: FLOAT, strafe: FLOAT, elev: FLOAT| -> bool {
        let mut get_args = HashMap::new();
        get_args.insert("op".to_string(), LLSDValue::String("get".to_string()));
        if let Ok(reply) = c.client.request("LLCamera", get_args, Duration::from_secs(3)) {
            let (cx, cy, cz) = parse_vec3(&reply, Some("position")).unwrap_or((128.0, 128.0, 25.0));
            let (tx, ty, tz) = parse_vec3(&reply, Some("target")).unwrap_or((128.0, 128.0, 26.0));
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
            c.client.request("LLCamera", set_args, Duration::from_secs(3)).is_ok()
        } else {
            false
        }
    });

    let c = ctx.clone();
    engine.register_fn("camera_orbit", move |yaw_deg: FLOAT, pitch_deg: FLOAT| -> bool {
        let mut get_args = HashMap::new();
        get_args.insert("op".to_string(), LLSDValue::String("get".to_string()));
        if let Ok(reply) = c.client.request("LLCamera", get_args, Duration::from_secs(3)) {
            let (cx, cy, cz) = parse_vec3(&reply, Some("position")).unwrap_or((124.0, 128.0, 26.5));
            let (tx, ty, tz) = parse_vec3(&reply, Some("target")).unwrap_or((128.0, 128.0, 26.0));
            let rx = cx - tx;
            let ry = cy - ty;
            let rz = cz - tz;
            let dist = (rx * rx + ry * ry + rz * rz).sqrt().max(0.1);
            let current_yaw = ry.atan2(rx);
            let current_pitch = (rz / dist).asin();
            let new_yaw = current_yaw + yaw_deg.to_radians();
            let new_pitch = (current_pitch + pitch_deg.to_radians()).clamp(-1.45, 1.45);
            let new_rx = dist * new_pitch.cos() * new_yaw.cos();
            let new_ry = dist * new_pitch.cos() * new_yaw.sin();
            let new_rz = dist * new_pitch.sin();

            let mut set_args = HashMap::new();
            set_args.insert("op".to_string(), LLSDValue::String("set".to_string()));
            set_args.insert("x".to_string(), LLSDValue::Real(tx + new_rx));
            set_args.insert("y".to_string(), LLSDValue::Real(tx + new_ry));
            set_args.insert("z".to_string(), LLSDValue::Real(tz + new_rz));
            c.client.request("LLCamera", set_args, Duration::from_secs(3)).is_ok()
        } else {
            false
        }
    });

    let c = ctx.clone();
    engine.register_fn("location_bookmark", move |name: &str| -> bool {
        let telem = c.client.cached_telemetry();
        let region = telem.region_name.clone().unwrap_or_else(|| "Welcome Island".to_string());
        let avatar_pos = telem.agent_pos.unwrap_or((128.0, 128.0, 25.0));
        let camera_pos = telem.camera_pos.unwrap_or((avatar_pos.0 - 3.5, avatar_pos.1, avatar_pos.2 + 1.8));
        let camera_target = telem.camera_target.unwrap_or((avatar_pos.0, avatar_pos.1, avatar_pos.2 + 1.2));
        let camera_fov = telem.camera_fov.unwrap_or(60.0);

        let loc = crate::ui::tab_camera::SavedLocation {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_string(),
            region,
            created_at: chrono::Local::now().format("%Y-%m-%d %H:%M").to_string(),
            avatar_pos,
            camera_pos,
            camera_target,
            camera_fov,
            notes: "Script created bookmark".to_string(),
        };

        let path = std::path::Path::new("saved_locations.json");
        let mut list = if path.exists() {
            std::fs::read_to_string(path).ok().and_then(|d| serde_json::from_str::<Vec<crate::ui::tab_camera::SavedLocation>>(&d).ok()).unwrap_or_default()
        } else {
            Vec::new()
        };
        list.push(loc);
        if let Ok(json) = serde_json::to_string_pretty(&list) {
            std::fs::write(path, json).is_ok()
        } else {
            false
        }
    });

    let c = ctx.clone();
    engine.register_fn("location_teleport", move |name_query: &str| -> bool {
        let path = std::path::Path::new("saved_locations.json");
        if let Ok(d) = std::fs::read_to_string(path) {
            if let Ok(list) = serde_json::from_str::<Vec<crate::ui::tab_camera::SavedLocation>>(&d) {
                if let Some(loc) = list.iter().find(|l| l.name.to_lowercase().contains(&name_query.to_lowercase())) {
                    let mut args = HashMap::new();
                    args.insert("op".to_string(), LLSDValue::String("teleport".to_string()));
                    args.insert("region".to_string(), LLSDValue::String(loc.region.clone()));
                    args.insert("x".to_string(), LLSDValue::Real(loc.avatar_pos.0));
                    args.insert("y".to_string(), LLSDValue::Real(loc.avatar_pos.1));
                    args.insert("z".to_string(), LLSDValue::Real(loc.avatar_pos.2));
                    return c.client.request("LLTeleport", args, Duration::from_secs(10)).is_ok();
                }
            }
        }
        false
    });

    let c = ctx.clone();
    engine.register_fn("location_restore_camera", move |name_query: &str| -> bool {
        let path = std::path::Path::new("saved_locations.json");
        if let Ok(d) = std::fs::read_to_string(path) {
            if let Ok(list) = serde_json::from_str::<Vec<crate::ui::tab_camera::SavedLocation>>(&d) {
                if let Some(loc) = list.iter().find(|l| l.name.to_lowercase().contains(&name_query.to_lowercase())) {
                    let mut args = HashMap::new();
                    args.insert("op".to_string(), LLSDValue::String("set".to_string()));
                    args.insert("x".to_string(), LLSDValue::Real(loc.camera_pos.0));
                    args.insert("y".to_string(), LLSDValue::Real(loc.camera_pos.1));
                    args.insert("z".to_string(), LLSDValue::Real(loc.camera_pos.2));
                    args.insert("target_x".to_string(), LLSDValue::Real(loc.camera_target.0));
                    args.insert("target_y".to_string(), LLSDValue::Real(loc.camera_target.1));
                    args.insert("target_z".to_string(), LLSDValue::Real(loc.camera_target.2));
                    args.insert("fov".to_string(), LLSDValue::Real(loc.camera_fov));
                    return c.client.request("LLCamera", args, Duration::from_secs(3)).is_ok();
                }
            }
        }
        false
    });

    // ── Snapshot Helpers ──────────────────────────────────────────────────────
    let c = ctx.clone();
    engine.register_fn("snapshot_capture", move |mode: &str, width: INT, height: INT| -> String {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("takePicture".to_string()));
        args.insert("mode".to_string(), LLSDValue::String(mode.to_string()));
        args.insert("width".to_string(), LLSDValue::Integer(width as i32));
        args.insert("height".to_string(), LLSDValue::Integer(height as i32));
        args.insert("ui".to_string(), LLSDValue::Boolean(true));
        c.log(LogLevel::Info, format!("Requesting snapshot ({width}x{height}, mode: {mode})..."));
        if let Ok(reply) = c.client.request("LLCamera", args, Duration::from_secs(10)) {
            if let Some(path) = get_str(&reply, "file_path") {
                c.log(LogLevel::Pass, format!("Snapshot saved to: {path}"));
                return path.to_string();
            }
        }
        c.log(LogLevel::Fail, "Failed to capture snapshot");
        String::new()
    });

    // ── Inventory Helpers ─────────────────────────────────────────────────────
    let c = ctx.clone();
    engine.register_fn("inventory_get_root", move || -> Map {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("getRoot".to_string()));
        let mut out = Map::new();
        if let Ok(reply) = c.client.request("LLInventory", args, Duration::from_secs(5)) {
            if let Some(folder_id) = get_str(&reply, "folder_id") {
                out.insert("folder_id".into(), Dynamic::from(folder_id.to_string()));
            }
        }
        out
    });

    // ── World & Simulator Helpers ─────────────────────────────────────────────
    let c = ctx.clone();
    engine.register_fn("world_get_region", move || -> Map {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("getRegionInfo".to_string()));
        if let Ok(reply) = c.client.request("LLWorld", args, Duration::from_secs(5)) {
            if let Some(map) = llsd_to_dynamic(&reply).try_cast::<Map>() {
                return map;
            }
        }
        Map::new()
    });

    let c = ctx.clone();
    engine.register_fn("world_get_parcel", move || -> Map {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("getParcelInfo".to_string()));
        if let Ok(reply) = c.client.request("LLWorld", args, Duration::from_secs(5)) {
            if let Some(map) = llsd_to_dynamic(&reply).try_cast::<Map>() {
                return map;
            }
        }
        Map::new()
    });

    let c = ctx.clone();
    engine.register_fn("world_get_sim_stats", move || -> Map {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("getSimStats".to_string()));
        if let Ok(reply) = c.client.request("LLWorld", args, Duration::from_secs(5)) {
            if let Some(map) = llsd_to_dynamic(&reply).try_cast::<Map>() {
                return map;
            }
        }
        Map::new()
    });

    // ── Radar & Nearby Avatars Helpers ────────────────────────────────────────
    let c = ctx.clone();
    engine.register_fn("radar_get_nearby", move || -> Array {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("getNearby".to_string()));
        if let Ok(reply) = c.client.request("LLNearbyAvatars", args, Duration::from_secs(5)) {
            if let Some(arr_val) = crate::codec::get_array(&reply, "avatars") {
                let mut out = Array::new();
                for item in arr_val {
                    out.push(llsd_to_dynamic(item));
                }
                return out;
            }
        }
        Array::new()
    });

    let c = ctx.clone();
    engine.register_fn("radar_get_avatar_count", move || -> INT {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("getNearby".to_string()));
        if let Ok(reply) = c.client.request("LLNearbyAvatars", args, Duration::from_secs(5)) {
            if let Some(arr_val) = crate::codec::get_array(&reply, "avatars") {
                return arr_val.len() as INT;
            }
        }
        0
    });

    // ── Inworld Objects Helpers ───────────────────────────────────────────────
    let c = ctx.clone();
    engine.register_fn("objects_get_nearby", move || -> Array {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("getNearbyObjects".to_string()));
        if let Ok(reply) = c.client.request("LLObjects", args, Duration::from_secs(5)) {
            if let Some(arr_val) = crate::codec::get_array(&reply, "objects") {
                let mut out = Array::new();
                for item in arr_val {
                    out.push(llsd_to_dynamic(item));
                }
                return out;
            }
        }
        Array::new()
    });

    let c = ctx.clone();
    engine.register_fn("objects_get_nearby", move |max_dist: FLOAT| -> Array {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("getNearbyObjects".to_string()));
        args.insert("max_distance".to_string(), LLSDValue::Real(max_dist as f64));
        if let Ok(reply) = c.client.request("LLObjects", args, Duration::from_secs(5)) {
            if let Some(arr_val) = crate::codec::get_array(&reply, "objects") {
                let mut out = Array::new();
                for item in arr_val {
                    out.push(llsd_to_dynamic(item));
                }
                return out;
            }
        }
        Array::new()
    });

    let c = ctx.clone();
    engine.register_fn("objects_inspect", move |local_id: INT| -> Map {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("inspect".to_string()));
        args.insert("local_id".to_string(), LLSDValue::Integer(local_id as i32));
        if let Ok(reply) = c.client.request("LLObjects", args, Duration::from_secs(5)) {
            if let Some(map) = llsd_to_dynamic(&reply).try_cast::<Map>() {
                return map;
            }
        }
        Map::new()
    });

    let c = ctx.clone();
    engine.register_fn("objects_select", move |local_id: INT| -> Map {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("selectObject".to_string()));
        args.insert("local_id".to_string(), LLSDValue::Integer(local_id as i32));
        if let Ok(reply) = c.client.request("LLObjects", args, Duration::from_secs(5)) {
            if let Some(map) = llsd_to_dynamic(&reply).try_cast::<Map>() {
                return map;
            }
        }
        Map::new()
    });

    let c = ctx.clone();
    engine.register_fn("objects_deselect", move || -> Map {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("deselect".to_string()));
        if let Ok(reply) = c.client.request("LLObjects", args, Duration::from_secs(5)) {
            if let Some(map) = llsd_to_dynamic(&reply).try_cast::<Map>() {
                return map;
            }
        }
        Map::new()
    });

    // ── Appearance Helpers ────────────────────────────────────────────────────
    let c = ctx.clone();
    engine.register_fn("appearance_get_worn", move || -> Map {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("getWornItems".to_string()));
        if let Ok(reply) = c.client.request("LLAppearance", args, Duration::from_secs(5)) {
            if let Some(map) = llsd_to_dynamic(&reply).try_cast::<Map>() {
                return map;
            }
        }
        Map::new()
    });

    let c = ctx.clone();
    engine.register_fn("appearance_get_bake_status", move || -> Map {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("getBakeStatus".to_string()));
        if let Ok(reply) = c.client.request("LLAppearance", args, Duration::from_secs(5)) {
            if let Some(map) = llsd_to_dynamic(&reply).try_cast::<Map>() {
                return map;
            }
        }
        Map::new()
    });

    // ── Animation Helpers ─────────────────────────────────────────────────────
    let c = ctx.clone();
    engine.register_fn("animation_get_active", move || -> Array {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("getActiveAnimations".to_string()));
        if let Ok(reply) = c.client.request("LLAnimation", args, Duration::from_secs(5)) {
            if let Some(arr_val) = crate::codec::get_array(&reply, "animations") {
                let mut out = Array::new();
                for item in arr_val {
                    out.push(llsd_to_dynamic(item));
                }
                return out;
            }
        }
        Array::new()
    });

    let c = ctx.clone();
    engine.register_fn("animation_start", move |anim_id: &str| -> Map {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("startAnimation".to_string()));
        args.insert("anim_id".to_string(), LLSDValue::String(anim_id.to_string()));
        if let Ok(reply) = c.client.request("LLAnimation", args, Duration::from_secs(5)) {
            if let Some(map) = llsd_to_dynamic(&reply).try_cast::<Map>() {
                return map;
            }
        }
        Map::new()
    });

    let c = ctx.clone();
    engine.register_fn("animation_stop", move || -> Map {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("stopAnimation".to_string()));
        if let Ok(reply) = c.client.request("LLAnimation", args, Duration::from_secs(5)) {
            if let Some(map) = llsd_to_dynamic(&reply).try_cast::<Map>() {
                return map;
            }
        }
        Map::new()
    });

    // ── Appearance Wear / Take Off Helpers ────────────────────────────────────
    let c = ctx.clone();
    engine.register_fn("appearance_wear", move |item_id: &str| -> Map {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("wearItem".to_string()));
        args.insert("item_id".to_string(), LLSDValue::String(item_id.to_string()));
        if let Ok(reply) = c.client.request("LLAppearance", args, Duration::from_secs(5)) {
            if let Some(map) = llsd_to_dynamic(&reply).try_cast::<Map>() {
                return map;
            }
        }
        Map::new()
    });

    let c = ctx.clone();
    engine.register_fn("appearance_take_off", move |item_id: &str| -> Map {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("takeOff".to_string()));
        args.insert("item_id".to_string(), LLSDValue::String(item_id.to_string()));
        if let Ok(reply) = c.client.request("LLAppearance", args, Duration::from_secs(5)) {
            if let Some(map) = llsd_to_dynamic(&reply).try_cast::<Map>() {
                return map;
            }
        }
        Map::new()
    });

    // ── Voice Telemetry & Control Helpers ─────────────────────────────────────
    let c = ctx.clone();
    engine.register_fn("voice_get_status", move || -> Map {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("getStatus".to_string()));
        if let Ok(reply) = c.client.request("LLVoice", args, Duration::from_secs(5)) {
            if let Some(map) = llsd_to_dynamic(&reply).try_cast::<Map>() {
                return map;
            }
        }
        Map::new()
    });

    let c = ctx.clone();
    engine.register_fn("voice_set_muted", move |muted: bool| -> Map {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("setMuted".to_string()));
        args.insert("muted".to_string(), LLSDValue::Boolean(muted));
        if let Ok(reply) = c.client.request("LLVoice", args, Duration::from_secs(5)) {
            if let Some(map) = llsd_to_dynamic(&reply).try_cast::<Map>() {
                return map;
            }
        }
        Map::new()
    });

    let c = ctx.clone();
    engine.register_fn("voice_set_volume", move |volume: FLOAT| -> Map {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("setVolume".to_string()));
        args.insert("volume".to_string(), LLSDValue::Real(volume as f64));
        if let Ok(reply) = c.client.request("LLVoice", args, Duration::from_secs(5)) {
            if let Some(map) = llsd_to_dynamic(&reply).try_cast::<Map>() {
                return map;
            }
        }
        Map::new()
    });

    let c = ctx.clone();
    engine.register_fn("voice_join", move |uri: &str, channel: &str| -> Map {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("joinChannel".to_string()));
        args.insert("uri".to_string(), LLSDValue::String(uri.to_string()));
        args.insert("channel".to_string(), LLSDValue::String(channel.to_string()));
        if let Ok(reply) = c.client.request("LLVoice", args, Duration::from_secs(5)) {
            if let Some(map) = llsd_to_dynamic(&reply).try_cast::<Map>() {
                return map;
            }
        }
        Map::new()
    });

    let c = ctx.clone();
    engine.register_fn("voice_leave", move || -> Map {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("leaveChannel".to_string()));
        if let Ok(reply) = c.client.request("LLVoice", args, Duration::from_secs(5)) {
            if let Some(map) = llsd_to_dynamic(&reply).try_cast::<Map>() {
                return map;
            }
        }
        Map::new()
    });

    // ── Dialog Notifications & Alerts (Milestone 8.2) ─────────────────────────
    let c = ctx.clone();
    engine.register_fn("notifications_get", move || -> Array {
        match c.client.get_notifications() {
            Ok(list) => list
                .into_iter()
                .map(|item| {
                    let mut m = Map::new();
                    m.insert("id".into(), Dynamic::from_float(item.id));
                    m.insert("id_str".into(), Dynamic::from(item.id_str));
                    m.insert("kind".into(), Dynamic::from(item.kind));
                    m.insert("title".into(), Dynamic::from(item.title));
                    m.insert("body".into(), Dynamic::from(item.body));
                    m.insert("interactive".into(), Dynamic::from_bool(item.interactive));
                    m.insert("stays_until_answered".into(), Dynamic::from_bool(item.stays_until_answered));
                    m.insert("notif_type".into(), Dynamic::from(item.notif_type.unwrap_or_default()));
                    m.insert("buttons".into(), Dynamic::from_array(item.buttons.into_iter().map(Dynamic::from).collect()));
                    m.insert("channel".into(), Dynamic::from_int(item.channel.unwrap_or(0) as i64));
                    m.insert("permissions".into(), Dynamic::from_array(item.permission_names.into_iter().map(Dynamic::from).collect()));
                    m.insert("url".into(), Dynamic::from(item.url.unwrap_or_default()));
                    Dynamic::from_map(m)
                })
                .collect(),
            Err(_) => Array::new(),
        }
    });

    let c = ctx.clone();
    engine.register_fn("notification_respond", move |id: FLOAT, action: &str| -> bool {
        c.client.respond_notification(id, action, None, None).is_ok()
    });

    let c = ctx.clone();
    engine.register_fn("notification_respond_button", move |id: FLOAT, button_index: INT| -> bool {
        c.client.respond_notification(id, "button", Some(button_index as usize), None).is_ok()
    });

    let c = ctx.clone();
    engine.register_fn("notification_dismiss", move |id: FLOAT| -> bool {
        c.client.dismiss_notification(id).is_ok()
    });

    engine
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossbeam_channel::unbounded;
    use rhai::Scope;

    #[test]
    fn test_llsd_dynamic_conversion() {
        let mut map = HashMap::new();
        map.insert("name".to_string(), LLSDValue::String("Test Avatar".to_string()));
        map.insert("count".to_string(), LLSDValue::Integer(42));
        map.insert("pos_x".to_string(), LLSDValue::Real(128.5));
        map.insert("active".to_string(), LLSDValue::Boolean(true));

        let llsd = LLSDValue::Map(map);
        let dyn_val = llsd_to_dynamic(&llsd);

        let roundtrip = dynamic_to_llsd(&dyn_val);
        match roundtrip {
            LLSDValue::Map(m) => {
                assert_eq!(m.get("name"), Some(&LLSDValue::String("Test Avatar".to_string())));
                assert_eq!(m.get("count"), Some(&LLSDValue::Integer(42)));
                assert_eq!(m.get("pos_x"), Some(&LLSDValue::Real(128.5)));
                assert_eq!(m.get("active"), Some(&LLSDValue::Boolean(true)));
            }
            _ => panic!("Expected Map roundtrip"),
        }
    }

    #[test]
    fn test_rhai_assertions_in_engine() {
        let (tx, _rx) = unbounded();
        let cancel = Arc::new(AtomicBool::new(false));
        let client = LeapClient::new();
        let ctx = ScriptContext::new(client, tx, cancel);
        let engine = create_script_engine(ctx.clone());

        let script = r#"
            let a = 10;
            let b = 20;
            assert_true(a < b, "A is less than B");
            assert_eq(a + b, 30, "A + B equals 30");
            assert_near(128.05, 128.00, 0.1, "Coordinates are within tolerance");
        "#;

        let res = engine.eval::<Dynamic>(script);
        assert!(res.is_ok(), "Script evaluated successfully: {:?}", res);

        let assertions = ctx.assertions.lock().unwrap();
        assert_eq!(assertions.len(), 3);
        assert!(assertions.iter().all(|a| a.passed));
    }

    #[test]
    fn test_run_script_01() {
        let (tx, _rx) = unbounded();
        let cancel = Arc::new(AtomicBool::new(false));
        let client = LeapClient::new();
        let ctx = ScriptContext::new(client, tx, cancel);
        let engine = create_script_engine(ctx);

        let code = std::fs::read_to_string("scripts/01_login_and_verify.rhai")
            .or_else(|_| std::fs::read_to_string("../scripts/01_login_and_verify.rhai"))
            .expect("Failed to read script 01");

        let mut scope = Scope::new();
        let res = engine.eval_with_scope::<Dynamic>(&mut scope, &code);
        assert!(res.is_ok(), "Script 01 failed: {:?}", res);
    }

    #[test]
    fn test_run_script_02() {
        let (tx, _rx) = unbounded();
        let cancel = Arc::new(AtomicBool::new(false));
        let client = LeapClient::new();
        let ctx = ScriptContext::new(client, tx, cancel);
        let engine = create_script_engine(ctx);

        let code = std::fs::read_to_string("scripts/02_movement_and_waypoints.rhai")
            .or_else(|_| std::fs::read_to_string("../scripts/02_movement_and_waypoints.rhai"))
            .expect("Failed to read script 02");

        let mut scope = Scope::new();
        let res = engine.eval_with_scope::<Dynamic>(&mut scope, &code);
        assert!(res.is_ok(), "Script 02 failed: {:?}", res);
    }

    #[test]
    fn test_run_script_03() {
        let (tx, _rx) = unbounded();
        let cancel = Arc::new(AtomicBool::new(false));
        let client = LeapClient::new();
        let ctx = ScriptContext::new(client, tx, cancel);
        let engine = create_script_engine(ctx);

        let code = std::fs::read_to_string("scripts/03_snapshot_regression.rhai")
            .or_else(|_| std::fs::read_to_string("../scripts/03_snapshot_regression.rhai"))
            .expect("Failed to read script 03");

        let mut scope = Scope::new();
        let res = engine.eval_with_scope::<Dynamic>(&mut scope, &code);
        assert!(res.is_ok(), "Script 03 failed: {:?}", res);
    }

    #[test]
    fn test_run_script_04() {
        let (tx, _rx) = unbounded();
        let cancel = Arc::new(AtomicBool::new(false));
        let client = LeapClient::new();
        let ctx = ScriptContext::new(client, tx, cancel);
        let engine = create_script_engine(ctx);

        let code = std::fs::read_to_string("scripts/04_inventory_inspection.rhai")
            .or_else(|_| std::fs::read_to_string("../scripts/04_inventory_inspection.rhai"))
            .expect("Failed to read script 04");

        let mut scope = Scope::new();
        let res = engine.eval_with_scope::<Dynamic>(&mut scope, &code);
        assert!(res.is_ok(), "Script 04 failed: {:?}", res);
    }

    #[test]
    fn test_run_script_05() {
        let (tx, _rx) = unbounded();
        let cancel = Arc::new(AtomicBool::new(false));
        let client = LeapClient::new();
        let ctx = ScriptContext::new(client, tx, cancel);
        let engine = create_script_engine(ctx);

        let code = std::fs::read_to_string("scripts/05_camera_and_views.rhai")
            .or_else(|_| std::fs::read_to_string("../scripts/05_camera_and_views.rhai"))
            .expect("Failed to read script 05");

        let mut scope = Scope::new();
        let res = engine.eval_with_scope::<Dynamic>(&mut scope, &code);
        assert!(res.is_ok(), "Script 05 failed: {:?}", res);
    }
}

