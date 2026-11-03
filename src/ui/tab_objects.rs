//! Inworld Objects & Linksets Visual Inspector Tab (`LLObjects` & `LLSelect`).
//!
//! Provides a visual spatial inventory of nearby 3D primitives and linksets within the simulator.
//! Features linkset hierarchical tree views, distance and text filters, non-blocking asynchronous queries,
//! one-click camera focus, click-to-teleport, in-world prim selection, and detailed LLSD metadata inspection.

use std::collections::HashMap;
use std::time::Duration;

use egui::{Button, Color32, RichText, ScrollArea, TextEdit, Ui};
use serde_llsd::LLSDValue;

use crate::client::{AsyncRequest, LeapClient};
use crate::codec::{get_array, get_bool, get_f64, get_i32, get_str, parse_vec3};
use crate::ui::theme::{
    card_frame, inner_box_frame, COLOR_ACCENT, COLOR_DANGER, COLOR_SUCCESS, COLOR_TEXT_MUTED,
    COLOR_WARNING,
};

/// Represents an inworld primitive or linkset member returned by `LLObjects.getNearbyObjects`.
#[derive(Debug, Clone)]
pub struct InworldObject {
    pub local_id: u32,
    pub uuid: String,
    pub parent_id: u32,
    pub pcode: i32,
    pub name: String,
    pub pos: (f64, f64, f64),
    pub scale: (f64, f64, f64),
    pub distance: f64,
    pub is_attachment: bool,
    pub attachment_point: i32,
    pub is_root: bool,
    pub details: Option<ObjectDetails>,
}

/// Extended metadata retrieved via `LLObjects.inspect`.
#[derive(Debug, Clone)]
pub struct ObjectDetails {
    pub prim_material: i32,
    pub click_action: i32,
    pub text: String,
    pub name_values: String,
    pub velocity: (f64, f64, f64),
    pub notation_preview: String,
}

pub struct ObjectsTab {
    // Objects cache
    pub objects: Vec<InworldObject>,
    pub selected_local_id: Option<u32>,

    // Search and filters
    pub search_query: String,
    pub max_distance: f64,
    pub hide_attachments: bool,
    pub roots_only: bool,

    // Async requests in-flight
    pending_query: Option<AsyncRequest<Vec<InworldObject>>>,
    pending_inspect: Option<(u32, AsyncRequest<ObjectDetails>)>,

    // Status message
    pub status_message: Option<String>,
    pub status_is_error: bool,
}

impl ObjectsTab {
    pub fn new() -> Self {
        Self {
            objects: Vec::new(),
            selected_local_id: None,
            search_query: String::new(),
            max_distance: 64.0,
            hide_attachments: true,
            roots_only: false,
            pending_query: None,
            pending_inspect: None,
            status_message: None,
            status_is_error: false,
        }
    }

    /// Poll any pending background async queries without blocking the render loop.
    pub fn update(&mut self) {
        if let Some(ref req) = self.pending_query {
            if let Some(res) = req.poll() {
                match res {
                    Ok(objs) => {
                        let count = objs.len();
                        self.objects = objs;
                        self.status_message = Some(format!("Found {count} nearby objects in simulator."));
                        self.status_is_error = false;
                    }
                    Err(e) => {
                        self.status_message = Some(format!("Failed to retrieve objects: {e}"));
                        self.status_is_error = true;
                    }
                }
                self.pending_query = None;
            }
        }

        if let Some((target_id, ref req)) = self.pending_inspect {
            if let Some(res) = req.poll() {
                match res {
                    Ok(details) => {
                        if let Some(obj) = self.objects.iter_mut().find(|o| o.local_id == target_id) {
                            obj.details = Some(details);
                        }
                    }
                    Err(e) => {
                        self.status_message = Some(format!("Inspection failed: {e}"));
                        self.status_is_error = true;
                    }
                }
                self.pending_inspect = None;
            }
        }
    }

    /// Render the Inworld Objects & Linksets Inspector.
    pub fn ui(&mut self, ui: &mut Ui, client: &LeapClient) {
        self.update();
        let is_connected = client.is_connected();

        ui.vertical(|ui| {
            // ── Top Header Toolbar ─────────────────────────────────────────────
            card_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("📦 Inworld Objects & Linksets Inspector (`LLObjects`)");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let is_busy = self.pending_query.is_some() || self.pending_inspect.is_some();
                        let refresh_btn = Button::new(RichText::new(if is_busy { "⏳ Querying…" } else { "🔄 Refresh Objects" }).strong());
                        if ui.add_enabled(is_connected && !is_busy, refresh_btn).clicked() {
                            self.trigger_refresh(client);
                        }

                        let deselect_btn = Button::new(RichText::new("✖ Deselect World").size(12.0));
                        if ui.add_enabled(is_connected, deselect_btn).clicked() {
                            let mut args = HashMap::new();
                            args.insert("op".to_string(), LLSDValue::String("deselect".to_string()));
                            let _ = client.send("LLObjects", args);
                            self.status_message = Some("Cleared in-world object selection.".to_string());
                            self.status_is_error = false;
                        }
                    });
                });

                ui.add_space(4.0);

                // Filter row
                ui.horizontal(|ui| {
                    ui.label("Search:");
                    ui.add_sized([160.0, 20.0], TextEdit::singleline(&mut self.search_query).hint_text("Name, ID or UUID…"));

                    ui.add_space(10.0);
                    ui.label("Radius:");
                    ui.add(egui::Slider::new(&mut self.max_distance, 8.0..=256.0).text("m").logarithmic(true));

                    ui.add_space(10.0);
                    ui.checkbox(&mut self.hide_attachments, "Hide Attachments");
                    ui.checkbox(&mut self.roots_only, "Roots Only");

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let total = self.objects.len();
                        ui.label(RichText::new(format!("Count: {total}")).strong().color(COLOR_TEXT_MUTED).size(12.0));
                    });
                });

                if let Some(ref status) = self.status_message {
                    ui.add_space(3.0);
                    let color = if self.status_is_error { COLOR_DANGER } else { COLOR_SUCCESS };
                    ui.label(RichText::new(status).color(color).size(12.0));
                }
            });

            ui.add_space(6.0);

            // ── Split View: Object Tree List (Left) & Inspector / Actions (Right) ─
            let available_height = ui.available_height().max(350.0);
            ui.columns(2, |cols| {
                // ── Left Column: Object Hierarchy & Tree ─────────────────────────
                cols[0].vertical(|ui| {
                    card_frame().show(ui, |ui| {
                        ui.label(RichText::new("🌲 Simulator Primitives & Linksets").strong().size(13.0));
                        ui.separator();

                        let filtered_objects = self.get_filtered_objects();

                        ScrollArea::vertical()
                            .id_salt("objects_tree_scroll")
                            .max_height(available_height - 60.0)
                            .show(ui, |ui| {
                                if filtered_objects.is_empty() {
                                    inner_box_frame().show(ui, |ui| {
                                        ui.vertical_centered(|ui| {
                                            ui.add_space(20.0);
                                            ui.label(RichText::new(if self.objects.is_empty() {
                                                "No objects scanned yet.\nClick '🔄 Refresh Objects' above to query the simulator."
                                            } else {
                                                "No objects match the current search filters."
                                            }).color(COLOR_TEXT_MUTED));
                                            ui.add_space(20.0);
                                        });
                                    });
                                } else {
                                    // Group by linkset root
                                    let mut roots = Vec::new();
                                    let mut children_map: HashMap<u32, Vec<InworldObject>> = HashMap::new();

                                    for obj in filtered_objects {
                                        if obj.parent_id == 0 {
                                            roots.push(obj);
                                        } else {
                                            children_map.entry(obj.parent_id).or_default().push(obj);
                                        }
                                    }

                                    for root in roots {
                                        let root_id = root.local_id;
                                        let is_selected = self.selected_local_id == Some(root_id);
                                        let child_count = children_map.get(&root_id).map(|c| c.len()).unwrap_or(0);

                                        inner_box_frame().show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                let icon = if root.is_attachment { "📎" } else if child_count > 0 { "🔗" } else { "🔷" };
                                                let title = format!("{icon} {} (#{root_id})", root.name);
                                                let btn = Button::new(
                                                    RichText::new(title)
                                                        .strong()
                                                        .color(if is_selected { COLOR_ACCENT } else { Color32::WHITE })
                                                ).fill(if is_selected { Color32::from_rgb(30, 42, 65) } else { Color32::TRANSPARENT });

                                                if ui.add(btn).clicked() {
                                                    self.select_object(root_id, client);
                                                }

                                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                    ui.label(RichText::new(format!("{:.1}m", root.distance)).color(COLOR_TEXT_MUTED).size(11.0));
                                                    if child_count > 0 {
                                                        ui.label(RichText::new(format!("+{child_count}")).color(COLOR_ACCENT).size(11.0));
                                                    }
                                                });
                                            });

                                            // Render children indented if linkset selected
                                            if is_selected && child_count > 0 {
                                                if let Some(children) = children_map.get(&root_id) {
                                                    for child in children {
                                                        let child_selected = self.selected_local_id == Some(child.local_id);
                                                        ui.horizontal(|ui| {
                                                            ui.add_space(20.0);
                                                            let child_btn = Button::new(
                                                                RichText::new(format!("↳ {} (#{})", child.name, child.local_id))
                                                                    .size(11.0)
                                                                    .color(if child_selected { COLOR_ACCENT } else { Color32::LIGHT_GRAY })
                                                            ).fill(if child_selected { Color32::from_rgb(26, 36, 52) } else { Color32::TRANSPARENT });

                                                            if ui.add(child_btn).clicked() {
                                                                self.select_object(child.local_id, client);
                                                            }
                                                        });
                                                    }
                                                }
                                            }
                                        });
                                        ui.add_space(2.0);
                                    }
                                }
                            });
                    });
                });

                // ── Right Column: Object Inspector & Actions ─────────────────────
                cols[1].vertical(|ui| {
                    card_frame().show(ui, |ui| {
                        ui.label(RichText::new("🔬 Object Inspector & Spatial Controls").strong().size(13.0));
                        ui.separator();

                        if let Some(obj) = self.selected_object().cloned() {
                            ScrollArea::vertical()
                                .id_salt("object_inspector_scroll")
                                .max_height(available_height - 60.0)
                                .show(ui, |ui| {
                                    // Primary Metadata Box
                                    inner_box_frame().show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new(&obj.name).strong().size(15.0).color(COLOR_ACCENT));
                                            if obj.is_root {
                                                ui.label(RichText::new("[Linkset Root]").color(COLOR_SUCCESS).size(11.0));
                                            } else {
                                                ui.label(RichText::new(format!("[Child of #{}]", obj.parent_id)).color(COLOR_TEXT_MUTED).size(11.0));
                                            }
                                        });

                                        ui.add_space(4.0);
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new(format!("Local ID: {}", obj.local_id)).size(12.0));
                                            ui.label(RichText::new(format!("PCode: {}", obj.pcode)).size(12.0).color(COLOR_TEXT_MUTED));
                                        });
                                        ui.label(RichText::new(format!("UUID: {}", obj.uuid)).size(11.0).color(COLOR_TEXT_MUTED));
                                        ui.label(RichText::new(format!("Position: ({:.2}, {:.2}, {:.2})", obj.pos.0, obj.pos.1, obj.pos.2)).size(12.0));
                                        ui.label(RichText::new(format!("Scale: ({:.2}m × {:.2}m × {:.2}m)", obj.scale.0, obj.scale.1, obj.scale.2)).size(12.0));
                                        ui.label(RichText::new(format!("Distance from Avatar: {:.1} m", obj.distance)).size(12.0));

                                        if obj.is_attachment {
                                            ui.label(RichText::new(format!("Attachment Point: {}", obj.attachment_point)).color(COLOR_WARNING).size(12.0));
                                        }
                                    });

                                    ui.add_space(6.0);

                                    // Spatial Actions Bar
                                    inner_box_frame().show(ui, |ui| {
                                        ui.label(RichText::new("🎮 Spatial Actions").strong().size(12.0));
                                        ui.add_space(4.0);

                                        ui.horizontal_wrapped(|ui| {
                                            // 🎯 Focus Camera Button
                                            if ui.add(Button::new(RichText::new("🎯 Focus Camera").size(12.0))).clicked() {
                                                let cx = obj.pos.0 - 3.5;
                                                let cy = obj.pos.1 - 3.5;
                                                let cz = obj.pos.2 + 2.0;
                                                let mut set_args = HashMap::new();
                                                set_args.insert("op".to_string(), LLSDValue::String("set".to_string()));
                                                set_args.insert("x".to_string(), LLSDValue::Real(cx));
                                                set_args.insert("y".to_string(), LLSDValue::Real(cy));
                                                set_args.insert("z".to_string(), LLSDValue::Real(cz));
                                                set_args.insert("target_x".to_string(), LLSDValue::Real(obj.pos.0));
                                                set_args.insert("target_y".to_string(), LLSDValue::Real(obj.pos.1));
                                                set_args.insert("target_z".to_string(), LLSDValue::Real(obj.pos.2));
                                                set_args.insert("fov".to_string(), LLSDValue::Real(60.0));
                                                let _ = client.send("LLCamera", set_args);
                                                self.status_message = Some(format!("Camera focused on '{}'.", obj.name));
                                                self.status_is_error = false;
                                            }

                                            // 🚶 Teleport to Object Button
                                            if ui.add(Button::new(RichText::new("🚶 Teleport To").size(12.0))).clicked() {
                                                let mut tp_args = HashMap::new();
                                                tp_args.insert("op".to_string(), LLSDValue::String("teleport".to_string()));
                                                tp_args.insert("x".to_string(), LLSDValue::Real(obj.pos.0));
                                                tp_args.insert("y".to_string(), LLSDValue::Real(obj.pos.1));
                                                tp_args.insert("z".to_string(), LLSDValue::Real(obj.pos.2 + 1.0));
                                                let _ = client.send("LLAgent", tp_args);
                                                self.status_message = Some(format!("Teleporting toward '{}'.", obj.name));
                                                self.status_is_error = false;
                                            }

                                            // 🔲 Select In-World Button
                                            if ui.add(Button::new(RichText::new("🔲 Highlight In-World").size(12.0))).clicked() {
                                                let mut sel_args = HashMap::new();
                                                sel_args.insert("op".to_string(), LLSDValue::String("selectObject".to_string()));
                                                sel_args.insert("local_id".to_string(), LLSDValue::Integer(obj.local_id as i32));
                                                let _ = client.send("LLObjects", sel_args);
                                                self.status_message = Some(format!("Sent selectObject for #{}.", obj.local_id));
                                                self.status_is_error = false;
                                            }
                                        });
                                    });

                                    ui.add_space(6.0);

                                    // Detailed Inspected Metadata
                                    if let Some(ref details) = obj.details {
                                        inner_box_frame().show(ui, |ui| {
                                            ui.label(RichText::new("📋 Detailed Prim Attributes").strong().size(12.0));
                                            ui.add_space(3.0);
                                            ui.label(RichText::new(format!("Material ID: {}", details.prim_material)).size(11.0));
                                            ui.label(RichText::new(format!("Click Action: {}", details.click_action)).size(11.0));
                                            ui.label(RichText::new(format!("Velocity: ({:.2}, {:.2}, {:.2})", details.velocity.0, details.velocity.1, details.velocity.2)).size(11.0));
                                            if !details.text.is_empty() {
                                                ui.label(RichText::new(format!("Floating Text: \"{}\"", details.text)).color(COLOR_WARNING).size(11.0));
                                            }
                                            if !details.name_values.is_empty() {
                                                ui.label(RichText::new(format!("Name Values: {}", details.name_values)).size(11.0).color(COLOR_TEXT_MUTED));
                                            }

                                            ui.add_space(6.0);
                                            ui.collapsing("🔍 Raw LLSD Metadata Preview", |ui| {
                                                ui.add(
                                                    TextEdit::multiline(&mut details.notation_preview.as_str())
                                                        .font(egui::TextStyle::Monospace)
                                                        .desired_rows(6)
                                                        .desired_width(ui.available_width())
                                                );
                                            });
                                        });
                                    } else {
                                        inner_box_frame().show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                ui.label(RichText::new("Fetching detailed metadata…").color(COLOR_TEXT_MUTED).size(11.0));
                                                let inspect_btn = Button::new(RichText::new("Inspect Now").size(11.0));
                                                if ui.add(inspect_btn).clicked() {
                                                    self.trigger_inspect(obj.local_id, client);
                                                }
                                            });
                                        });
                                    }
                                });
                        } else {
                            inner_box_frame().show(ui, |ui| {
                                ui.vertical_centered(|ui| {
                                    ui.add_space(40.0);
                                    ui.label(RichText::new("Select an object from the tree on the left\nto inspect coordinates, linksets, bounding boxes, and camera focus.").color(COLOR_TEXT_MUTED));
                                    ui.add_space(40.0);
                                });
                            });
                        }
                    });
                });
            });
        });
    }

    /// Return reference to currently selected object if any.
    fn selected_object(&self) -> Option<&InworldObject> {
        self.selected_local_id.and_then(|id| self.objects.iter().find(|o| o.local_id == id))
    }

    /// Trigger non-blocking async query to `LLObjects.getNearbyObjects`.
    fn trigger_refresh(&mut self, client: &LeapClient) {
        let max_dist = self.max_distance;
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("getNearbyObjects".to_string()));
        args.insert("max_distance".to_string(), LLSDValue::Real(max_dist));
        args.insert("limit".to_string(), LLSDValue::Integer(150));
        args.insert("include_attachments".to_string(), LLSDValue::Boolean(!self.hide_attachments));

        self.pending_query = Some(client.request_async("LLObjects", args, Duration::from_secs(5), |reply| {
            let status = get_bool(&reply, "status").unwrap_or(false);
            if !status {
                let err = get_str(&reply, "error").unwrap_or("Failed to query LLObjects");
                return Err(err.to_string());
            }

            let mut list = Vec::new();
            if let Some(arr) = get_array(&reply, "objects") {
                for item in arr {
                    let local_id = get_i32(item, "local_id").unwrap_or(0) as u32;
                    let uuid = get_str(item, "uuid").unwrap_or("").to_string();
                    let parent_id = get_i32(item, "parent_id").unwrap_or(0) as u32;
                    let pcode = get_i32(item, "pcode").unwrap_or(0);
                    let name = get_str(item, "name").unwrap_or("Primitive").to_string();
                    let dist = get_f64(item, "distance").unwrap_or(0.0);
                    let is_attachment = get_bool(item, "is_attachment").unwrap_or(false);
                    let attachment_point = get_i32(item, "attachment_point").unwrap_or(0);
                    let is_root = get_bool(item, "is_root").unwrap_or(parent_id == 0);

                    let pos = parse_vec3(item, Some("position")).unwrap_or((0.0, 0.0, 0.0));
                    let scale = (
                        get_f64(item, "scale_x").unwrap_or(1.0),
                        get_f64(item, "scale_y").unwrap_or(1.0),
                        get_f64(item, "scale_z").unwrap_or(1.0),
                    );

                    list.push(InworldObject {
                        local_id,
                        uuid,
                        parent_id,
                        pcode,
                        name,
                        pos,
                        scale,
                        distance: dist,
                        is_attachment,
                        attachment_point,
                        is_root,
                        details: None,
                    });
                }
            }
            Ok(list)
        }));
    }

    /// Select object and trigger detailed inspection.
    fn select_object(&mut self, local_id: u32, client: &LeapClient) {
        self.selected_local_id = Some(local_id);
        if let Some(obj) = self.objects.iter().find(|o| o.local_id == local_id) {
            if obj.details.is_none() {
                self.trigger_inspect(local_id, client);
            }
        }
    }

    /// Trigger non-blocking inspection query for specific local_id.
    fn trigger_inspect(&mut self, local_id: u32, client: &LeapClient) {
        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("inspect".to_string()));
        args.insert("local_id".to_string(), LLSDValue::Integer(local_id as i32));

        let req = client.request_async("LLObjects", args, Duration::from_secs(4), |reply| {
            let prim_material = get_i32(&reply, "prim_material").unwrap_or(0);
            let click_action = get_i32(&reply, "click_action").unwrap_or(0);
            let text = get_str(&reply, "text").unwrap_or("").to_string();
            let name_values = get_str(&reply, "name_values").unwrap_or("").to_string();
            let velocity = parse_vec3(&reply, Some("velocity")).unwrap_or((0.0, 0.0, 0.0));
            let notation_preview = crate::codec::to_clean_notation(&reply).unwrap_or_else(|_| format!("{reply:?}"));

            Ok(ObjectDetails {
                prim_material,
                click_action,
                text,
                name_values,
                velocity,
                notation_preview,
            })
        });

        self.pending_inspect = Some((local_id, req));
    }

    /// Filter objects according to active search string and filter flags.
    fn get_filtered_objects(&self) -> Vec<InworldObject> {
        let q = self.search_query.trim().to_lowercase();
        self.objects
            .iter()
            .filter(|o| {
                if self.hide_attachments && o.is_attachment {
                    return false;
                }
                if self.roots_only && !o.is_root {
                    return false;
                }
                if o.distance > self.max_distance {
                    return false;
                }
                if !q.is_empty() {
                    let matches_name = o.name.to_lowercase().contains(&q);
                    let matches_id = o.local_id.to_string().contains(&q);
                    let matches_uuid = o.uuid.to_lowercase().contains(&q);
                    return matches_name || matches_id || matches_uuid;
                }
                true
            })
            .cloned()
            .collect()
    }
}
