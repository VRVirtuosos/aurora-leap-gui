//! Camera Control, Cinematic Sequencer & Location Presets Tab (`LLCamera`, `LLAgent`, `LLTeleport`).

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use chrono::Local;
use egui::{Button, Color32, ComboBox, RichText, ScrollArea, Slider, TextEdit, Ui, Vec2};
use serde::{Deserialize, Serialize};
use serde_llsd::LLSDValue;

use crate::client::{AsyncRequest, LeapClient};
use crate::ui::theme::{
    card_frame, inner_box_frame, COLOR_ACCENT, COLOR_CARD_BG, COLOR_DANGER,
    COLOR_SUCCESS, COLOR_TEXT_MUTED, COLOR_WARNING,
};

/// Sub-tabs within Camera view.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CameraSubTab {
    ControlsAndPresets,
    CinematicSequencer,
}

/// Easing curve applied between camera keyframes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CameraEasing {
    Linear,
    EaseInOutQuad,
    CatmullRomSpline,
}

/// Individual waypoint keyframe in a cinematic camera sequence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraKeyframe {
    pub id: String,
    pub name: String,
    pub duration_to_next_sec: f64,
    pub camera_pos: (f64, f64, f64),
    pub camera_target: (f64, f64, f64),
    pub camera_fov: f64,
    pub easing: CameraEasing,
}

/// A named multi-keyframe fly-through camera sequence.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CameraSequence {
    pub id: String,
    pub name: String,
    pub keyframes: Vec<CameraKeyframe>,
    pub loop_playback: bool,
}

/// A bookmarked location saving avatar coordinates, region, and camera viewpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedLocation {
    pub id: String,
    pub name: String,
    pub region: String,
    pub created_at: String,
    pub avatar_pos: (f64, f64, f64),
    pub camera_pos: (f64, f64, f64),
    pub camera_target: (f64, f64, f64),
    pub camera_fov: f64,
    pub notes: String,
}

pub struct CameraTab {
    // Sub-tab navigation
    pub active_subtab: CameraSubTab,

    // Live Telemetry
    pub camera_pos: Option<(f64, f64, f64)>,
    pub camera_target: Option<(f64, f64, f64)>,
    pub camera_fov: f64,
    pub agent_pos: Option<(f64, f64, f64)>,
    pub region_name: Option<String>,

    // Manual Coordinate Inputs
    pub edit_cam_x: String,
    pub edit_cam_y: String,
    pub edit_cam_z: String,
    pub edit_tgt_x: String,
    pub edit_tgt_y: String,
    pub edit_tgt_z: String,

    // Step & Angle Settings
    pub step_size: f64,
    pub orbit_angle_deg: f64,

    // Location Bookmarks
    pub saved_locations: Vec<SavedLocation>,
    pub new_bookmark_name: String,
    pub storage_path: PathBuf,

    // Cinematic Sequencer
    pub sequences: Vec<CameraSequence>,
    pub active_sequence_idx: usize,
    pub is_playing: bool,
    pub playback_time_sec: f64,
    pub playback_speed: f64,
    pub last_frame_time: Option<Instant>,
    pub sequences_storage_path: PathBuf,
    pub benchmark_running: bool,
    pub benchmark_frame_count: u64,
    pub benchmark_start: Option<Instant>,
    pub benchmark_result: Option<String>,

    // Status Notification (text, is_error, timestamp)
    pub status_msg: Option<(String, bool, Instant)>,

    // Async Request Handles
    pending_request: Option<AsyncRequest<LLSDValue>>,
    pending_agent_request: Option<AsyncRequest<LLSDValue>>,
    pub last_telemetry_poll: Instant,
}

impl CameraTab {
    pub fn new() -> Self {
        let storage_path = PathBuf::from("saved_locations.json");
        let sequences_storage_path = PathBuf::from("camera_sequences.json");

        let mut tab = Self {
            active_subtab: CameraSubTab::ControlsAndPresets,
            camera_pos: None,
            camera_target: None,
            camera_fov: 60.0,
            agent_pos: None,
            region_name: None,
            edit_cam_x: "128.00".to_string(),
            edit_cam_y: "128.00".to_string(),
            edit_cam_z: "25.00".to_string(),
            edit_tgt_x: "128.00".to_string(),
            edit_tgt_y: "128.00".to_string(),
            edit_tgt_z: "26.00".to_string(),
            step_size: 1.0,
            orbit_angle_deg: 15.0,
            saved_locations: Vec::new(),
            new_bookmark_name: String::new(),
            storage_path,
            sequences: Vec::new(),
            active_sequence_idx: 0,
            is_playing: false,
            playback_time_sec: 0.0,
            playback_speed: 1.0,
            last_frame_time: None,
            sequences_storage_path,
            benchmark_running: false,
            benchmark_frame_count: 0,
            benchmark_start: None,
            benchmark_result: None,
            status_msg: None,
            pending_request: None,
            pending_agent_request: None,
            last_telemetry_poll: Instant::now() - Duration::from_secs(5),
        };
        tab.load_saved_locations();
        tab.load_sequences();
        tab
    }

    /// Load saved locations from local JSON file.
    pub fn load_saved_locations(&mut self) {
        if self.storage_path.exists() {
            if let Ok(data) = fs::read_to_string(&self.storage_path) {
                if let Ok(locs) = serde_json::from_str::<Vec<SavedLocation>>(&data) {
                    self.saved_locations = locs;
                    return;
                }
            }
        }

        // Seed initial locations if file missing
        if self.saved_locations.is_empty() {
            self.saved_locations = vec![
                SavedLocation {
                    id: "default-front".to_string(),
                    name: "Front Portrait Close-Up".to_string(),
                    region: "Horizons Info Hub".to_string(),
                    created_at: Local::now().format("%Y-%m-%d %H:%M").to_string(),
                    avatar_pos: (128.0, 128.0, 25.0),
                    camera_pos: (128.0, 126.2, 26.3),
                    camera_target: (128.0, 128.0, 26.2),
                    camera_fov: 50.0,
                    notes: "Tight eye-level portrait shot".to_string(),
                },
                SavedLocation {
                    id: "default-panoramic".to_string(),
                    name: "Bird's-Eye Overhead".to_string(),
                    region: "Horizons Info Hub".to_string(),
                    created_at: Local::now().format("%Y-%m-%d %H:%M").to_string(),
                    avatar_pos: (128.0, 128.0, 25.0),
                    camera_pos: (128.0, 128.0, 85.0),
                    camera_target: (128.0, 128.0, 25.0),
                    camera_fov: 75.0,
                    notes: "High altitude panoramic angle".to_string(),
                },
            ];
            self.save_locations_to_disk();
        }
    }

    /// Persist current bookmarks list to disk.
    pub fn save_locations_to_disk(&self) {
        if let Ok(json) = serde_json::to_string_pretty(&self.saved_locations) {
            let _ = fs::write(&self.storage_path, json);
        }
    }

    /// Load camera fly-through sequences from disk.
    pub fn load_sequences(&mut self) {
        if self.sequences_storage_path.exists() {
            if let Ok(data) = fs::read_to_string(&self.sequences_storage_path) {
                if let Ok(seqs) = serde_json::from_str::<Vec<CameraSequence>>(&data) {
                    if !seqs.is_empty() {
                        self.sequences = seqs;
                        return;
                    }
                }
            }
        }

        // Seed default showcase tour sequence
        self.sequences = vec![CameraSequence {
            id: "seq-showcase".to_string(),
            name: "Showcase Tour (Catmull-Rom Spline)".to_string(),
            loop_playback: true,
            keyframes: vec![
                CameraKeyframe {
                    id: "kf-1".to_string(),
                    name: "Wide Panoramic Intro".to_string(),
                    duration_to_next_sec: 3.5,
                    camera_pos: (120.0, 120.0, 35.0),
                    camera_target: (128.0, 128.0, 26.0),
                    camera_fov: 70.0,
                    easing: CameraEasing::CatmullRomSpline,
                },
                CameraKeyframe {
                    id: "kf-2".to_string(),
                    name: "Side Orbit Sweep".to_string(),
                    duration_to_next_sec: 3.0,
                    camera_pos: (124.0, 133.0, 28.0),
                    camera_target: (128.0, 128.0, 26.0),
                    camera_fov: 55.0,
                    easing: CameraEasing::CatmullRomSpline,
                },
                CameraKeyframe {
                    id: "kf-3".to_string(),
                    name: "Close Front Portrait".to_string(),
                    duration_to_next_sec: 3.0,
                    camera_pos: (128.0, 125.0, 26.5),
                    camera_target: (128.0, 128.0, 26.2),
                    camera_fov: 45.0,
                    easing: CameraEasing::CatmullRomSpline,
                },
                CameraKeyframe {
                    id: "kf-4".to_string(),
                    name: "Return to Wide Panorama".to_string(),
                    duration_to_next_sec: 3.5,
                    camera_pos: (120.0, 120.0, 35.0),
                    camera_target: (128.0, 128.0, 26.0),
                    camera_fov: 70.0,
                    easing: CameraEasing::CatmullRomSpline,
                },
            ],
        }];
        self.save_sequences_to_disk();
    }

    /// Persist camera sequences to disk.
    pub fn save_sequences_to_disk(&self) {
        if let Ok(json) = serde_json::to_string_pretty(&self.sequences) {
            let _ = fs::write(&self.sequences_storage_path, json);
        }
    }

    /// Set a transient status banner message.
    pub fn set_status(&mut self, text: impl Into<String>, is_error: bool) {
        self.status_msg = Some((text.into(), is_error, Instant::now()));
    }

    /// Query the viewer for live camera parameters asynchronously via LLCamera.get.
    pub fn fetch_camera(&mut self, client: &LeapClient) {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("get".to_string()));
        self.pending_request = Some(client.request_raw_async("LLCamera", args, Duration::from_secs(3)));
    }

    /// Query the viewer for live avatar position asynchronously via LLAgent.get.
    pub fn fetch_agent(&mut self, client: &LeapClient) {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("get".to_string()));
        self.pending_agent_request = Some(client.request_raw_async("LLAgent", args, Duration::from_secs(3)));
    }

    /// Send full camera transform (eye pos, target pos, fov) via LLCamera.set.
    pub fn apply_camera_transform(
        &mut self,
        client: &LeapClient,
        pos: (f64, f64, f64),
        tgt: (f64, f64, f64),
        fov: f64,
    ) {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("set".to_string()));
        args.insert("x".to_string(), LLSDValue::Real(pos.0));
        args.insert("y".to_string(), LLSDValue::Real(pos.1));
        args.insert("z".to_string(), LLSDValue::Real(pos.2));
        args.insert("target_x".to_string(), LLSDValue::Real(tgt.0));
        args.insert("target_y".to_string(), LLSDValue::Real(tgt.1));
        args.insert("target_z".to_string(), LLSDValue::Real(tgt.2));
        args.insert("fov".to_string(), LLSDValue::Real(fov));

        let _ = client.send("LLCamera", args);

        self.camera_pos = Some(pos);
        self.camera_target = Some(tgt);
        self.camera_fov = fov;
        self.edit_cam_x = format!("{:.2}", pos.0);
        self.edit_cam_y = format!("{:.2}", pos.1);
        self.edit_cam_z = format!("{:.2}", pos.2);
        self.edit_tgt_x = format!("{:.2}", tgt.0);
        self.edit_tgt_y = format!("{:.2}", tgt.1);
        self.edit_tgt_z = format!("{:.2}", tgt.2);

        self.set_status(format!("Set camera to ({:.1}, {:.1}, {:.1})", pos.0, pos.1, pos.2), false);
    }

    /// Reset camera view to default trailing avatar position.
    pub fn reset_camera(&mut self, client: &LeapClient) {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("reset".to_string()));
        let _ = client.send("LLCamera", args);
        self.set_status("Reset camera to default avatar view", false);
        self.fetch_camera(client);
    }

    /// Look directly at the avatar's head from current camera position.
    pub fn look_at_avatar(&mut self, client: &LeapClient) {
        let telem = client.cached_telemetry();
        if let Some((ax, ay, az)) = telem.agent_pos {
            let (cx, cy, cz) = self.camera_pos.unwrap_or((ax - 3.5, ay - 3.5, az + 1.5));
            self.apply_camera_transform(client, (cx, cy, cz), (ax, ay, az + 0.8), self.camera_fov);
            self.set_status(format!("Camera focus locked to avatar at ({ax:.1}, {ay:.1}, {az:.1})"), false);
        } else {
            // Also issue direct look_at_avatar to viewer to compute from internal agent state
            let mut args = HashMap::new();
            args.insert("op".to_string(), LLSDValue::String("look_at_avatar".to_string()));
            let _ = client.send("LLCamera", args);
            self.fetch_agent(client);
            self.fetch_camera(client);
            self.set_status("Sent look_at_avatar command to viewer", false);
        }
    }

    /// Relative 3D pan in camera coordinate space.
    pub fn pan(&mut self, client: &LeapClient, forward_step: f64, strafe_step: f64, elevation_step: f64) {
        let (cx, cy, cz) = self.camera_pos.unwrap_or((128.0, 128.0, 25.0));
        let (tx, ty, tz) = self.camera_target.unwrap_or((128.0, 128.0, 26.0));

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

        let delta_x = dx * forward_step + rx * strafe_step;
        let delta_y = dy * forward_step + ry * strafe_step;
        let delta_z = dz * forward_step + elevation_step;

        let new_cam = (cx + delta_x, cy + delta_y, cz + delta_z);
        let new_tgt = (tx + delta_x, ty + delta_y, tz + delta_z);
        self.apply_camera_transform(client, new_cam, new_tgt, self.camera_fov);
    }

    /// Orbit camera around the focus target by yaw and pitch degrees.
    pub fn orbit(&mut self, client: &LeapClient, yaw_degrees: f64, pitch_degrees: f64) {
        let (cx, cy, cz) = self.camera_pos.unwrap_or((124.0, 128.0, 26.5));
        let (tx, ty, tz) = self.camera_target.unwrap_or((128.0, 128.0, 26.0));

        let rx = cx - tx;
        let ry = cy - ty;
        let rz = cz - tz;
        let dist = (rx * rx + ry * ry + rz * rz).sqrt().max(0.1);

        let current_yaw = ry.atan2(rx);
        let current_pitch = (rz / dist).asin();

        let new_yaw = current_yaw + yaw_degrees.to_radians();
        let new_pitch = (current_pitch + pitch_degrees.to_radians()).clamp(-1.45, 1.45);

        let new_rx = dist * new_pitch.cos() * new_yaw.cos();
        let new_ry = dist * new_pitch.cos() * new_yaw.sin();
        let new_rz = dist * new_pitch.sin();

        let new_cam = (tx + new_rx, ty + new_ry, tz + new_rz);
        self.apply_camera_transform(client, new_cam, (tx, ty, tz), self.camera_fov);
    }

    /// Apply preset framing angles.
    pub fn apply_preset(&mut self, client: &LeapClient, preset_name: &str) {
        let telem = client.cached_telemetry();
        if let Some((ax, ay, az)) = telem.agent_pos {
            let yaw = telem.agent_yaw.unwrap_or(0.0);

            let (cam, tgt, fov) = match preset_name {
                // Front views (facing the avatar from the front)
                "front_portrait" | "portrait" => {
                    let dist = 1.6;
                    let cx = ax + dist * yaw.cos();
                    let cy = ay + dist * yaw.sin();
                    ((cx, cy, az + 1.2), (ax, ay, az + 1.2), 40.0)
                }
                "front_medium" | "front_torso" => {
                    let dist = 2.6;
                    let cx = ax + dist * yaw.cos();
                    let cy = ay + dist * yaw.sin();
                    ((cx, cy, az + 1.0), (ax, ay, az + 0.9), 50.0)
                }
                "front_full_body" | "front_body" => {
                    let dist = 3.8;
                    let cx = ax + dist * yaw.cos();
                    let cy = ay + dist * yaw.sin();
                    ((cx, cy, az + 0.7), (ax, ay, az + 0.5), 55.0)
                }
                "front_left_angle" | "front_three_quarter_left" => {
                    let angle = yaw + 45.0_f64.to_radians();
                    let dist = 2.4;
                    let cx = ax + dist * angle.cos();
                    let cy = ay + dist * angle.sin();
                    ((cx, cy, az + 1.1), (ax, ay, az + 1.0), 45.0)
                }
                "front_right_angle" | "front_three_quarter_right" => {
                    let angle = yaw - 45.0_f64.to_radians();
                    let dist = 2.4;
                    let cx = ax + dist * angle.cos();
                    let cy = ay + dist * angle.sin();
                    ((cx, cy, az + 1.1), (ax, ay, az + 1.0), 45.0)
                }
                // Rear views (behind the avatar, looking forward)
                "rear_third_person" | "rear_follow" | "full_body" => {
                    let dist = 3.8;
                    let cx = ax - dist * yaw.cos();
                    let cy = ay - dist * yaw.sin();
                    ((cx, cy, az + 0.8), (ax, ay, az + 0.5), 55.0)
                }
                "rear_portrait" | "rear_over_shoulder" => {
                    let dist = 1.6;
                    let cx = ax - dist * yaw.cos();
                    let cy = ay - dist * yaw.sin();
                    ((cx, cy, az + 1.2), (ax + 2.0 * yaw.cos(), ay + 2.0 * yaw.sin(), az + 1.2), 40.0)
                }
                "wide_landscape" | "rear_wide_landscape" => {
                    let dist = 14.0;
                    let cx = ax - dist * yaw.cos();
                    let cy = ay - dist * yaw.sin();
                    ((cx, cy, az + 4.5), (ax, ay, az + 1.0), 80.0)
                }
                // Profiles
                "profile_left" => {
                    let dist = 2.5;
                    let cx = ax - dist * yaw.sin();
                    let cy = ay + dist * yaw.cos();
                    ((cx, cy, az + 1.1), (ax, ay, az + 1.1), 45.0)
                }
                "profile_right" => {
                    let dist = 2.5;
                    let cx = ax + dist * yaw.sin();
                    let cy = ay - dist * yaw.cos();
                    ((cx, cy, az + 1.1), (ax, ay, az + 1.1), 45.0)
                }
                // Overhead
                "overhead" | "birds_eye" => ((ax, ay, az + 35.0), (ax, ay, az), 75.0),
                _ => return,
            };

            self.apply_camera_transform(client, cam, tgt, fov);
            self.set_status(format!("Applied framing preset: '{preset_name}'"), false);
        } else {
            // Send preset directly to viewer which has live inworld avatar position
            let mut args = HashMap::new();
            args.insert("op".to_string(), LLSDValue::String("preset".to_string()));
            args.insert("preset".to_string(), LLSDValue::String(preset_name.to_string()));
            let _ = client.send("LLCamera", args);
            self.fetch_agent(client);
            self.fetch_camera(client);
            self.set_status(format!("Applied framing preset '{preset_name}' on viewer"), false);
        }
    }

    /// Teleport avatar to saved bookmark coordinates.
    pub fn teleport_avatar(&mut self, client: &LeapClient, region: &str, pos: (f64, f64, f64)) {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("teleport".to_string()));
        if !region.is_empty() {
            args.insert("region".to_string(), LLSDValue::String(region.to_string()));
        }
        args.insert("x".to_string(), LLSDValue::Real(pos.0));
        args.insert("y".to_string(), LLSDValue::Real(pos.1));
        args.insert("z".to_string(), LLSDValue::Real(pos.2));
        let _ = client.send("LLAgent", args);
        self.set_status(format!("Teleporting avatar to {region} ({:.1}, {:.1}, {:.1})", pos.0, pos.1, pos.2), false);
    }

    /// Bookmark current location.
    pub fn bookmark_current_location(&mut self) {
        let name = if !self.new_bookmark_name.trim().is_empty() {
            self.new_bookmark_name.trim().to_string()
        } else if let Some(ref reg) = self.region_name {
            format!("{reg} View #{:03}", self.saved_locations.len() + 1)
        } else {
            format!("Location #{:03}", self.saved_locations.len() + 1)
        };

        let loc = SavedLocation {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            region: self.region_name.clone().unwrap_or_else(|| "Unknown".to_string()),
            created_at: Local::now().format("%Y-%m-%d %H:%M").to_string(),
            avatar_pos: self.agent_pos.unwrap_or((128.0, 128.0, 25.0)),
            camera_pos: self.camera_pos.unwrap_or((124.0, 128.0, 26.5)),
            camera_target: self.camera_target.unwrap_or((128.0, 128.0, 26.0)),
            camera_fov: self.camera_fov,
            notes: String::new(),
        };

        self.saved_locations.push(loc);
        self.save_locations_to_disk();
        self.new_bookmark_name.clear();
        self.set_status("Location bookmark saved successfully!", false);
    }

    // ── Cinematic Sequencer Logic ──────────────────────────────────────────────

    /// Total duration of active sequence in seconds.
    pub fn active_sequence_duration(&self) -> f64 {
        if let Some(seq) = self.sequences.get(self.active_sequence_idx) {
            if seq.keyframes.len() < 2 {
                return 0.0;
            }
            seq.keyframes[..seq.keyframes.len() - 1]
                .iter()
                .map(|k| k.duration_to_next_sec)
                .sum()
        } else {
            0.0
        }
    }

    /// Add current live camera view as a new keyframe in active sequence.
    pub fn add_current_keyframe(&mut self) {
        let (cx, cy, cz) = self.camera_pos.unwrap_or((124.0, 128.0, 26.5));
        let (tx, ty, tz) = self.camera_target.unwrap_or((128.0, 128.0, 26.0));
        let fov = self.camera_fov;

        if let Some(seq) = self.sequences.get_mut(self.active_sequence_idx) {
            let count = seq.keyframes.len() + 1;
            seq.keyframes.push(CameraKeyframe {
                id: uuid::Uuid::new_v4().to_string(),
                name: format!("Waypoint #{count}"),
                duration_to_next_sec: 3.0,
                camera_pos: (cx, cy, cz),
                camera_target: (tx, ty, tz),
                camera_fov: fov,
                easing: CameraEasing::CatmullRomSpline,
            });
            self.save_sequences_to_disk();
            self.set_status(format!("Added Keyframe #{count} at ({cx:.1}, {cy:.1}, {cz:.1})"), false);
        }
    }

    /// Automatically generate 360° circular orbit keyframes around focus target.
    pub fn generate_orbit_template(&mut self, radius: f64, duration_total: f64) {
        let (tx, ty, tz) = self.camera_target.or(self.agent_pos).unwrap_or((128.0, 128.0, 26.0));
        let height = 1.5;
        let segment_duration = duration_total / 4.0;
        let mut keyframes = Vec::new();

        for deg in [0.0f64, 90.0, 180.0, 270.0, 360.0] {
            let rad = deg.to_radians();
            let cx = tx + radius * rad.cos();
            let cy = ty + radius * rad.sin();
            let cz = tz + height;

            keyframes.push(CameraKeyframe {
                id: uuid::Uuid::new_v4().to_string(),
                name: format!("Orbit {:03}°", deg as i32),
                duration_to_next_sec: segment_duration,
                camera_pos: (cx, cy, cz),
                camera_target: (tx, ty, tz),
                camera_fov: 60.0,
                easing: CameraEasing::CatmullRomSpline,
            });
        }

        let new_seq = CameraSequence {
            id: uuid::Uuid::new_v4().to_string(),
            name: format!("360° Orbit (R={radius:.1}m, {duration_total:.0}s)"),
            keyframes,
            loop_playback: true,
        };

        self.sequences.push(new_seq);
        self.active_sequence_idx = self.sequences.len() - 1;
        self.playback_time_sec = 0.0;
        self.save_sequences_to_disk();
        self.set_status("Generated 360° Orbit sequence template", false);
    }

    /// Automatically generate a crane reveal path.
    pub fn generate_crane_template(&mut self) {
        let (tx, ty, tz) = self.camera_target.or(self.agent_pos).unwrap_or((128.0, 128.0, 26.0));
        let keyframes = vec![
            CameraKeyframe {
                id: uuid::Uuid::new_v4().to_string(),
                name: "High Altitude Crane Start".to_string(),
                duration_to_next_sec: 4.0,
                camera_pos: (tx - 12.0, ty - 12.0, tz + 24.0),
                camera_target: (tx, ty, tz),
                camera_fov: 75.0,
                easing: CameraEasing::EaseInOutQuad,
            },
            CameraKeyframe {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Mid-Level Deceleration".to_string(),
                duration_to_next_sec: 3.0,
                camera_pos: (tx - 6.0, ty - 6.0, tz + 8.0),
                camera_target: (tx, ty, tz + 0.4),
                camera_fov: 60.0,
                easing: CameraEasing::EaseInOutQuad,
            },
            CameraKeyframe {
                id: uuid::Uuid::new_v4().to_string(),
                name: "Eye-Level Portrait Finish".to_string(),
                duration_to_next_sec: 2.0,
                camera_pos: (tx - 2.5, ty - 2.5, tz + 1.2),
                camera_target: (tx, ty, tz + 0.8),
                camera_fov: 48.0,
                easing: CameraEasing::Linear,
            },
        ];

        let new_seq = CameraSequence {
            id: uuid::Uuid::new_v4().to_string(),
            name: "Skyline Crane Down Reveal".to_string(),
            keyframes,
            loop_playback: false,
        };

        self.sequences.push(new_seq);
        self.active_sequence_idx = self.sequences.len() - 1;
        self.playback_time_sec = 0.0;
        self.save_sequences_to_disk();
        self.set_status("Generated Crane Down Reveal sequence template", false);
    }

    /// Update playback state and stream camera coordinates to viewer every frame.
    pub fn update_playback(&mut self, client: &LeapClient) {
        if !self.is_playing {
            self.last_frame_time = None;
            return;
        }

        let now = Instant::now();
        let dt = if let Some(last) = self.last_frame_time {
            now.duration_since(last).as_secs_f64()
        } else {
            0.016
        };
        self.last_frame_time = Some(now);

        let total_duration = self.active_sequence_duration();
        if total_duration <= 0.001 {
            self.is_playing = false;
            return;
        }

        // Advance timeline
        self.playback_time_sec += dt * self.playback_speed;

        let should_loop = self
            .sequences
            .get(self.active_sequence_idx)
            .map(|s| s.loop_playback)
            .unwrap_or(false);

        if self.playback_time_sec >= total_duration {
            if self.benchmark_running {
                // Benchmark completion
                let start = self.benchmark_start.unwrap_or(now);
                let duration_real = now.duration_since(start).as_secs_f64();
                let fps = (self.benchmark_frame_count as f64) / duration_real.max(0.001);
                self.benchmark_result = Some(format!(
                    "Benchmark Finished: {} frames in {:.2}s ({:.1} avg FPS)",
                    self.benchmark_frame_count, duration_real, fps
                ));
                self.benchmark_running = false;
                self.is_playing = false;
                self.playback_time_sec = total_duration;
                return;
            }

            if should_loop {
                self.playback_time_sec %= total_duration;
            } else {
                self.playback_time_sec = total_duration;
                self.is_playing = false;
            }
        }

        // Sample interpolated camera transform
        if let Some(seq) = self.sequences.get(self.active_sequence_idx) {
            let (cam_pos, cam_target, fov) = interpolate_sequence(&seq.keyframes, self.playback_time_sec);

            // Stream non-blocking set command to viewer
            let mut args = HashMap::new();
            args.insert("op".to_string(), LLSDValue::String("set".to_string()));
            args.insert("x".to_string(), LLSDValue::Real(cam_pos.0));
            args.insert("y".to_string(), LLSDValue::Real(cam_pos.1));
            args.insert("z".to_string(), LLSDValue::Real(cam_pos.2));
            args.insert("target_x".to_string(), LLSDValue::Real(cam_target.0));
            args.insert("target_y".to_string(), LLSDValue::Real(cam_target.1));
            args.insert("target_z".to_string(), LLSDValue::Real(cam_target.2));
            args.insert("fov".to_string(), LLSDValue::Real(fov));
            let _ = client.send("LLCamera", args);

            self.camera_pos = Some(cam_pos);
            self.camera_target = Some(cam_target);
            self.camera_fov = fov;

            if self.benchmark_running {
                self.benchmark_frame_count += 1;
            }
        }
    }

    /// Render main UI.
    pub fn ui(&mut self, ui: &mut Ui, client: &LeapClient) {
        let is_connected = client.is_connected();

        // 1. Live telemetry synchronization
        let telem = client.cached_telemetry();
        if let Some(pos) = telem.camera_pos {
            self.camera_pos = Some(pos);
        }
        if let Some(tgt) = telem.camera_target {
            self.camera_target = Some(tgt);
        }
        if let Some(fov) = telem.camera_fov {
            self.camera_fov = fov;
        }
        if let Some(pos) = telem.agent_pos {
            self.agent_pos = Some(pos);
        }
        if let Some(ref reg) = telem.region_name {
            self.region_name = Some(reg.clone());
        }

        // 2. Poll pending async request
        if let Some(ref mut req) = self.pending_request {
            if let Some(res) = req.poll() {
                match res {
                    Ok(val) => {
                        if let Some(pos) = crate::codec::parse_vec3(&val, Some("position"))
                            .or_else(|| crate::codec::parse_vec3(&val, Some("camera_pos")))
                        {
                            self.camera_pos = Some(pos);
                            self.edit_cam_x = format!("{:.2}", pos.0);
                            self.edit_cam_y = format!("{:.2}", pos.1);
                            self.edit_cam_z = format!("{:.2}", pos.2);
                        }
                        if let Some(tgt) = crate::codec::parse_vec3(&val, Some("target"))
                            .or_else(|| crate::codec::parse_vec3(&val, Some("camera_target")))
                        {
                            self.camera_target = Some(tgt);
                            self.edit_tgt_x = format!("{:.2}", tgt.0);
                            self.edit_tgt_y = format!("{:.2}", tgt.1);
                            self.edit_tgt_z = format!("{:.2}", tgt.2);
                        }
                        if let Some(fov) = crate::codec::get_f64(&val, "fov") {
                            let deg = if fov > 0.0 && fov < 3.2 { fov.to_degrees() } else { fov };
                            self.camera_fov = deg;
                        }
                    }
                    Err(err) => {
                        self.set_status(format!("Camera Error: {err}"), true);
                    }
                }
                self.pending_request = None;
            }
        }

        // Poll pending async agent telemetry request
        if let Some(ref mut req) = self.pending_agent_request {
            if let Some(res) = req.poll() {
                if let Ok(val) = res {
                    let in_world = crate::codec::get_bool(&val, "in_world")
                        .or_else(|| crate::codec::get_bool(&val, "logged_in"))
                        .unwrap_or(true);
                    let mut found_pos = None;
                    if let Some(pos) = crate::codec::parse_vec3(&val, Some("position")) {
                        if pos.0 != 0.0 || pos.1 != 0.0 || pos.2 != 0.0 {
                            found_pos = Some(pos);
                        }
                    } else if let (Some(x), Some(y), Some(z)) = (
                        val.as_map().and_then(|m| m.get("x")).and_then(crate::codec::val_to_f64),
                        val.as_map().and_then(|m| m.get("y")).and_then(crate::codec::val_to_f64),
                        val.as_map().and_then(|m| m.get("z")).and_then(crate::codec::val_to_f64),
                    ) {
                        if x != 0.0 || y != 0.0 || z != 0.0 {
                            found_pos = Some((x, y, z));
                        }
                    }

                    if in_world && found_pos.is_some() {
                        self.agent_pos = found_pos;
                    } else if !in_world {
                        self.agent_pos = None;
                    }

                    if let Some(reg) = crate::codec::get_str(&val, "region")
                        .or_else(|| crate::codec::get_str(&val, "region_name"))
                    {
                        if !reg.is_empty() {
                            self.region_name = Some(reg.to_string());
                        }
                    }

                    // Auto-sync camera textboxes if camera was stuck at high login altitude (>1000m) while avatar is on ground (<100m)
                    if let (Some(c), Some(a)) = (self.camera_pos, self.agent_pos) {
                        if c.2 > 500.0 && a.2 < 100.0 {
                            self.fetch_camera(client);
                        }
                    }
                }
                self.pending_agent_request = None;
            }
        }

        // 3. Update active sequencer playback
        self.update_playback(client);
        if self.is_playing {
            ui.ctx().request_repaint_after(Duration::from_millis(16));
        }

        // Periodic live telemetry synchronization (every 1.5s) while connected
        if is_connected && self.pending_request.is_none() && self.pending_agent_request.is_none() && self.last_telemetry_poll.elapsed() >= Duration::from_millis(1500) {
            self.fetch_camera(client);
            self.fetch_agent(client);
            self.last_telemetry_poll = Instant::now();
        }

        // 4. Render Header Bar
        card_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading(RichText::new("🎥 Camera View, Sequencer & Presets").strong().color(COLOR_ACCENT));
                ui.add_space(8.0);
                if is_connected {
                    ui.label(RichText::new("● Live Synced").color(COLOR_SUCCESS).size(12.0));
                } else {
                    ui.label(RichText::new("○ Disconnected").color(COLOR_TEXT_MUTED).size(12.0));
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add_enabled(is_connected, egui::Button::new("🔄 Sync Telemetry")).clicked() {
                        self.fetch_camera(client);
                        self.fetch_agent(client);
                        self.set_status("Querying camera and avatar telemetry from viewer...", false);
                    }
                    if ui.add_enabled(is_connected, egui::Button::new("🎯 Look at Avatar")).clicked() {
                        self.look_at_avatar(client);
                    }
                    if ui.add_enabled(is_connected, egui::Button::new("🔄 Reset View")).clicked() {
                        self.reset_camera(client);
                    }
                });
            });

            // Status notification message banner
            if let Some((ref text, is_error, timestamp)) = self.status_msg {
                if timestamp.elapsed() < Duration::from_secs(6) {
                    ui.add_space(4.0);
                    let color = if is_error { COLOR_DANGER } else { COLOR_SUCCESS };
                    ui.label(RichText::new(text).color(color).size(12.0));
                }
            }

            ui.add_space(6.0);

            // Metrics readout chips
            ui.horizontal(|ui| {
                inner_box_frame().show(ui, |ui| {
                    ui.label(RichText::new("Camera Eye:").color(COLOR_TEXT_MUTED).size(11.0));
                    if let Some((x, y, z)) = self.camera_pos {
                        ui.label(RichText::new(format!("{x:.2}, {y:.2}, {z:.2}")).strong().color(COLOR_ACCENT).monospace());
                    } else {
                        ui.label(RichText::new("Not queried").color(COLOR_TEXT_MUTED).monospace());
                    }
                });

                inner_box_frame().show(ui, |ui| {
                    ui.label(RichText::new("Look-At Target:").color(COLOR_TEXT_MUTED).size(11.0));
                    if let Some((tx, ty, tz)) = self.camera_target {
                        ui.label(RichText::new(format!("{tx:.2}, {ty:.2}, {tz:.2}")).strong().color(COLOR_ACCENT).monospace());
                    } else {
                        ui.label(RichText::new("Not queried").color(COLOR_TEXT_MUTED).monospace());
                    }
                });

                inner_box_frame().show(ui, |ui| {
                    ui.label(RichText::new("Distance:").color(COLOR_TEXT_MUTED).size(11.0));
                    if let (Some((cx, cy, cz)), Some((tx, ty, tz))) = (self.camera_pos, self.camera_target) {
                        let dist = ((cx - tx).powi(2) + (cy - ty).powi(2) + (cz - tz).powi(2)).sqrt();
                        ui.label(RichText::new(format!("{dist:.2} m")).strong().monospace());
                    } else {
                        ui.label(RichText::new("--").color(COLOR_TEXT_MUTED));
                    }
                });

                inner_box_frame().show(ui, |ui| {
                    ui.label(RichText::new("Field of View:").color(COLOR_TEXT_MUTED).size(11.0));
                    ui.label(RichText::new(format!("{:.1}°", self.camera_fov)).strong().monospace());
                });

                inner_box_frame().show(ui, |ui| {
                    ui.label(RichText::new("Avatar Position:").color(COLOR_TEXT_MUTED).size(11.0));
                    if let Some((ax, ay, az)) = self.agent_pos {
                        let reg = self.region_name.as_deref().unwrap_or("Inworld");
                        ui.label(RichText::new(format!("{reg} ({ax:.1}, {ay:.1}, {az:.1})")).size(11.0).monospace());
                    } else {
                        ui.label(RichText::new("Not logged in").color(COLOR_TEXT_MUTED).size(11.0));
                    }
                });
            });
        });

        ui.add_space(6.0);

        // 5. Sub-Tab Switcher: Manual Controls vs Cinematic Sequencer
        ui.horizontal(|ui| {
            let is_controls = self.active_subtab == CameraSubTab::ControlsAndPresets;
            let controls_btn = egui::Button::new(
                RichText::new("🕹️ Manual Controls & Bookmarks")
                    .strong()
                    .color(if is_controls { COLOR_ACCENT } else { Color32::LIGHT_GRAY })
            ).fill(if is_controls { Color32::from_rgb(32, 40, 56) } else { Color32::TRANSPARENT });

            if ui.add(controls_btn).clicked() {
                self.active_subtab = CameraSubTab::ControlsAndPresets;
            }

            let is_seq = self.active_subtab == CameraSubTab::CinematicSequencer;
            let seq_btn = egui::Button::new(
                RichText::new("🎬 Cinematic Fly-Through Sequencer")
                    .strong()
                    .color(if is_seq { COLOR_ACCENT } else { Color32::LIGHT_GRAY })
            ).fill(if is_seq { Color32::from_rgb(32, 40, 56) } else { Color32::TRANSPARENT });

            if ui.add(seq_btn).clicked() {
                self.active_subtab = CameraSubTab::CinematicSequencer;
            }
        });

        ui.add_space(6.0);

        // 6. Sub-Tab Content Rendering
        match self.active_subtab {
            CameraSubTab::ControlsAndPresets => {
                self.render_controls_and_presets_ui(ui, client);
            }
            CameraSubTab::CinematicSequencer => {
                self.render_sequencer_ui(ui, client);
            }
        }
    }

    /// Render standard manual controls and saved bookmarks view.
    fn render_controls_and_presets_ui(&mut self, ui: &mut Ui, client: &LeapClient) {
        let is_connected = client.is_connected();

        ui.columns(2, |cols| {
            // LEFT COLUMN: 3D Camera Controls, Pan, Orbit, Optics, Presets
            cols[0].vertical(|ui| {
                // Exact Coordinates & Manual Transform Card
                card_frame().show(ui, |ui| {
                    ui.label(RichText::new("🎯 Exact Coordinates & Manual Placement").strong().size(13.0));
                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Eye (X, Y, Z):").color(COLOR_TEXT_MUTED).size(11.0));
                        ui.add(TextEdit::singleline(&mut self.edit_cam_x).desired_width(55.0));
                        ui.add(TextEdit::singleline(&mut self.edit_cam_y).desired_width(55.0));
                        ui.add(TextEdit::singleline(&mut self.edit_cam_z).desired_width(55.0));
                    });

                    ui.add_space(4.0);

                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Target (X, Y, Z):").color(COLOR_TEXT_MUTED).size(11.0));
                        ui.add(TextEdit::singleline(&mut self.edit_tgt_x).desired_width(55.0));
                        ui.add(TextEdit::singleline(&mut self.edit_tgt_y).desired_width(55.0));
                        ui.add(TextEdit::singleline(&mut self.edit_tgt_z).desired_width(55.0));
                    });

                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        if ui.add_enabled(is_connected, egui::Button::new("🚀 Apply Coordinates")).clicked() {
                            if let (Ok(cx), Ok(cy), Ok(cz), Ok(tx), Ok(ty), Ok(tz)) = (
                                self.edit_cam_x.trim().parse::<f64>(),
                                self.edit_cam_y.trim().parse::<f64>(),
                                self.edit_cam_z.trim().parse::<f64>(),
                                self.edit_tgt_x.trim().parse::<f64>(),
                                self.edit_tgt_y.trim().parse::<f64>(),
                                self.edit_tgt_z.trim().parse::<f64>(),
                            ) {
                                self.apply_camera_transform(client, (cx, cy, cz), (tx, ty, tz), self.camera_fov);
                            } else {
                                self.set_status("Invalid coordinate numbers entered", true);
                            }
                        }

                        if ui.button("📋 Copy").clicked() {
                            let text = format!(
                                "Eye: ({}, {}, {}) Target: ({}, {}, {}) FOV: {:.1}°",
                                self.edit_cam_x, self.edit_cam_y, self.edit_cam_z,
                                self.edit_tgt_x, self.edit_tgt_y, self.edit_tgt_z,
                                self.camera_fov
                            );
                            ui.output_mut(|o| o.copied_text = text);
                            self.set_status("Copied camera coordinates to clipboard", false);
                        }
                    });
                });

                ui.add_space(6.0);

                // Directional Pan & Elevation Card
                card_frame().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("🕹️ 3D Pan & Elevation").strong().size(13.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new("Step:").color(COLOR_TEXT_MUTED).size(11.0));
                            for s in [0.25, 0.5, 1.0, 2.5, 5.0, 10.0] {
                                let selected = (self.step_size - s).abs() < 0.01;
                                let btn = egui::Button::new(format!("{s}m"))
                                    .small()
                                    .fill(if selected { COLOR_ACCENT } else { COLOR_CARD_BG });
                                if ui.add(btn).clicked() {
                                    self.step_size = s;
                                }
                            }
                        });
                    });

                    ui.add_space(8.0);

                    // Directional D-Pad Layout
                    let step = self.step_size;
                    ui.horizontal(|ui| {
                        ui.vertical(|ui| {
                            ui.label(RichText::new("Camera Pan (Relative)").color(COLOR_TEXT_MUTED).size(11.0));
                            ui.horizontal(|ui| {
                                ui.add_space(38.0);
                                if ui.add_enabled(is_connected, egui::Button::new("⬆️ Fwd").min_size(Vec2::new(60.0, 28.0))).clicked() {
                                    self.pan(client, step, 0.0, 0.0);
                                }
                            });
                            ui.horizontal(|ui| {
                                if ui.add_enabled(is_connected, egui::Button::new("⬅️ Left").min_size(Vec2::new(60.0, 28.0))).clicked() {
                                    self.pan(client, 0.0, -step, 0.0);
                                }
                                ui.add_space(16.0);
                                if ui.add_enabled(is_connected, egui::Button::new("➡️ Right").min_size(Vec2::new(60.0, 28.0))).clicked() {
                                    self.pan(client, 0.0, step, 0.0);
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.add_space(38.0);
                                if ui.add_enabled(is_connected, egui::Button::new("⬇️ Back").min_size(Vec2::new(60.0, 28.0))).clicked() {
                                    self.pan(client, -step, 0.0, 0.0);
                                }
                            });
                        });

                        ui.add_space(20.0);

                        ui.vertical(|ui| {
                            ui.label(RichText::new("Elevation (Z)").color(COLOR_TEXT_MUTED).size(11.0));
                            if ui.add_enabled(is_connected, egui::Button::new("⏫ Up (+Z)").min_size(Vec2::new(75.0, 28.0))).clicked() {
                                self.pan(client, 0.0, 0.0, step);
                            }
                            ui.add_space(4.0);
                            if ui.add_enabled(is_connected, egui::Button::new("⏬ Down (-Z)").min_size(Vec2::new(75.0, 28.0))).clicked() {
                                self.pan(client, 0.0, 0.0, -step);
                            }
                        });
                    });
                });

                ui.add_space(6.0);

                // Orbit & Rotation Card
                card_frame().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("🔄 Target Orbit & Turntable").strong().size(13.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new("Angle:").color(COLOR_TEXT_MUTED).size(11.0));
                            for a in [5.0, 15.0, 45.0, 90.0] {
                                let selected = (self.orbit_angle_deg - a).abs() < 0.01;
                                let btn = egui::Button::new(format!("{a:.0}°"))
                                    .small()
                                    .fill(if selected { COLOR_ACCENT } else { COLOR_CARD_BG });
                                if ui.add(btn).clicked() {
                                    self.orbit_angle_deg = a;
                                }
                            }
                        });
                    });

                    ui.add_space(8.0);

                    let ang = self.orbit_angle_deg;
                    ui.horizontal(|ui| {
                        ui.label("Orbit Yaw:");
                        if ui.add_enabled(is_connected, egui::Button::new(format!("↶ Orbit -{ang:.0}°"))).clicked() {
                            self.orbit(client, -ang, 0.0);
                        }
                        if ui.add_enabled(is_connected, egui::Button::new(format!("↷ Orbit +{ang:.0}°"))).clicked() {
                            self.orbit(client, ang, 0.0);
                        }

                        ui.add_space(16.0);
                        ui.label("Orbit Pitch:");
                        if ui.add_enabled(is_connected, egui::Button::new(format!("🔼 Pitch +{ang:.0}°"))).clicked() {
                            self.orbit(client, 0.0, ang);
                        }
                        if ui.add_enabled(is_connected, egui::Button::new(format!("🔽 Pitch -{ang:.0}°"))).clicked() {
                            self.orbit(client, 0.0, -ang);
                        }
                    });
                });

                ui.add_space(6.0);

                // Optics & Field of View Card
                card_frame().show(ui, |ui| {
                    ui.label(RichText::new("🔍 Field of View & Lenses").strong().size(13.0));
                    ui.add_space(6.0);

                    ui.horizontal(|ui| {
                        ui.label("FOV Slider:");
                        let old_fov = self.camera_fov;
                        if ui.add(Slider::new(&mut self.camera_fov, 20.0..=120.0).suffix("°")).changed() {
                            if (self.camera_fov - old_fov).abs() > 0.1 && is_connected {
                                if let (Some(c), Some(t)) = (self.camera_pos, self.camera_target) {
                                    self.apply_camera_transform(client, c, t, self.camera_fov);
                                }
                            }
                        }
                    });

                    ui.add_space(4.0);

                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("Lens Presets:").color(COLOR_TEXT_MUTED).size(11.0));
                        for (name, fov_val) in [
                            ("Telephoto (30°)", 30.0),
                            ("Portrait (50°)", 50.0),
                            ("Standard (60°)", 60.0),
                            ("Wide (80°)", 80.0),
                            ("Action / Fisheye (105°)", 105.0),
                        ] {
                            let selected = (self.camera_fov - fov_val).abs() < 1.0;
                            let btn = egui::Button::new(name)
                                .small()
                                .fill(if selected { COLOR_ACCENT } else { COLOR_CARD_BG });
                            if ui.add_enabled(is_connected, btn).clicked() {
                                self.camera_fov = fov_val;
                                if let (Some(c), Some(t)) = (self.camera_pos, self.camera_target) {
                                    self.apply_camera_transform(client, c, t, fov_val);
                                }
                            }
                        }
                    });
                });

                ui.add_space(6.0);

                // Framing Presets Card
                card_frame().show(ui, |ui| {
                    ui.label(RichText::new("📐 Framing Presets (Avatar Relative)").strong().size(13.0));
                    ui.add_space(6.0);

                    ui.label(RichText::new("Front Views (Facing Avatar):").color(COLOR_ACCENT).size(11.0).strong());
                    ui.horizontal_wrapped(|ui| {
                        for (label, key) in [
                            ("👤 Front Portrait", "front_portrait"),
                            ("🧍 Front Medium", "front_medium"),
                            ("🚶 Front Full Body", "front_full_body"),
                            ("📐 3/4 Left Angle", "front_left_angle"),
                            ("📐 3/4 Right Angle", "front_right_angle"),
                        ] {
                            if ui.add_enabled(is_connected, egui::Button::new(label)).clicked() {
                                self.apply_preset(client, key);
                            }
                        }
                    });

                    ui.add_space(6.0);
                    ui.label(RichText::new("Rear Views (Behind Avatar):").color(COLOR_TEXT_MUTED).size(11.0).strong());
                    ui.horizontal_wrapped(|ui| {
                        for (label, key) in [
                            ("🎒 Rear 3rd-Person", "rear_third_person"),
                            ("👥 Over-the-Shoulder", "rear_over_shoulder"),
                            ("🏞️ Wide Landscape", "wide_landscape"),
                        ] {
                            if ui.add_enabled(is_connected, egui::Button::new(label)).clicked() {
                                self.apply_preset(client, key);
                            }
                        }
                    });

                    ui.add_space(6.0);
                    ui.label(RichText::new("Profile & Overhead Views:").color(COLOR_TEXT_MUTED).size(11.0).strong());
                    ui.horizontal_wrapped(|ui| {
                        for (label, key) in [
                            ("👈 Left Profile (90°)", "profile_left"),
                            ("👉 Right Profile (90°)", "profile_right"),
                            ("🚁 Bird's-Eye Overhead", "overhead"),
                        ] {
                            if ui.add_enabled(is_connected, egui::Button::new(label)).clicked() {
                                self.apply_preset(client, key);
                            }
                        }
                    });
                });
            });

            // RIGHT COLUMN: Saved Locations & Viewpoint Bookmarks Manager
            cols[1].vertical(|ui| {
                card_frame().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("💾 Saved Viewer Locations & Bookmarks").strong().size(13.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new(format!("{} saved", self.saved_locations.len())).color(COLOR_TEXT_MUTED).size(11.0));
                        });
                    });

                    ui.add_space(8.0);

                    // Add new bookmark box
                    inner_box_frame().show(ui, |ui| {
                        ui.label(RichText::new("Save Current Inworld Position & Camera:").strong().size(12.0));
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            let hint = if let Some(ref reg) = self.region_name {
                                format!("{reg} View")
                            } else {
                                "Location Name...".to_string()
                            };
                            ui.add(TextEdit::singleline(&mut self.new_bookmark_name).hint_text(hint).desired_width(220.0));
                            if ui.add_enabled(is_connected, egui::Button::new("💾 Save Location")).clicked() {
                                self.bookmark_current_location();
                            }
                        });
                    });

                    ui.add_space(10.0);

                    // List of saved locations
                    let mut to_delete = None;
                    let mut teleport_target = None;
                    let mut apply_cam_target = None;
                    let mut copy_slurl_text = None;

                    ScrollArea::vertical().id_salt("saved_locations_scroll").max_height(450.0).show(ui, |ui| {
                        for (idx, loc) in self.saved_locations.iter().enumerate() {
                            inner_box_frame().show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(&loc.name).strong().color(COLOR_ACCENT));
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        let del_btn = egui::Button::new(RichText::new("🗑").color(COLOR_DANGER)).small();
                                        if ui.add(del_btn).on_hover_text("Delete bookmark").clicked() {
                                            to_delete = Some(idx);
                                        }
                                        ui.label(RichText::new(&loc.created_at).color(COLOR_TEXT_MUTED).size(10.0));
                                    });
                                });

                                ui.add_space(2.0);
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(format!("Region: {}", loc.region)).size(11.0));
                                    ui.label(RichText::new(format!("Avatar: ({:.1}, {:.1}, {:.1})", loc.avatar_pos.0, loc.avatar_pos.1, loc.avatar_pos.2)).size(11.0).color(COLOR_TEXT_MUTED));
                                });
                                ui.label(RichText::new(format!("Camera: ({:.1}, {:.1}, {:.1}) → ({:.1}, {:.1}, {:.1}) FOV {:.0}°",
                                    loc.camera_pos.0, loc.camera_pos.1, loc.camera_pos.2,
                                    loc.camera_target.0, loc.camera_target.1, loc.camera_target.2,
                                    loc.camera_fov)).size(11.0).color(COLOR_TEXT_MUTED));

                                ui.add_space(4.0);

                                ui.horizontal(|ui| {
                                    if ui.add_enabled(is_connected, egui::Button::new("🚀 Teleport").small()).on_hover_text("Teleport avatar to this saved position").clicked() {
                                        teleport_target = Some((loc.region.clone(), loc.avatar_pos));
                                    }
                                    if ui.add_enabled(is_connected, egui::Button::new("🎥 Restore Camera").small()).on_hover_text("Restore saved camera angle and FOV").clicked() {
                                        apply_cam_target = Some((loc.camera_pos, loc.camera_target, loc.camera_fov));
                                    }
                                    if ui.add_enabled(is_connected, egui::Button::new("🚀🎥 Both").small()).on_hover_text("Teleport avatar and restore camera angle").clicked() {
                                        teleport_target = Some((loc.region.clone(), loc.avatar_pos));
                                        apply_cam_target = Some((loc.camera_pos, loc.camera_target, loc.camera_fov));
                                    }
                                    if ui.add(egui::Button::new("📋 Copy SLURL").small()).clicked() {
                                        let slurl = format!("secondlife://{}/{:.0}/{:.0}/{:.0}", loc.region, loc.avatar_pos.0, loc.avatar_pos.1, loc.avatar_pos.2);
                                        copy_slurl_text = Some(slurl);
                                    }
                                });
                            });
                            ui.add_space(6.0);
                        }
                    });

                    // Handle deferred actions
                    if let Some(idx) = to_delete {
                        self.saved_locations.remove(idx);
                        self.save_locations_to_disk();
                        self.set_status("Bookmark deleted", false);
                    }
                    if let Some((reg, pos)) = teleport_target {
                        self.teleport_avatar(client, &reg, pos);
                    }
                    if let Some((pos, tgt, fov)) = apply_cam_target {
                        self.apply_camera_transform(client, pos, tgt, fov);
                    }
                    if let Some(slurl) = copy_slurl_text {
                        ui.output_mut(|o| o.copied_text = slurl.clone());
                        self.set_status(format!("Copied SLURL to clipboard: {slurl}"), false);
                    }
                });
            });
        });
    }

    /// Render Cinematic Fly-Through Sequencer panel with timeline, track editor, and templates.
    fn render_sequencer_ui(&mut self, ui: &mut Ui, client: &LeapClient) {
        let is_connected = client.is_connected();
        let total_duration = self.active_sequence_duration();

        card_frame().show(ui, |ui| {
            // Sequence Selection & Management Row
            ui.horizontal(|ui| {
                ui.label(RichText::new("🎬 Active Sequence:").strong());

                let current_name = self.sequences.get(self.active_sequence_idx).map(|s| s.name.clone()).unwrap_or_else(|| "None".to_string());
                ComboBox::from_id_salt("seq_selector_combo")
                    .selected_text(&current_name)
                    .width(260.0)
                    .show_ui(ui, |ui| {
                        for (idx, seq) in self.sequences.iter().enumerate() {
                            if ui.selectable_value(&mut self.active_sequence_idx, idx, &seq.name).clicked() {
                                self.playback_time_sec = 0.0;
                                self.is_playing = false;
                            }
                        }
                    });

                if ui.button("➕ New Sequence").clicked() {
                    let count = self.sequences.len() + 1;
                    let new_seq = CameraSequence {
                        id: uuid::Uuid::new_v4().to_string(),
                        name: format!("Sequence #{count}"),
                        keyframes: Vec::new(),
                        loop_playback: true,
                    };
                    self.sequences.push(new_seq);
                    self.active_sequence_idx = self.sequences.len() - 1;
                    self.playback_time_sec = 0.0;
                    self.save_sequences_to_disk();
                }

                if self.sequences.len() > 1 && ui.button("🗑 Delete").clicked() {
                    self.sequences.remove(self.active_sequence_idx);
                    self.active_sequence_idx = self.active_sequence_idx.saturating_sub(1);
                    self.playback_time_sec = 0.0;
                    self.save_sequences_to_disk();
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button("💾 Save Sequences").clicked() {
                        self.save_sequences_to_disk();
                        self.set_status("Camera sequences saved to disk.", false);
                    }
                });
            });

            ui.add_space(8.0);

            // Transport Bar & Playback Controls
            inner_box_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    if self.is_playing {
                        if ui.add(Button::new(RichText::new("⏸ Pause").strong()).fill(COLOR_WARNING)).clicked() {
                            self.is_playing = false;
                        }
                    } else {
                        let play_btn = Button::new(RichText::new("▶ Play").strong().color(Color32::WHITE)).fill(COLOR_SUCCESS);
                        if ui.add_enabled(is_connected && total_duration > 0.1, play_btn).clicked() {
                            if self.playback_time_sec >= total_duration {
                                self.playback_time_sec = 0.0;
                            }
                            self.is_playing = true;
                            self.last_frame_time = Some(Instant::now());
                        }
                    }

                    if ui.button("⏹ Stop").clicked() {
                        self.is_playing = false;
                        self.playback_time_sec = 0.0;
                    }

                    ui.add_space(10.0);

                    // Scrubber progress slider
                    let time_str = format!("{:.1}s / {:.1}s", self.playback_time_sec, total_duration);
                    ui.label(RichText::new(time_str).monospace().strong());
                    ui.add(Slider::new(&mut self.playback_time_sec, 0.0..=total_duration.max(0.1)).show_value(false));

                    ui.add_space(10.0);

                    if let Some(seq) = self.sequences.get_mut(self.active_sequence_idx) {
                        ui.checkbox(&mut seq.loop_playback, "🔁 Loop");
                    }

                    ui.add_space(8.0);
                    ui.label("Speed:");
                    for spd in [0.5, 1.0, 2.0] {
                        let selected = (self.playback_speed - spd).abs() < 0.01;
                        if ui.selectable_label(selected, format!("{spd}x")).clicked() {
                            self.playback_speed = spd;
                        }
                    }

                    // Benchmark button
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let b_btn = Button::new(RichText::new(if self.benchmark_running { "⏳ Benchmarking…" } else { "📊 Benchmark Track" }).strong());
                        if ui.add_enabled(is_connected && !self.benchmark_running && total_duration > 0.1, b_btn).clicked() {
                            self.benchmark_running = true;
                            self.benchmark_frame_count = 0;
                            self.benchmark_start = Some(Instant::now());
                            self.playback_time_sec = 0.0;
                            self.playback_speed = 1.0;
                            self.is_playing = true;
                            self.last_frame_time = Some(Instant::now());
                        }
                    });
                });
            });

            if let Some(ref b_res) = self.benchmark_result {
                ui.add_space(4.0);
                ui.label(RichText::new(b_res).color(COLOR_SUCCESS).strong().size(12.0));
            }

            ui.add_space(8.0);

            // Generator Templates Bar
            ui.horizontal(|ui| {
                ui.label(RichText::new("⚡ Generate Path:").color(COLOR_TEXT_MUTED));
                if ui.button("🎥 360° Orbit Path").on_hover_text("Generate a silky 4-point orbital tour around avatar").clicked() {
                    self.generate_orbit_template(5.0, 10.0);
                }
                if ui.button("🏙 Skyline Crane Down").on_hover_text("Generate a high-altitude crane tilt-down reveal").clicked() {
                    self.generate_crane_template();
                }
                if ui.button("🏎 Forward Fly-Through").on_hover_text("Generate 60m forward fly-through sweep").clicked() {
                    let (cx, cy, cz) = self.camera_pos.unwrap_or((128.0, 128.0, 25.0));
                    let (tx, ty, tz) = self.camera_target.unwrap_or((128.0, 140.0, 26.0));
                    let new_seq = CameraSequence {
                        id: uuid::Uuid::new_v4().to_string(),
                        name: "Forward Fly-Through 60m".to_string(),
                        loop_playback: true,
                        keyframes: vec![
                            CameraKeyframe {
                                id: uuid::Uuid::new_v4().to_string(),
                                name: "Start Point".to_string(),
                                duration_to_next_sec: 4.0,
                                camera_pos: (cx, cy, cz),
                                camera_target: (tx, ty, tz),
                                camera_fov: 65.0,
                                easing: CameraEasing::CatmullRomSpline,
                            },
                            CameraKeyframe {
                                id: uuid::Uuid::new_v4().to_string(),
                                name: "Waypoint 30m".to_string(),
                                duration_to_next_sec: 4.0,
                                camera_pos: (cx, cy + 30.0, cz + 2.0),
                                camera_target: (tx, ty + 30.0, tz + 2.0),
                                camera_fov: 65.0,
                                easing: CameraEasing::CatmullRomSpline,
                            },
                            CameraKeyframe {
                                id: uuid::Uuid::new_v4().to_string(),
                                name: "End Point 60m".to_string(),
                                duration_to_next_sec: 4.0,
                                camera_pos: (cx, cy + 60.0, cz + 4.0),
                                camera_target: (tx, ty + 60.0, tz + 4.0),
                                camera_fov: 65.0,
                                easing: CameraEasing::CatmullRomSpline,
                            },
                        ],
                    };
                    self.sequences.push(new_seq);
                    self.active_sequence_idx = self.sequences.len() - 1;
                    self.save_sequences_to_disk();
                    self.set_status("Generated Forward Fly-Through sequence", false);
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.add_enabled(is_connected, Button::new(RichText::new("➕ Add Current View as Keyframe").strong())).clicked() {
                        self.add_current_keyframe();
                    }
                });
            });

            ui.add_space(8.0);

            // Keyframes Track Table
            let mut remove_kf_idx = None;
            let mut move_up_idx = None;
            let mut move_down_idx = None;
            let mut jump_to_kf = None;

            if let Some(seq) = self.sequences.get_mut(self.active_sequence_idx) {
                ScrollArea::vertical().id_salt("keyframe_track_scroll").max_height(350.0).show(ui, |ui| {
                    if seq.keyframes.is_empty() {
                        inner_box_frame().show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                ui.add_space(30.0);
                                ui.label(RichText::new("No keyframes in this sequence yet.\nUse '➕ Add Current View as Keyframe' or click a generator above.").color(COLOR_TEXT_MUTED));
                                ui.add_space(30.0);
                            });
                        });
                    } else {
                        let kf_count = seq.keyframes.len();
                        for (idx, kf) in seq.keyframes.iter_mut().enumerate() {
                            inner_box_frame().show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(format!("#{}", idx + 1)).strong().color(COLOR_ACCENT));
                                    ui.add(TextEdit::singleline(&mut kf.name).desired_width(140.0));

                                    ui.add_space(6.0);
                                    ui.label("Pos:");
                                    ui.label(RichText::new(format!("({:.1}, {:.1}, {:.1})", kf.camera_pos.0, kf.camera_pos.1, kf.camera_pos.2)).monospace().size(11.0));

                                    ui.label("Target:");
                                    ui.label(RichText::new(format!("({:.1}, {:.1}, {:.1})", kf.camera_target.0, kf.camera_target.1, kf.camera_target.2)).monospace().size(11.0));

                                    ui.label("FOV:");
                                    ui.add(Slider::new(&mut kf.camera_fov, 20.0..=120.0).suffix("°"));

                                    ui.add_space(6.0);
                                    ui.label("Duration:");
                                    ui.add(Slider::new(&mut kf.duration_to_next_sec, 0.5..=20.0).suffix("s"));

                                    ui.add_space(6.0);
                                    ui.label("Easing:");
                                    let easing_str = match kf.easing {
                                        CameraEasing::Linear => "Linear",
                                        CameraEasing::EaseInOutQuad => "EaseInOut",
                                        CameraEasing::CatmullRomSpline => "Spline",
                                    };
                                    ComboBox::from_id_salt(format!("kf_ease_{idx}"))
                                        .selected_text(easing_str)
                                        .width(90.0)
                                        .show_ui(ui, |ui| {
                                            ui.selectable_value(&mut kf.easing, CameraEasing::Linear, "Linear");
                                            ui.selectable_value(&mut kf.easing, CameraEasing::EaseInOutQuad, "EaseInOut");
                                            ui.selectable_value(&mut kf.easing, CameraEasing::CatmullRomSpline, "Spline");
                                        });

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.add(Button::new(RichText::new("🗑").color(COLOR_DANGER)).small()).on_hover_text("Delete keyframe").clicked() {
                                            remove_kf_idx = Some(idx);
                                        }
                                        if idx < kf_count - 1 && ui.button("⬇").on_hover_text("Move down").clicked() {
                                            move_down_idx = Some(idx);
                                        }
                                        if idx > 0 && ui.button("⬆").on_hover_text("Move up").clicked() {
                                            move_up_idx = Some(idx);
                                        }
                                        if ui.add_enabled(is_connected, Button::new("👁 Jump").small()).on_hover_text("Jump camera to this keyframe").clicked() {
                                            jump_to_kf = Some((kf.camera_pos, kf.camera_target, kf.camera_fov));
                                        }
                                    });
                                });
                            });
                            ui.add_space(3.0);
                        }
                    }
                });
            }

            // Handle track mutations
            let mut save_needed = false;
            if let Some(seq) = self.sequences.get_mut(self.active_sequence_idx) {
                if let Some(idx) = remove_kf_idx {
                    seq.keyframes.remove(idx);
                    save_needed = true;
                }
                if let Some(idx) = move_up_idx {
                    seq.keyframes.swap(idx, idx - 1);
                    save_needed = true;
                }
                if let Some(idx) = move_down_idx {
                    seq.keyframes.swap(idx, idx + 1);
                    save_needed = true;
                }
            }
            if save_needed {
                self.save_sequences_to_disk();
            }

            if let Some((pos, tgt, fov)) = jump_to_kf {
                self.apply_camera_transform(client, pos, tgt, fov);
            }
        });
    }
}

// ── Spline & Interpolation Helpers ─────────────────────────────────────────────

fn ease_in_out_quad(t: f64) -> f64 {
    if t < 0.5 {
        2.0 * t * t
    } else {
        1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
    }
}

fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

fn lerp_vec3(a: (f64, f64, f64), b: (f64, f64, f64), t: f64) -> (f64, f64, f64) {
    (lerp(a.0, b.0, t), lerp(a.1, b.1, t), lerp(a.2, b.2, t))
}

fn catmull_rom(p0: f64, p1: f64, p2: f64, p3: f64, t: f64) -> f64 {
    0.5 * ((2.0 * p1)
        + (-p0 + p2) * t
        + (2.0 * p0 - 5.0 * p1 + 4.0 * p2 - p3) * t * t
        + (-p0 + 3.0 * p1 - 3.0 * p2 + p3) * t * t * t)
}

fn catmull_rom_vec3(
    p0: (f64, f64, f64),
    p1: (f64, f64, f64),
    p2: (f64, f64, f64),
    p3: (f64, f64, f64),
    t: f64,
) -> (f64, f64, f64) {
    (
        catmull_rom(p0.0, p1.0, p2.0, p3.0, t),
        catmull_rom(p0.1, p1.1, p2.1, p3.1, t),
        catmull_rom(p0.2, p1.2, p2.2, p3.2, t),
    )
}

/// Calculate interpolated camera position, target, and FOV at arbitrary time along keyframe track.
pub fn interpolate_sequence(
    keyframes: &[CameraKeyframe],
    time_sec: f64,
) -> ((f64, f64, f64), (f64, f64, f64), f64) {
    if keyframes.is_empty() {
        return ((128.0, 128.0, 25.0), (128.0, 128.0, 26.0), 60.0);
    }
    if keyframes.len() == 1 {
        let k = &keyframes[0];
        return (k.camera_pos, k.camera_target, k.camera_fov);
    }

    let mut accumulated = 0.0;
    for i in 0..keyframes.len() - 1 {
        let dur = keyframes[i].duration_to_next_sec.max(0.01);
        if time_sec <= accumulated + dur || i == keyframes.len() - 2 {
            let local_t = ((time_sec - accumulated) / dur).clamp(0.0, 1.0);
            let k0 = &keyframes[i];
            let k1 = &keyframes[i + 1];

            return match k0.easing {
                CameraEasing::Linear => {
                    let pos = lerp_vec3(k0.camera_pos, k1.camera_pos, local_t);
                    let tgt = lerp_vec3(k0.camera_target, k1.camera_target, local_t);
                    let fov = lerp(k0.camera_fov, k1.camera_fov, local_t);
                    (pos, tgt, fov)
                }
                CameraEasing::EaseInOutQuad => {
                    let t_eased = ease_in_out_quad(local_t);
                    let pos = lerp_vec3(k0.camera_pos, k1.camera_pos, t_eased);
                    let tgt = lerp_vec3(k0.camera_target, k1.camera_target, t_eased);
                    let fov = lerp(k0.camera_fov, k1.camera_fov, t_eased);
                    (pos, tgt, fov)
                }
                CameraEasing::CatmullRomSpline => {
                    let p_prev = if i > 0 {
                        keyframes[i - 1].camera_pos
                    } else {
                        (
                            2.0 * k0.camera_pos.0 - k1.camera_pos.0,
                            2.0 * k0.camera_pos.1 - k1.camera_pos.1,
                            2.0 * k0.camera_pos.2 - k1.camera_pos.2,
                        )
                    };
                    let p_next2 = if i + 2 < keyframes.len() {
                        keyframes[i + 2].camera_pos
                    } else {
                        (
                            2.0 * k1.camera_pos.0 - k0.camera_pos.0,
                            2.0 * k1.camera_pos.1 - k0.camera_pos.1,
                            2.0 * k1.camera_pos.2 - k0.camera_pos.2,
                        )
                    };
                    let pos = catmull_rom_vec3(p_prev, k0.camera_pos, k1.camera_pos, p_next2, local_t);

                    let t_prev = if i > 0 {
                        keyframes[i - 1].camera_target
                    } else {
                        (
                            2.0 * k0.camera_target.0 - k1.camera_target.0,
                            2.0 * k0.camera_target.1 - k1.camera_target.1,
                            2.0 * k0.camera_target.2 - k1.camera_target.2,
                        )
                    };
                    let t_next2 = if i + 2 < keyframes.len() {
                        keyframes[i + 2].camera_target
                    } else {
                        (
                            2.0 * k1.camera_target.0 - k0.camera_target.0,
                            2.0 * k1.camera_target.1 - k0.camera_target.1,
                            2.0 * k1.camera_target.2 - k0.camera_target.2,
                        )
                    };
                    let tgt = catmull_rom_vec3(t_prev, k0.camera_target, k1.camera_target, t_next2, local_t);

                    let fov = lerp(k0.camera_fov, k1.camera_fov, ease_in_out_quad(local_t));
                    (pos, tgt, fov)
                }
            };
        }
        accumulated += dur;
    }

    let last = keyframes.last().unwrap();
    (last.camera_pos, last.camera_target, last.camera_fov)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_camera_sequence_interpolation_endpoints() {
        let keyframes = vec![
            CameraKeyframe {
                id: "k1".to_string(),
                name: "Start".to_string(),
                duration_to_next_sec: 4.0,
                camera_pos: (100.0, 100.0, 10.0),
                camera_target: (128.0, 128.0, 25.0),
                camera_fov: 60.0,
                easing: CameraEasing::Linear,
            },
            CameraKeyframe {
                id: "k2".to_string(),
                name: "End".to_string(),
                duration_to_next_sec: 4.0,
                camera_pos: (200.0, 200.0, 50.0),
                camera_target: (128.0, 128.0, 25.0),
                camera_fov: 80.0,
                easing: CameraEasing::Linear,
            },
        ];

        // At t = 0
        let (pos_0, _, fov_0) = interpolate_sequence(&keyframes, 0.0);
        assert_eq!(pos_0, (100.0, 100.0, 10.0));
        assert_eq!(fov_0, 60.0);

        // At t = 2.0 (midway)
        let (pos_mid, _, fov_mid) = interpolate_sequence(&keyframes, 2.0);
        assert_eq!(pos_mid, (150.0, 150.0, 30.0));
        assert_eq!(fov_mid, 70.0);

        // At t = 4.0 (end)
        let (pos_end, _, fov_end) = interpolate_sequence(&keyframes, 4.0);
        assert_eq!(pos_end, (200.0, 200.0, 50.0));
        assert_eq!(fov_end, 80.0);
    }

    #[test]
    fn test_catmull_rom_spline_smoothness() {
        let keyframes = vec![
            CameraKeyframe {
                id: "k1".to_string(),
                name: "P1".to_string(),
                duration_to_next_sec: 2.0,
                camera_pos: (0.0, 0.0, 0.0),
                camera_target: (10.0, 10.0, 10.0),
                camera_fov: 50.0,
                easing: CameraEasing::CatmullRomSpline,
            },
            CameraKeyframe {
                id: "k2".to_string(),
                name: "P2".to_string(),
                duration_to_next_sec: 2.0,
                camera_pos: (10.0, 5.0, 2.0),
                camera_target: (10.0, 10.0, 10.0),
                camera_fov: 60.0,
                easing: CameraEasing::CatmullRomSpline,
            },
            CameraKeyframe {
                id: "k3".to_string(),
                name: "P3".to_string(),
                duration_to_next_sec: 2.0,
                camera_pos: (20.0, 0.0, 4.0),
                camera_target: (10.0, 10.0, 10.0),
                camera_fov: 70.0,
                easing: CameraEasing::CatmullRomSpline,
            },
        ];

        // Sample at 0.1s increments and verify coordinates are non-NaN, non-Inf, and continuous
        for step in 0..=40 {
            let t = step as f64 * 0.1;
            let (pos, tgt, fov) = interpolate_sequence(&keyframes, t);
            assert!(pos.0.is_finite(), "pos.x not finite at t={t}");
            assert!(pos.1.is_finite(), "pos.y not finite at t={t}");
            assert!(pos.2.is_finite(), "pos.z not finite at t={t}");
            assert!(tgt.0.is_finite(), "target.x not finite at t={t}");
            assert!(fov >= 50.0 && fov <= 70.0, "FOV out of range at t={t}: {fov}");
        }
    }
}
