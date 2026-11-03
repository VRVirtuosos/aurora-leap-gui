//! Avatar Movement Controls & Landmarks Tab — sends commands via `LLAgent.move` and `LLAgent.teleport`.

use std::collections::{HashMap, HashSet};

use egui::{Color32, RichText, Ui, Vec2};
use serde_llsd::LLSDValue;

use crate::client::LeapClient;
use crate::landmarks::LandmarkStore;
use crate::ui::theme::{card_frame, inner_box_frame, COLOR_ACCENT, COLOR_DANGER, COLOR_SUCCESS, COLOR_TEXT_MUTED};

pub struct MovementTab {
    // D-pad step size
    step_size: f64,

    // Teleport destination inputs
    tp_region: String,
    tp_x: String,
    tp_y: String,
    tp_z: String,

    // Live position display
    agent_region: String,
    agent_x: f64,
    agent_y: f64,
    agent_z: f64,
    agent_yaw: f64,
    flying: bool,
    current_slurl: String,

    // Landmarks store
    pub landmark_store: LandmarkStore,
    new_landmark_name: String,

    // Status message
    status: Option<String>,
    status_ok: bool,

    // Continuously held movement keys/buttons
    active_held: HashSet<String>,
}

impl MovementTab {
    pub fn new() -> Self {
        Self {
            step_size: 2.0,
            tp_region: String::new(),
            tp_x: "128".to_string(),
            tp_y: "128".to_string(),
            tp_z: "25".to_string(),
            agent_region: "Unknown".to_string(),
            agent_x: 0.0,
            agent_y: 0.0,
            agent_z: 0.0,
            agent_yaw: 0.0,
            flying: false,
            current_slurl: String::new(),
            landmark_store: LandmarkStore::load(),
            new_landmark_name: String::new(),
            status: None,
            status_ok: true,
            active_held: HashSet::new(),
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, client: &LeapClient) {
        let is_connected = client.is_connected();
        let mut desired_held = HashSet::new();

        // 1. Reactive telemetry update (zero wait)
        let telem = client.cached_telemetry();
        if let Some((x, y, z)) = telem.agent_pos {
            self.agent_x = x;
            self.agent_y = y;
            self.agent_z = z;
            self.landmark_store.update_last(&self.agent_region, x, y, z);
        }
        if let Some(yaw) = telem.agent_yaw {
            self.agent_yaw = yaw;
        }
        if let Some(ref reg) = telem.region_name {
            if !reg.is_empty() {
                self.agent_region = reg.clone();
            }
        }
        if let Some(ref sl) = telem.slurl {
            self.current_slurl = sl.clone();
        }
        if let Some(f) = telem.flying {
            self.flying = f;
        }

        // Keyboard controls (WASD, Arrows, Q, E, PgUp, PgDn) when not typing
        let is_typing = ui.memory(|m| m.focused().is_some());
        if !is_typing && is_connected {
            ui.input(|i| {
                if i.key_down(egui::Key::W) || i.key_down(egui::Key::ArrowUp) {
                    desired_held.insert("forward".to_string());
                }
                if i.key_down(egui::Key::S) || i.key_down(egui::Key::ArrowDown) {
                    desired_held.insert("backward".to_string());
                }
                if i.key_down(egui::Key::A) || i.key_down(egui::Key::ArrowLeft) {
                    desired_held.insert("turn_left".to_string());
                }
                if i.key_down(egui::Key::D) || i.key_down(egui::Key::ArrowRight) {
                    desired_held.insert("turn_right".to_string());
                }
                if i.key_down(egui::Key::Q) {
                    desired_held.insert("strafe_left".to_string());
                }
                if i.key_down(egui::Key::E) {
                    desired_held.insert("strafe_right".to_string());
                }
                if i.key_down(egui::Key::PageUp) {
                    desired_held.insert("up".to_string());
                }
                if i.key_down(egui::Key::PageDown) {
                    desired_held.insert("down".to_string());
                }
            });
        }

        ui.vertical(|ui| {
            // ── Header & Live Telemetry ─────────────────────────────────────────
            card_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("🕹️ Avatar Movement & Landmarks");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(
                                is_connected,
                                egui::Button::new(RichText::new("🔄 Refresh Position").size(13.0)),
                            )
                            .clicked()
                        {
                            self.refresh_position(client);
                        }

                        let stop_btn = egui::Button::new(
                            RichText::new("⏹ Stop Movement").strong().color(Color32::WHITE),
                        )
                        .fill(COLOR_DANGER);
                        if ui.add_enabled(is_connected, stop_btn).clicked() {
                            self.send_stop(client);
                        }
                    });
                });

                // Live position banner
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    inner_box_frame().show(ui, |ui| {
                        ui.label(
                            RichText::new(format!(
                                "📍 Region: {}  |  X: {:.2}  Y: {:.2}  Z: {:.2}",
                                self.agent_region, self.agent_x, self.agent_y, self.agent_z
                            ))
                            .monospace()
                            .size(12.0)
                            .color(COLOR_ACCENT),
                        );
                    });
                    ui.add_space(6.0);
                    inner_box_frame().show(ui, |ui| {
                        ui.label(
                            RichText::new(format!("↻ Yaw: {:.1}°", self.agent_yaw.to_degrees()))
                                .monospace()
                                .size(12.0),
                        );
                    });
                    ui.add_space(6.0);
                    inner_box_frame().show(ui, |ui| {
                        let fly_text = if self.flying { "✈ Flying" } else { "🚶 Walking" };
                        ui.label(
                            RichText::new(fly_text)
                                .size(12.0)
                                .color(if self.flying { COLOR_SUCCESS } else { COLOR_TEXT_MUTED }),
                        );
                    });
                });

                if !self.current_slurl.is_empty() {
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("SLURL: {}", self.current_slurl)).monospace().size(11.0).color(COLOR_TEXT_MUTED));
                        if ui.small_button("📋 Copy").clicked() {
                            ui.ctx().copy_text(self.current_slurl.clone());
                        }
                        if ui.small_button("⭐ Save Landmark").clicked() {
                            let name = format!("{} ({:.0}, {:.0})", self.agent_region, self.agent_x, self.agent_y);
                            self.landmark_store.add(&name, &self.agent_region, self.agent_x, self.agent_y, self.agent_z);
                            self.status = Some(format!("Saved landmark '{name}'!"));
                            self.status_ok = true;
                        }
                    });
                }

                if let Some(ref msg) = self.status {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(msg)
                            .size(11.0)
                            .color(if self.status_ok { COLOR_SUCCESS } else { COLOR_DANGER }),
                    );
                }
            });

            ui.add_space(8.0);

            // ── Main two-column layout: D-pad / Flight | Landmarks & Teleport ────
            ui.columns(2, |cols| {
                // Left Column: Direction D-pad + Flight
                cols[0].vertical(|ui| {
                    card_frame().show(ui, |ui| {
                        ui.label(RichText::new("Direction & Walk Controls").strong().size(13.0));
                        ui.add_space(4.0);

                        // Step size slider
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Step distance:").color(COLOR_TEXT_MUTED));
                            ui.add(
                                egui::Slider::new(&mut self.step_size, 0.5..=20.0)
                                    .suffix(" m")
                                    .clamping(egui::SliderClamping::Always),
                            );
                        });

                        ui.add_space(8.0);

                        // D-pad grid — 3×3 button layout
                        let btn_size = Vec2::new(72.0, 42.0);
                        let enabled = is_connected;

                        // Row 1: Turn Left | Forward | Turn Right
                        ui.horizontal(|ui| {
                            let btn_tl = ui.add_enabled(enabled, egui::Button::new("↺\nTurn L").min_size(btn_size));
                            if btn_tl.is_pointer_button_down_on() {
                                desired_held.insert("turn_left".to_string());
                            }
                            if btn_tl.clicked() {
                                self.send_move(client, "turn_left", self.step_size);
                            }

                            let btn_fwd = ui.add_enabled(enabled, egui::Button::new("▲\nForward").min_size(btn_size));
                            if btn_fwd.is_pointer_button_down_on() {
                                desired_held.insert("forward".to_string());
                            }
                            if btn_fwd.clicked() {
                                self.send_move(client, "forward", self.step_size);
                            }

                            let btn_tr = ui.add_enabled(enabled, egui::Button::new("↻\nTurn R").min_size(btn_size));
                            if btn_tr.is_pointer_button_down_on() {
                                desired_held.insert("turn_right".to_string());
                            }
                            if btn_tr.clicked() {
                                self.send_move(client, "turn_right", self.step_size);
                            }
                        });

                        // Row 2: Strafe Left | Stop | Strafe Right
                        ui.horizontal(|ui| {
                            let btn_sl = ui.add_enabled(enabled, egui::Button::new("◄\nStrafe L").min_size(btn_size));
                            if btn_sl.is_pointer_button_down_on() {
                                desired_held.insert("strafe_left".to_string());
                            }
                            if btn_sl.clicked() {
                                self.send_move(client, "left", self.step_size);
                            }

                            let stop_mid = egui::Button::new("⏹\nSTOP")
                                .min_size(btn_size)
                                .fill(Color32::from_rgb(120, 30, 30));
                            if ui.add_enabled(enabled, stop_mid).clicked() {
                                self.send_stop(client);
                            }

                            let btn_sr = ui.add_enabled(enabled, egui::Button::new("►\nStrafe R").min_size(btn_size));
                            if btn_sr.is_pointer_button_down_on() {
                                desired_held.insert("strafe_right".to_string());
                            }
                            if btn_sr.clicked() {
                                self.send_move(client, "right", self.step_size);
                            }
                        });

                        // Row 3: Blank | Backward | Blank
                        ui.horizontal(|ui| {
                            let _ = ui.add_enabled(false, egui::Button::new("").min_size(btn_size));
                            let btn_bk = ui.add_enabled(enabled, egui::Button::new("▼\nBack").min_size(btn_size));
                            if btn_bk.is_pointer_button_down_on() {
                                desired_held.insert("backward".to_string());
                            }
                            if btn_bk.clicked() {
                                self.send_move(client, "backward", self.step_size);
                            }
                            let _ = ui.add_enabled(false, egui::Button::new("").min_size(btn_size));
                        });

                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(6.0);

                        // Vertical flight controls
                        ui.label(RichText::new("Flight & Elevation:").color(COLOR_TEXT_MUTED).size(11.0));
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            let fly_btn_label = if self.flying { "🛬 Land" } else { "🛫 Fly" };
                            let fly_btn = egui::Button::new(RichText::new(fly_btn_label).size(12.0));
                            if ui.add_enabled(enabled, fly_btn).clicked() {
                                self.send_move(client, "fly", 0.0);
                            }

                            let btn_up = ui.add_enabled(enabled, egui::Button::new(RichText::new("▲ Fly Up").size(12.0)));
                            if btn_up.is_pointer_button_down_on() {
                                desired_held.insert("up".to_string());
                            }
                            if btn_up.clicked() {
                                self.send_move(client, "up", self.step_size);
                            }

                            let btn_down = ui.add_enabled(enabled, egui::Button::new(RichText::new("▼ Fly Down").size(12.0)));
                            if btn_down.is_pointer_button_down_on() {
                                desired_held.insert("down".to_string());
                            }
                            if btn_down.clicked() {
                                self.send_move(client, "down", self.step_size);
                            }
                        });
                    });
                });

                // Right Column: Saved Landmarks Gallery & Teleport
                cols[1].vertical(|ui| {
                    card_frame().show(ui, |ui| {
                        ui.label(RichText::new("Saved Landmarks & Teleport").strong().size(13.0));
                        ui.add_space(4.0);

                        // Add new landmark box
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Name:").size(11.0));
                            ui.add_sized([160.0, 20.0], egui::TextEdit::singleline(&mut self.new_landmark_name).hint_text("Landmark Name"));
                            if ui.button("Save Current Place").clicked() {
                                if !self.new_landmark_name.trim().is_empty() {
                                    self.landmark_store.add(&self.new_landmark_name, &self.agent_region, self.agent_x, self.agent_y, self.agent_z);
                                    self.status = Some(format!("Saved landmark '{}'", self.new_landmark_name));
                                    self.status_ok = true;
                                    self.new_landmark_name.clear();
                                }
                            }
                        });

                        ui.add_space(6.0);
                        ui.separator();
                        ui.add_space(4.0);

                        // Landmarks scroll list
                        egui::ScrollArea::vertical().max_height(280.0).show(ui, |ui| {
                            let mut delete_idx = None;
                            let mut teleport_target = None;

                            for (idx, lm) in self.landmark_store.landmarks.iter().enumerate() {
                                inner_box_frame().show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.vertical(|ui| {
                                            ui.label(RichText::new(&lm.name).strong().size(12.0));
                                            ui.label(RichText::new(format!("{} ({:.0}, {:.0}, {:.0})", lm.region, lm.x, lm.y, lm.z)).size(10.0).color(COLOR_TEXT_MUTED));
                                            ui.label(RichText::new(&lm.slurl).monospace().size(10.0).color(COLOR_ACCENT));
                                        });

                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            if !lm.is_special {
                                                if ui.small_button("🗑").on_hover_text("Delete landmark").clicked() {
                                                    delete_idx = Some(idx);
                                                }
                                            }
                                            if ui.small_button("📋").on_hover_text("Copy SLURL").clicked() {
                                                ui.ctx().copy_text(lm.slurl.clone());
                                            }
                                            let tp_btn = egui::Button::new("✈ Teleport").small().fill(COLOR_ACCENT);
                                            if ui.add_enabled(is_connected, tp_btn).clicked() {
                                                teleport_target = Some(lm.clone());
                                            }
                                        });
                                    });
                                });
                                ui.add_space(3.0);
                            }

                            if let Some(idx) = delete_idx {
                                self.landmark_store.remove(idx);
                            }
                            if let Some(lm) = teleport_target {
                                self.teleport_to_landmark(client, &lm);
                            }
                        });

                        ui.add_space(8.0);
                        ui.separator();
                        ui.add_space(4.0);

                        // Direct Teleport Form
                        ui.label(RichText::new("Direct Teleport to Coordinates:").strong().size(11.0));
                        ui.horizontal(|ui| {
                            ui.label("Region:");
                            ui.add_sized([100.0, 20.0], egui::TextEdit::singleline(&mut self.tp_region).hint_text("Leave blank for current"));
                            ui.label("X:");
                            ui.add_sized([35.0, 20.0], egui::TextEdit::singleline(&mut self.tp_x));
                            ui.label("Y:");
                            ui.add_sized([35.0, 20.0], egui::TextEdit::singleline(&mut self.tp_y));
                            ui.label("Z:");
                            ui.add_sized([35.0, 20.0], egui::TextEdit::singleline(&mut self.tp_z));

                            let btn = egui::Button::new("✈ Teleport").fill(COLOR_ACCENT);
                            if ui.add_enabled(is_connected, btn).clicked() {
                                self.do_teleport(client);
                            }
                        });
                    });
                });
            });
        });

        // ── Handle continuously held movement (WASD, Arrows, D-pad hold) ───────
        if is_connected {
            for dir in &desired_held {
                if !self.active_held.contains(dir) {
                    self.send_start_move(client, dir);
                }
            }
            for dir in &self.active_held {
                if !desired_held.contains(dir) {
                    self.send_stop_move(client, dir);
                }
            }
            self.active_held = desired_held;

            if !self.active_held.is_empty() {
                ui.ctx().request_repaint();
            }
        }
    }

    // ── Helpers ───────────────────────────────────────────────────────────────

    fn send_start_move(&self, client: &LeapClient, direction: &str) {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("start_move".to_string()));
        args.insert(
            "direction".to_string(),
            LLSDValue::String(direction.to_string()),
        );
        let _ = client.send("LLAgent", args);
    }

    fn send_stop_move(&self, client: &LeapClient, direction: &str) {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("stop_move".to_string()));
        args.insert(
            "direction".to_string(),
            LLSDValue::String(direction.to_string()),
        );
        let _ = client.send("LLAgent", args);
    }

    fn send_move(&mut self, client: &LeapClient, direction: &str, step: f64) {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("move".to_string()));
        args.insert(
            "direction".to_string(),
            LLSDValue::String(direction.to_string()),
        );
        args.insert("step".to_string(), LLSDValue::Real(step));

        let _ = client.send("LLAgent", args);
        self.status = Some(format!("Moving {direction}"));
        self.status_ok = true;
    }

    fn send_stop(&mut self, client: &LeapClient) {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("stop".to_string()));
        let _ = client.send("LLAgent", args);
        self.active_held.clear();
        self.status = Some("Stopped avatar movement".to_string());
        self.status_ok = true;
    }

    fn refresh_position(&mut self, client: &LeapClient) {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("getPosition".to_string()));
        let _ = client.send("LLAgent", args);
    }

    fn teleport_to_landmark(&mut self, client: &LeapClient, lm: &crate::landmarks::Landmark) {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("teleport".to_string()));
        args.insert("slurl".to_string(), LLSDValue::String(lm.slurl.clone()));
        args.insert("region".to_string(), LLSDValue::String(lm.region.clone()));
        args.insert("x".to_string(), LLSDValue::Real(lm.x));
        args.insert("y".to_string(), LLSDValue::Real(lm.y));
        args.insert("z".to_string(), LLSDValue::Real(lm.z));

        let _ = client.send("LLAgent", args);
        self.status = Some(format!("Teleporting to '{}' ({})…", lm.name, lm.slurl));
        self.status_ok = true;
    }

    fn do_teleport(&mut self, client: &LeapClient) {
        let x: f64 = self.tp_x.parse().unwrap_or(128.0);
        let y: f64 = self.tp_y.parse().unwrap_or(128.0);
        let z: f64 = self.tp_z.parse().unwrap_or(25.0);

        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("teleport".to_string()));
        if !self.tp_region.trim().is_empty() {
            args.insert("region".to_string(), LLSDValue::String(self.tp_region.trim().to_string()));
        }
        args.insert("x".to_string(), LLSDValue::Real(x));
        args.insert("y".to_string(), LLSDValue::Real(y));
        args.insert("z".to_string(), LLSDValue::Real(z));

        let _ = client.send("LLAgent", args);
        self.status = Some(format!("Teleport requested to ({x}, {y}, {z})"));
        self.status_ok = true;
    }
}
