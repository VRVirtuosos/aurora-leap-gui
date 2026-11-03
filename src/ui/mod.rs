//! Main UI Application Module.

pub mod theme;
pub mod tab_dashboard;
pub mod tab_login;
pub mod tab_movement;
pub mod tab_camera;
pub mod tab_photos;
pub mod tab_inventory;
pub mod tab_scripting;
pub mod tab_apis;
pub mod tab_settings;
pub mod tab_chat;
pub mod tab_world;
pub mod tab_radar;
pub mod tab_session;
pub mod tab_events;
pub mod tab_raw;
pub mod tab_logs;
pub mod tab_objects;
pub mod repl_drawer;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use egui::{Color32, RichText, Ui};

use crate::client::{LeapClient, ProcessStatus};
use crate::tests::TestRunState;
use self::theme::*;
use self::tab_dashboard::DashboardTab;
use self::tab_login::LoginTab;
use self::tab_movement::MovementTab;
use self::tab_camera::CameraTab;
use self::tab_objects::ObjectsTab;
use self::tab_photos::PhotosTab;
use self::tab_inventory::InventoryTab;
use self::tab_scripting::ScriptingTab;
use self::tab_apis::ApisTab;
use self::tab_settings::SettingsTab;
use self::tab_chat::ChatTab;
use self::tab_world::WorldTab;
use self::tab_radar::RadarTab;
use self::tab_session::SessionTab;
use self::tab_events::EventsTab;
use self::tab_raw::RawTab;
use self::tab_logs::LogsTab;
use self::repl_drawer::ReplDrawer;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveTab {
    Dashboard,
    Login,
    Movement,
    Camera,
    Objects,
    Inventory,
    Photos,
    Scripting,
    Apis,
    Settings,
    Chat,
    World,
    Radar,
    Session,
    Events,
    Raw,
    Logs,
}

pub struct LeapStudioApp {
    client: LeapClient,
    test_state: Arc<Mutex<TestRunState>>,

    // Path & Configuration
    exe_path: String,
    extra_args: String,

    // Active Navigation Tab
    active_tab: ActiveTab,

    // Sub-Tabs
    tab_dashboard: DashboardTab,
    tab_login: LoginTab,
    tab_movement: MovementTab,
    tab_camera: CameraTab,
    tab_objects: ObjectsTab,
    tab_inventory: InventoryTab,
    tab_photos: PhotosTab,
    tab_scripting: ScriptingTab,
    tab_apis: ApisTab,
    tab_settings: SettingsTab,
    tab_chat: ChatTab,
    tab_world: WorldTab,
    tab_radar: RadarTab,
    tab_session: SessionTab,
    tab_events: EventsTab,
    tab_raw: RawTab,
    tab_logs: LogsTab,
    repl_drawer: ReplDrawer,

    connection_error: Option<String>,
}

impl LeapStudioApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        // Look for default binary path
        let candidate_paths = [
            "../Aurora-Viewer/target/debug/aurora-viewer.exe",
            "Aurora-Viewer/target/debug/aurora-viewer.exe",
            "target/debug/aurora-viewer.exe",
        ];

        let mut default_exe = "Aurora-Viewer/target/debug/aurora-viewer.exe".to_string();
        for p in candidate_paths {
            let path = PathBuf::from(p);
            if path.exists() {
                if let Ok(canon) = path.canonicalize() {
                    default_exe = canon.to_string_lossy().to_string();
                    break;
                }
            }
        }

        Self {
            client: LeapClient::new(),
            test_state: Arc::new(Mutex::new(TestRunState::default())),
            exe_path: default_exe,
            extra_args: String::new(),
            active_tab: ActiveTab::Dashboard,
            tab_dashboard: DashboardTab::new(),
            tab_login: LoginTab::new(),
            tab_movement: MovementTab::new(),
            tab_camera: CameraTab::new(),
            tab_objects: ObjectsTab::new(),
            tab_inventory: InventoryTab::new(),
            tab_photos: PhotosTab::new(),
            tab_scripting: ScriptingTab::new(),
            tab_apis: ApisTab::new(),
            tab_settings: SettingsTab::new(),
            tab_chat: ChatTab::new(),
            tab_world: WorldTab::new(),
            tab_radar: RadarTab::new(),
            tab_session: SessionTab::new(),
            tab_events: EventsTab::new(),
            tab_raw: RawTab::new(),
            tab_logs: LogsTab::new(),
            repl_drawer: ReplDrawer::new(),
            connection_error: None,
        }
    }

    fn render_top_bar(&mut self, ui: &mut Ui) {
        card_frame().show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Aurora-Viewer LEAP Studio").heading().strong().color(COLOR_ACCENT));
                ui.label(RichText::new("v0.1.0 (serde-llsd)").color(COLOR_TEXT_MUTED).size(12.0));

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let is_connected = self.client.is_connected();

                    if is_connected {
                        let disc_btn = egui::Button::new(RichText::new("⏹ Disconnect Viewer").color(Color32::WHITE)).fill(COLOR_DANGER);
                        if ui.add(disc_btn).clicked() {
                            self.client.disconnect();
                        }
                    } else {
                        let conn_btn = egui::Button::new(RichText::new("▶ Launch & Connect").strong().color(Color32::WHITE)).fill(COLOR_SUCCESS);
                        if ui.add(conn_btn).clicked() {
                            let path = PathBuf::from(&self.exe_path);
                            let args: Vec<String> = self.extra_args
                                .split_whitespace()
                                .map(|s| s.to_string())
                                .collect();

                            match self.client.connect(&path, &args, None) {
                                Ok(_) => self.connection_error = None,
                                Err(err) => self.connection_error = Some(err),
                            }
                        }
                    }

                    // Status Pill
                    match self.client.status() {
                        ProcessStatus::Connected { pid } => {
                            inner_box_frame().show(ui, |ui| {
                                ui.label(RichText::new(format!("🟢 Connected (PID: {pid})")).strong().color(COLOR_SUCCESS));
                            });
                        }
                        ProcessStatus::Starting => {
                            inner_box_frame().show(ui, |ui| {
                                ui.label(RichText::new("🟡 Starting & Handshaking...").color(COLOR_WARNING));
                            });
                        }
                        ProcessStatus::Terminated { exit_code } => {
                            inner_box_frame().show(ui, |ui| {
                                ui.label(RichText::new(format!("🔴 Terminated ({:?})", exit_code)).color(COLOR_DANGER));
                            });
                        }
                        ProcessStatus::Disconnected => {
                            inner_box_frame().show(ui, |ui| {
                                ui.label(RichText::new("⚪ Disconnected").color(COLOR_TEXT_MUTED));
                            });
                        }
                    }
                });
            });

            ui.add_space(8.0);

            // Path & Argument Inputs
            ui.horizontal(|ui| {
                ui.label("Viewer Binary:");
                ui.add_sized([ui.available_width() - 250.0, 20.0], egui::TextEdit::singleline(&mut self.exe_path));

                if ui.button("Browse...").clicked() {
                    if let Some(file) = rfd::FileDialog::new()
                        .add_filter("Executable", &["exe"])
                        .pick_file()
                    {
                        self.exe_path = file.to_string_lossy().to_string();
                    }
                }

                ui.label("Extra Args:");
                ui.add_sized([120.0, 20.0], egui::TextEdit::singleline(&mut self.extra_args));
            });

            // Handshake & Telemetry Banner if connected
            if let Some(handshake) = self.client.handshake() {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    stat_pill(ui, "Cmd UUID", &handshake.cmd_uuid);
                    stat_pill(ui, "Reply UUID", &handshake.reply_uuid);
                    stat_pill(ui, "Sent", &self.client.sent_count().to_string());
                    stat_pill(ui, "Recv", &self.client.recv_count().to_string());
                });
            }

            if let Some(ref err) = self.connection_error {
                ui.add_space(6.0);
                ui.label(RichText::new(format!("Connection Error: {err}")).color(COLOR_DANGER).size(12.0));
            }
        });
    }

    fn render_navigation_tabs(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let tabs = [
                (ActiveTab::Dashboard, "🚀 Feature Tests"),
                (ActiveTab::Scripting, "📜 Scripts & Automation"),
                (ActiveTab::Login, "🔑 Login"),
                (ActiveTab::Movement, "🕹️ Movement"),
                (ActiveTab::Camera, "🎥 Camera & Views"),
                (ActiveTab::Objects, "📦 Objects & Linksets"),
                (ActiveTab::Inventory, "🎒 Inventory"),
                (ActiveTab::Photos, "📸 Snapshots"),
                (ActiveTab::Apis, "🔍 Introspection"),
                (ActiveTab::Settings, "⚙️ Settings"),
                (ActiveTab::Chat, "💬 Chat"),
                (ActiveTab::World, "🧭 Avatar & World"),
                (ActiveTab::Radar, "📡 Radar & Map"),
                (ActiveTab::Session, "🔐 Session"),
                (ActiveTab::Events, "📡 Event Stream"),
                (ActiveTab::Raw, "🛠️ Raw LLSD"),
                (ActiveTab::Logs, "📜 Logs"),
            ];

            for (tab, label) in tabs {
                let is_selected = self.active_tab == tab;
                let btn = egui::Button::new(
                    RichText::new(label)
                        .strong()
                        .color(if is_selected { COLOR_ACCENT } else { Color32::LIGHT_GRAY })
                )
                .fill(if is_selected { Color32::from_rgb(32, 40, 56) } else { Color32::TRANSPARENT });

                if ui.add(btn).clicked() {
                    self.active_tab = tab;
                }
            }
        });
    }
}

impl eframe::App for LeapStudioApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // Request repaint continuously if running tests or connected to maintain live data streams
        if self.client.is_connected() || self.test_state.lock().unwrap().is_running {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }

        // Render Bottom Interactive REPL Drawer
        egui::TopBottomPanel::bottom("repl_drawer_panel")
            .frame(egui::Frame::none().fill(Color32::from_rgb(18, 20, 24)))
            .show(ctx, |ui| {
                self.repl_drawer.ui(ui, &self.client);
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.render_top_bar(ui);
            ui.add_space(8.0);

            self.render_navigation_tabs(ui);
            ui.add_space(10.0);

            // Render Active Tab Content
            match self.active_tab {
                ActiveTab::Dashboard => self.tab_dashboard.ui(ui, &self.client, &self.test_state),
                ActiveTab::Scripting => self.tab_scripting.ui(ui, &self.client),
                ActiveTab::Login => self.tab_login.ui(ui, &self.client),
                ActiveTab::Movement => self.tab_movement.ui(ui, &self.client),
                ActiveTab::Camera => self.tab_camera.ui(ui, &self.client),
                ActiveTab::Objects => self.tab_objects.ui(ui, &self.client),
                ActiveTab::Inventory => self.tab_inventory.ui(ui, &self.client),
                ActiveTab::Photos => self.tab_photos.ui(ui, &self.client),
                ActiveTab::Apis => self.tab_apis.ui(ui, &self.client),
                ActiveTab::Settings => self.tab_settings.ui(ui, &self.client),
                ActiveTab::Chat => self.tab_chat.ui(ui, &self.client),
                ActiveTab::World => self.tab_world.ui(ui, &self.client),
                ActiveTab::Radar => self.tab_radar.ui(ui, &self.client),
                ActiveTab::Session => self.tab_session.ui(ui, &mut self.client),
                ActiveTab::Events => self.tab_events.ui(ui, &self.client),
                ActiveTab::Raw => self.tab_raw.ui(ui, &self.client),
                ActiveTab::Logs => self.tab_logs.ui(ui, &self.client),
            }
        });
    }
}

fn stat_pill(ui: &mut Ui, label: &str, val: &str) {
    inner_box_frame().show(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(RichText::new(format!("{label}:")).color(COLOR_TEXT_MUTED).size(10.0));
            ui.label(RichText::new(val).monospace().size(10.0).color(COLOR_ACCENT));
        });
    });
    ui.add_space(2.0);
}
