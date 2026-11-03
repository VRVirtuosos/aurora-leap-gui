//! Raw LLSD Notation Console Tab.

use std::time::Duration;

use egui::{Color32, RichText, Ui};
use serde_llsd::LLSDValue;

use crate::client::LeapClient;
use crate::codec::{from_notation_str, to_clean_notation};
use crate::ui::theme::{card_frame, inner_box_frame, COLOR_ACCENT, COLOR_DANGER, COLOR_SUCCESS};

pub struct RawTab {
    target_pump: String,
    notation_input: String,
    response_output: String,
    error_msg: Option<String>,
}

impl RawTab {
    pub fn new() -> Self {
        Self {
            target_pump: "LLAppViewer".to_string(),
            notation_input: "{'op':'ping'}".to_string(),
            response_output: "Ready to send custom LLSD Notation commands.".to_string(),
            error_msg: None,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, client: &LeapClient) {
        let is_connected = client.is_connected();

        ui.vertical(|ui| {
            card_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("🛠️ Raw LLSD Console (`serde-llsd`)");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let is_valid = from_notation_str(&self.notation_input).is_ok();
                        ui.label(
                            RichText::new(if is_valid { "✔ Valid LLSD Notation" } else { "⚠ Syntax Error" })
                                .color(if is_valid { COLOR_SUCCESS } else { COLOR_DANGER })
                                .size(11.0),
                        );
                    });
                });

                ui.add_space(8.0);

                // Controls Bar
                ui.horizontal(|ui| {
                    ui.label("Target Pump:");
                    ui.add_sized([160.0, 20.0], egui::TextEdit::singleline(&mut self.target_pump));

                    for pump in &["LLAppViewer", "LLViewerControl", "LLChat", "LLAgent", "LLCamera"] {
                        if ui.small_button(*pump).clicked() {
                            self.target_pump = (*pump).to_string();
                        }
                    }
                });

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label("Presets:");
                    if ui.small_button("Ping").clicked() {
                        self.target_pump = "LLAppViewer".to_string();
                        self.notation_input = "{'op':'ping'}".to_string();
                    }
                    if ui.small_button("Get DrawDistance").clicked() {
                        self.target_pump = "LLViewerControl".to_string();
                        self.notation_input = "{'op':'get','key':'DrawDistance'}".to_string();
                    }
                    if ui.small_button("Toggle VSync").clicked() {
                        self.target_pump = "LLViewerControl".to_string();
                        self.notation_input = "{'op':'toggle','key':'VSync'}".to_string();
                    }
                    if ui.small_button("Send Chat").clicked() {
                        self.target_pump = "LLChat".to_string();
                        self.notation_input = "{'op':'sendChat','message':'Hello from Raw LLSD Console!','channel':i0}".to_string();
                    }
                    if ui.small_button("Camera Get").clicked() {
                        self.target_pump = "LLCamera".to_string();
                        self.notation_input = "{'op':'get'}".to_string();
                    }
                });
            });

            ui.add_space(10.0);

            // Editor and Output Split View
            ui.columns(2, |cols| {
                // Column 0: Input Notation Editor
                cols[0].vertical(|ui| {
                    card_frame().show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("LLSD Payload (Notation Format)").strong().size(13.0));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let send_btn = egui::Button::new(RichText::new("▶ Send Request").color(Color32::WHITE)).fill(COLOR_ACCENT);
                                if ui.add_enabled(is_connected, send_btn).clicked() {
                                    self.execute_request(client);
                                }
                            });
                        });
                        ui.add_space(6.0);

                        ui.add_sized(
                            [ui.available_width(), 350.0],
                            egui::TextEdit::multiline(&mut self.notation_input)
                                .font(egui::TextStyle::Monospace)
                                .desired_rows(16),
                        );
                    });
                });

                // Column 1: Response Output
                cols[1].vertical(|ui| {
                    card_frame().show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Correlated Response").strong().size(13.0));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button("Clear Output").clicked() {
                                    self.response_output.clear();
                                    self.error_msg = None;
                                }
                            });
                        });
                        ui.add_space(6.0);

                        egui::ScrollArea::vertical().show(ui, |ui| {
                            if let Some(ref err) = self.error_msg {
                                inner_box_frame().show(ui, |ui| {
                                    ui.label(RichText::new(err).color(COLOR_DANGER).monospace());
                                });
                                ui.add_space(6.0);
                            }

                            inner_box_frame().show(ui, |ui| {
                                ui.label(RichText::new(&self.response_output).monospace().size(11.0));
                            });
                        });
                    });
                });
            });
        });
    }

    fn execute_request(&mut self, client: &LeapClient) {
        self.error_msg = None;
        let llsd_data = match from_notation_str(&self.notation_input) {
            Ok(v) => v,
            Err(e) => {
                self.error_msg = Some(format!("Parse error in input: {e}"));
                return;
            }
        };

        let map = match llsd_data {
            LLSDValue::Map(m) => m,
            other => {
                self.error_msg = Some(format!("Expected LLSD Map at top level, got: {other:?}"));
                return;
            }
        };

        let t0 = std::time::Instant::now();
        match client.request(&self.target_pump, map, Duration::from_secs(5)) {
            Ok(reply) => {
                let elapsed = t0.elapsed();
                let notation = to_clean_notation(&reply).unwrap_or_else(|_| format!("{reply:?}"));
                self.response_output = format!("// Response received in {:.2?}\n{}", elapsed, notation);
            }
            Err(e) => {
                self.error_msg = Some(format!("Request error: {e}"));
            }
        }
    }
}
