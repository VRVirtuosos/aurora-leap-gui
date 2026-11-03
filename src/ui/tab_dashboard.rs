//! Dashboard, Simulator Health Profiler & Automated Test Suite Tab.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{Color32, Pos2, RichText, Sense, Stroke, Ui, Vec2};
use serde_llsd::LLSDValue;

use crate::client::{AsyncRequest, LeapClient, NotificationItem};
use crate::codec::{get_bool, get_f64, get_i32, get_str};
use crate::tests::{TestRunState, TestRunner, TestStatus};
use crate::ui::theme::{
    card_frame, inner_box_frame, COLOR_ACCENT, COLOR_DANGER, COLOR_SUCCESS,
    COLOR_TEXT_MUTED, COLOR_WARNING,
};

/// Maximum seconds of historical data retained in the rolling sparkline buffers.
const MAX_HISTORY_SAMPLES: usize = 60;

pub struct DashboardTab {
    // Simulator Health & Telemetry History
    pub fps_history: VecDeque<f64>,
    pub frametime_history: VecDeque<f64>,
    pub avatar_history: VecDeque<f64>,

    pub current_sim_fps: f64,
    pub current_uptime_sec: f64,
    pub current_avatar_count: i32,
    pub current_region_name: String,
    pub current_region_count: i32,

    pub last_poll_time: Option<Instant>,
    pending_stats_req: Option<AsyncRequest<LLSDValue>>,

    // Milestone 8.2: Dialog Notifications & Permissions
    pub notifications: Vec<NotificationItem>,
    pub pending_notifs_req: Option<AsyncRequest<LLSDValue>>,
    pub last_notifs_poll: Option<Instant>,
    pub notif_status_msg: Option<(String, Instant)>,
    pub respond_req: Option<AsyncRequest<LLSDValue>>,
}

impl DashboardTab {
    pub fn new() -> Self {
        Self {
            fps_history: VecDeque::with_capacity(MAX_HISTORY_SAMPLES),
            frametime_history: VecDeque::with_capacity(MAX_HISTORY_SAMPLES),
            avatar_history: VecDeque::with_capacity(MAX_HISTORY_SAMPLES),
            current_sim_fps: 0.0,
            current_uptime_sec: 0.0,
            current_avatar_count: 0,
            current_region_name: "None".to_string(),
            current_region_count: 0,
            last_poll_time: None,
            pending_stats_req: None,
            notifications: Vec::new(),
            pending_notifs_req: None,
            last_notifs_poll: None,
            notif_status_msg: None,
            respond_req: None,
        }
    }

    /// Asynchronously poll `LLWorld.getSimStats` every 1 second without blocking the UI thread.
    pub fn update_sim_stats(&mut self, client: &LeapClient) {
        // Poll in-flight async request
        if let Some(ref mut req) = self.pending_stats_req {
            if let Some(res) = req.poll() {
                if let Ok(reply) = res {
                    let fps = get_f64(&reply, "fps").unwrap_or(0.0);
                    let uptime = get_f64(&reply, "uptime").unwrap_or(0.0);
                    let avatars = get_i32(&reply, "avatar_count").unwrap_or(0);
                    let regions = get_i32(&reply, "region_count").unwrap_or(0);
                    let main_reg = get_str(&reply, "main_region").unwrap_or("None").to_string();

                    self.current_sim_fps = fps;
                    self.current_uptime_sec = uptime;
                    self.current_avatar_count = avatars;
                    self.current_region_count = regions;
                    if main_reg != "None" {
                        self.current_region_name = main_reg;
                    }

                    // Compute frame time ms
                    let ft_ms = if fps > 0.0 { 1000.0 / fps } else { 0.0 };

                    self.push_history(fps, ft_ms, avatars as f64);
                }
                self.pending_stats_req = None;
            }
        }

        // Fallback: ingest reactive cached telemetry if available
        let telem = client.cached_telemetry();
        if let Some(sim_fps) = telem.sim_fps {
            if (self.current_sim_fps - sim_fps).abs() > 0.1 && self.pending_stats_req.is_none() {
                self.current_sim_fps = sim_fps;
            }
        }
        if let Some(ref reg) = telem.region_name {
            if !reg.is_empty() {
                self.current_region_name = reg.clone();
            }
        }

        // Dispatch new request every 1.0s if connected
        if client.is_connected() && self.pending_stats_req.is_none() {
            let should_poll = match self.last_poll_time {
                Some(last) => last.elapsed() >= Duration::from_millis(1000),
                None => true,
            };

            if should_poll {
                self.last_poll_time = Some(Instant::now());
                let mut args = HashMap::new();
                args.insert("op".to_string(), LLSDValue::String("getSimStats".to_string()));
                self.pending_stats_req = Some(client.request_raw_async("LLWorld", args, Duration::from_secs(2)));
            }
        }
    }

    pub fn fetch_notifications(&mut self, client: &LeapClient) {
        if !client.is_connected() || self.pending_notifs_req.is_some() {
            return;
        }
        self.last_notifs_poll = Some(Instant::now());
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("getNotifications".to_string()));
        self.pending_notifs_req = Some(client.request_raw_async("LLNotifications", args, Duration::from_secs(3)));
    }

    pub fn update_notifications(&mut self, client: &LeapClient) {
        // Poll pending getNotifications request
        if let Some(ref mut req) = self.pending_notifs_req {
            if let Some(res) = req.poll() {
                if let Ok(reply) = res {
                    self.notifications = NotificationItem::list_from_llsd(&reply);
                }
                self.pending_notifs_req = None;
            }
        }

        // Poll pending respondNotification request
        if let Some(ref mut req) = self.respond_req {
            if let Some(res) = req.poll() {
                if let Ok(reply) = res {
                    if get_bool(&reply, "status").unwrap_or(false) {
                        self.notif_status_msg = Some(("Notification action applied successfully".to_string(), Instant::now()));
                        self.fetch_notifications(client);
                    } else {
                        let err = get_str(&reply, "error").unwrap_or("Failed to process notification");
                        self.notif_status_msg = Some((format!("Error: {err}"), Instant::now()));
                    }
                }
                self.respond_req = None;
            }
        }

        // Cooldown polling every 2s
        if client.is_connected() && self.pending_notifs_req.is_none() {
            let should_poll = match self.last_notifs_poll {
                Some(last) => last.elapsed() >= Duration::from_millis(2000),
                None => true,
            };
            if should_poll {
                self.fetch_notifications(client);
            }
        }
    }

    /// Push new telemetry sample into the rolling sparkline ring buffers.
    pub fn push_history(&mut self, fps: f64, ft_ms: f64, avatars: f64) {
        if self.fps_history.len() >= MAX_HISTORY_SAMPLES {
            self.fps_history.pop_front();
        }
        self.fps_history.push_back(fps);

        if self.frametime_history.len() >= MAX_HISTORY_SAMPLES {
            self.frametime_history.pop_front();
        }
        self.frametime_history.push_back(ft_ms);

        if self.avatar_history.len() >= MAX_HISTORY_SAMPLES {
            self.avatar_history.pop_front();
        }
        self.avatar_history.push_back(avatars);
    }

    pub fn ui(&mut self, ui: &mut Ui, client: &LeapClient, test_state: &Arc<Mutex<TestRunState>>) {
        self.update_sim_stats(client);
        self.update_notifications(client);

        let state = test_state.lock().unwrap().clone();
        let is_connected = client.is_connected();

        ui.vertical(|ui| {
            // ── TOP SECTION: Simulator Health & Performance Sparkline Profiler ──
            card_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("📊 Simulator Health & Performance Profiler (`LLWorld.getSimStats`)");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Health status badge
                        let (badge_text, badge_color, fill_color) = if !is_connected {
                            ("○ OFFLINE", COLOR_TEXT_MUTED, Color32::from_rgb(28, 30, 36))
                        } else if self.current_sim_fps >= 35.0 {
                            ("● SIM HEALTHY", COLOR_SUCCESS, Color32::from_rgb(20, 45, 30))
                        } else if self.current_sim_fps >= 20.0 {
                            ("▲ DEGRADED PERFORMANCE", COLOR_WARNING, Color32::from_rgb(45, 40, 20))
                        } else {
                            ("✖ SEVERE SIM LAG", COLOR_DANGER, Color32::from_rgb(50, 20, 20))
                        };

                        let badge_btn = egui::Button::new(
                            RichText::new(badge_text).strong().size(12.0).color(badge_color)
                        ).fill(fill_color);
                        ui.add(badge_btn);
                    });
                });

                ui.add_space(6.0);

                // Stats readout metrics chips
                ui.horizontal(|ui| {
                    inner_box_frame().show(ui, |ui| {
                        ui.label(RichText::new("Sim FPS:").color(COLOR_TEXT_MUTED).size(11.0));
                        let color = if self.current_sim_fps >= 35.0 { COLOR_SUCCESS } else if self.current_sim_fps >= 20.0 { COLOR_WARNING } else { COLOR_DANGER };
                        ui.label(RichText::new(format!("{:.1} / 45.0", self.current_sim_fps)).strong().color(color).size(13.0).monospace());
                    });

                    inner_box_frame().show(ui, |ui| {
                        let ft = if self.current_sim_fps > 0.0 { 1000.0 / self.current_sim_fps } else { 0.0 };
                        ui.label(RichText::new("Frame Time:").color(COLOR_TEXT_MUTED).size(11.0));
                        ui.label(RichText::new(format!("{ft:.1} ms")).strong().color(COLOR_ACCENT).size(13.0).monospace());
                    });

                    inner_box_frame().show(ui, |ui| {
                        ui.label(RichText::new("Avatars:").color(COLOR_TEXT_MUTED).size(11.0));
                        ui.label(RichText::new(format!("{}", self.current_avatar_count)).strong().color(COLOR_ACCENT).size(13.0).monospace());
                    });

                    inner_box_frame().show(ui, |ui| {
                        ui.label(RichText::new("Main Region:").color(COLOR_TEXT_MUTED).size(11.0));
                        ui.label(RichText::new(&self.current_region_name).strong().size(13.0));
                    });

                    inner_box_frame().show(ui, |ui| {
                        let uptime_fmt = format_uptime(self.current_uptime_sec);
                        ui.label(RichText::new("Uptime:").color(COLOR_TEXT_MUTED).size(11.0));
                        ui.label(RichText::new(uptime_fmt).strong().color(COLOR_TEXT_MUTED).size(13.0).monospace());
                    });
                });

                ui.add_space(8.0);

                // Rolling Sparkline Plots (3 side-by-side columns)
                ui.columns(3, |cols| {
                    cols[0].vertical(|ui| {
                        render_sparkline(
                            ui,
                            "📈 Sim FPS (Target: 45)",
                            &self.fps_history,
                            "FPS",
                            Some(45.0),
                            COLOR_SUCCESS,
                            0.0,
                            60.0,
                        );
                    });

                    cols[1].vertical(|ui| {
                        render_sparkline(
                            ui,
                            "⏱ Frame Time (Target: 22.2ms)",
                            &self.frametime_history,
                            "ms",
                            Some(22.2),
                            COLOR_ACCENT,
                            0.0,
                            80.0,
                        );
                    });

                    cols[2].vertical(|ui| {
                        render_sparkline(
                            ui,
                            "👥 Simulator Avatar Load",
                            &self.avatar_history,
                            "avs",
                            None,
                            Color32::from_rgb(180, 140, 240),
                            0.0,
                            10.0,
                        );
                    });
                });
            });

            ui.add_space(10.0);

            // ── MIDDLE SECTION: Active Notifications & Dialogs (Milestone 8.2) ──
            card_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("🔔 Active Notifications, Dialogs & Requests");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.add_enabled(is_connected && self.pending_notifs_req.is_none(), egui::Button::new("🔄 Refresh")).clicked() {
                            self.fetch_notifications(client);
                        }
                        if !self.notifications.is_empty() {
                            let badge_text = format!("● {} PENDING", self.notifications.len());
                            ui.label(RichText::new(badge_text).color(COLOR_WARNING).strong().size(12.0));
                        } else {
                            ui.label(RichText::new("0 PENDING").color(COLOR_TEXT_MUTED).size(12.0));
                        }
                    });
                });

                if let Some((ref msg, time)) = self.notif_status_msg {
                    if time.elapsed() < Duration::from_secs(4) {
                        ui.add_space(4.0);
                        ui.label(RichText::new(msg).color(COLOR_ACCENT).size(11.0));
                    }
                }

                ui.add_space(6.0);

                if self.notifications.is_empty() {
                    ui.label(
                        RichText::new("No dialog notifications, group invitations, or script permission requests currently pending.")
                            .color(COLOR_TEXT_MUTED)
                            .size(12.0),
                    );
                } else {
                    for notif in &self.notifications {
                        inner_box_frame().show(ui, |ui| {
                            ui.horizontal(|ui| {
                                let (badge_name, badge_color) = match notif.kind.as_str() {
                                    "Dialog" => ("💬 SCRIPT DIALOG", COLOR_ACCENT),
                                    "Alert" => ("⚠ ALERT", COLOR_WARNING),
                                    "Permission" => ("🔑 PERMISSION REQUEST", Color32::from_rgb(180, 140, 240)),
                                    "GroupInvite" => ("👥 GROUP INVITE", COLOR_SUCCESS),
                                    "Offer" => ("🎁 INVENTORY OFFER", COLOR_SUCCESS),
                                    _ => ("ℹ NOTIFICATION", COLOR_TEXT_MUTED),
                                };
                                ui.label(RichText::new(badge_name).color(badge_color).strong().size(11.0));
                                ui.label(RichText::new(&notif.title).strong().size(12.0));
                            });

                            if !notif.body.is_empty() {
                                ui.add_space(3.0);
                                ui.label(RichText::new(&notif.body).size(12.0));
                            }

                            if !notif.permission_names.is_empty() {
                                ui.add_space(2.0);
                                ui.label(
                                    RichText::new(format!("Requested Permissions: {}", notif.permission_names.join(", ")))
                                        .color(COLOR_WARNING)
                                        .size(11.0),
                                );
                            }

                            if let Some(ref u) = notif.url {
                                if !u.is_empty() {
                                    ui.add_space(2.0);
                                    if ui.link(format!("🌐 Open Link: {u}")).clicked() {
                                        ui.ctx().open_url(egui::OpenUrl::new_tab(u));
                                    }
                                }
                            }

                            ui.add_space(6.0);
                            ui.horizontal_wrapped(|ui| {
                                if notif.buttons.is_empty() {
                                    if ui.button("✔ Accept").clicked() {
                                        let mut args = HashMap::new();
                                        args.insert("op".to_string(), LLSDValue::String("respondNotification".to_string()));
                                        args.insert("id".to_string(), LLSDValue::Real(notif.id));
                                        args.insert("action".to_string(), LLSDValue::String("accept".to_string()));
                                        self.respond_req = Some(client.request_raw_async("LLNotifications", args, Duration::from_secs(3)));
                                    }
                                    if ui.button("✖ Decline").clicked() {
                                        let mut args = HashMap::new();
                                        args.insert("op".to_string(), LLSDValue::String("respondNotification".to_string()));
                                        args.insert("id".to_string(), LLSDValue::Real(notif.id));
                                        args.insert("action".to_string(), LLSDValue::String("decline".to_string()));
                                        self.respond_req = Some(client.request_raw_async("LLNotifications", args, Duration::from_secs(3)));
                                    }
                                    if ui.button("Cancel / Dismiss").clicked() {
                                        let mut args = HashMap::new();
                                        args.insert("op".to_string(), LLSDValue::String("respondNotification".to_string()));
                                        args.insert("id".to_string(), LLSDValue::Real(notif.id));
                                        args.insert("action".to_string(), LLSDValue::String("ignore".to_string()));
                                        self.respond_req = Some(client.request_raw_async("LLNotifications", args, Duration::from_secs(3)));
                                    }
                                } else {
                                    for (btn_idx, btn_label) in notif.buttons.iter().enumerate() {
                                        if ui.button(btn_label).clicked() {
                                            let mut args = HashMap::new();
                                            args.insert("op".to_string(), LLSDValue::String("respondNotification".to_string()));
                                            args.insert("id".to_string(), LLSDValue::Real(notif.id));
                                            args.insert("action".to_string(), LLSDValue::String("button".to_string()));
                                            args.insert("button_index".to_string(), LLSDValue::Integer(btn_idx as i32));
                                            self.respond_req = Some(client.request_raw_async("LLNotifications", args, Duration::from_secs(3)));
                                        }
                                    }
                                    if ui.button("Dismiss").clicked() {
                                        let mut args = HashMap::new();
                                        args.insert("op".to_string(), LLSDValue::String("respondNotification".to_string()));
                                        args.insert("id".to_string(), LLSDValue::Real(notif.id));
                                        args.insert("action".to_string(), LLSDValue::String("ignore".to_string()));
                                        self.respond_req = Some(client.request_raw_async("LLNotifications", args, Duration::from_secs(3)));
                                    }
                                }
                            });
                        });
                        ui.add_space(4.0);
                    }
                }
            });

            ui.add_space(10.0);

            // ── BOTTOM SECTION: Automated Feature Verification Suite ───────────
            card_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("🚀 Automated Feature Verification Suite");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let btn_enabled = is_connected && !state.is_running;
                        let run_btn = egui::Button::new(
                            RichText::new(if state.is_running { "⏳ Running Tests..." } else { "▶ Run All Feature Tests" })
                                .size(14.0)
                                .color(Color32::WHITE)
                        )
                        .fill(if btn_enabled { COLOR_ACCENT } else { Color32::from_rgb(60, 65, 75) });

                        if ui.add_enabled(btn_enabled, run_btn).clicked() {
                            TestRunner::start_suite(client.clone(), test_state.clone());
                        }

                        if !is_connected {
                            ui.label(RichText::new("Connect to viewer above to run tests").color(COLOR_WARNING).size(12.0));
                        }
                    });
                });

                ui.add_space(6.0);
                ui.label(
                    RichText::new(
                        "Executes the full suite of LEAP protocol features in Aurora-Viewer including handshake UUID verification, ping/reqid roundtrips, API introspection, LLViewerControl settings mutations, subsystems telemetry (LLAppViewer, LLAgent, LLLogin, LLCamera, LLInventory), pub/sub pumps, LLChat transmission, and pipelining throughput."
                    ).color(COLOR_TEXT_MUTED).size(12.0)
                );

                ui.add_space(8.0);

                // Progress Bar
                if state.is_running || state.progress > 0.0 {
                    ui.horizontal(|ui| {
                        let bar = egui::ProgressBar::new(state.progress)
                            .show_percentage()
                            .animate(state.is_running);
                        ui.add_sized([ui.available_width() - 160.0, 18.0], bar);
                        ui.label(RichText::new(&state.current_test).size(12.0));
                    });
                    ui.add_space(8.0);
                }

                // Summary Stats Metrics
                ui.horizontal(|ui| {
                    stat_badge(ui, "Total Tests", &state.total_tests.to_string(), Color32::LIGHT_GRAY);
                    stat_badge(ui, "Passed", &state.passed_tests.to_string(), COLOR_SUCCESS);
                    stat_badge(ui, "Failed", &state.failed_tests.to_string(), if state.failed_tests > 0 { COLOR_DANGER } else { COLOR_TEXT_MUTED });
                    if let Some(d) = state.total_duration {
                        stat_badge(ui, "Duration", &format!("{:.2}s", d.as_secs_f64()), COLOR_ACCENT);
                    }
                });
            });

            ui.add_space(8.0);

            // Test Cases List
            ui.label(RichText::new("Feature Test Cases & Assertions").strong().size(13.0));
            ui.add_space(4.0);

            egui::ScrollArea::vertical().id_salt("feature_tests_scroll").max_height(280.0).show(ui, |ui| {
                for case in &state.cases {
                    card_frame().show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (status_icon, status_color) = match case.status {
                                TestStatus::Pending => ("⚪ Pending", COLOR_TEXT_MUTED),
                                TestStatus::Running => ("🔄 Running", COLOR_WARNING),
                                TestStatus::Pass => ("✔ PASS", COLOR_SUCCESS),
                                TestStatus::Fail => ("✖ FAIL", COLOR_DANGER),
                            };

                            ui.label(RichText::new(status_icon).strong().color(status_color).size(13.0));
                            ui.add_space(8.0);

                            ui.vertical(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(case.name).strong().size(13.0));
                                    ui.label(
                                        RichText::new(format!("[{}]", case.category))
                                            .color(COLOR_TEXT_MUTED)
                                            .size(11.0),
                                    );
                                    if let Some(d) = case.duration {
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            ui.label(RichText::new(format!("{:.3}s", d.as_secs_f64())).color(COLOR_TEXT_MUTED).size(11.0));
                                        });
                                    }
                                });
                                ui.label(RichText::new(case.description).color(COLOR_TEXT_MUTED).size(11.0));
                            });
                        });

                        // Show details or error output if available
                        if !case.details.is_empty() {
                            ui.add_space(6.0);
                            inner_box_frame().show(ui, |ui| {
                                ui.label(
                                    RichText::new(&case.details)
                                        .monospace()
                                        .size(11.0)
                                        .color(if case.status == TestStatus::Fail { COLOR_DANGER } else { Color32::LIGHT_GRAY }),
                                );
                            });
                        }
                    });
                    ui.add_space(4.0);
                }
            });
        });
    }
}

/// Helper function to format seconds into HH:MM:SS.
fn format_uptime(secs: f64) -> String {
    let s = secs as u64;
    let hours = s / 3600;
    let mins = (s % 3600) / 60;
    let rem = s % 60;
    format!("{hours:02}:{mins:02}:{rem:02}")
}

/// Custom sparkline chart renderer for rolling telemetry points.
fn render_sparkline(
    ui: &mut Ui,
    title: &str,
    history: &VecDeque<f64>,
    unit: &str,
    target: Option<f64>,
    stroke_color: Color32,
    min_floor: f64,
    max_ceil: f64,
) {
    ui.vertical(|ui| {
        let (latest_val, min_val, max_val) = if !history.is_empty() {
            let latest = *history.back().unwrap();
            let mut min = latest;
            let mut max = latest;
            for v in history {
                if *v < min { min = *v; }
                if *v > max { max = *v; }
            }
            (latest, min, max)
        } else {
            (0.0, 0.0, 0.0)
        };

        // Header label with current value
        ui.horizontal(|ui| {
            ui.label(RichText::new(title).size(11.0).color(COLOR_TEXT_MUTED));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(format!("{latest_val:.1} {unit}")).strong().color(stroke_color).size(12.0));
            });
        });

        ui.add_space(2.0);

        // Sparkline Canvas
        let height = 65.0;
        let width = ui.available_width().max(100.0);
        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height), Sense::hover());
        let painter = ui.painter_at(rect);

        // Background
        painter.rect_filled(rect, 4.0, Color32::from_rgb(18, 20, 26));
        painter.rect_stroke(rect, 4.0, Stroke::new(1.0_f32, Color32::from_rgb(32, 38, 48)));

        // Compute Y range
        let actual_max = max_val.max(max_ceil);
        let actual_min = min_val.min(min_floor);
        let y_range = (actual_max - actual_min).max(1.0);

        let map_y = |val: f64| -> f32 {
            let frac = ((val - actual_min) / y_range).clamp(0.0, 1.0) as f32;
            rect.bottom() - 6.0 - frac * (rect.height() - 12.0)
        };

        // Draw Target Line if provided
        if let Some(tgt) = target {
            if tgt >= actual_min && tgt <= actual_max {
                let y = map_y(tgt);
                painter.line_segment(
                    [Pos2::new(rect.left() + 4.0, y), Pos2::new(rect.right() - 4.0, y)],
                    Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(100, 150, 220, 90)),
                );
            }
        }

        // Draw Sparkline Points
        let n = history.len();
        if n >= 2 {
            let x_step = (rect.width() - 12.0) / ((MAX_HISTORY_SAMPLES - 1) as f32);
            let offset_x = (MAX_HISTORY_SAMPLES - n) as f32 * x_step;

            let mut pts = Vec::with_capacity(n);
            for (idx, v) in history.iter().enumerate() {
                let x = rect.left() + 6.0 + offset_x + (idx as f32 * x_step);
                let y = map_y(*v);
                pts.push(Pos2::new(x, y));
            }

            // Draw line curve
            for w in pts.windows(2) {
                painter.line_segment([w[0], w[1]], Stroke::new(1.8_f32, stroke_color));
            }

            // Draw dot on latest sample
            if let Some(last_pt) = pts.last() {
                painter.circle_filled(*last_pt, 3.0, Color32::WHITE);
                painter.circle_filled(*last_pt, 2.0, stroke_color);
            }
        }

        // Min/Max annotations in footer
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("min: {min_val:.1}")).color(COLOR_TEXT_MUTED).size(9.0));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(format!("max: {max_val:.1}")).color(COLOR_TEXT_MUTED).size(9.0));
            });
        });
    });
}

fn stat_badge(ui: &mut Ui, title: &str, value: &str, color: Color32) {
    inner_box_frame().show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{title}:")).color(COLOR_TEXT_MUTED).size(11.0));
            ui.label(RichText::new(value).strong().color(color).size(12.0));
        });
    });
    ui.add_space(4.0);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sparkline_history_buffer_capacity() {
        let mut tab = DashboardTab::new();
        for i in 0..100 {
            tab.push_history(i as f64, 20.0, 1.0);
        }
        assert_eq!(tab.fps_history.len(), MAX_HISTORY_SAMPLES);
        assert_eq!(*tab.fps_history.back().unwrap(), 99.0);
        assert_eq!(*tab.fps_history.front().unwrap(), 40.0);
    }

    #[test]
    fn test_format_uptime() {
        assert_eq!(format_uptime(0.0), "00:00:00");
        assert_eq!(format_uptime(65.0), "00:01:05");
        assert_eq!(format_uptime(3665.0), "01:01:05");
    }
}
