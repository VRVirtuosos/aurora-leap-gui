//! Avatar Login Panel Tab — connects via `LLLogin` LEAP API.
//! Features saved avatar profiles with passwords, grid selections, and accurate SLurls for Home and Last locations.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use egui::{Color32, RichText, Ui};
use serde::{Deserialize, Serialize};
use serde_llsd::LLSDValue;

use crate::client::{AsyncRequest, LeapClient};
use crate::codec::get_str;
use crate::landmarks::LandmarkStore;
use crate::ui::theme::{
    card_frame, inner_box_frame, COLOR_ACCENT, COLOR_DANGER, COLOR_SUCCESS, COLOR_TEXT_MUTED,
    COLOR_WARNING,
};

/// Known grids / login URIs.
pub const KNOWN_GRIDS: &[(&str, &str)] = &[
    ("Second Life (Agni)", "https://login.agni.lindenlab.com/cgi-bin/login.cgi"),
    ("Second Life Beta (Aditi)", "https://login.aditi.lindenlab.com/cgi-bin/login.cgi"),
    ("OSGrid", "http://login.osgrid.org/"),
    ("Custom URI…", ""),
];

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SavedAvatar {
    pub username: String,
    #[serde(default)]
    pub password: String,
    pub grid_index: usize,
    pub custom_uri: String,
    pub start_location: String,
    pub remember_password: bool,
}

#[derive(Debug, Clone, PartialEq)]
enum LoginState {
    Idle,
    Pending,
    LoggedIn,
    Failed(String),
}

pub struct LoginTab {
    // Saved avatar profiles
    pub saved_avatars: Vec<SavedAvatar>,
    pub selected_avatar_index: Option<usize>,
    pub remember_password: bool,
    pub show_password: bool,
    pub profile_status: Option<String>,

    // Credential fields
    pub username: String,        // "First Last" format
    pub password: String,        // hidden / revealed
    pub grid_index: usize,       // which entry in KNOWN_GRIDS
    pub custom_uri: String,      // filled when grid_index points to "Custom URI…"
    pub start_location: String,  // "last", "home", or a SLURL
    pub custom_slurl: String,

    // Live state
    login_state: LoginState,
    state_from_viewer: String,
    agent_id: String,
    last_poll_time: Option<std::time::Instant>,

    // Landmarks store for Home and Last SLurls
    pub landmark_store: LandmarkStore,

    // In-flight non-blocking requests
    pending_login: Option<AsyncRequest<LLSDValue>>,
    pending_query: Option<AsyncRequest<LLSDValue>>,
}

impl LoginTab {
    pub fn new() -> Self {
        let mut tab = Self {
            saved_avatars: Vec::new(),
            selected_avatar_index: None,
            remember_password: true,
            show_password: false,
            profile_status: None,
            username: String::new(),
            password: String::new(),
            grid_index: 0,
            custom_uri: String::new(),
            start_location: "last".to_string(),
            custom_slurl: String::new(),
            login_state: LoginState::Idle,
            state_from_viewer: String::new(),
            agent_id: String::new(),
            last_poll_time: None,
            landmark_store: LandmarkStore::load(),
            pending_login: None,
            pending_query: None,
        };

        tab.load_saved_avatars();
        if let Some(first) = tab.saved_avatars.first().cloned() {
            tab.apply_avatar(&first);
            tab.selected_avatar_index = Some(0);
        }

        tab
    }

    fn load_saved_avatars(&mut self) {
        let path = PathBuf::from("saved_avatars.json");
        if path.exists() {
            if let Ok(data) = std::fs::read_to_string(&path) {
                if let Ok(list) = serde_json::from_str::<Vec<SavedAvatar>>(&data) {
                    self.saved_avatars = list;
                }
            }
        }
    }

    fn save_avatars_to_disk(&self) {
        let path = PathBuf::from("saved_avatars.json");
        if let Ok(json) = serde_json::to_string_pretty(&self.saved_avatars) {
            let _ = std::fs::write(path, json);
        }
    }

    fn apply_avatar(&mut self, av: &SavedAvatar) {
        self.username = av.username.clone();
        self.password = if av.remember_password { av.password.clone() } else { String::new() };
        self.grid_index = av.grid_index.min(KNOWN_GRIDS.len() - 1);
        self.custom_uri = av.custom_uri.clone();
        self.start_location = av.start_location.clone();
        self.remember_password = av.remember_password;
    }

    pub fn ui(&mut self, ui: &mut Ui, client: &LeapClient) {
        let is_connected = client.is_connected();

        // 1. Reactive telemetry update from inbound LEAP events (zero wait)
        let telem = client.cached_telemetry();
        if let Some(ref st) = telem.startup_state {
            self.state_from_viewer = st.clone();
            if st == "STATE_STARTED" || telem.login_state.as_deref() == Some("LoggedIn") {
                self.login_state = LoginState::LoggedIn;
            } else if st == "STATE_LOGIN_FAILED" || telem.login_state.as_deref() == Some("LoginFailed") {
                self.login_state = LoginState::Failed("Login failed on grid".to_string());
            } else if st == "STATE_LOGIN_SHOW" && self.login_state == LoginState::Pending {
                self.login_state = LoginState::Idle;
            }
        }
        if let Some(ref id) = telem.agent_id {
            self.agent_id = id.clone();
        }
        if let (Some(ref reg), Some((x, y, z))) = (&telem.region_name, telem.agent_pos) {
            self.landmark_store.update_last(reg, x, y, z);
        }

        // 2. Poll in-flight non-blocking login request
        if let Some(ref req) = self.pending_login {
            if let Some(res) = req.poll() {
                match res {
                    Ok(reply) => {
                        let state = get_str(&reply, "state").unwrap_or("").to_string();
                        if state == "STATE_STARTED" {
                            self.login_state = LoginState::LoggedIn;
                        } else {
                            self.login_state = LoginState::Pending;
                        }
                        self.state_from_viewer = state;
                    }
                    Err(e) => {
                        self.login_state = LoginState::Failed(e);
                    }
                }
                self.pending_login = None;
            } else {
                ui.ctx().request_repaint_after(Duration::from_millis(50));
            }
        }

        // 3. Poll in-flight non-blocking status query
        if let Some(ref req) = self.pending_query {
            if let Some(res) = req.poll() {
                if let Ok(reply) = res {
                    self.state_from_viewer = get_str(&reply, "state").unwrap_or("").to_string();
                    self.agent_id = get_str(&reply, "agent_id").unwrap_or("").to_string();
                    let is_logged_in = crate::codec::get_bool(&reply, "logged_in").unwrap_or(false);
                    if self.state_from_viewer == "STATE_STARTED" || is_logged_in {
                        self.login_state = LoginState::LoggedIn;
                    } else if self.state_from_viewer == "STATE_LOGIN_SHOW" && self.login_state == LoginState::Pending {
                        self.login_state = LoginState::Idle;
                    }
                }
                self.pending_query = None;
            }
        }

        // 4. Periodic background polling without blocking
        if is_connected && self.pending_query.is_none() {
            let interval = if self.login_state == LoginState::Pending {
                Duration::from_millis(1000)
            } else {
                Duration::from_secs(3)
            };
            if self.last_poll_time.map_or(true, |t| t.elapsed() >= interval) {
                self.query_status(client);
                self.last_poll_time = Some(std::time::Instant::now());
                ui.ctx().request_repaint_after(interval);
            }
        }

        ui.vertical(|ui| {
            // ── Header ────────────────────────────────────────────────────────────
            card_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("🔑 Avatar Login & Account Manager");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui
                            .add_enabled(
                                is_connected,
                                egui::Button::new(RichText::new("🔄 Query Status").size(13.0)),
                            )
                            .clicked()
                        {
                            self.query_status(client);
                        }
                    });
                });

                // Live state banner
                if !self.state_from_viewer.is_empty() {
                    ui.add_space(4.0);
                    let (colour, icon) = match self.state_from_viewer.as_str() {
                        "STATE_STARTED" => (COLOR_SUCCESS, "🟢"),
                        "STATE_LOGIN_WAIT" => (COLOR_WARNING, "🟡"),
                        _ => (COLOR_TEXT_MUTED, "⚪"),
                    };
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("{icon} Viewer state: {}", self.state_from_viewer))
                                .color(colour)
                                .size(12.0),
                        );
                        if !self.agent_id.is_empty() {
                            ui.add_space(12.0);
                            ui.label(
                                RichText::new(format!("Agent UUID: {}", self.agent_id))
                                    .monospace()
                                    .size(11.0)
                                    .color(COLOR_ACCENT),
                            );
                        }
                    });
                }
            });

            ui.add_space(8.0);

            // ── Saved Avatars Profiles Bar ─────────────────────────────────────────
            card_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("👤 Saved Avatars:").strong());

                    let current_name = self.selected_avatar_index
                        .and_then(|idx| self.saved_avatars.get(idx))
                        .map(|a| a.username.as_str())
                        .unwrap_or("— Select or Create New —");

                    let mut chosen_idx = None;
                    egui::ComboBox::from_id_salt("saved_avatars_combo")
                        .width(220.0)
                        .selected_text(current_name)
                        .show_ui(ui, |ui| {
                            for (idx, av) in self.saved_avatars.iter().enumerate() {
                                let label = format!("{} ({})", av.username, KNOWN_GRIDS[av.grid_index.min(KNOWN_GRIDS.len() - 1)].0);
                                if ui.selectable_label(self.selected_avatar_index == Some(idx), label).clicked() {
                                    chosen_idx = Some(idx);
                                }
                            }
                        });

                    if let Some(idx) = chosen_idx {
                        self.selected_avatar_index = Some(idx);
                        let copy = self.saved_avatars[idx].clone();
                        self.apply_avatar(&copy);
                        self.profile_status = Some(format!("Loaded profile for '{}'", copy.username));
                    }

                    if ui.button("💾 Save Profile").clicked() {
                        if !self.username.trim().is_empty() {
                            let pass_to_save = if self.remember_password { self.password.clone() } else { String::new() };
                            let new_profile = SavedAvatar {
                                username: self.username.trim().to_string(),
                                password: pass_to_save,
                                grid_index: self.grid_index,
                                custom_uri: self.custom_uri.clone(),
                                start_location: self.start_location.clone(),
                                remember_password: self.remember_password,
                            };

                            if let Some(pos) = self.saved_avatars.iter().position(|a| a.username.eq_ignore_ascii_case(&new_profile.username)) {
                                self.saved_avatars[pos] = new_profile;
                                self.selected_avatar_index = Some(pos);
                                self.profile_status = Some(format!("Updated profile for '{}'", self.username));
                            } else {
                                self.saved_avatars.push(new_profile);
                                self.selected_avatar_index = Some(self.saved_avatars.len() - 1);
                                self.profile_status = Some(format!("Saved new profile for '{}'", self.username));
                            }
                            self.save_avatars_to_disk();
                        }
                    }

                    if ui.button("➕ New").clicked() {
                        self.selected_avatar_index = None;
                        self.username.clear();
                        self.password.clear();
                        self.profile_status = Some("Cleared fields for new avatar".to_string());
                    }

                    if let Some(idx) = self.selected_avatar_index {
                        if ui.button("🗑 Delete Profile").clicked() {
                            if idx < self.saved_avatars.len() {
                                let name = self.saved_avatars.remove(idx).username;
                                self.selected_avatar_index = None;
                                self.save_avatars_to_disk();
                                self.profile_status = Some(format!("Deleted profile for '{name}'"));
                            }
                        }
                    }
                });

                if let Some(ref msg) = self.profile_status {
                    ui.add_space(3.0);
                    ui.label(RichText::new(msg).size(11.0).color(COLOR_SUCCESS));
                }
            });

            ui.add_space(8.0);

            // ── Two-column layout: Credentials | Grid & Location ──────────────────
            ui.columns(2, |cols| {
                // Left: credentials
                cols[0].vertical(|ui| {
                    card_frame().show(ui, |ui| {
                        ui.label(RichText::new("Account Credentials").strong().size(13.0));
                        ui.add_space(8.0);

                        inner_box_frame().show(ui, |ui| {
                            ui.label(
                                RichText::new("Username (First Last):")
                                    .color(COLOR_TEXT_MUTED)
                                    .size(11.0),
                            );
                            ui.add_sized(
                                [ui.available_width(), 22.0],
                                egui::TextEdit::singleline(&mut self.username)
                                    .hint_text("e.g. Resident Name or Username"),
                            );
                        });

                        ui.add_space(6.0);

                        inner_box_frame().show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(
                                    RichText::new("Password:")
                                        .color(COLOR_TEXT_MUTED)
                                        .size(11.0),
                                );
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    let eye_btn = if self.show_password { "👁 Hide" } else { "🔒 Show" };
                                    if ui.small_button(eye_btn).clicked() {
                                        self.show_password = !self.show_password;
                                    }
                                });
                            });

                            let text_edit = if self.show_password {
                                egui::TextEdit::singleline(&mut self.password).hint_text("Enter password")
                            } else {
                                egui::TextEdit::singleline(&mut self.password).password(true).hint_text("••••••••")
                            };
                            ui.add_sized([ui.available_width(), 22.0], text_edit);
                        });

                        ui.add_space(6.0);
                        ui.checkbox(&mut self.remember_password, "Remember password for this avatar");
                    });
                });

                // Right: grid & location
                cols[1].vertical(|ui| {
                    card_frame().show(ui, |ui| {
                        ui.label(RichText::new("Grid & Start Location").strong().size(13.0));
                        ui.add_space(8.0);

                        inner_box_frame().show(ui, |ui| {
                            ui.label(
                                RichText::new("Grid / Login URI:")
                                    .color(COLOR_TEXT_MUTED)
                                    .size(11.0),
                            );
                            let current_label = KNOWN_GRIDS[self.grid_index].0;
                            egui::ComboBox::from_id_salt("grid_combo")
                                .selected_text(current_label)
                                .show_ui(ui, |ui| {
                                    for (idx, &(label, _)) in KNOWN_GRIDS.iter().enumerate() {
                                        ui.selectable_value(&mut self.grid_index, idx, label);
                                    }
                                });

                            if self.grid_index == KNOWN_GRIDS.len() - 1 {
                                ui.add_space(4.0);
                                ui.add_sized(
                                    [ui.available_width(), 22.0],
                                    egui::TextEdit::singleline(&mut self.custom_uri)
                                        .hint_text("https://example.com/login"),
                                );
                            }
                        });

                        ui.add_space(6.0);

                        inner_box_frame().show(ui, |ui| {
                            ui.label(
                                RichText::new("Start Location:")
                                    .color(COLOR_TEXT_MUTED)
                                    .size(11.0),
                            );
                            ui.horizontal(|ui| {
                                if ui.selectable_label(self.start_location == "last" || self.start_location.is_empty(), "📍 Last Location").clicked() {
                                    self.start_location = "last".to_string();
                                }
                                if ui.selectable_label(self.start_location == "home", "🏠 Home").clicked() {
                                    self.start_location = "home".to_string();
                                }
                                if ui.selectable_label(!["last", "home", ""].contains(&self.start_location.as_str()), "🌐 Custom SLURL").clicked() {
                                    self.start_location = self.landmark_store.last_slurl();
                                }
                            });

                            ui.add_space(4.0);
                            // Show current SLurl for the selected choice
                            let active_slurl = if self.start_location == "home" {
                                self.landmark_store.home_slurl()
                            } else if self.start_location == "last" || self.start_location.is_empty() {
                                self.landmark_store.last_slurl()
                            } else {
                                self.start_location.clone()
                            };

                            ui.label(RichText::new(format!("SLURL: {active_slurl}")).size(10.0).monospace().color(COLOR_ACCENT));

                            if !["last", "home", ""].contains(&self.start_location.as_str()) {
                                ui.add_space(4.0);
                                ui.add_sized(
                                    [ui.available_width(), 20.0],
                                    egui::TextEdit::singleline(&mut self.start_location)
                                        .hint_text("secondlife://Region/128/128/25"),
                                );
                            }
                        });
                    });
                });
            });

            ui.add_space(10.0);

            // ── Login / Logout Buttons & Status ───────────────────────────────────
            card_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    let login_enabled = is_connected
                        && !self.username.is_empty()
                        && !self.password.is_empty()
                        && self.login_state != LoginState::Pending;

                    let login_btn = egui::Button::new(
                        RichText::new("▶ Login").strong().color(Color32::WHITE),
                    )
                    .fill(COLOR_SUCCESS);

                    if ui.add_enabled(login_enabled, login_btn).clicked() {
                        self.do_login(client);
                    }

                    ui.add_space(8.0);

                    let logout_btn = egui::Button::new(
                        RichText::new("⏹ Logout").color(Color32::WHITE),
                    )
                    .fill(COLOR_DANGER);

                    if ui.add_enabled(is_connected, logout_btn).clicked() {
                        self.do_logout(client);
                    }

                    ui.add_space(16.0);

                    // Inline status message
                    match &self.login_state {
                        LoginState::Idle => {}
                        LoginState::Pending => {
                            ui.label(
                                RichText::new("⏳ Login request sent — waiting for viewer…")
                                    .color(COLOR_WARNING),
                            );
                        }
                        LoginState::LoggedIn => {
                            ui.label(
                                RichText::new("✅ Logged in successfully!")
                                    .color(COLOR_SUCCESS)
                                    .strong(),
                            );
                        }
                        LoginState::Failed(ref msg) => {
                            ui.label(
                                RichText::new(format!("❌ Login failed: {msg}"))
                                    .color(COLOR_DANGER),
                            );
                        }
                    }
                });

                ui.add_space(6.0);

                if !is_connected {
                    ui.label(
                        RichText::new("⚠ Connect to a running Aurora Viewer instance first (top bar).")
                            .color(COLOR_WARNING)
                            .size(11.0),
                    );
                } else if self.username.is_empty() || self.password.is_empty() {
                    ui.label(
                        RichText::new("Select a saved avatar or enter your username and password above.")
                            .color(COLOR_TEXT_MUTED)
                            .size(11.0),
                    );
                }
            });
        });
    }

    fn grid_uri(&self) -> &str {
        if self.grid_index == KNOWN_GRIDS.len() - 1 {
            &self.custom_uri
        } else {
            KNOWN_GRIDS[self.grid_index].1
        }
    }

    fn do_login(&mut self, client: &LeapClient) {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("login".to_string()));
        args.insert(
            "username".to_string(),
            LLSDValue::String(self.username.clone()),
        );
        args.insert(
            "password".to_string(),
            LLSDValue::String(self.password.clone()),
        );
        let uri = self.grid_uri().to_string();
        if !uri.is_empty() {
            args.insert("uri".to_string(), LLSDValue::String(uri));
        }
        let start_str = if self.start_location == "home" {
            self.landmark_store.home_slurl()
        } else if self.start_location == "last" || self.start_location.is_empty() {
            self.landmark_store.last_slurl()
        } else {
            self.start_location.clone()
        };
        if !start_str.is_empty() {
            args.insert(
                "start".to_string(),
                LLSDValue::String(start_str),
            );
        }

        self.login_state = LoginState::Pending;
        self.last_poll_time = Some(std::time::Instant::now());

        // Non-blocking async login dispatch
        self.pending_login = Some(client.request_raw_async("LLLogin", args, Duration::from_secs(15)));
    }

    fn do_logout(&mut self, client: &LeapClient) {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("logout".to_string()));
        let _ = client.send("LLLogin", args);
        self.login_state = LoginState::Idle;
        self.state_from_viewer.clear();
        self.pending_login = None;
    }

    pub fn query_status(&mut self, client: &LeapClient) {
        if self.pending_query.is_some() {
            return;
        }

        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("getState".to_string()));
        self.pending_query = Some(client.request_raw_async("LLLogin", args, Duration::from_secs(5)));

        // Non-blocking trigger for position and home update
        let mut pos_args = HashMap::new();
        pos_args.insert("op".to_string(), LLSDValue::String("getPosition".to_string()));
        let _ = client.send("LLAgent", pos_args);

        let mut home_args = HashMap::new();
        home_args.insert("op".to_string(), LLSDValue::String("getHome".to_string()));
        let _ = client.send("LLAgent", home_args);
    }
}
