//! Inventory Browser Tab — presents a rich hierarchical folder tree like Aurora Viewer.

use std::collections::HashMap;
use std::time::Duration;

use egui::{Color32, RichText, Ui};
use serde_llsd::LLSDValue;

use crate::client::{AsyncRequest, LeapClient};
use crate::codec::{get_array, get_bool, get_i32, get_str};
use crate::ui::theme::{
    card_frame, inner_box_frame, COLOR_ACCENT, COLOR_DANGER, COLOR_SUCCESS, COLOR_TEXT_MUTED,
};

#[derive(Debug, Clone)]
pub struct InvItemEntry {
    pub id: String,
    pub name: String,
    pub desc: String,
    pub parent: String,
    pub asset_type: i32,
    pub inv_type: i32,
    pub asset_id: String,
}

#[derive(Debug, Clone)]
pub struct InvFolderEntry {
    pub id: String,
    pub name: String,
    pub parent: String,
    pub type_default: i32,
    pub children: Vec<String>,
    pub items: Vec<String>,
    pub library: bool,
}

#[derive(Clone)]
pub struct InventoryFetchData {
    pub root_id: String,
    pub lib_root_id: String,
    pub owner_id: String,
    pub folders: HashMap<String, InvFolderEntry>,
    pub items: HashMap<String, InvItemEntry>,
}

pub struct InventoryTab {
    pub root_id: String,
    pub lib_root_id: String,
    pub owner_id: String,
    pub folders: HashMap<String, InvFolderEntry>,
    pub items: HashMap<String, InvItemEntry>,

    pub search_query: String,
    pub selected_item: Option<String>,
    pub selected_folder: Option<String>,

    pub is_loading: bool,
    pub status_message: Option<String>,
    pub status_is_error: bool,

    // In-flight background fetch request (non-blocking)
    pub pending_fetch: Option<AsyncRequest<InventoryFetchData>>,
}

impl InventoryTab {
    pub fn new() -> Self {
        Self {
            root_id: String::new(),
            lib_root_id: String::new(),
            owner_id: String::new(),
            folders: HashMap::new(),
            items: HashMap::new(),
            search_query: String::new(),
            selected_item: None,
            selected_folder: None,
            is_loading: false,
            status_message: None,
            status_is_error: false,
            pending_fetch: None,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, client: &LeapClient) {
        let is_connected = client.is_connected();

        // Poll in-flight non-blocking inventory fetch
        if let Some(ref req) = self.pending_fetch {
            if let Some(res) = req.poll() {
                match res {
                    Ok(data) => {
                        self.root_id = data.root_id;
                        self.lib_root_id = data.lib_root_id;
                        self.owner_id = data.owner_id;
                        self.folders = data.folders;
                        self.items = data.items;
                        self.status_message = Some(format!(
                            "Successfully loaded inventory: {} folders, {} items",
                            self.folders.len(),
                            self.items.len()
                        ));
                        self.status_is_error = false;
                    }
                    Err(err) => {
                        self.status_message = Some(format!("Failed to fetch inventory: {err}"));
                        self.status_is_error = true;
                    }
                }
                self.is_loading = false;
                self.pending_fetch = None;
            } else {
                ui.ctx().request_repaint_after(Duration::from_millis(50));
            }
        }

        ui.vertical(|ui| {
            // ── Top Header & Search Bar ─────────────────────────────────
            card_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("📦 Inworld Inventory");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let btn = egui::Button::new(
                            RichText::new(if self.is_loading { "⏳ Fetching..." } else { "🔄 Fetch Inventory" })
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(if self.is_loading { Color32::from_rgb(100, 100, 120) } else { COLOR_ACCENT });

                        if ui.add_enabled(is_connected && !self.is_loading, btn).clicked() {
                            self.fetch_inventory(client);
                        }

                        if self.is_loading {
                            ui.spinner();
                        }
                    });
                });

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(RichText::new("🔍 Search:").strong());
                    ui.add_sized(
                        [ui.available_width() - 260.0, 22.0],
                        egui::TextEdit::singleline(&mut self.search_query)
                            .hint_text("Filter items or folders by name..."),
                    );
                    if !self.search_query.is_empty() && ui.button("Clear").clicked() {
                        self.search_query.clear();
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let count_text = format!(
                            "📁 {} Folders  •  📄 {} Items",
                            self.folders.len(),
                            self.items.len()
                        );
                        ui.label(RichText::new(count_text).size(11.0).color(COLOR_TEXT_MUTED));
                    });
                });

                if let Some(ref msg) = self.status_message {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(msg)
                            .size(11.0)
                            .color(if self.status_is_error { COLOR_DANGER } else { COLOR_SUCCESS }),
                    );
                }
            });

            ui.add_space(8.0);

            // ── Main Two Column: Tree / Inspector ──────────────────────
            ui.columns(2, |cols| {
                // Left Column: Hierarchical Tree View
                cols[0].vertical(|ui| {
                    card_frame().show(ui, |ui| {
                        ui.label(RichText::new("Inventory Tree").strong().size(13.0));
                        ui.add_space(4.0);

                        if self.folders.is_empty() && self.items.is_empty() {
                            inner_box_frame().show(ui, |ui| {
                                ui.label(
                                    RichText::new(
                                        "No inventory loaded yet.\nClick '🔄 Fetch Inventory' above to download your inventory tree from the viewer.",
                                    )
                                    .color(COLOR_TEXT_MUTED)
                                    .size(11.0),
                                );
                            });
                        } else if !self.search_query.trim().is_empty() {
                            // Search results list
                            self.render_search_results(ui);
                        } else {
                            // Tree view starting at Root
                            egui::ScrollArea::vertical()
                                .max_height(580.0)
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    if !self.root_id.is_empty() && self.folders.contains_key(&self.root_id) {
                                        let root_id = self.root_id.clone();
                                        self.render_folder_tree(ui, &root_id, 0);
                                    } else {
                                        // Render top-level orphan folders
                                        let top_folders: Vec<String> = self.folders.iter()
                                            .filter(|(_, f)| f.parent.is_empty() || f.parent == "00000000-0000-0000-0000-000000000000")
                                            .map(|(id, _)| id.clone())
                                            .collect();
                                        for fid in top_folders {
                                            self.render_folder_tree(ui, &fid, 0);
                                        }
                                    }

                                    // Library root if present
                                    if !self.lib_root_id.is_empty() && self.folders.contains_key(&self.lib_root_id) {
                                        ui.add_space(8.0);
                                        ui.separator();
                                        let lib_id = self.lib_root_id.clone();
                                        self.render_folder_tree(ui, &lib_id, 0);
                                    }
                                });
                        }
                    });
                });

                // Right Column: Item / Folder Inspector Details
                cols[1].vertical(|ui| {
                    card_frame().show(ui, |ui| {
                        ui.label(RichText::new("Item & Asset Details").strong().size(13.0));
                        ui.add_space(6.0);

                        if let Some(ref item_id) = self.selected_item {
                            if let Some(item) = self.items.get(item_id).cloned() {
                                let icon = Self::item_icon(item.asset_type);
                                let type_name = Self::type_name(item.asset_type);

                                inner_box_frame().show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(icon).size(22.0));
                                        ui.vertical(|ui| {
                                            ui.label(RichText::new(&item.name).strong().size(14.0));
                                            ui.label(RichText::new(format!("Type: {type_name}")).size(11.0).color(COLOR_ACCENT));
                                        });
                                    });
                                });

                                ui.add_space(6.0);
                                inner_box_frame().show(ui, |ui| {
                                    if !item.desc.trim().is_empty() {
                                        ui.label(RichText::new(format!("Description: {}", item.desc)).size(11.0));
                                        ui.add_space(4.0);
                                    }
                                    ui.label(RichText::new(format!("Item UUID: {}", item.id)).monospace().size(10.0));
                                    ui.label(RichText::new(format!("Asset UUID: {}", item.asset_id)).monospace().size(10.0));
                                    ui.label(RichText::new(format!("Parent Folder: {}", item.parent)).monospace().size(10.0));
                                });

                                ui.add_space(8.0);
                                ui.horizontal(|ui| {
                                    if item.asset_type == 3 {
                                        // Landmark item: teleport button
                                        if ui.button(RichText::new("✈ Teleport to Landmark").strong()).clicked() {
                                            let mut args = HashMap::new();
                                            args.insert("op".to_string(), LLSDValue::String("teleport".to_string()));
                                            args.insert("landmark_id".to_string(), LLSDValue::String(item.id.clone()));
                                            let _ = client.send("LLAgent", args);
                                        }
                                    } else if item.asset_type == 5 || item.asset_type == 13 || item.asset_type == 6 {
                                        // Wearable, Bodypart or Attachment
                                        if ui.button(RichText::new("👕 Wear / Attach").strong()).clicked() {
                                            let mut args = HashMap::new();
                                            args.insert("op".to_string(), LLSDValue::String("wearItem".to_string()));
                                            args.insert("item_id".to_string(), LLSDValue::String(item.id.clone()));
                                            args.insert("replace".to_string(), LLSDValue::Boolean(false));
                                            let _ = client.send("LLAppearance", args);
                                        }
                                        if ui.button(RichText::new("❌ Take Off / Detach")).clicked() {
                                            let mut args = HashMap::new();
                                            args.insert("op".to_string(), LLSDValue::String("takeOff".to_string()));
                                            args.insert("item_id".to_string(), LLSDValue::String(item.id.clone()));
                                            let _ = client.send("LLAppearance", args);
                                        }
                                    } else if item.asset_type == 20 {
                                        // Animation
                                        if ui.button(RichText::new("▶ Play Animation").strong()).clicked() {
                                            let mut args = HashMap::new();
                                            args.insert("op".to_string(), LLSDValue::String("startAnimation".to_string()));
                                            args.insert("anim_id".to_string(), LLSDValue::String(item.asset_id.clone()));
                                            let _ = client.send("LLAnimation", args);
                                        }
                                        if ui.button(RichText::new("⏹ Stop Animation")).clicked() {
                                            let mut args = HashMap::new();
                                            args.insert("op".to_string(), LLSDValue::String("stopAnimation".to_string()));
                                            let _ = client.send("LLAnimation", args);
                                        }
                                    }

                                    if ui.button("📋 Copy Item UUID").clicked() {
                                        ui.ctx().copy_text(item.id.clone());
                                    }
                                    if ui.button("📋 Copy Asset UUID").clicked() {
                                        ui.ctx().copy_text(item.asset_id.clone());
                                    }
                                });
                            }
                        } else if let Some(ref folder_id) = self.selected_folder {
                            if let Some(folder) = self.folders.get(folder_id).cloned() {
                                let icon = Self::folder_icon(folder.type_default);
                                inner_box_frame().show(ui, |ui| {
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(icon).size(20.0));
                                        ui.vertical(|ui| {
                                            ui.label(RichText::new(&folder.name).strong().size(13.0));
                                            ui.label(RichText::new(format!(
                                                "Contains {} subfolders, {} items",
                                                folder.children.len(),
                                                folder.items.len()
                                            )).size(11.0).color(COLOR_TEXT_MUTED));
                                        });
                                    });
                                });

                                ui.add_space(6.0);
                                inner_box_frame().show(ui, |ui| {
                                    ui.label(RichText::new(format!("Folder UUID: {}", folder.id)).monospace().size(10.0));
                                    ui.label(RichText::new(format!("Parent UUID: {}", folder.parent)).monospace().size(10.0));
                                });
                            }
                        } else {
                            inner_box_frame().show(ui, |ui| {
                                ui.label(
                                    RichText::new("Select an item or folder from the tree to inspect details.")
                                        .color(COLOR_TEXT_MUTED)
                                        .size(11.0),
                                );
                            });
                        }
                    });
                });
            });
        });
    }

    fn render_folder_tree(&mut self, ui: &mut Ui, folder_id: &str, depth: usize) {
        if depth > 12 {
            return;
        }

        let folder = match self.folders.get(folder_id).cloned() {
            Some(f) => f,
            None => return,
        };

        let icon = Self::folder_icon(folder.type_default);
        let header_label = format!("{} {} ({})", icon, folder.name, folder.items.len());

        let id_salt = format!("tree_folder_{folder_id}");
        let default_open = depth == 0 || folder.type_default == 8; // open roots by default

        egui::CollapsingHeader::new(RichText::new(header_label).strong().size(12.0))
            .id_salt(id_salt)
            .default_open(default_open)
            .show(ui, |ui| {
                // Subfolders
                for child_id in &folder.children {
                    self.render_folder_tree(ui, child_id, depth + 1);
                }

                // Items in this folder
                for item_id in &folder.items {
                    if let Some(item) = self.items.get(item_id).cloned() {
                        let item_icon = Self::item_icon(item.asset_type);
                        let is_selected = self.selected_item.as_deref() == Some(&item.id);

                        let label = format!("{} {}", item_icon, item.name);
                        let btn = egui::Button::new(
                            RichText::new(label)
                                .color(if is_selected { COLOR_ACCENT } else { Color32::WHITE })
                                .size(11.0),
                        )
                        .fill(if is_selected {
                            Color32::from_rgb(32, 40, 56)
                        } else {
                            Color32::TRANSPARENT
                        });

                        if ui.add(btn).clicked() {
                            self.selected_item = Some(item.id.clone());
                            self.selected_folder = None;
                        }
                    }
                }
            });
    }

    fn render_search_results(&mut self, ui: &mut Ui) {
        let query = self.search_query.trim().to_lowercase();
        let matches: Vec<InvItemEntry> = self.items.values()
            .filter(|it| it.name.to_lowercase().contains(&query) || it.desc.to_lowercase().contains(&query))
            .cloned()
            .collect();

        ui.label(RichText::new(format!("Search matches ({} items):", matches.len())).size(11.0).color(COLOR_TEXT_MUTED));
        ui.add_space(4.0);

        egui::ScrollArea::vertical().max_height(540.0).show(ui, |ui| {
            for item in matches {
                let is_selected = self.selected_item.as_deref() == Some(&item.id);
                let item_icon = Self::item_icon(item.asset_type);
                let label = format!("{} {}", item_icon, item.name);

                let btn = egui::Button::new(
                    RichText::new(label)
                        .color(if is_selected { COLOR_ACCENT } else { Color32::WHITE })
                        .size(11.0),
                )
                .fill(if is_selected {
                    Color32::from_rgb(32, 40, 56)
                } else {
                    Color32::TRANSPARENT
                });

                if ui.add(btn).clicked() {
                    self.selected_item = Some(item.id);
                    self.selected_folder = None;
                }
            }
        });
    }

    pub fn fetch_inventory(&mut self, client: &LeapClient) {
        self.is_loading = true;
        self.status_message = Some("Fetching inventory hierarchy from simulator in background...".to_string());
        self.status_is_error = false;

        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("getTree".to_string()));

        self.pending_fetch = Some(client.request_async(
            "LLInventory",
            args,
            Duration::from_secs(12),
            |reply| {
                let root_id = get_str(&reply, "root").unwrap_or("").to_string();
                let lib_root_id = get_str(&reply, "lib_root").unwrap_or("").to_string();
                let owner_id = get_str(&reply, "owner").unwrap_or("").to_string();

                // Parse folders
                let mut folders = HashMap::new();
                if let Some(folder_arr) = get_array(&reply, "folders") {
                    for val in folder_arr {
                        let id = get_str(val, "id").unwrap_or("").to_string();
                        let name = get_str(val, "name").unwrap_or("").to_string();
                        let parent = get_str(val, "parent").unwrap_or("").to_string();
                        let type_default = get_i32(val, "type_default").unwrap_or(-1);
                        let library = get_bool(val, "library").unwrap_or(false);

                        let mut children = Vec::new();
                        if let Some(c_arr) = get_array(val, "children") {
                            for c in c_arr {
                                if let Some(s) = c.as_string() {
                                    children.push(s.to_string());
                                }
                            }
                        }

                        let mut items = Vec::new();
                        if let Some(i_arr) = get_array(val, "items") {
                            for it in i_arr {
                                if let Some(s) = it.as_string() {
                                    items.push(s.to_string());
                                }
                            }
                        }

                        if !id.is_empty() {
                            folders.insert(id.clone(), InvFolderEntry {
                                id,
                                name,
                                parent,
                                type_default,
                                children,
                                items,
                                library,
                            });
                        }
                    }
                }

                // Parse items
                let mut items = HashMap::new();
                if let Some(item_arr) = get_array(&reply, "items") {
                    for val in item_arr {
                        let id = get_str(val, "id").unwrap_or("").to_string();
                        let name = get_str(val, "name").unwrap_or("").to_string();
                        let desc = get_str(val, "desc").unwrap_or("").to_string();
                        let parent = get_str(val, "parent").unwrap_or("").to_string();
                        let asset_type = get_i32(val, "type").unwrap_or(-1);
                        let inv_type = get_i32(val, "inv_type").unwrap_or(-1);
                        let asset_id = get_str(val, "asset_id").unwrap_or("").to_string();

                        if !id.is_empty() {
                            items.insert(id.clone(), InvItemEntry {
                                id,
                                name,
                                desc,
                                parent,
                                asset_type,
                                inv_type,
                                asset_id,
                            });
                        }
                    }
                }

                Ok(InventoryFetchData {
                    root_id,
                    lib_root_id,
                    owner_id,
                    folders,
                    items,
                })
            },
        ));
    }

    pub fn folder_icon(type_default: i32) -> &'static str {
        match type_default {
            0 => "🖼️",  // Texture
            1 => "🔊",  // Sound
            2 => "📇",  // Calling Card
            3 => "📌",  // Landmarks
            5 => "👕",  // Clothing
            6 => "📦",  // Objects
            7 => "📜",  // Notecards
            8 => "📁",  // Root
            10 => "⚙️", // Scripts
            13 => "👤", // Body Parts
            14 => "🗑️", // Trash
            15 => "📸", // Snapshot
            16 => "❓", // Lost And Found
            20 => "🏃", // Animations
            21 => "🎭", // Gestures
            24 => "👗", // Current Outfit
            46 => "⭐", // Favorites
            _ => "📁",
        }
    }

    pub fn item_icon(asset_type: i32) -> &'static str {
        match asset_type {
            0 => "🖼️",  // Texture
            1 => "🔊",  // Sound
            2 => "📇",  // Calling Card
            3 => "📌",  // Landmark
            5 => "👕",  // Clothing
            6 => "📦",  // Object
            7 => "📜",  // Notecard
            10 => "⚙️", // LSL Script
            13 => "👤", // Body Part
            20 => "🏃", // Animation
            21 => "🎭", // Gesture
            49 => "🔗", // Mesh
            _ => "📄",
        }
    }

    pub fn type_name(asset_type: i32) -> &'static str {
        match asset_type {
            0 => "Texture",
            1 => "Sound Effect",
            2 => "Calling Card",
            3 => "Landmark",
            5 => "Clothing / Wearable",
            6 => "Object / Primitive",
            7 => "Notecard",
            10 => "LSL Bytecode / Script",
            13 => "Body Part (Shape/Skin/Eyes/Hair)",
            20 => "Animation",
            21 => "Gesture",
            49 => "3D Mesh Model",
            _ => "Inventory Item",
        }
    }
}
