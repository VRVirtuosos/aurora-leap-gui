//! API Introspection & Ping Tab (`LLLeapListener`).

use std::collections::HashMap;
use std::time::Duration;

use egui::{Color32, RichText, Ui};
use serde_llsd::LLSDValue;

use crate::client::LeapClient;
use crate::codec::{get_array, get_map, get_str};
use crate::ui::theme::{card_frame, inner_box_frame, COLOR_ACCENT, COLOR_SUCCESS, COLOR_TEXT_MUTED};

pub struct ApisTab {
    search_query: String,
    apis: Vec<(String, String)>, // (name, description)
    selected_api: Option<String>,
    api_schema: Option<LLSDValue>,
    ping_status: Option<String>,
}

impl ApisTab {
    pub fn new() -> Self {
        Self {
            search_query: String::new(),
            apis: Vec::new(),
            selected_api: None,
            api_schema: None,
            ping_status: None,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, client: &LeapClient) {
        let is_connected = client.is_connected();

        ui.vertical(|ui| {
            // Top Controls Card
            card_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("🔍 API Introspection (`LLLeapListener`)");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let refresh_btn = egui::Button::new(RichText::new("🔄 Discover APIs (getAPIs)").size(13.0));
                        if ui.add_enabled(is_connected, refresh_btn).clicked() {
                            self.fetch_apis(client);
                        }
                    });
                });

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label("Filter APIs:");
                    ui.add_sized([250.0, 20.0], egui::TextEdit::singleline(&mut self.search_query));

                    ui.add_space(20.0);

                    // Quick Ping Tool
                    let ping_btn = egui::Button::new(RichText::new("⚡ Ping Core (ping)").size(12.0));
                    if ui.add_enabled(is_connected, ping_btn).clicked() {
                        let t0 = std::time::Instant::now();
                        match client.ping(Duration::from_secs(4)) {
                            Ok(reply) => {
                                let elapsed = t0.elapsed();
                                let reqid = reply.as_map().and_then(|m| m.get("reqid")).map(|v| format!("{v:?}")).unwrap_or_default();
                                self.ping_status = Some(format!("Pong received in {:.2?} (reqid: {})", elapsed, reqid));
                            }
                            Err(e) => {
                                self.ping_status = Some(format!("Ping error: {e}"));
                            }
                        }
                    }

                    if let Some(ref st) = self.ping_status {
                        ui.label(RichText::new(st).color(COLOR_SUCCESS).size(12.0));
                    }
                });
            });

            ui.add_space(10.0);

            let mut to_select = None;
            let mut to_refresh_schema = None;

            // Two-column layout: APIs list on left, Schema inspector on right
            ui.columns(2, |cols| {
                // Column 0: Discovered APIs List
                cols[0].vertical(|ui| {
                    card_frame().show(ui, |ui| {
                        ui.label(RichText::new(format!("Discovered Subsystem APIs ({})", self.apis.len())).strong().size(13.0));
                        ui.add_space(6.0);

                        egui::ScrollArea::vertical().id_salt("apis_list_scroll").show(ui, |ui| {
                            let query = self.search_query.to_lowercase();
                            for (name, desc) in &self.apis {
                                if !query.is_empty() && !name.to_lowercase().contains(&query) && !desc.to_lowercase().contains(&query) {
                                    continue;
                                }

                                let is_selected = self.selected_api.as_deref() == Some(name);
                                inner_box_frame().show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        let btn = ui.selectable_label(is_selected, RichText::new(name).strong());
                                        if btn.clicked() {
                                            to_select = Some(name.clone());
                                        }
                                    });
                                    ui.label(RichText::new(desc).color(COLOR_TEXT_MUTED).size(11.0));
                                });
                                ui.add_space(4.0);
                            }

                            if self.apis.is_empty() {
                                ui.label(RichText::new("No APIs loaded yet. Click 'Discover APIs' above.").color(COLOR_TEXT_MUTED));
                            }
                        });
                    });
                });

                // Column 1: API Schema & Operations Details
                cols[1].vertical(|ui| {
                    card_frame().show(ui, |ui| {
                        if let Some(selected) = self.selected_api.clone() {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(format!("API Schema: {selected}")).strong().size(14.0));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if ui.button("🔄 Refresh Schema").clicked() {
                                        to_refresh_schema = Some(selected.clone());
                                    }
                                });
                            });

                            ui.add_space(8.0);

                            if let Some(ref schema) = self.api_schema {
                                if let Some(desc) = get_str(schema, "desc") {
                                    ui.label(RichText::new(desc).color(COLOR_TEXT_MUTED).size(12.0));
                                    ui.add_space(6.0);
                                }

                                ui.label(RichText::new("Operations:").strong().size(12.0));
                                ui.add_space(4.0);

                                egui::ScrollArea::vertical().id_salt("api_ops_scroll").show(ui, |ui| {
                                    if let Some(ops) = get_array(schema, "ops") {
                                        for op in ops {
                                            inner_box_frame().show(ui, |ui| {
                                                let op_name = get_str(op, "name").unwrap_or("unknown");
                                                let op_desc = get_str(op, "desc").unwrap_or("");
                                                ui.horizontal(|ui| {
                                                    ui.label(RichText::new(op_name).strong().color(COLOR_ACCENT));
                                                });
                                                if !op_desc.is_empty() {
                                                    ui.label(RichText::new(op_desc).color(COLOR_TEXT_MUTED).size(11.0));
                                                }

                                                // Parameters
                                                if let Some(req) = get_array(op, "required") {
                                                    let names: Vec<_> = req.iter().filter_map(|v| match v {
                                                        LLSDValue::String(s) => Some(s.as_str()),
                                                        _ => None,
                                                    }).collect();
                                                    if !names.is_empty() {
                                                        ui.label(RichText::new(format!("Required: {}", names.join(", "))).color(Color32::from_rgb(255, 140, 140)).size(10.0));
                                                    }
                                                }
                                                if let Some(opt) = get_array(op, "optional") {
                                                    let names: Vec<_> = opt.iter().filter_map(|v| match v {
                                                        LLSDValue::String(s) => Some(s.as_str()),
                                                        _ => None,
                                                    }).collect();
                                                    if !names.is_empty() {
                                                        ui.label(RichText::new(format!("Optional: {}", names.join(", "))).color(COLOR_TEXT_MUTED).size(10.0));
                                                    }
                                                }
                                            });
                                            ui.add_space(4.0);
                                        }
                                    }
                                });
                            } else {
                                ui.label(RichText::new("Loading schema...").color(COLOR_TEXT_MUTED));
                            }
                        } else {
                            ui.label(RichText::new("Select an API on the left to inspect its operations and schemas.").color(COLOR_TEXT_MUTED));
                        }
                    });
                });
            });

            if let Some(name) = to_select {
                self.selected_api = Some(name.clone());
                self.fetch_api_schema(client, &name);
            }
            if let Some(name) = to_refresh_schema {
                self.fetch_api_schema(client, &name);
            }
        });
    }

    fn fetch_apis(&mut self, client: &LeapClient) {
        match client.command("getAPIs", HashMap::new(), Duration::from_secs(5)) {
            Ok(reply) => {
                if let Some(apis_map) = get_map(&reply, "apis") {
                    let mut list = Vec::new();
                    for (name, val) in apis_map {
                        let desc = get_str(val, "desc").unwrap_or("").to_string();
                        list.push((name.clone(), desc));
                    }
                    list.sort_by(|a, b| a.0.cmp(&b.0));
                    self.apis = list;
                }
            }
            Err(e) => {
                self.ping_status = Some(format!("Failed to get APIs: {e}"));
            }
        }
    }

    fn fetch_api_schema(&mut self, client: &LeapClient, name: &str) {
        let mut args = HashMap::new();
        args.insert("api".to_string(), LLSDValue::String(name.to_string()));
        match client.command("getAPI", args, Duration::from_secs(5)) {
            Ok(reply) => {
                self.api_schema = Some(reply);
            }
            Err(e) => {
                self.ping_status = Some(format!("Failed to get schema for {name}: {e}"));
            }
        }
    }
}
