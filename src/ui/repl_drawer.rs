//! Interactive LEAP CLI REPL Drawer for real-time protocol debugging and query execution.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use egui::{Color32, RichText, Ui};
use serde_llsd::LLSDValue;

use crate::client::{AsyncRequest, LeapClient};
use crate::codec::{from_notation_str, to_clean_notation};
use crate::ui::theme::{
    card_frame, inner_box_frame, COLOR_ACCENT,
    COLOR_DANGER, COLOR_SUCCESS, COLOR_TEXT_MUTED, COLOR_WARNING,
};

#[derive(Clone, Debug)]
pub struct ReplEntry {
    pub timestamp: String,
    pub pump: String,
    pub request: String,
    pub response: Result<String, String>,
    pub duration_ms: u64,
}

pub struct ReplDrawer {
    pub is_open: bool,
    pub input_cmd: String,
    pub selected_pump: String,
    pub history: Vec<String>,
    pub history_idx: Option<usize>,
    pub entries: Vec<ReplEntry>,
    pub pending_req: Option<(String, String, AsyncRequest<LLSDValue>, Instant)>,
    pub status_msg: Option<String>,
}

impl ReplDrawer {
    pub fn new() -> Self {
        Self {
            is_open: false,
            input_cmd: String::new(),
            selected_pump: "LLAppViewer".to_string(),
            history: Vec::new(),
            history_idx: None,
            entries: Vec::new(),
            pending_req: None,
            status_msg: None,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, client: &LeapClient) {
        let is_connected = client.is_connected();

        // 1. Poll in-flight request
        if let Some((pump, req_str, req, start_time)) = self.pending_req.take() {
            if let Some(res) = req.poll() {
                let duration_ms = start_time.elapsed().as_millis() as u64;
                let now = chrono::Local::now().format("%H:%M:%S%.3f").to_string();
                match res {
                    Ok(reply) => {
                        let pretty = to_clean_notation(&reply).unwrap_or_else(|_| format!("{reply:?}"));
                        self.entries.push(ReplEntry {
                            timestamp: now,
                            pump,
                            request: req_str,
                            response: Ok(pretty),
                            duration_ms,
                        });
                        self.status_msg = Some(format!("Received reply ({duration_ms} ms)"));
                    }
                    Err(err) => {
                        self.entries.push(ReplEntry {
                            timestamp: now,
                            pump,
                            request: req_str,
                            response: Err(err.clone()),
                            duration_ms,
                        });
                        self.status_msg = Some(format!("Request error: {err}"));
                    }
                }
            } else {
                self.pending_req = Some((pump, req_str, req, start_time));
            }
        }

        // 2. Drawer Header Bar
        inner_box_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                let toggle_icon = if self.is_open { "▼" } else { "▲" };
                let btn = egui::Button::new(
                    RichText::new(format!("{toggle_icon} ⌨️ LEAP REPL Console ({})", self.entries.len()))
                        .strong()
                        .color(if self.is_open { COLOR_ACCENT } else { COLOR_TEXT_MUTED })
                ).small();

                if ui.add(btn).clicked() {
                    self.is_open = !self.is_open;
                }

                if self.is_open {
                    ui.label(RichText::new("Direct length-prefixed LLSD command evaluation").color(COLOR_TEXT_MUTED).size(11.0));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("🗑 Clear Console").clicked() {
                            self.entries.clear();
                            self.status_msg = None;
                        }

                        if let Some(ref msg) = self.status_msg {
                            ui.label(RichText::new(msg).size(11.0).color(COLOR_SUCCESS));
                        }
                    });
                }
            });
        });

        // 3. Expanded Drawer Content
        if self.is_open {
            ui.add_space(4.0);
            card_frame().show(ui, |ui| {
                // Command Input Strip
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Pump:").strong().size(12.0));

                    egui::ComboBox::from_id_salt("repl_pump_select")
                        .selected_text(&self.selected_pump)
                        .width(130.0)
                        .show_ui(ui, |ui| {
                            for pump in &[
                                "LLAppViewer",
                                "LLViewerControl",
                                "LLChat",
                                "LLAgent",
                                "LLCamera",
                                "LLWorld",
                                "LLSimulator",
                                "LLNearbyAvatars",
                                "LLObjects",
                                "LLAppearance",
                                "LLAnimation",
                                "LLInventory",
                                "LLTeleport",
                                "Custom...",
                            ] {
                                ui.selectable_value(&mut self.selected_pump, pump.to_string(), *pump);
                            }
                        });

                    if self.selected_pump == "Custom..." {
                        let mut custom = String::new();
                        ui.add_sized([100.0, 20.0], egui::TextEdit::singleline(&mut custom));
                        if !custom.is_empty() {
                            self.selected_pump = custom;
                        }
                    }

                    ui.label(RichText::new("Cmd / Payload:").strong().size(12.0));

                    let input_edit = egui::TextEdit::singleline(&mut self.input_cmd)
                        .hint_text("e.g. op=ping, or {'op': 'get'}, or takePicture")
                        .desired_width(ui.available_width() - 90.0);

                    let response = ui.add(input_edit);
                    let enter_pressed = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

                    let is_waiting = self.pending_req.is_some();
                    let send_btn = egui::Button::new(
                        RichText::new(if is_waiting { "⏳" } else { "Send ▶" })
                            .strong()
                            .color(Color32::WHITE)
                    )
                    .fill(if is_waiting { COLOR_WARNING } else { COLOR_ACCENT });

                    if (ui.add_enabled(is_connected && !is_waiting, send_btn).clicked() || (enter_pressed && !is_waiting && is_connected))
                        && !self.input_cmd.trim().is_empty()
                    {
                        self.execute_command(client);
                    }
                });

                ui.add_space(6.0);

                // REPL Log Output Terminal
                egui::ScrollArea::vertical()
                    .max_height(180.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        if self.entries.is_empty() {
                            ui.vertical_centered(|ui| {
                                ui.add_space(30.0);
                                ui.label(RichText::new("REPL history is empty. Select a pump and dispatch a command above.").color(COLOR_TEXT_MUTED).italics());
                                ui.add_space(30.0);
                            });
                        } else {
                            for entry in &self.entries {
                                inner_box_frame().show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(&entry.timestamp).monospace().size(10.0).color(COLOR_TEXT_MUTED));
                                        ui.label(RichText::new(&entry.pump).monospace().strong().size(11.0).color(COLOR_ACCENT));
                                        ui.label(RichText::new("➡").color(COLOR_TEXT_MUTED));
                                        ui.label(RichText::new(&entry.request).monospace().size(11.0));

                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            ui.label(RichText::new(format!("{} ms", entry.duration_ms)).monospace().size(10.0).color(COLOR_TEXT_MUTED));
                                        });
                                    });

                                    ui.add_space(2.0);

                                    match &entry.response {
                                        Ok(res_str) => {
                                            ui.label(RichText::new(res_str).monospace().size(11.0).color(COLOR_SUCCESS));
                                        }
                                        Err(err_str) => {
                                            ui.label(RichText::new(format!("Error: {err_str}")).monospace().size(11.0).color(COLOR_DANGER));
                                        }
                                    }
                                });
                                ui.add_space(3.0);
                            }
                        }
                    });
            });
        }
    }

    fn execute_command(&mut self, client: &LeapClient) {
        let raw = self.input_cmd.trim().to_string();
        if raw.is_empty() {
            return;
        }

        self.history.push(raw.clone());
        self.history_idx = None;

        let pump = self.selected_pump.clone();
        let payload = parse_cli_payload(&raw);

        let req = client.request_raw_async(&pump, payload, Duration::from_secs(5));
        self.pending_req = Some((pump, raw, req, Instant::now()));
        self.input_cmd.clear();
    }
}

/// Parse flexible CLI arguments:
/// 1. If starts with `{` -> parse as LLSD notation/JSON map.
/// 2. If single word -> interpret as `op=<word>` (e.g. `ping` -> `{"op": "ping"}`).
/// 3. If key=value tokens -> parse into a Map (e.g. `op=ping count=5`).
fn parse_cli_payload(raw: &str) -> HashMap<String, LLSDValue> {
    let mut map = HashMap::new();
    let trimmed = raw.trim();

    if trimmed.starts_with('{') {
        if let Ok(LLSDValue::Map(parsed_map)) = from_notation_str(trimmed) {
            return parsed_map;
        }
    }

    if !trimmed.contains('=') && !trimmed.contains(' ') {
        map.insert("op".to_string(), LLSDValue::String(trimmed.to_string()));
        return map;
    }

    for token in trimmed.split_whitespace() {
        if let Some((k, v)) = token.split_once('=') {
            let key = k.trim().to_string();
            let val_str = v.trim().trim_matches('"').trim_matches('\'');

            let val = if let Ok(i) = val_str.parse::<i32>() {
                LLSDValue::Integer(i)
            } else if let Ok(f) = val_str.parse::<f64>() {
                LLSDValue::Real(f)
            } else if val_str == "true" {
                LLSDValue::Boolean(true)
            } else if val_str == "false" {
                LLSDValue::Boolean(false)
            } else {
                LLSDValue::String(val_str.to_string())
            };

            map.insert(key, val);
        } else if !map.contains_key("op") {
            map.insert("op".to_string(), LLSDValue::String(token.to_string()));
        }
    }

    map
}
