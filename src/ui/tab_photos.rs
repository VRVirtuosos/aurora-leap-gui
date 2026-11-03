//! Snapshot / Inworld Photo Capture Tab — via `LLCamera.takePicture`.

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use base64::Engine;
use egui::{Color32, RichText, TextureHandle, TextureOptions, Ui, Vec2};
use serde_llsd::LLSDValue;

use crate::client::{AsyncRequest, LeapClient};
use crate::codec::{get_bool, get_i32, get_str};
use crate::ui::theme::{
    card_frame, inner_box_frame, COLOR_ACCENT, COLOR_DANGER, COLOR_SUCCESS, COLOR_TEXT_MUTED,
};

#[derive(Clone)]
pub struct CapturedPhoto {
    pub timestamp: String,
    pub width: u32,
    pub height: u32,
    pub file_path: String,
    pub position: String,
    pub png_bytes: Vec<u8>,
}

pub struct PhotosTab {
    // Capture settings
    width_input: u32,
    height_input: u32,
    include_ui: bool,
    custom_save_path: String,

    // Status / Progress
    is_capturing: bool,
    status_message: Option<String>,
    is_status_error: bool,

    // Current & History
    photos: Vec<CapturedPhoto>,
    selected_index: Option<usize>,

    // Cached egui texture for preview
    preview_texture: Option<(usize, TextureHandle)>,

    // In-flight background capture request (non-blocking)
    pending_capture: Option<AsyncRequest<CapturedPhoto>>,
}

impl PhotosTab {
    pub fn new() -> Self {
        Self {
            width_input: 0,
            height_input: 0,
            include_ui: true,
            custom_save_path: String::new(),
            is_capturing: false,
            status_message: None,
            is_status_error: false,
            photos: Vec::new(),
            selected_index: None,
            preview_texture: None,
            pending_capture: None,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, client: &LeapClient) {
        let is_connected = client.is_connected();

        // Poll in-flight non-blocking snapshot capture
        if let Some(ref req) = self.pending_capture {
            if let Some(res) = req.poll() {
                match res {
                    Ok(photo) => {
                        let w = photo.width;
                        let h = photo.height;
                        self.photos.push(photo);
                        self.selected_index = Some(self.photos.len() - 1);
                        self.status_message = Some(format!("Snapshot captured successfully ({}x{})!", w, h));
                        self.is_status_error = false;
                    }
                    Err(err) => {
                        self.status_message = Some(format!("Snapshot failed: {err}"));
                        self.is_status_error = true;
                    }
                }
                self.is_capturing = false;
                self.pending_capture = None;
            } else {
                ui.ctx().request_repaint_after(Duration::from_millis(50));
            }
        }

        ui.vertical(|ui| {
            // ── Top Header ────────────────────────────────────────────
            card_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("📸 Inworld Photo Capture & Gallery");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let btn = egui::Button::new(
                            RichText::new(if self.is_capturing { "⏳ Capturing..." } else { "📷 Take Snapshot Now" })
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(if self.is_capturing { Color32::from_rgb(100, 100, 120) } else { COLOR_ACCENT });

                        if ui.add_enabled(is_connected && !self.is_capturing, btn).clicked() {
                            self.take_snapshot(ui.ctx(), client);
                        }

                        if self.is_capturing {
                            ui.spinner();
                        }
                    });
                });

                if let Some(ref msg) = self.status_message {
                    ui.add_space(4.0);
                    ui.label(
                        RichText::new(msg)
                            .size(11.0)
                            .color(if self.is_status_error { COLOR_DANGER } else { COLOR_SUCCESS }),
                    );
                }
            });

            ui.add_space(8.0);

            // ── Controls & Resolution Settings ───────────────────────────
            card_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Resolution:").strong());
                    ui.add_space(4.0);

                    // Preset buttons
                    for (w, h, label) in [
                        (0, 0, "Native Window"),
                        (640, 480, "640x480 (SD)"),
                        (1280, 720, "1280x720 (HD)"),
                        (1920, 1080, "1920x1080 (FHD)"),
                    ] {
                        if ui.button(label).clicked() {
                            self.width_input = w;
                            self.height_input = h;
                        }
                    }

                    ui.add_space(8.0);
                    ui.label("Width (0=native):");
                    ui.add(egui::DragValue::new(&mut self.width_input).range(0..=7680).speed(10));
                    ui.label("Height (0=native):");
                    ui.add(egui::DragValue::new(&mut self.height_input).range(0..=4320).speed(10));
                });

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.checkbox(&mut self.include_ui, "Include Viewer Interface (Menus, Toolbars & HUDs)");
                    ui.label(
                        RichText::new("— Matches exact viewer screen when checked; uncheck for clean 3D scene")
                            .color(COLOR_TEXT_MUTED)
                            .size(11.0),
                    );
                });

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.label("Custom Save Path (optional):");
                    ui.add_sized(
                        [ui.available_width() - 80.0, 20.0],
                        egui::TextEdit::singleline(&mut self.custom_save_path)
                            .hint_text("Leave blank to auto-save to snapshots directory"),
                    );
                    if ui.button("Browse...").clicked() {
                        if let Some(path) = rfd::FileDialog::new()
                            .add_filter("PNG Image", &["png"])
                            .save_file()
                        {
                            self.custom_save_path = path.to_string_lossy().to_string();
                        }
                    }
                });
            });

            ui.add_space(10.0);

            // ── Main Two Column: Photo Gallery List / Preview ───────────
            ui.columns(2, |cols| {
                // Left: Gallery list
                cols[0].vertical(|ui| {
                    card_frame().show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("Gallery ({})", self.photos.len())).strong());
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if !self.photos.is_empty() && ui.button("Clear All").clicked() {
                                    self.photos.clear();
                                    self.selected_index = None;
                                    self.preview_texture = None;
                                }
                            });
                        });
                        ui.add_space(6.0);

                        if self.photos.is_empty() {
                            inner_box_frame().show(ui, |ui| {
                                ui.label(
                                    RichText::new("No screenshots captured yet.\nClick 'Take Snapshot Now' above.")
                                        .color(COLOR_TEXT_MUTED)
                                        .size(11.0),
                                );
                            });
                        } else {
                            egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                                for i in 0..self.photos.len() {
                                    let is_selected = self.selected_index == Some(i);
                                    let photo = &self.photos[i];
                                    let label = format!(
                                        "📸 #{} [{}x{}] - {}",
                                        i + 1,
                                        photo.width,
                                        photo.height,
                                        photo.timestamp
                                    );

                                    let item_btn = egui::Button::new(
                                        RichText::new(label)
                                            .color(if is_selected { COLOR_ACCENT } else { Color32::WHITE })
                                            .size(11.0),
                                    )
                                    .fill(if is_selected {
                                        Color32::from_rgb(32, 40, 56)
                                    } else {
                                        Color32::TRANSPARENT
                                    });

                                    if ui.add(item_btn).clicked() {
                                        self.selected_index = Some(i);
                                    }
                                }
                            });
                        }
                    });
                });

                // Right: Image Preview & Details
                cols[1].vertical(|ui| {
                    card_frame().show(ui, |ui| {
                        ui.label(RichText::new("Snapshot Preview").strong());
                        ui.add_space(6.0);

                        if let Some(idx) = self.selected_index {
                            if idx < self.photos.len() {
                                let photo = &self.photos[idx];

                                // Prepare Texture if needed
                                let texture_ready = match &self.preview_texture {
                                    Some((cached_idx, _)) if *cached_idx == idx => true,
                                    _ => false,
                                };

                                if !texture_ready {
                                    if let Ok(img) = image::load_from_memory(&photo.png_bytes) {
                                        let size = [img.width() as usize, img.height() as usize];
                                        let rgba = img.to_rgba8();
                                        let pixels = rgba.as_flat_samples();
                                        let color_image = egui::ColorImage::from_rgba_unmultiplied(
                                            size,
                                            pixels.as_slice(),
                                        );
                                        let tex = ui.ctx().load_texture(
                                            format!("photo_preview_{idx}"),
                                            color_image,
                                            TextureOptions::LINEAR,
                                        );
                                        self.preview_texture = Some((idx, tex));
                                    }
                                }

                                if let Some((_, ref tex)) = self.preview_texture {
                                    let avail_w = ui.available_width().clamp(200.0, 560.0);
                                    let aspect = if photo.height > 0 {
                                        photo.width as f32 / photo.height as f32
                                    } else {
                                        16.0 / 9.0
                                    };
                                    let preview_size = Vec2::new(avail_w, avail_w / aspect);
                                    ui.image((tex.id(), preview_size));
                                }

                                ui.add_space(8.0);
                                inner_box_frame().show(ui, |ui| {
                                    ui.label(RichText::new(format!("File: {}", photo.file_path)).size(10.0));
                                    ui.label(RichText::new(format!("Agent Position: {}", photo.position)).size(10.0));
                                    ui.label(RichText::new(format!("Resolution: {}x{}", photo.width, photo.height)).size(10.0));
                                });

                                ui.add_space(6.0);
                                ui.horizontal(|ui| {
                                    if ui.button("💾 Save Copy As...").clicked() {
                                        if let Some(path) = rfd::FileDialog::new()
                                            .add_filter("PNG Image", &["png"])
                                            .set_file_name("snapshot.png")
                                            .save_file()
                                        {
                                            let _ = std::fs::write(&path, &photo.png_bytes);
                                            self.status_message = Some(format!("Saved to {}", path.display()));
                                            self.is_status_error = false;
                                        }
                                    }

                                    if !photo.file_path.is_empty() && PathBuf::from(&photo.file_path).exists() {
                                        if ui.button("📂 Open Folder").clicked() {
                                            #[cfg(target_os = "windows")]
                                            let _ = std::process::Command::new("explorer")
                                                .args(["/select,", &photo.file_path])
                                                .spawn();
                                        }
                                    }
                                });
                            }
                        } else {
                            inner_box_frame().show(ui, |ui| {
                                ui.label(
                                    RichText::new("Select a screenshot from the list to preview.")
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

    fn take_snapshot(&mut self, _ctx: &egui::Context, client: &LeapClient) {
        self.is_capturing = true;
        self.status_message = Some("Capturing inworld snapshot from viewer in background...".to_string());
        self.is_status_error = false;

        let width_in = self.width_input;
        let height_in = self.height_input;

        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("takePicture".to_string()));
        args.insert("width".to_string(), LLSDValue::Integer(self.width_input as i32));
        args.insert("height".to_string(), LLSDValue::Integer(self.height_input as i32));
        args.insert("ui".to_string(), LLSDValue::Boolean(self.include_ui));

        if !self.custom_save_path.trim().is_empty() {
            args.insert(
                "save_to".to_string(),
                LLSDValue::String(self.custom_save_path.trim().to_string()),
            );
        }

        self.pending_capture = Some(client.request_async(
            "LLCamera",
            args,
            Duration::from_secs(15),
            move |reply| {
                let status = get_bool(&reply, "status").unwrap_or(false);
                if status {
                    let w = get_i32(&reply, "width").unwrap_or(width_in as i32) as u32;
                    let h = get_i32(&reply, "height").unwrap_or(height_in as i32) as u32;
                    let file_path = get_str(&reply, "file_path").unwrap_or("").to_string();
                    let position = get_str(&reply, "position").unwrap_or("").to_string();
                    let png_base64 = get_str(&reply, "png_base64").unwrap_or("");

                    let png_bytes = base64::engine::general_purpose::STANDARD
                        .decode(png_base64)
                        .unwrap_or_default();

                    Ok(CapturedPhoto {
                        timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
                        width: w,
                        height: h,
                        file_path,
                        position,
                        png_bytes,
                    })
                } else {
                    let err = get_str(&reply, "error").unwrap_or("Snapshot request failed");
                    Err(err.to_string())
                }
            },
        ));
    }
}
