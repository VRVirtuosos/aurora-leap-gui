//! Viewer Console Logs (`stderr`) Tab.

use egui::{Color32, RichText, Ui};

use crate::client::LeapClient;
use crate::ui::theme::{card_frame, COLOR_DANGER, COLOR_SUCCESS, COLOR_TEXT_MUTED};

pub struct LogsTab {
    auto_scroll: bool,
    filter_query: String,
    export_status: Option<(bool, String)>,
}

impl LogsTab {
    pub fn new() -> Self {
        Self {
            auto_scroll: true,
            filter_query: String::new(),
            export_status: None,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, client: &LeapClient) {
        let logs = client.get_logs();

        ui.vertical(|ui| {
            card_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("📜 Viewer Console Logs (`stderr`)");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Clear Logs").clicked() {
                            client.clear_logs();
                        }
                        ui.checkbox(&mut self.auto_scroll, "Auto-scroll");

                        // Milestone 6.5: Export Blackbox Incident Bundle Button
                        let bb_count = client.get_blackbox_frames().len();
                        let bundle_label = format!("📦 Export Incident Bundle (.zip) [{bb_count} frames]");
                        if ui.button(RichText::new(bundle_label).strong().color(Color32::WHITE))
                            .on_hover_text("Exports blackbox LEAP traffic (jsonl), stderr console output, and system telemetry into a compressed zip archive")
                            .clicked()
                        {
                            let default_name = format!("aurora_incident_bundle_{}.zip", chrono::Local::now().format("%Y%m%d_%H%M%S"));
                            if let Some(path) = rfd::FileDialog::new()
                                .set_file_name(&default_name)
                                .add_filter("Zip Archive", &["zip"])
                                .save_file()
                            {
                                match client.export_incident_bundle(&path) {
                                    Ok(_) => {
                                        self.export_status = Some((true, format!("Incident bundle saved to '{}'", path.display())));
                                    }
                                    Err(e) => {
                                        self.export_status = Some((false, format!("Failed to export bundle: {e}")));
                                    }
                                }
                            }
                        }
                    });
                });

                if let Some((ok, ref msg)) = self.export_status {
                    ui.add_space(4.0);
                    let col = if ok { COLOR_SUCCESS } else { COLOR_DANGER };
                    ui.label(RichText::new(msg).color(col).size(11.0));
                }

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label("Filter text:");
                    ui.add_sized([250.0, 18.0], egui::TextEdit::singleline(&mut self.filter_query));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(RichText::new(format!("Log Entries: {}", logs.len())).color(COLOR_TEXT_MUTED).size(12.0));
                    });
                });
            });

            ui.add_space(10.0);

            card_frame().show(ui, |ui| {
                let scroll = egui::ScrollArea::vertical().stick_to_bottom(self.auto_scroll);
                scroll.show(ui, |ui| {
                    let query = self.filter_query.to_lowercase();
                    for entry in &logs {
                        if !query.is_empty() && !entry.text.to_lowercase().contains(&query) {
                            continue;
                        }
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&entry.timestamp).color(COLOR_TEXT_MUTED).monospace().size(10.0));
                            ui.label(RichText::new(&entry.text).monospace().size(11.0));
                        });
                    }

                    if logs.is_empty() {
                        ui.label(RichText::new("No logs captured yet. Launch the viewer to stream console stderr output.").color(COLOR_TEXT_MUTED));
                    }
                });
            });
        });
    }
}
