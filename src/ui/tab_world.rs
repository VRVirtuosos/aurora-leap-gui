//! Avatar, Camera, World & Inventory Telemetry Tab (`LLAgent`, `LLCamera`, `LLWorld`, `LLInventory`).

use std::collections::HashMap;
use std::time::Duration;

use egui::{RichText, Ui};
use serde_llsd::LLSDValue;

use crate::client::{AsyncRequest, LeapClient};
use crate::codec::{get_array, get_f64, get_i32, get_str, parse_vec3};
use crate::ui::theme::{card_frame, inner_box_frame, COLOR_ACCENT, COLOR_SUCCESS, COLOR_TEXT_MUTED, COLOR_WARNING};

pub struct WorldTab {
    // Agent
    agent_id: Option<String>,
    agent_pos: Option<(f64, f64, f64)>,

    // Camera
    camera_pos: Option<(f64, f64, f64)>,
    camera_target: Option<(f64, f64, f64)>,
    camera_fov: Option<f64>,

    // Region & Simulator
    region_name: Option<String>,
    region_id: Option<String>,
    water_height: Option<f64>,
    region_size: Option<(i32, i32)>,
    product_name: Option<String>,
    sim_fps: Option<f64>,
    sim_uptime: Option<f64>,
    sim_avatars: Option<i32>,

    // Parcel
    parcel_name: Option<String>,
    parcel_desc: Option<String>,
    parcel_area: Option<i32>,
    parcel_owner: Option<String>,

    // Inventory
    inv_root_uuid: Option<String>,
    inv_owner_uuid: Option<String>,

    // Milestone 3: Animation, Appearance, Objects
    active_anims: Vec<(String, f64)>, // (UUID, duration)
    worn_attachments: Vec<(i32, String, String)>, // (point, name, uuid)
    worn_clothing_count: usize,
    cof_uuid: Option<String>,
    nearby_objects_count: usize,
    closest_object: Option<(String, f64, i32)>, // (name, distance, local_id)

    status_msg: Option<String>,

    // Async requests
    pending_agent: Option<AsyncRequest<LLSDValue>>,
    pending_camera: Option<AsyncRequest<LLSDValue>>,
    pending_region: Option<AsyncRequest<LLSDValue>>,
    pending_parcel: Option<AsyncRequest<LLSDValue>>,
    pending_stats: Option<AsyncRequest<LLSDValue>>,
    pending_inventory: Option<AsyncRequest<LLSDValue>>,
    pending_animations: Option<AsyncRequest<LLSDValue>>,
    pending_appearance: Option<AsyncRequest<LLSDValue>>,
    pending_objects: Option<AsyncRequest<LLSDValue>>,
}

impl WorldTab {
    pub fn new() -> Self {
        Self {
            agent_id: None,
            agent_pos: None,
            camera_pos: None,
            camera_target: None,
            camera_fov: None,
            region_name: None,
            region_id: None,
            water_height: None,
            region_size: None,
            product_name: None,
            sim_fps: None,
            sim_uptime: None,
            sim_avatars: None,
            parcel_name: None,
            parcel_desc: None,
            parcel_area: None,
            parcel_owner: None,
            inv_root_uuid: None,
            inv_owner_uuid: None,
            active_anims: Vec::new(),
            worn_attachments: Vec::new(),
            worn_clothing_count: 0,
            cof_uuid: None,
            nearby_objects_count: 0,
            closest_object: None,
            status_msg: None,
            pending_agent: None,
            pending_camera: None,
            pending_region: None,
            pending_parcel: None,
            pending_stats: None,
            pending_inventory: None,
            pending_animations: None,
            pending_appearance: None,
            pending_objects: None,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, client: &LeapClient) {
        let is_connected = client.is_connected();

        // 1. Reactive telemetry update (zero latency)
        let telem = client.cached_telemetry();
        if let Some(ref id) = telem.agent_id {
            self.agent_id = Some(id.clone());
        }
        if let Some((x, y, z)) = telem.agent_pos {
            self.agent_pos = Some((x, y, z));
        }
        if let Some(pos) = telem.camera_pos {
            self.camera_pos = Some(pos);
        }
        if let Some(tgt) = telem.camera_target {
            self.camera_target = Some(tgt);
        }
        if let Some(fov) = telem.camera_fov {
            self.camera_fov = Some(fov);
        }
        if let Some(ref reg) = telem.region_name {
            self.region_name = Some(reg.clone());
        }
        if let Some(ref reg_id) = telem.region_id {
            self.region_id = Some(reg_id.clone());
        }
        if let Some(wh) = telem.water_height {
            self.water_height = Some(wh);
        }
        if let Some(ref pn) = telem.parcel_name {
            self.parcel_name = Some(pn.clone());
        }
        if let Some(pa) = telem.parcel_area {
            self.parcel_area = Some(pa);
        }
        if let Some(fps) = telem.sim_fps {
            self.sim_fps = Some(fps);
        }

        // 2. Poll in-flight non-blocking requests
        if let Some(ref req) = self.pending_agent {
            if let Some(res) = req.poll() {
                if let Ok(reply) = res {
                    if let Some(id) = get_str(&reply, "agent_id") {
                        self.agent_id = Some(id.to_string());
                    }
                    let x = get_f64(&reply, "x").unwrap_or(0.0);
                    let y = get_f64(&reply, "y").unwrap_or(0.0);
                    let z = get_f64(&reply, "z").unwrap_or(0.0);
                    if x != 0.0 || y != 0.0 || z != 0.0 {
                        self.agent_pos = Some((x, y, z));
                    }
                }
                self.pending_agent = None;
            }
        }

        if let Some(ref req) = self.pending_camera {
            if let Some(res) = req.poll() {
                match res {
                    Ok(reply) => {
                        let mut updated = false;
                        if let Some(pos) = parse_vec3(&reply, Some("position"))
                            .or_else(|| parse_vec3(&reply, Some("camera_pos")))
                        {
                            self.camera_pos = Some(pos);
                            updated = true;
                        }
                        if let Some(target) = parse_vec3(&reply, Some("target"))
                            .or_else(|| parse_vec3(&reply, Some("camera_target")))
                        {
                            self.camera_target = Some(target);
                            updated = true;
                        }
                        if let Some(fov) = get_f64(&reply, "fov") {
                            let deg = if fov > 0.0 && fov < 3.2 { fov.to_degrees() } else { fov };
                            self.camera_fov = Some(deg);
                            updated = true;
                        }
                        if updated {
                            self.status_msg = Some("Camera telemetry updated".to_string());
                        } else {
                            self.status_msg = Some("Camera query returned empty data".to_string());
                        }
                    }
                    Err(err) => {
                        self.status_msg = Some(format!("Camera query failed: {err}"));
                    }
                }
                self.pending_camera = None;
            }
        }

        // Auto-query once when tab is viewed if connected and camera has not yet loaded
        if is_connected && self.camera_pos.is_none() && self.pending_camera.is_none() {
            self.fetch_camera(client);
        }

        if let Some(ref req) = self.pending_region {
            if let Some(res) = req.poll() {
                if let Ok(reply) = res {
                    if let Some(name) = get_str(&reply, "name") {
                        self.region_name = Some(name.to_string());
                    }
                    if let Some(id) = get_str(&reply, "region_id") {
                        self.region_id = Some(id.to_string());
                    }
                    if let Some(wh) = get_f64(&reply, "water_height") {
                        self.water_height = Some(wh);
                    }
                    let sx = get_i32(&reply, "size_x").unwrap_or(256);
                    let sy = get_i32(&reply, "size_y").unwrap_or(256);
                    self.region_size = Some((sx, sy));
                    if let Some(prod) = get_str(&reply, "product_name") {
                        self.product_name = Some(prod.to_string());
                    }
                    self.status_msg = Some("Region information updated".to_string());
                }
                self.pending_region = None;
            }
        }

        if let Some(ref req) = self.pending_parcel {
            if let Some(res) = req.poll() {
                if let Ok(reply) = res {
                    if let Some(name) = get_str(&reply, "name").or_else(|| get_str(&reply, "parcel_name")) {
                        self.parcel_name = Some(name.to_string());
                    }
                    if let Some(desc) = get_str(&reply, "desc") {
                        self.parcel_desc = Some(desc.to_string());
                    }
                    if let Some(area) = get_i32(&reply, "area") {
                        self.parcel_area = Some(area);
                    }
                    if let Some(owner) = get_str(&reply, "owner_id") {
                        self.parcel_owner = Some(owner.to_string());
                    }
                    self.status_msg = Some("Parcel information updated".to_string());
                }
                self.pending_parcel = None;
            }
        }

        if let Some(ref req) = self.pending_stats {
            if let Some(res) = req.poll() {
                if let Ok(reply) = res {
                    if let Some(fps) = get_f64(&reply, "fps") {
                        self.sim_fps = Some(fps);
                    }
                    if let Some(up) = get_f64(&reply, "uptime") {
                        self.sim_uptime = Some(up);
                    }
                    if let Some(av) = get_i32(&reply, "avatar_count") {
                        self.sim_avatars = Some(av);
                    }
                    self.status_msg = Some("Simulator statistics updated".to_string());
                }
                self.pending_stats = None;
            }
        }

        if let Some(ref req) = self.pending_inventory {
            if let Some(res) = req.poll() {
                if let Ok(reply) = res {
                    self.inv_root_uuid = get_str(&reply, "root").map(|s| s.to_string());
                    self.inv_owner_uuid = get_str(&reply, "owner").map(|s| s.to_string());
                }
                self.pending_inventory = None;
            }
        }

        if let Some(ref req) = self.pending_animations {
            if let Some(res) = req.poll() {
                if let Ok(reply) = res {
                    let mut anims = Vec::new();
                    if let Some(arr) = get_array(&reply, "animations") {
                        for item in arr {
                            if let Some(uuid) = get_str(item, "uuid") {
                                let dur = get_f64(item, "duration_sec").unwrap_or(0.0);
                                anims.push((uuid.to_string(), dur));
                            }
                        }
                    }
                    self.active_anims = anims;
                    self.status_msg = Some(format!("Updated active animations ({})", self.active_anims.len()));
                }
                self.pending_animations = None;
            }
        }

        if let Some(ref req) = self.pending_appearance {
            if let Some(res) = req.poll() {
                if let Ok(reply) = res {
                    self.cof_uuid = get_str(&reply, "cof_id").map(|s| s.to_string());
                    self.worn_clothing_count = get_i32(&reply, "clothing_count").unwrap_or(0) as usize;
                    let mut atts = Vec::new();
                    if let Some(arr) = get_array(&reply, "attachments") {
                        for item in arr {
                            let pt = get_i32(item, "attachment_point").unwrap_or(0);
                            let name = get_str(item, "name").unwrap_or("Attachment").to_string();
                            let uuid = get_str(item, "uuid").unwrap_or("").to_string();
                            atts.push((pt, name, uuid));
                        }
                    }
                    self.worn_attachments = atts;
                    self.status_msg = Some("Avatar appearance details updated".to_string());
                }
                self.pending_appearance = None;
            }
        }

        if let Some(ref req) = self.pending_objects {
            if let Some(res) = req.poll() {
                if let Ok(reply) = res {
                    if let Some(arr) = get_array(&reply, "objects") {
                        self.nearby_objects_count = arr.len();
                        if let Some(first) = arr.first() {
                            let name = get_str(first, "name").unwrap_or("Primitive").to_string();
                            let dist = get_f64(first, "distance").unwrap_or(0.0);
                            let local_id = get_i32(first, "local_id").unwrap_or(0);
                            self.closest_object = Some((name, dist, local_id));
                        } else {
                            self.closest_object = None;
                        }
                    }
                    self.status_msg = Some(format!("Discovered {} nearby primitives", self.nearby_objects_count));
                }
                self.pending_objects = None;
            }
        }

        ui.vertical(|ui| {
            card_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("🧭 Avatar, Camera, World & Simulator Telemetry");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let refresh_btn = egui::Button::new(RichText::new("🔄 Refresh All Telemetry").size(13.0));
                        if ui.add_enabled(is_connected, refresh_btn).clicked() {
                            self.fetch_all(client);
                        }
                    });
                });

                if let Some(ref st) = self.status_msg {
                    ui.add_space(4.0);
                    ui.label(RichText::new(st).color(COLOR_SUCCESS).size(11.0));
                }
            });

            ui.add_space(8.0);

            // Row 1: Agent, Camera, Region
            ui.columns(3, |cols| {
                // Column 0: LLAgent
                cols[0].vertical(|ui| {
                    card_frame().show(ui, |ui| {
                        let is_loading = self.pending_agent.is_some();
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Avatar Telemetry (`LLAgent`)").strong().size(13.0));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let label = if is_loading { "⏳" } else { "Query" };
                                if ui.add_enabled(is_connected && !is_loading, egui::Button::new(label).small()).clicked() {
                                    self.fetch_agent(client);
                                }
                            });
                        });
                        ui.add_space(8.0);

                        inner_box_frame().show(ui, |ui| {
                            ui.label(RichText::new("Agent UUID:").color(COLOR_TEXT_MUTED).size(11.0));
                            ui.label(RichText::new(self.agent_id.as_deref().unwrap_or("Not queried")).monospace().color(COLOR_ACCENT));
                        });
                        ui.add_space(4.0);

                        inner_box_frame().show(ui, |ui| {
                            ui.label(RichText::new("Region Coordinates:").color(COLOR_TEXT_MUTED).size(11.0));
                            if let Some((x, y, z)) = self.agent_pos {
                                ui.label(RichText::new(format!("X: {x:.2}\nY: {y:.2}\nZ: {z:.2}")).monospace());
                            } else {
                                ui.label(RichText::new("Unknown / Off-region").color(COLOR_TEXT_MUTED));
                            }
                        });
                    });
                });

                // Column 1: LLCamera
                cols[1].vertical(|ui| {
                    card_frame().show(ui, |ui| {
                        let is_cam_loading = self.pending_camera.is_some();
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Camera Telemetry (`LLCamera`)").strong().size(13.0));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let label = if is_cam_loading { "⏳ Querying..." } else { "Query" };
                                if ui.add_enabled(is_connected && !is_cam_loading, egui::Button::new(label).small()).clicked() {
                                    self.fetch_camera(client);
                                    self.status_msg = Some("Querying camera telemetry...".to_string());
                                }
                            });
                        });
                        ui.add_space(8.0);

                        inner_box_frame().show(ui, |ui| {
                            ui.label(RichText::new("Camera Position:").color(COLOR_TEXT_MUTED).size(11.0));
                            if let Some((x, y, z)) = self.camera_pos {
                                ui.label(RichText::new(format!("X: {x:.2}, Y: {y:.2}, Z: {z:.2}")).monospace());
                            } else if is_cam_loading {
                                ui.label(RichText::new("Querying viewer...").color(COLOR_WARNING));
                            } else {
                                ui.label(RichText::new("Not queried").color(COLOR_TEXT_MUTED));
                            }
                        });
                        ui.add_space(4.0);

                        inner_box_frame().show(ui, |ui| {
                            ui.label(RichText::new("Focus Target & FOV:").color(COLOR_TEXT_MUTED).size(11.0));
                            if let Some((tx, ty, tz)) = self.camera_target {
                                let fov_text = if let Some(fov) = self.camera_fov {
                                    format!("{fov:.1}°")
                                } else {
                                    "60.0°".to_string()
                                };
                                ui.label(RichText::new(format!("Target: ({tx:.2}, {ty:.2}, {tz:.2})\nFOV: {fov_text}")).monospace());
                            } else if let Some(fov) = self.camera_fov {
                                ui.label(RichText::new(format!("FOV: {fov:.1}°")).monospace());
                            } else if is_cam_loading {
                                ui.label(RichText::new("Querying viewer...").color(COLOR_WARNING));
                            } else {
                                ui.label(RichText::new("Not queried").color(COLOR_TEXT_MUTED));
                            }
                        });
                        ui.add_space(6.0);

                        if ui.add_enabled(is_connected, egui::Button::new("Reset Camera View")).clicked() {
                            let mut args = HashMap::new();
                            args.insert("op".to_string(), LLSDValue::String("reset".to_string()));
                            let _ = client.send("LLCamera", args);
                            self.fetch_camera(client);
                        }
                    });
                });

                // Column 2: LLWorld Region
                cols[2].vertical(|ui| {
                    card_frame().show(ui, |ui| {
                        let is_reg_loading = self.pending_region.is_some();
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Region Info (`LLWorld`)").strong().size(13.0));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let label = if is_reg_loading { "⏳" } else { "Query" };
                                if ui.add_enabled(is_connected && !is_reg_loading, egui::Button::new(label).small()).clicked() {
                                    self.fetch_region(client);
                                }
                            });
                        });
                        ui.add_space(8.0);

                        inner_box_frame().show(ui, |ui| {
                            ui.label(RichText::new("Region Name & Size:").color(COLOR_TEXT_MUTED).size(11.0));
                            let reg_str = self.region_name.as_deref().unwrap_or("Unknown / Not Queried");
                            let size_str = if let Some((sx, sy)) = self.region_size {
                                format!("{sx}m × {sy}m")
                            } else {
                                "256m × 256m".to_string()
                            };
                            ui.label(RichText::new(format!("{reg_str} ({size_str})")).monospace().strong().color(COLOR_ACCENT));
                        });
                        ui.add_space(4.0);

                        inner_box_frame().show(ui, |ui| {
                            ui.label(RichText::new("Water Elevation:").color(COLOR_TEXT_MUTED).size(11.0));
                            if let Some(wh) = self.water_height {
                                ui.label(RichText::new(format!("{wh:.1} meters")).monospace());
                            } else {
                                ui.label(RichText::new("20.0 meters (default)").color(COLOR_TEXT_MUTED));
                            }
                        });
                        ui.add_space(4.0);

                        inner_box_frame().show(ui, |ui| {
                            ui.label(RichText::new("Product / Estate:").color(COLOR_TEXT_MUTED).size(11.0));
                            let prod = self.product_name.as_deref().unwrap_or("Mainland / Full Region");
                            ui.label(RichText::new(prod).monospace().size(11.0));
                        });
                    });
                });
            });

            ui.add_space(10.0);

            // Row 2: Parcel, Simulator Stats, Inventory
            ui.columns(3, |cols| {
                // Column 0: LLWorld Parcel
                cols[0].vertical(|ui| {
                    card_frame().show(ui, |ui| {
                        let is_parc_loading = self.pending_parcel.is_some();
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Parcel Info (`LLWorld`)").strong().size(13.0));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let label = if is_parc_loading { "⏳" } else { "Query" };
                                if ui.add_enabled(is_connected && !is_parc_loading, egui::Button::new(label).small()).clicked() {
                                    self.fetch_parcel(client);
                                }
                            });
                        });
                        ui.add_space(8.0);

                        inner_box_frame().show(ui, |ui| {
                            ui.label(RichText::new("Parcel Name:").color(COLOR_TEXT_MUTED).size(11.0));
                            let p_name = self.parcel_name.as_deref().unwrap_or("Not queried");
                            ui.label(RichText::new(p_name).monospace().strong());
                        });
                        ui.add_space(4.0);

                        inner_box_frame().show(ui, |ui| {
                            ui.label(RichText::new("Area & Ownership:").color(COLOR_TEXT_MUTED).size(11.0));
                            let area_str = self.parcel_area.map(|a| format!("{a} m²")).unwrap_or_else(|| "Unknown".to_string());
                            let owner_str = self.parcel_owner.as_deref().unwrap_or("Unknown");
                            ui.label(RichText::new(format!("Area: {area_str}\nOwner: {owner_str}")).monospace().size(11.0));
                        });
                        ui.add_space(4.0);

                        if let Some(ref desc) = self.parcel_desc {
                            if !desc.is_empty() {
                                inner_box_frame().show(ui, |ui| {
                                    ui.label(RichText::new("Description:").color(COLOR_TEXT_MUTED).size(11.0));
                                    ui.label(RichText::new(desc).size(11.0));
                                });
                            }
                        }
                    });
                });

                // Column 1: LLSimulator Stats
                cols[1].vertical(|ui| {
                    card_frame().show(ui, |ui| {
                        let is_stats_loading = self.pending_stats.is_some();
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Simulator Stats (`LLSimulator`)").strong().size(13.0));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let label = if is_stats_loading { "⏳" } else { "Query" };
                                if ui.add_enabled(is_connected && !is_stats_loading, egui::Button::new(label).small()).clicked() {
                                    self.fetch_stats(client);
                                }
                            });
                        });
                        ui.add_space(8.0);

                        inner_box_frame().show(ui, |ui| {
                            ui.label(RichText::new("Simulator Frame Rate:").color(COLOR_TEXT_MUTED).size(11.0));
                            if let Some(fps) = self.sim_fps {
                                let fps_color = if fps >= 45.0 { COLOR_SUCCESS } else { COLOR_WARNING };
                                ui.label(RichText::new(format!("{fps:.1} FPS")).monospace().strong().color(fps_color));
                            } else {
                                ui.label(RichText::new("Not queried").color(COLOR_TEXT_MUTED));
                            }
                        });
                        ui.add_space(4.0);

                        inner_box_frame().show(ui, |ui| {
                            ui.label(RichText::new("Simulator Metrics:").color(COLOR_TEXT_MUTED).size(11.0));
                            let up = self.sim_uptime.map(|u| format!("{:.0}s", u)).unwrap_or_else(|| "N/A".to_string());
                            let av = self.sim_avatars.map(|a| a.to_string()).unwrap_or_else(|| "N/A".to_string());
                            ui.label(RichText::new(format!("Uptime: {up}\nAvatars in Region: {av}")).monospace().size(11.0));
                        });
                    });
                });

                // Column 2: LLInventory
                cols[2].vertical(|ui| {
                    card_frame().show(ui, |ui| {
                        let is_inv_loading = self.pending_inventory.is_some();
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Inventory (`LLInventory`)").strong().size(13.0));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let label = if is_inv_loading { "⏳" } else { "Query" };
                                if ui.add_enabled(is_connected && !is_inv_loading, egui::Button::new(label).small()).clicked() {
                                    self.fetch_inventory(client);
                                }
                            });
                        });
                        ui.add_space(8.0);

                        inner_box_frame().show(ui, |ui| {
                            ui.label(RichText::new("Root Folder UUID:").color(COLOR_TEXT_MUTED).size(11.0));
                            ui.label(RichText::new(self.inv_root_uuid.as_deref().unwrap_or("Not queried")).monospace().color(COLOR_ACCENT));
                        });
                        ui.add_space(4.0);

                        inner_box_frame().show(ui, |ui| {
                            ui.label(RichText::new("Owner UUID:").color(COLOR_TEXT_MUTED).size(11.0));
                            ui.label(RichText::new(self.inv_owner_uuid.as_deref().unwrap_or("Not queried")).monospace());
                        });
                    });
                });
            });

            ui.add_space(10.0);

            // Row 3: Animations, Appearance, Objects
            ui.columns(3, |cols| {
                // Column 0: LLAnimation
                cols[0].vertical(|ui| {
                    card_frame().show(ui, |ui| {
                        let is_anim_loading = self.pending_animations.is_some();
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Animations (`LLAnimation`)").strong().size(13.0));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let label = if is_anim_loading { "⏳" } else { "Query" };
                                if ui.add_enabled(is_connected && !is_anim_loading, egui::Button::new(label).small()).clicked() {
                                    self.fetch_animations(client);
                                }
                            });
                        });
                        ui.add_space(8.0);

                        inner_box_frame().show(ui, |ui| {
                            ui.label(RichText::new("Active Animations:").color(COLOR_TEXT_MUTED).size(11.0));
                            if self.active_anims.is_empty() {
                                ui.label(RichText::new("None or Not Queried").color(COLOR_TEXT_MUTED));
                            } else {
                                for (uuid, dur) in &self.active_anims {
                                    ui.label(RichText::new(format!("{uuid} ({dur:.1}s)")).monospace().size(11.0).color(COLOR_ACCENT));
                                }
                            }
                        });
                        ui.add_space(4.0);

                        if ui.add_enabled(is_connected, egui::Button::new("⏹ Stop Animations").small()).clicked() {
                            let mut args = HashMap::new();
                            args.insert("op".to_string(), LLSDValue::String("stopAnimation".to_string()));
                            let _ = client.send("LLAnimation", args);
                            self.fetch_animations(client);
                        }
                    });
                });

                // Column 1: LLAppearance
                cols[1].vertical(|ui| {
                    card_frame().show(ui, |ui| {
                        let is_app_loading = self.pending_appearance.is_some();
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Appearance (`LLAppearance`)").strong().size(13.0));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let label = if is_app_loading { "⏳" } else { "Query" };
                                if ui.add_enabled(is_connected && !is_app_loading, egui::Button::new(label).small()).clicked() {
                                    self.fetch_appearance(client);
                                }
                            });
                        });
                        ui.add_space(8.0);

                        inner_box_frame().show(ui, |ui| {
                            ui.label(RichText::new("Current Outfit Folder (COF):").color(COLOR_TEXT_MUTED).size(11.0));
                            ui.label(RichText::new(self.cof_uuid.as_deref().unwrap_or("Not Queried")).monospace().color(COLOR_ACCENT).size(11.0));
                        });
                        ui.add_space(4.0);

                        inner_box_frame().show(ui, |ui| {
                            ui.label(RichText::new("Worn Clothing & Attachments:").color(COLOR_TEXT_MUTED).size(11.0));
                            ui.label(RichText::new(format!("Clothing Layers: {}\nWorn Attachments: {}", self.worn_clothing_count, self.worn_attachments.len())).size(11.0));
                        });
                    });
                });

                // Column 2: LLObjects
                cols[2].vertical(|ui| {
                    card_frame().show(ui, |ui| {
                        let is_obj_loading = self.pending_objects.is_some();
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Objects (`LLObjects`)").strong().size(13.0));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let label = if is_obj_loading { "⏳" } else { "Scan" };
                                if ui.add_enabled(is_connected && !is_obj_loading, egui::Button::new(label).small()).clicked() {
                                    self.fetch_objects(client);
                                }
                            });
                        });
                        ui.add_space(8.0);

                        inner_box_frame().show(ui, |ui| {
                            ui.label(RichText::new("Nearby Inworld Primitives:").color(COLOR_TEXT_MUTED).size(11.0));
                            ui.label(RichText::new(format!("{} Primitives Detected", self.nearby_objects_count)).strong().color(COLOR_SUCCESS));
                        });
                        ui.add_space(4.0);

                        inner_box_frame().show(ui, |ui| {
                            ui.label(RichText::new("Nearest Object:").color(COLOR_TEXT_MUTED).size(11.0));
                            if let Some((ref name, dist, local_id)) = self.closest_object {
                                ui.label(RichText::new(format!("{name} (#{local_id})\n{dist:.1} meters away")).monospace().size(11.0));
                            } else {
                                ui.label(RichText::new("None or Not Scanned").color(COLOR_TEXT_MUTED).size(11.0));
                            }
                        });
                    });
                });
            });
        });
    }

    fn fetch_all(&mut self, client: &LeapClient) {
        self.fetch_agent(client);
        self.fetch_camera(client);
        self.fetch_region(client);
        self.fetch_parcel(client);
        self.fetch_stats(client);
        self.fetch_inventory(client);
        self.fetch_animations(client);
        self.fetch_appearance(client);
        self.fetch_objects(client);
    }

    fn fetch_agent(&mut self, client: &LeapClient) {
        let mut pos_args = HashMap::new();
        pos_args.insert("op".to_string(), LLSDValue::String("getPosition".to_string()));
        self.pending_agent = Some(client.request_raw_async("LLAgent", pos_args, Duration::from_secs(4)));
    }

    fn fetch_camera(&mut self, client: &LeapClient) {
        let mut cam_args = HashMap::new();
        cam_args.insert("op".to_string(), LLSDValue::String("get".to_string()));
        self.pending_camera = Some(client.request_raw_async("LLCamera", cam_args, Duration::from_secs(4)));
    }

    fn fetch_region(&mut self, client: &LeapClient) {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("getRegionInfo".to_string()));
        self.pending_region = Some(client.request_raw_async("LLWorld", args, Duration::from_secs(4)));
    }

    fn fetch_parcel(&mut self, client: &LeapClient) {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("getParcelInfo".to_string()));
        self.pending_parcel = Some(client.request_raw_async("LLWorld", args, Duration::from_secs(4)));
    }

    fn fetch_stats(&mut self, client: &LeapClient) {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("getSimStats".to_string()));
        self.pending_stats = Some(client.request_raw_async("LLWorld", args, Duration::from_secs(4)));
    }

    fn fetch_inventory(&mut self, client: &LeapClient) {
        let mut inv_args = HashMap::new();
        inv_args.insert("op".to_string(), LLSDValue::String("getRoot".to_string()));
        self.pending_inventory = Some(client.request_raw_async("LLInventory", inv_args, Duration::from_secs(4)));
    }

    fn fetch_animations(&mut self, client: &LeapClient) {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("getActiveAnimations".to_string()));
        self.pending_animations = Some(client.request_raw_async("LLAnimation", args, Duration::from_secs(4)));
    }

    fn fetch_appearance(&mut self, client: &LeapClient) {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("getWornItems".to_string()));
        self.pending_appearance = Some(client.request_raw_async("LLAppearance", args, Duration::from_secs(4)));
    }

    fn fetch_objects(&mut self, client: &LeapClient) {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("getNearbyObjects".to_string()));
        args.insert("max_distance".to_string(), LLSDValue::Real(64.0));
        self.pending_objects = Some(client.request_raw_async("LLObjects", args, Duration::from_secs(4)));
    }
}
