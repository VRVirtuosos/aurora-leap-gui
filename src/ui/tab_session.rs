//! Viewer Application & Session Lifecycle Tab (`LLAppViewer` & `LLLogin`).

use std::collections::HashMap;
use std::time::Duration;

use egui::{RichText, Ui};
use serde_llsd::LLSDValue;

use crate::client::{AsyncRequest, LeapClient};
use crate::codec::{get_f64, get_i32, get_str};
use crate::ui::theme::{card_frame, inner_box_frame, COLOR_ACCENT, COLOR_DANGER, COLOR_SUCCESS, COLOR_TEXT_MUTED, COLOR_WARNING};

pub struct SessionTab {
    version: Option<String>,
    uptime: Option<f64>,
    fps: Option<f64>,
    frame_count: Option<i32>,
    screen: Option<String>,
    login_state: Option<String>,
    logged_in: Option<bool>,
    status_msg: Option<String>,
    pending_status: Option<AsyncRequest<LLSDValue>>,
}

impl SessionTab {
    pub fn new() -> Self {
        Self {
            version: None,
            uptime: None,
            fps: None,
            frame_count: None,
            screen: None,
            login_state: None,
            logged_in: None,
            status_msg: None,
            pending_status: None,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, client: &mut LeapClient) {
        let is_connected = client.is_connected();

        // Update reactive telemetry
        let telem = client.cached_telemetry();
        if let Some(ref st) = telem.startup_state {
            self.login_state = Some(st.clone());
            self.logged_in = Some(st == "STATE_STARTED");
        }

        // Poll non-blocking status
        if let Some(ref req) = self.pending_status {
            if let Some(res) = req.poll() {
                if let Ok(reply) = res {
                    self.version = get_str(&reply, "version").map(|s| s.to_string());
                    self.uptime = get_f64(&reply, "uptime");
                    self.fps = get_f64(&reply, "fps");
                    self.frame_count = get_i32(&reply, "frame_count");
                    self.screen = get_str(&reply, "screen").map(|s| s.to_string());
                }
                self.pending_status = None;
            }
        }

        ui.vertical(|ui| {
            card_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("🔐 Application Lifecycle & Session Status");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let refresh_btn = egui::Button::new(RichText::new("🔄 Refresh Status").size(13.0));
                        if ui.add_enabled(is_connected, refresh_btn).clicked() {
                            self.fetch_status(client);
                        }
                    });
                });

                if let Some(ref st) = self.status_msg {
                    ui.add_space(4.0);
                    ui.label(RichText::new(st).color(COLOR_SUCCESS).size(11.0));
                }
            });

            ui.add_space(10.0);

            // Two-column layout: App Status and Login Session
            ui.columns(2, |cols| {
                // Column 0: LLAppViewer
                cols[0].vertical(|ui| {
                    card_frame().show(ui, |ui| {
                        ui.label(RichText::new("Viewer Runtime (`LLAppViewer`)").strong().size(13.0));
                        ui.add_space(8.0);

                        inner_box_frame().show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Version:").color(COLOR_TEXT_MUTED).size(11.0));
                                ui.label(RichText::new(self.version.as_deref().unwrap_or("Unknown")).strong().color(COLOR_ACCENT));
                            });
                        });
                        ui.add_space(4.0);

                        inner_box_frame().show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Active Screen:").color(COLOR_TEXT_MUTED).size(11.0));
                                ui.label(RichText::new(self.screen.as_deref().unwrap_or("Unknown")).strong());
                            });
                        });
                        ui.add_space(4.0);

                        inner_box_frame().show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Uptime & FPS:").color(COLOR_TEXT_MUTED).size(11.0));
                                if let Some(u) = self.uptime {
                                    let fps_str = self.fps.map(|f| format!(", {f:.1} FPS")).unwrap_or_default();
                                    ui.label(RichText::new(format!("{u:.1}s{fps_str}")).strong());
                                } else {
                                    ui.label(RichText::new("Unknown").color(COLOR_TEXT_MUTED));
                                }
                            });
                        });
                        ui.add_space(4.0);

                        inner_box_frame().show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Frames Rendered:").color(COLOR_TEXT_MUTED).size(11.0));
                                ui.label(RichText::new(self.frame_count.map(|f| f.to_string()).unwrap_or_else(|| "0".to_string())).strong());
                            });
                        });
                        ui.add_space(12.0);

                        ui.label(RichText::new("Application Lifecycle Actions:").strong().size(12.0));
                        ui.add_space(6.0);
                        ui.horizontal(|ui| {
                            let quit_btn = egui::Button::new(RichText::new("Request Quit (Graceful)").color(COLOR_WARNING));
                            if ui.add_enabled(is_connected, quit_btn).clicked() {
                                let mut args = HashMap::new();
                                args.insert("op".to_string(), LLSDValue::String("requestQuit".to_string()));
                                let _ = client.send("LLAppViewer", args);
                                self.status_msg = Some("Sent requestQuit to LLAppViewer".to_string());
                            }

                            let kill_btn = egui::Button::new(RichText::new("Force Terminate").color(COLOR_DANGER));
                            if ui.add_enabled(is_connected, kill_btn).clicked() {
                                client.disconnect();
                                self.status_msg = Some("Viewer process terminated".to_string());
                            }
                        });
                    });
                });

                // Column 1: LLLogin
                cols[1].vertical(|ui| {
                    card_frame().show(ui, |ui| {
                        ui.label(RichText::new("Login Session (`LLLogin`)").strong().size(13.0));
                        ui.add_space(8.0);

                        inner_box_frame().show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Login State:").color(COLOR_TEXT_MUTED).size(11.0));
                                ui.label(RichText::new(self.login_state.as_deref().unwrap_or("STATE_FIRST")).strong().color(COLOR_ACCENT));
                            });
                        });
                        ui.add_space(4.0);

                        inner_box_frame().show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Session Authenticated:").color(COLOR_TEXT_MUTED).size(11.0));
                                let logged = self.logged_in.unwrap_or(false);
                                ui.label(
                                    RichText::new(if logged { "Yes (Online)" } else { "No (Offline / Login Screen)" })
                                        .strong()
                                        .color(if logged { COLOR_SUCCESS } else { COLOR_WARNING }),
                                );
                            });
                        });
                        ui.add_space(12.0);

                        ui.label(RichText::new("Session Actions:").strong().size(12.0));
                        ui.add_space(6.0);
                        let logout_btn = egui::Button::new("Logout Session (logout)");
                        if ui.add_enabled(is_connected, logout_btn).clicked() {
                            let mut args = HashMap::new();
                            args.insert("op".to_string(), LLSDValue::String("logout".to_string()));
                            let _ = client.send("LLLogin", args);
                            self.fetch_status(client);
                        }
                    });
                });
            });
        });
    }

    fn fetch_status(&mut self, client: &LeapClient) {
        let mut app_args = HashMap::new();
        app_args.insert("op".to_string(), LLSDValue::String("status".to_string()));
        self.pending_status = Some(client.request_raw_async("LLAppViewer", app_args, Duration::from_secs(3)));

        let telem = client.cached_telemetry();
        if let Some(ref st) = telem.startup_state {
            self.login_state = Some(st.clone());
            self.logged_in = Some(st == "STATE_STARTED");
        }
    }
}
