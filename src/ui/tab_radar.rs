//! 2D Spatial Radar & Minimap Tab (`LLNearbyAvatars`, `LLWorld`, `LLAgent`).
//!
//! Provides top-down spatial visualization of the simulator, region boundaries,
//! current avatar heading, and real-time blips for nearby avatars with elevation deltas.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use egui::{pos2, vec2, Color32, Pos2, Rect, RichText, Sense, Stroke, Ui};
use serde_llsd::LLSDValue;

use crate::client::{AsyncRequest, LeapClient};
use crate::codec::{get_array, get_bool, get_f64, get_str};
use crate::ui::theme::{
    card_frame, inner_box_frame, COLOR_ACCENT, COLOR_CARD_BORDER,
    COLOR_SUCCESS, COLOR_TEXT_MUTED, COLOR_WARNING,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RadarViewMode {
    RegionMap, // 0..256m simulator coordinates
    Relative,  // Avatar at center with concentric range rings
}

#[derive(Debug, Clone)]
pub struct RadarAvatar {
    pub uuid: String,
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub distance: f64,
    pub is_self: bool,
    pub seated: bool,
}

pub struct RadarTab {
    avatars: Vec<RadarAvatar>,
    agent_pos: (f64, f64, f64),
    agent_yaw: f64,

    // Settings
    view_mode: RadarViewMode,
    range: f32, // meters (50..512)
    auto_refresh_sec: f32, // 0 = off, 1.0, 2.0, 5.0
    last_poll: Instant,
    search_filter: String,

    // Selection & Navigation
    selected_uuid: Option<String>,
    clicked_world_pos: Option<(f64, f64)>,

    status_msg: Option<String>,
    pending_radar: Option<AsyncRequest<LLSDValue>>,
}

impl RadarTab {
    pub fn new() -> Self {
        Self {
            avatars: Vec::new(),
            agent_pos: (128.0, 128.0, 25.0),
            agent_yaw: 0.0,
            view_mode: RadarViewMode::RegionMap,
            range: 256.0,
            auto_refresh_sec: 2.0,
            last_poll: Instant::now() - Duration::from_secs(10),
            search_filter: String::new(),
            selected_uuid: None,
            clicked_world_pos: None,
            status_msg: None,
            pending_radar: None,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, client: &LeapClient) {
        let is_connected = client.is_connected();

        // 1. Reactive telemetry update (zero latency)
        let telem = client.cached_telemetry();
        if let Some((x, y, z)) = telem.agent_pos {
            self.agent_pos = (x, y, z);
        }
        if let Some(yaw) = telem.agent_yaw {
            self.agent_yaw = yaw;
        }

        // 2. Poll in-flight non-blocking radar query
        if let Some(ref req) = self.pending_radar {
            if let Some(res) = req.poll() {
                match res {
                    Ok(reply) => {
                        let mut new_list = Vec::new();
                        if let Some(arr) = get_array(&reply, "avatars") {
                            for item in arr {
                                let uuid = get_str(item, "uuid").unwrap_or("").to_string();
                                let name = get_str(item, "name").unwrap_or("Resident").to_string();
                                let x = get_f64(item, "x").unwrap_or(0.0);
                                let y = get_f64(item, "y").unwrap_or(0.0);
                                let z = get_f64(item, "z").unwrap_or(0.0);
                                let distance = get_f64(item, "distance").unwrap_or(0.0);
                                let is_self = get_bool(item, "is_self").unwrap_or(false);
                                let seated = get_bool(item, "seated").unwrap_or(false);

                                new_list.push(RadarAvatar {
                                    uuid,
                                    name,
                                    x,
                                    y,
                                    z,
                                    distance,
                                    is_self,
                                    seated,
                                });
                            }
                        }
                        if let Some(ax) = get_f64(&reply, "agent_x") {
                            let ay = get_f64(&reply, "agent_y").unwrap_or(self.agent_pos.1);
                            let az = get_f64(&reply, "agent_z").unwrap_or(self.agent_pos.2);
                            self.agent_pos = (ax, ay, az);
                        }
                        if let Some(yaw) = get_f64(&reply, "agent_yaw") {
                            self.agent_yaw = yaw;
                        }
                        self.avatars = new_list;
                        self.status_msg = Some(format!("Radar refreshed: {} avatars nearby", self.avatars.len()));
                    }
                    Err(err) => {
                        self.status_msg = Some(format!("Radar query failed: {err}"));
                    }
                }
                self.pending_radar = None;
            }
        }

        // 3. Auto-refresh timer
        if is_connected && self.auto_refresh_sec > 0.0 && self.pending_radar.is_none() {
            if self.last_poll.elapsed() >= Duration::from_secs_f32(self.auto_refresh_sec) {
                self.fetch_radar(client);
            }
        }

        // UI Header Card
        card_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.heading("📡 Simulator 2D Radar & Minimap");
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let is_loading = self.pending_radar.is_some();
                    let btn_label = if is_loading { "⏳ Querying Radar..." } else { "🔄 Refresh Radar" };
                    if ui.add_enabled(is_connected && !is_loading, egui::Button::new(btn_label)).clicked() {
                        self.fetch_radar(client);
                    }

                    // Auto-refresh radio
                    ui.label(RichText::new("Auto-Poll:").color(COLOR_TEXT_MUTED));
                    ui.selectable_value(&mut self.auto_refresh_sec, 0.0, "Off");
                    ui.selectable_value(&mut self.auto_refresh_sec, 5.0, "5s");
                    ui.selectable_value(&mut self.auto_refresh_sec, 2.0, "2s");
                    ui.selectable_value(&mut self.auto_refresh_sec, 1.0, "1s");

                    ui.separator();

                    // Mode toggle
                    ui.selectable_value(&mut self.view_mode, RadarViewMode::RegionMap, "🗺️ Region (256m)");
                    ui.selectable_value(&mut self.view_mode, RadarViewMode::Relative, "🎯 Avatar-Centric");
                });
            });

            if let Some(ref st) = self.status_msg {
                ui.add_space(4.0);
                ui.label(RichText::new(st).color(COLOR_SUCCESS).size(11.0));
            }
        });

        ui.add_space(8.0);

        // Main Horizontal Split: Left = Canvas (440px), Right = Avatar Table & Commands
        ui.horizontal(|ui| {
            // ── Left Column: 2D Minimap Canvas ──
            ui.vertical(|ui| {
                ui.set_width(450.0);
                card_frame().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Spatial Canvas").strong().size(13.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let (x, y, z) = self.agent_pos;
                            ui.label(RichText::new(format!("You: ({x:.1}, {y:.1}, {z:.1})")).monospace().color(COLOR_ACCENT));
                        });
                    });
                    ui.add_space(6.0);

                    // Canvas Alloc
                    let canvas_size = vec2(420.0, 420.0);
                    let (response, painter) = ui.allocate_painter(canvas_size, Sense::click_and_drag());
                    let rect = response.rect;

                    // Handle Canvas Click to Walk / Teleport
                    if response.clicked() {
                        if let Some(hover_pos) = response.hover_pos() {
                            let world_coord = self.canvas_to_world(hover_pos, rect);
                            self.clicked_world_pos = Some(world_coord);
                        }
                    }

                    // Milestone 6.8: Right-click to orient camera / Double-click to walk directly
                    if response.secondary_clicked() {
                        if let Some(hover_pos) = response.hover_pos() {
                            let (wx, wy) = self.canvas_to_world(hover_pos, rect);
                            self.aim_camera_at(client, wx, wy, self.agent_pos.2 + 1.0);
                        }
                    }

                    if response.double_clicked() {
                        if let Some(hover_pos) = response.hover_pos() {
                            let (wx, wy) = self.canvas_to_world(hover_pos, rect);
                            self.send_walk(client, wx, wy);
                        }
                    }

                    // Background & Border
                    painter.rect_filled(rect, 4.0, Color32::from_rgb(18, 24, 38));
                    painter.rect_stroke(rect, 4.0, Stroke::new(1.5_f32, COLOR_CARD_BORDER));

                    match self.view_mode {
                        RadarViewMode::RegionMap => {
                            self.paint_region_map(&painter, rect);
                        }
                        RadarViewMode::Relative => {
                            self.paint_relative_radar(&painter, rect);
                        }
                    }

                    ui.add_space(6.0);

                    // Canvas Click Action Bar
                    if let Some((cx, cy)) = self.clicked_world_pos {
                        inner_box_frame().show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(format!("Target: ({cx:.1}, {cy:.1})")).monospace().strong());
                                if ui.add_enabled(is_connected, egui::Button::new("🚶 Walk Here").small()).clicked() {
                                    self.send_walk(client, cx, cy);
                                }
                                if ui.add_enabled(is_connected, egui::Button::new("⚡ Teleport Here").small()).clicked() {
                                    self.send_teleport(client, cx, cy);
                                }
                                if ui.small_button("✖").clicked() {
                                    self.clicked_world_pos = None;
                                }
                            });
                        });
                    } else {
                        ui.label(RichText::new("💡 Click anywhere on the map to set target coordinates for walking or teleporting.").color(COLOR_TEXT_MUTED).size(11.0));
                    }
                });
            });

            ui.add_space(8.0);

            // ── Right Column: Avatar Table & Actions ──
            ui.vertical(|ui| {
                card_frame().show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("Nearby Avatars ({})", self.avatars.len())).strong().size(13.0));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.add(egui::TextEdit::singleline(&mut self.search_filter).hint_text("🔍 Search name or UUID...").desired_width(180.0));
                        });
                    });

                    ui.add_space(8.0);

                    // Avatar Table
                    let filtered: Vec<&RadarAvatar> = self.avatars.iter()
                        .filter(|a| {
                            if self.search_filter.is_empty() {
                                true
                            } else {
                                a.name.to_lowercase().contains(&self.search_filter.to_lowercase())
                                    || a.uuid.to_lowercase().contains(&self.search_filter.to_lowercase())
                            }
                        })
                        .collect();

                    let mut action_aim = None;
                    let mut action_tp = None;
                    let mut action_copy = None;
                    let mut new_selection = None;

                    egui::ScrollArea::vertical().max_height(350.0).show(ui, |ui| {
                        egui::Grid::new("radar_avatars_grid")
                            .num_columns(6)
                            .striped(true)
                            .spacing([10.0, 6.0])
                            .show(ui, |ui| {
                                ui.label(RichText::new("Name").strong().color(COLOR_TEXT_MUTED));
                                ui.label(RichText::new("Distance").strong().color(COLOR_TEXT_MUTED));
                                ui.label(RichText::new("Elevation").strong().color(COLOR_TEXT_MUTED));
                                ui.label(RichText::new("Position (X, Y, Z)").strong().color(COLOR_TEXT_MUTED));
                                ui.label(RichText::new("Status").strong().color(COLOR_TEXT_MUTED));
                                ui.label(RichText::new("Actions").strong().color(COLOR_TEXT_MUTED));
                                ui.end_row();

                                for av in &filtered {
                                    let is_selected = self.selected_uuid.as_deref() == Some(&av.uuid);

                                    // 1. Name
                                    let name_text = if av.is_self {
                                        RichText::new(format!("{} (You)", av.name)).strong().color(COLOR_SUCCESS)
                                    } else {
                                        RichText::new(&av.name).strong()
                                    };
                                    if ui.selectable_label(is_selected, name_text).clicked() {
                                        new_selection = Some(av.uuid.clone());
                                    }

                                    // 2. Distance
                                    ui.label(RichText::new(format!("{:.1} m", av.distance)).monospace());

                                    // 3. Elevation delta
                                    let dz = av.z - self.agent_pos.2;
                                    let (elev_icon, elev_color) = if dz > 2.0 {
                                        ("▲ Above", COLOR_WARNING)
                                    } else if dz < -2.0 {
                                        ("▼ Below", COLOR_ACCENT)
                                    } else {
                                        ("● Level", COLOR_TEXT_MUTED)
                                    };
                                    ui.label(RichText::new(format!("{elev_icon} ({dz:+.1}m)")).color(elev_color).size(11.0));

                                    // 4. Coordinates
                                    ui.label(RichText::new(format!("({:.1}, {:.1}, {:.1})", av.x, av.y, av.z)).monospace());

                                    // 5. Status
                                    let status_text = if av.seated { "🪑 Seated" } else { "🧍 Standing" };
                                    ui.label(RichText::new(status_text).size(11.0));

                                    // 6. Actions
                                    ui.horizontal(|ui| {
                                        if !av.is_self {
                                            if ui.small_button("🎯 Aim Cam").clicked() {
                                                action_aim = Some((av.x, av.y, av.z));
                                            }
                                            if ui.small_button("⚡ TP To").clicked() {
                                                action_tp = Some((av.x, av.y));
                                            }
                                        }
                                        if ui.small_button("📋").on_hover_text("Copy UUID").clicked() {
                                            action_copy = Some((av.uuid.clone(), av.name.clone()));
                                        }
                                    });

                                    ui.end_row();
                                }
                            });

                        if filtered.is_empty() {
                            ui.add_space(20.0);
                            ui.vertical_centered(|ui| {
                                ui.label(RichText::new("No avatars detected within range.").color(COLOR_TEXT_MUTED).italics());
                            });
                        }
                    });

                    // Execute deferred actions from grid
                    if let Some(uuid) = new_selection {
                        self.selected_uuid = Some(uuid);
                    }
                    if let Some((x, y, z)) = action_aim {
                        self.aim_camera_at(client, x, y, z);
                    }
                    if let Some((x, y)) = action_tp {
                        self.send_teleport(client, x, y);
                    }
                    if let Some((uuid, name)) = action_copy {
                        ui.output_mut(|o| o.copied_text = uuid);
                        self.status_msg = Some(format!("Copied UUID for {name}"));
                    }

                    // Selected avatar details box
                    let mut action_walk_avatar = None;
                    if let Some(ref sel_id) = self.selected_uuid {
                        if let Some(av) = self.avatars.iter().find(|a| &a.uuid == sel_id) {
                            let name = av.name.clone();
                            let uuid = av.uuid.clone();
                            let pos = (av.x, av.y);
                            ui.add_space(8.0);
                            inner_box_frame().show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(format!("Selected: {name}")).strong().color(COLOR_ACCENT));
                                    ui.label(RichText::new(&uuid).monospace().size(11.0));
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.small_button("🚶 Walk to Avatar").clicked() {
                                            action_walk_avatar = Some(pos);
                                        }
                                    });
                                });
                            });
                        }
                    }
                    if let Some((x, y)) = action_walk_avatar {
                        self.send_walk(client, x, y);
                    }
                });
            });
        });
    }

    fn fetch_radar(&mut self, client: &LeapClient) {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("getNearby".to_string()));
        args.insert("max_distance".to_string(), LLSDValue::Real(self.range as f64));
        self.pending_radar = Some(client.request_raw_async("LLNearbyAvatars", args, Duration::from_secs(4)));
        self.last_poll = Instant::now();
    }

    fn send_walk(&mut self, client: &LeapClient, x: f64, y: f64) {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("move".to_string()));
        args.insert("target_x".to_string(), LLSDValue::Real(x));
        args.insert("target_y".to_string(), LLSDValue::Real(y));
        let _ = client.send("LLAgent", args);
        self.status_msg = Some(format!("Dispatched walk towards ({x:.1}, {y:.1})"));
    }

    fn send_teleport(&mut self, client: &LeapClient, x: f64, y: f64) {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("teleport".to_string()));
        args.insert("x".to_string(), LLSDValue::Real(x));
        args.insert("y".to_string(), LLSDValue::Real(y));
        args.insert("z".to_string(), LLSDValue::Real(self.agent_pos.2));
        let _ = client.send("LLAgent", args);
        self.status_msg = Some(format!("Dispatched teleport to ({x:.1}, {y:.1})"));
    }

    fn aim_camera_at(&mut self, client: &LeapClient, x: f64, y: f64, z: f64) {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("look_at".to_string()));
        args.insert("x".to_string(), LLSDValue::Real(x));
        args.insert("y".to_string(), LLSDValue::Real(y));
        args.insert("z".to_string(), LLSDValue::Real(z));
        let _ = client.send("LLCamera", args);
        self.status_msg = Some(format!("Aimed camera at target ({x:.1}, {y:.1}, {z:.1})"));
    }

    fn canvas_to_world(&self, hover_pos: Pos2, rect: Rect) -> (f64, f64) {
        match self.view_mode {
            RadarViewMode::RegionMap => {
                let norm_x = ((hover_pos.x - rect.min.x) / rect.width()).clamp(0.0, 1.0) as f64;
                let norm_y = ((rect.max.y - hover_pos.y) / rect.height()).clamp(0.0, 1.0) as f64; // Invert Y (SL Y points North)
                (norm_x * 256.0, norm_y * 256.0)
            }
            RadarViewMode::Relative => {
                let center = rect.center();
                let scale = (rect.width() / 2.0) / self.range;
                let dx = ((hover_pos.x - center.x) / scale) as f64;
                let dy = ((center.y - hover_pos.y) / scale) as f64; // SL Y points North
                (self.agent_pos.0 + dx, self.agent_pos.1 + dy)
            }
        }
    }

    fn world_to_canvas(&self, x: f64, y: f64, rect: Rect) -> Pos2 {
        match self.view_mode {
            RadarViewMode::RegionMap => {
                let px = rect.min.x + (x as f32 / 256.0) * rect.width();
                let py = rect.max.y - (y as f32 / 256.0) * rect.height(); // SL Y points North
                pos2(px, py)
            }
            RadarViewMode::Relative => {
                let center = rect.center();
                let scale = (rect.width() / 2.0) / self.range;
                let dx = (x - self.agent_pos.0) as f32;
                let dy = (y - self.agent_pos.1) as f32;
                pos2(center.x + dx * scale, center.y - dy * scale)
            }
        }
    }

    fn paint_region_map(&self, painter: &egui::Painter, rect: Rect) {
        // Grid lines every 64m
        let grid_stroke = Stroke::new(0.5_f32, Color32::from_rgb(35, 45, 68));
        for i in 1..4 {
            let frac = i as f32 / 4.0;
            // Vertical lines (East)
            let x = rect.min.x + frac * rect.width();
            painter.line_segment([pos2(x, rect.min.y), pos2(x, rect.max.y)], grid_stroke);
            // Horizontal lines (North)
            let y = rect.min.y + frac * rect.height();
            painter.line_segment([pos2(rect.min.x, y), pos2(rect.max.x, y)], grid_stroke);
        }

        // Compass Labels
        let label_color = Color32::from_rgb(90, 110, 150);
        painter.text(pos2(rect.center().x, rect.min.y + 10.0), egui::Align2::CENTER_CENTER, "N", egui::FontId::monospace(11.0), label_color);
        painter.text(pos2(rect.center().x, rect.max.y - 10.0), egui::Align2::CENTER_CENTER, "S", egui::FontId::monospace(11.0), label_color);
        painter.text(pos2(rect.max.x - 10.0, rect.center().y), egui::Align2::CENTER_CENTER, "E", egui::FontId::monospace(11.0), label_color);
        painter.text(pos2(rect.min.x + 10.0, rect.center().y), egui::Align2::CENTER_CENTER, "W", egui::FontId::monospace(11.0), label_color);

        // Corner Coordinates
        painter.text(pos2(rect.min.x + 12.0, rect.max.y - 8.0), egui::Align2::LEFT_BOTTOM, "(0,0)", egui::FontId::monospace(9.0), label_color);
        painter.text(pos2(rect.max.x - 8.0, rect.min.y + 12.0), egui::Align2::RIGHT_TOP, "(256,256)", egui::FontId::monospace(9.0), label_color);

        // Click marker
        if let Some((cx, cy)) = self.clicked_world_pos {
            let cp = self.world_to_canvas(cx, cy, rect);
            painter.circle_stroke(cp, 6.0, Stroke::new(1.5_f32, COLOR_WARNING));
            painter.line_segment([pos2(cp.x - 8.0, cp.y), pos2(cp.x + 8.0, cp.y)], Stroke::new(1.0_f32, COLOR_WARNING));
            painter.line_segment([pos2(cp.x, cp.y - 8.0), pos2(cp.x, cp.y + 8.0)], Stroke::new(1.0_f32, COLOR_WARNING));
        }

        // Render other avatars
        for av in &self.avatars {
            if av.is_self {
                continue;
            }
            let pt = self.world_to_canvas(av.x, av.y, rect);
            let is_sel = self.selected_uuid.as_deref() == Some(&av.uuid);

            let blip_color = if is_sel { Color32::from_rgb(255, 200, 50) } else { Color32::from_rgb(100, 180, 255) };
            painter.circle_filled(pt, if is_sel { 5.5 } else { 4.0 }, blip_color);

            // Elevation Indicator
            let dz = av.z - self.agent_pos.2;
            let elev_marker = if dz > 2.0 { "▲" } else if dz < -2.0 { "▼" } else { "" };
            let label = if elev_marker.is_empty() {
                format!("{} ({:.0}m)", av.name.split_whitespace().next().unwrap_or(""), av.distance)
            } else {
                format!("{elev_marker} {} ({:.0}m)", av.name.split_whitespace().next().unwrap_or(""), av.distance)
            };
            painter.text(pos2(pt.x + 6.0, pt.y - 4.0), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(10.0), Color32::from_rgb(200, 220, 255));
        }

        // Render Self Avatar
        let my_pt = self.world_to_canvas(self.agent_pos.0, self.agent_pos.1, rect);
        painter.circle_filled(my_pt, 6.0, COLOR_SUCCESS);
        painter.circle_stroke(my_pt, 8.5, Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(46, 204, 113, 100)));

        // Heading vector (SL yaw: 0 = East, PI/2 = North)
        let (sin_y, cos_y) = self.agent_yaw.sin_cos();
        let head_vec = vec2(cos_y as f32, -(sin_y as f32)) * 14.0;
        painter.line_segment([my_pt, my_pt + head_vec], Stroke::new(2.0_f32, COLOR_SUCCESS));
    }

    fn paint_relative_radar(&self, painter: &egui::Painter, rect: Rect) {
        let center = rect.center();
        let max_r = rect.width() / 2.0;

        // Concentric Rings: 25%, 50%, 75%, 100% of range
        let ring_stroke = Stroke::new(0.5_f32, Color32::from_rgb(35, 45, 68));
        for i in 1..=4 {
            let r = max_r * (i as f32 / 4.0);
            painter.circle_stroke(center, r, ring_stroke);
            let dist_label = format!("{:.0}m", self.range * (i as f32 / 4.0));
            painter.text(pos2(center.x + r - 12.0, center.y - 6.0), egui::Align2::LEFT_BOTTOM, dist_label, egui::FontId::monospace(9.0), Color32::from_rgb(70, 90, 120));
        }

        // Crosshairs
        painter.line_segment([pos2(center.x, rect.min.y), pos2(center.x, rect.max.y)], ring_stroke);
        painter.line_segment([pos2(rect.min.x, center.y), pos2(rect.max.x, center.y)], ring_stroke);

        // Render other avatars
        for av in &self.avatars {
            if av.is_self {
                continue;
            }
            let pt = self.world_to_canvas(av.x, av.y, rect);
            if rect.contains(pt) {
                let is_sel = self.selected_uuid.as_deref() == Some(&av.uuid);
                let blip_color = if is_sel { Color32::from_rgb(255, 200, 50) } else { Color32::from_rgb(100, 180, 255) };
                painter.circle_filled(pt, if is_sel { 5.5 } else { 4.0 }, blip_color);

                let dz = av.z - self.agent_pos.2;
                let elev_marker = if dz > 2.0 { "▲" } else if dz < -2.0 { "▼" } else { "" };
                let label = format!("{elev_marker} {} ({:.0}m)", av.name.split_whitespace().next().unwrap_or(""), av.distance);
                painter.text(pos2(pt.x + 6.0, pt.y - 4.0), egui::Align2::LEFT_CENTER, label, egui::FontId::proportional(10.0), Color32::from_rgb(200, 220, 255));
            }
        }

        // Center Self Avatar
        painter.circle_filled(center, 6.0, COLOR_SUCCESS);
        painter.circle_stroke(center, 8.5, Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(46, 204, 113, 100)));

        // Heading vector
        let (sin_y, cos_y) = self.agent_yaw.sin_cos();
        let head_vec = vec2(cos_y as f32, -(sin_y as f32)) * 14.0;
        painter.line_segment([center, center + head_vec], Stroke::new(2.0_f32, COLOR_SUCCESS));
    }
}
