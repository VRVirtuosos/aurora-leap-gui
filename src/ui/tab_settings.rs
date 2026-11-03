//! Viewer Settings Tab (`LLViewerControl`).

use std::collections::HashMap;
use std::time::Duration;

use egui::{RichText, Ui};
use serde_llsd::LLSDValue;

use crate::client::{AsyncRequest, LeapClient};
use crate::codec::get_map;
use crate::ui::theme::{card_frame, COLOR_ACCENT, COLOR_SUCCESS, COLOR_TEXT_MUTED};

pub struct SettingsTab {
    search_query: String,
    selected_group: String,
    groups: Vec<String>,
    vars: HashMap<String, SettingItem>,
    status_msg: Option<String>,
    is_loading: bool,
    pending_load: Option<AsyncRequest<(Vec<String>, HashMap<String, SettingItem>)>>,
}

#[derive(Clone)]
struct SettingItem {
    pub key: String,
    pub group: String,
    pub value_str: String,
    pub is_bool: bool,
    pub bool_val: bool,
    pub edit_buffer: String,
}

impl SettingsTab {
    pub fn new() -> Self {
        Self {
            search_query: String::new(),
            selected_group: "All".to_string(),
            groups: vec!["All".to_string()],
            vars: HashMap::new(),
            status_msg: None,
            is_loading: false,
            pending_load: None,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, client: &LeapClient) {
        let is_connected = client.is_connected();

        // Poll background load without blocking
        if let Some(ref req) = self.pending_load {
            if let Some(res) = req.poll() {
                match res {
                    Ok((groups, items)) => {
                        self.groups = groups;
                        self.vars = items;
                        self.status_msg = Some(format!("Loaded {} variables.", self.vars.len()));
                    }
                    Err(e) => {
                        self.status_msg = Some(format!("Failed to load variables: {e}"));
                    }
                }
                self.is_loading = false;
                self.pending_load = None;
            } else {
                ui.ctx().request_repaint_after(Duration::from_millis(50));
            }
        }

        ui.vertical(|ui| {
            card_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("⚙️ Viewer Configuration (`LLViewerControl`)");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let btn_label = if self.is_loading { "⏳ Loading..." } else { "🔄 Refresh Variables (vars)" };
                        let refresh_btn = egui::Button::new(RichText::new(btn_label).size(13.0));
                        if ui.add_enabled(is_connected && !self.is_loading, refresh_btn).clicked() {
                            self.fetch_vars(client);
                        }
                        if self.is_loading {
                            ui.spinner();
                        }
                    });
                });

                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label("Search:");
                    ui.add_sized([200.0, 20.0], egui::TextEdit::singleline(&mut self.search_query));

                    ui.add_space(12.0);
                    ui.label("Group:");
                    egui::ComboBox::from_id_salt("settings_group_select")
                        .selected_text(&self.selected_group)
                        .show_ui(ui, |ui| {
                            for g in &self.groups {
                                ui.selectable_value(&mut self.selected_group, g.clone(), g);
                            }
                        });

                    if let Some(ref st) = self.status_msg {
                        ui.add_space(12.0);
                        ui.label(RichText::new(st).color(COLOR_SUCCESS).size(12.0));
                    }
                });
            });

            ui.add_space(10.0);

            // Settings Variables Cards
            egui::ScrollArea::vertical().show(ui, |ui| {
                let query = self.search_query.to_lowercase();
                let mut keys: Vec<String> = self.vars.keys().cloned().collect();
                keys.sort();

                let mut to_toggle = None;
                let mut to_set = None;

                for k in keys {
                    if let Some(item) = self.vars.get_mut(&k) {
                        if self.selected_group != "All" && item.group != self.selected_group {
                            continue;
                        }
                        if !query.is_empty() && !item.key.to_lowercase().contains(&query) {
                            continue;
                        }

                        card_frame().show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.vertical(|ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(&item.key).strong().size(13.0));
                                        if !item.group.is_empty() {
                                            ui.label(RichText::new(format!("[{}]", item.group)).color(COLOR_TEXT_MUTED).size(10.0));
                                        }
                                    });
                                    ui.label(RichText::new(format!("Current Value: {}", item.value_str)).color(COLOR_ACCENT).size(11.0));
                                });

                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if item.is_bool {
                                        let toggle_text = if item.bool_val { "Disable (Toggle)" } else { "Enable (Toggle)" };
                                        let btn = egui::Button::new(RichText::new(toggle_text).size(11.0));
                                        if ui.add_enabled(is_connected, btn).clicked() {
                                            to_toggle = Some(item.key.clone());
                                        }
                                    } else {
                                        if ui.add_enabled(is_connected, egui::Button::new("Apply").small()).clicked() {
                                            to_set = Some((item.key.clone(), item.edit_buffer.clone()));
                                        }
                                        ui.add_sized([120.0, 18.0], egui::TextEdit::singleline(&mut item.edit_buffer));
                                    }
                                });
                            });
                        });
                        ui.add_space(4.0);
                    }
                }

                if let Some(toggle_key) = to_toggle {
                    self.toggle_var(client, &toggle_key);
                }
                if let Some((set_key, val_str)) = to_set {
                    self.set_var(client, &set_key, &val_str);
                }

                if self.vars.is_empty() {
                    ui.label(RichText::new("No configuration variables loaded yet. Click 'Refresh Variables' above.").color(COLOR_TEXT_MUTED));
                }
            });
        });
    }

    fn fetch_vars(&mut self, client: &LeapClient) {
        self.is_loading = true;
        self.status_msg = Some("Loading variables from viewer in background...".to_string());

        let mut vars_args = HashMap::new();
        vars_args.insert("op".to_string(), LLSDValue::String("vars".to_string()));

        self.pending_load = Some(client.request_async(
            "LLViewerControl",
            vars_args,
            Duration::from_secs(6),
            |reply| {
                let mut items = HashMap::new();
                let mut groups_set = std::collections::BTreeSet::new();
                groups_set.insert("All".to_string());

                if let Some(vars_map) = get_map(&reply, "vars") {
                    for (k, val) in vars_map {
                        let (val_str, is_bool, bool_val) = match val {
                            LLSDValue::Boolean(b) => (b.to_string(), true, *b),
                            LLSDValue::Integer(i) => (i.to_string(), false, false),
                            LLSDValue::Real(r) => (format!("{r:.2}"), false, false),
                            LLSDValue::String(s) => (s.clone(), false, false),
                            other => (format!("{other:?}"), false, false),
                        };

                        let group = if k.contains("Display") || k.contains("Draw") || k.contains("VSync") {
                            "Display".to_string()
                        } else if k.contains("Shadow") || k.contains("Shader") || k.contains("Render") {
                            "Graphics".to_string()
                        } else {
                            "General".to_string()
                        };
                        groups_set.insert(group.clone());

                        items.insert(k.clone(), SettingItem {
                            key: k.clone(),
                            group,
                            value_str: val_str.clone(),
                            is_bool,
                            bool_val,
                            edit_buffer: val_str,
                        });
                    }
                }
                Ok((groups_set.into_iter().collect(), items))
            },
        ));
    }

    fn toggle_var(&mut self, client: &LeapClient, key: &str) {
        if let Some(item) = self.vars.get_mut(key) {
            item.bool_val = !item.bool_val;
            item.value_str = item.bool_val.to_string();
            self.status_msg = Some(format!("Toggled {key} -> {}", item.bool_val));
        }

        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("toggle".to_string()));
        args.insert("key".to_string(), LLSDValue::String(key.to_string()));
        let _ = client.send("LLViewerControl", args);
    }

    fn set_var(&mut self, client: &LeapClient, key: &str, val_str: &str) {
        let llsd_val = if let Ok(i) = val_str.parse::<i32>() {
            LLSDValue::Integer(i)
        } else if let Ok(f) = val_str.parse::<f64>() {
            LLSDValue::Real(f)
        } else if val_str.eq_ignore_ascii_case("true") {
            LLSDValue::Boolean(true)
        } else if val_str.eq_ignore_ascii_case("false") {
            LLSDValue::Boolean(false)
        } else {
            LLSDValue::String(val_str.to_string())
        };

        if let Some(item) = self.vars.get_mut(key) {
            item.value_str = val_str.to_string();
            self.status_msg = Some(format!("Updated {key} = {val_str}"));
        }

        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("set".to_string()));
        args.insert("key".to_string(), LLSDValue::String(key.to_string()));
        args.insert("value".to_string(), llsd_val);
        let _ = client.send("LLViewerControl", args);
    }
}
