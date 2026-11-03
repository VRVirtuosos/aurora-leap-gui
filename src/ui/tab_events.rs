//! Live Event Stream & Pub/Sub Monitor Tab.

use egui::{Color32, RichText, Ui};

use crate::client::LeapClient;
use crate::ui::theme::{card_frame, inner_box_frame, COLOR_ACCENT, COLOR_SUCCESS, COLOR_TEXT_MUTED};

pub struct EventsTab {
    subscribe_pump_input: String,
    filter_pump: String,
    search_query: String,
    status_msg: Option<String>,
}

impl EventsTab {
    pub fn new() -> Self {
        Self {
            subscribe_pump_input: "chat".to_string(),
            filter_pump: "All".to_string(),
            search_query: String::new(),
            status_msg: None,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, client: &LeapClient) {
        let is_connected = client.is_connected();
        let events = client.get_events();
        let subscriptions = client.get_subscriptions();

        ui.vertical(|ui| {
            card_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("📡 Live Event Stream & Pub/Sub Monitor");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Clear Events").clicked() {
                            client.clear_events();
                        }
                    });
                });

                ui.add_space(8.0);

                // Subscription Manager Bar
                ui.horizontal(|ui| {
                    ui.label("Subscribe to Pump:");
                    ui.add_sized([160.0, 20.0], egui::TextEdit::singleline(&mut self.subscribe_pump_input));

                    // Quick presets
                    for preset in &["chat", "teleport", "StartupState", "settings", "im"] {
                        if ui.small_button(*preset).clicked() {
                            self.subscribe_pump_input = (*preset).to_string();
                        }
                    }

                    let sub_btn = egui::Button::new(RichText::new("Subscribe (listen)").color(Color32::WHITE)).fill(COLOR_ACCENT);
                    if ui.add_enabled(is_connected && !self.subscribe_pump_input.trim().is_empty(), sub_btn).clicked() {
                        let pump = self.subscribe_pump_input.trim();
                        let listener = format!("gui_{pump}_listener");
                        match client.listen(pump, &listener) {
                            Ok(true) => self.status_msg = Some(format!("Subscribed to pump '{pump}'")),
                            Ok(false) => self.status_msg = Some(format!("Already subscribed or rejected for '{pump}'")),
                            Err(e) => self.status_msg = Some(format!("Error: {e}")),
                        }
                    }
                });

                // Active Subscriptions
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Active Subscriptions:").color(COLOR_TEXT_MUTED).size(11.0));
                    if subscriptions.is_empty() {
                        ui.label(RichText::new("None").color(COLOR_TEXT_MUTED).size(11.0));
                    } else {
                        for (pump, listener) in &subscriptions {
                            inner_box_frame().show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(pump).strong().size(11.0));
                                    if ui.small_button("✖").clicked() {
                                        let _ = client.stop_listening(pump, listener);
                                    }
                                });
                            });
                        }
                    }
                });

                if let Some(ref st) = self.status_msg {
                    ui.add_space(4.0);
                    ui.label(RichText::new(st).color(COLOR_SUCCESS).size(11.0));
                }
            });

            ui.add_space(10.0);

            // Filter Bar
            card_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Filter by Text:");
                    ui.add_sized([220.0, 18.0], egui::TextEdit::singleline(&mut self.search_query));

                    ui.add_space(16.0);
                    ui.label("Filter by Pump:");
                    ui.add_sized([160.0, 18.0], egui::TextEdit::singleline(&mut self.filter_pump));
                    if ui.small_button("All").clicked() {
                        self.filter_pump = "All".to_string();
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(format!("Total Events: {}", events.len())).color(COLOR_TEXT_MUTED).size(12.0));
                    });
                });
            });

            ui.add_space(10.0);

            // Scrolling Events Feed
            egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                let query = self.search_query.to_lowercase();
                let pump_filter = self.filter_pump.trim().to_lowercase();

                for ev in events.iter().rev() {
                    if pump_filter != "all" && !pump_filter.is_empty() && !ev.pump.to_lowercase().contains(&pump_filter) {
                        continue;
                    }
                    if !query.is_empty() && !ev.notation_preview.to_lowercase().contains(&query) && !ev.pump.to_lowercase().contains(&query) {
                        continue;
                    }

                    card_frame().show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&ev.timestamp).color(COLOR_TEXT_MUTED).size(10.0));
                            ui.label(RichText::new(&ev.pump).strong().color(COLOR_ACCENT).size(12.0));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                ui.label(RichText::new(format!("#{id}", id = ev.id)).color(COLOR_TEXT_MUTED).size(10.0));
                            });
                        });
                        ui.add_space(4.0);
                        inner_box_frame().show(ui, |ui| {
                            ui.label(RichText::new(&ev.notation_preview).monospace().size(11.0));
                        });
                    });
                    ui.add_space(4.0);
                }

                if events.is_empty() {
                    ui.label(RichText::new("No events captured yet. Subscribe to pumps above to stream live notifications.").color(COLOR_TEXT_MUTED));
                }
            });
        });
    }
}
