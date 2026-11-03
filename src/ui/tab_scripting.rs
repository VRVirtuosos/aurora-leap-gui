//! Automated Scripting & Testing Tab — write, validate, and execute automated Rhai test scripts.

use egui::{Color32, FontId, RichText, Rounding, ScrollArea, Ui, Vec2};

use crate::client::LeapClient;
use crate::scripting::catalog::ScriptCatalog;
use crate::scripting::models::{LogLevel, ScriptStatus};
use crate::scripting::runner::ScriptRunner;
use crate::scripting::syntax::highlight_rhai_script;
use crate::ui::theme::{
    card_frame, inner_box_frame, COLOR_ACCENT, COLOR_DANGER, COLOR_SUCCESS, COLOR_TEXT_MUTED,
    COLOR_WARNING,
};

#[derive(Clone, Debug, PartialEq)]
pub struct LintDiagnostic {
    pub line: usize,
    pub col: usize,
    pub message: String,
    pub is_error: bool,
}

#[derive(Clone, Debug)]
pub struct BatchScriptResult {
    pub name: String,
    pub status: ScriptStatus,
    pub passed_assertions: usize,
    pub failed_assertions: usize,
    pub duration_ms: u64,
    pub error_msg: Option<String>,
}

pub struct ScriptingTab {
    catalog: ScriptCatalog,
    selected_script_index: Option<usize>,
    script_name_input: String,
    editor_code: String,
    loop_count: usize,
    runner: ScriptRunner,
    status_message: Option<String>,

    // Batch Test Suite Runner
    batch_running: bool,
    batch_index: usize,
    batch_results: Vec<BatchScriptResult>,
    console_view_matrix: bool,

    // Resizable Panels State
    catalog_width: f32,
    console_width: f32,
    show_catalog: bool,
    show_console: bool,
    is_maximized: bool,

    // Editor Preferences & Enhancements
    font_size: f32,
    show_line_numbers: bool,
    syntax_validation_status: Option<(bool, String)>,

    // Realtime Linter State
    lint_diagnostic: Option<LintDiagnostic>,
    last_linted_code: String,

    // Find & Replace State
    show_find_replace: bool,
    find_text: String,
    replace_text: String,
    last_find_count: usize,
}

impl ScriptingTab {
    pub fn new() -> Self {
        let catalog = ScriptCatalog::new();
        let mut selected_idx = None;
        let mut code = String::new();
        let mut name = String::new();

        if !catalog.scripts.is_empty() {
            selected_idx = Some(0);
            name = catalog.scripts[0].clone();
            code = catalog.load_script(&name).unwrap_or_default();
        }

        Self {
            catalog,
            selected_script_index: selected_idx,
            script_name_input: name,
            editor_code: code,
            loop_count: 1,
            runner: ScriptRunner::new(),
            status_message: None,

            batch_running: false,
            batch_index: 0,
            batch_results: Vec::new(),
            console_view_matrix: false,

            catalog_width: 220.0,
            console_width: 360.0,
            show_catalog: true,
            show_console: true,
            is_maximized: false,

            font_size: 13.0,
            show_line_numbers: true,
            syntax_validation_status: None,
            lint_diagnostic: None,
            last_linted_code: String::new(),

            show_find_replace: false,
            find_text: String::new(),
            replace_text: String::new(),
            last_find_count: 0,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, client: &LeapClient) {
        let is_connected = client.is_connected();

        // Continuously poll background worker thread events
        self.runner.poll_events();
        if self.runner.status == ScriptStatus::Running {
            ui.ctx().request_repaint();
        } else if self.batch_running {
            // Previous batch step finished
            let last_error = if self.runner.status == ScriptStatus::Failed {
                self.runner.report.logs.iter().rev()
                    .find(|l| l.level == LogLevel::Fail)
                    .map(|l| l.message.clone())
            } else {
                None
            };
            self.batch_results.push(BatchScriptResult {
                name: self.runner.current_script_name.clone(),
                status: self.runner.status,
                passed_assertions: self.runner.report.passed_assertions,
                failed_assertions: self.runner.report.failed_assertions,
                duration_ms: self.runner.report.duration_ms,
                error_msg: last_error,
            });

            self.batch_index += 1;
            if self.batch_index < self.catalog.scripts.len() {
                let next_name = self.catalog.scripts[self.batch_index].clone();
                if let Some(code) = self.catalog.load_script(&next_name) {
                    self.script_name_input = next_name.clone();
                    self.editor_code = code.clone();
                    self.runner.start(next_name, code, 1, client.clone());
                }
            } else {
                self.batch_running = false;
                let passed = self.batch_results.iter().filter(|r| r.status == ScriptStatus::Succeeded).count();
                let total = self.batch_results.len();
                self.status_message = Some(format!("Batch test suite complete: {passed}/{total} passed"));
            }
            ui.ctx().request_repaint();
        }

        // Live Realtime Linting check
        if self.editor_code != self.last_linted_code {
            self.run_realtime_linter();
        }

        ui.vertical(|ui| {
            // ── Top Header Bar & Global View Controls ───────────────────────────
            card_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("📜 Test Automation & Scripting");

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Export Report button
                        if ui.button("📥 Export Report").clicked() {
                            let (report_content, default_filename) = if self.console_view_matrix && !self.batch_results.is_empty() {
                                (self.generate_batch_report_markdown(), "batch_test_suite_report.md".to_string())
                            } else {
                                (self.runner.report.to_markdown(), format!("{}_report.md", self.script_name_input))
                            };
                            if let Some(path) = rfd::FileDialog::new()
                                .set_file_name(&default_filename)
                                .add_filter("Markdown", &["md"])
                                .save_file()
                            {
                                let _ = std::fs::write(path, report_content);
                                self.status_message = Some("Report saved successfully.".to_string());
                            }
                        }

                        // Clear Console
                        if ui.button("🧹 Clear Output").clicked() {
                            self.runner.clear();
                        }

                        ui.separator();

                        // Panel Visibility Toggles
                        let con_btn = if self.show_console { "📊 Console" } else { "📊 Console (Hidden)" };
                        if ui.selectable_label(self.show_console && !self.is_maximized, con_btn).clicked() {
                            self.show_console = !self.show_console;
                        }

                        let cat_btn = if self.show_catalog { "📁 Catalog" } else { "📁 Catalog (Hidden)" };
                        if ui.selectable_label(self.show_catalog && !self.is_maximized, cat_btn).clicked() {
                            self.show_catalog = !self.show_catalog;
                        }

                        let max_btn = if self.is_maximized { "🗗 Restore View" } else { "⛶ Maximize Editor" };
                        if ui.selectable_label(self.is_maximized, max_btn).clicked() {
                            self.is_maximized = !self.is_maximized;
                        }

                        // Status message preview
                        if let Some(ref msg) = self.status_message {
                            ui.label(RichText::new(msg).size(11.0).color(COLOR_SUCCESS));
                        }
                    });
                });
            });

            ui.add_space(4.0);

            // ── Main Flexible Workspace Layout ──────────────────────────────────
            let total_avail_w = ui.available_width();
            let avail_h = (ui.available_height() - 8.0).max(300.0);

            ui.horizontal(|ui| {
                // 1. Script Catalog Panel (Left)
                if self.show_catalog && !self.is_maximized {
                    ui.push_id("panel_catalog", |ui| {
                        ui.allocate_ui_with_layout(
                            Vec2::new(self.catalog_width, avail_h),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                card_frame().show(ui, |ui| {
                                    ui.set_min_size(Vec2::new(self.catalog_width - 24.0, avail_h - 24.0));
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new("📁 Catalog").strong().size(12.0));
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            if let Some(idx) = self.selected_script_index {
                                                if ui.button("🗑").on_hover_text("Delete selected script").clicked() {
                                                    if idx < self.catalog.scripts.len() {
                                                        let name = self.catalog.scripts[idx].clone();
                                                        let _ = self.catalog.delete_script(&name);
                                                        self.selected_script_index = None;
                                                        self.editor_code.clear();
                                                    }
                                                }
                                            }
                                            if ui.button("💾").on_hover_text("Save script to disk").clicked() {
                                                if !self.script_name_input.trim().is_empty() {
                                                    if let Err(e) = self.catalog.save_script(&self.script_name_input, &self.editor_code) {
                                                        self.status_message = Some(e);
                                                    } else {
                                                        self.status_message = Some(format!("Saved '{}'", self.script_name_input));
                                                    }
                                                }
                                            }
                                            if ui.button("➕").on_hover_text("Create new script").clicked() {
                                                self.selected_script_index = None;
                                                self.script_name_input = "custom_test.rhai".to_string();
                                                self.editor_code = "// New Automated LEAP Test\n\nprint(\"Test script started...\");\n".to_string();
                                                self.syntax_validation_status = None;
                                            }
                                        });
                                    });

                                    ui.add_space(4.0);
                                    ui.label(RichText::new("File Name:").size(10.0).color(COLOR_TEXT_MUTED));
                                    ui.add_sized([ui.available_width(), 18.0], egui::TextEdit::singleline(&mut self.script_name_input));

                                    ui.add_space(4.0);
                                    let batch_label = if self.batch_running {
                                        format!("⏳ Batch ({}/{})", self.batch_index + 1, self.catalog.scripts.len())
                                    } else {
                                        "🚀 Run Batch Suite (All)".to_string()
                                    };
                                    let batch_btn = egui::Button::new(
                                        RichText::new(batch_label)
                                            .size(11.0)
                                            .strong()
                                            .color(Color32::WHITE),
                                    )
                                    .fill(if self.batch_running { Color32::from_rgb(180, 120, 40) } else { COLOR_ACCENT });

                                    if ui.add_enabled(is_connected && !self.batch_running && self.runner.status != ScriptStatus::Running, batch_btn).clicked() {
                                        self.start_batch_suite(client);
                                    }

                                    ui.add_space(4.0);
                                    ui.separator();
                                    ui.add_space(2.0);

                                    let catalog_scroll_h = (ui.available_height() - 8.0).max(100.0);
                                    ScrollArea::vertical()
                                        .id_salt("script_catalog_scroll")
                                        .auto_shrink([false, false])
                                        .min_scrolled_height(catalog_scroll_h)
                                        .max_height(catalog_scroll_h)
                                        .show(ui, |ui| {
                                            let mut chosen_idx = None;
                                            for (idx, name) in self.catalog.scripts.iter().enumerate() {
                                                let is_sel = self.selected_script_index == Some(idx);
                                                if ui.selectable_label(is_sel, format!("📄 {name}")).clicked() {
                                                    chosen_idx = Some(idx);
                                                }
                                            }

                                            if let Some(idx) = chosen_idx {
                                                self.selected_script_index = Some(idx);
                                                let name = self.catalog.scripts[idx].clone();
                                                self.script_name_input = name.clone();
                                                if let Some(content) = self.catalog.load_script(&name) {
                                                    self.editor_code = content;
                                                    self.syntax_validation_status = None;
                                                }
                                            }
                                        });
                                });
                            },
                        );
                    });

                    // Draggable Splitter Handle between Catalog & Editor
                    render_splitter(ui, &mut self.catalog_width, 140.0, 420.0, true, avail_h);
                }

                // Calculate center editor width
                let right_w = if self.show_console && !self.is_maximized { self.console_width + 8.0 } else { 0.0 };
                let left_w = if self.show_catalog && !self.is_maximized { self.catalog_width + 8.0 } else { 0.0 };
                let center_editor_w = (total_avail_w - left_w - right_w).max(280.0);

                // 2. Center Code Editor Panel
                ui.push_id("panel_editor", |ui| {
                    ui.allocate_ui_with_layout(
                        Vec2::new(center_editor_w, avail_h),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            card_frame().show(ui, |ui| {
                                ui.set_min_size(Vec2::new(center_editor_w - 24.0, avail_h - 24.0));

                                // ── SCRIPT TITLE & PRIMARY EXECUTION CONTROLS (DIRECTLY ABOVE SCRIPT) ──
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(format!("📝 {}", self.script_name_input)).strong().size(13.0));

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        // Stop button
                                        let stop_btn = egui::Button::new(
                                            RichText::new("⏹ Stop").strong().color(Color32::WHITE),
                                        )
                                        .fill(COLOR_DANGER)
                                        .min_size(Vec2::new(68.0, 24.0));
                                        let is_running = self.runner.status == ScriptStatus::Running || self.batch_running;
                                        if ui.add_enabled(is_running, stop_btn).clicked() {
                                            self.batch_running = false;
                                            self.runner.stop();
                                        }

                                        // Run button
                                        let run_btn = egui::Button::new(
                                            RichText::new("▶ Run Test").strong().color(Color32::WHITE),
                                        )
                                        .fill(COLOR_SUCCESS)
                                        .min_size(Vec2::new(94.0, 24.0));
                                        if ui.add_enabled(is_connected && !is_running, run_btn).clicked() {
                                            self.run_realtime_linter();
                                            if let Some(ref diag) = self.lint_diagnostic.as_ref().filter(|d| d.is_error) {
                                                self.status_message = Some(format!("Syntax Error on Line {}: {}", diag.line, diag.message));
                                                self.syntax_validation_status = Some((false, format!("Line {}, Col {}: {}", diag.line, diag.col, diag.message)));
                                            } else {
                                                let name = if self.script_name_input.is_empty() {
                                                    "untitled.rhai".to_string()
                                                } else {
                                                    self.script_name_input.clone()
                                                };
                                                self.runner.start(name, self.editor_code.clone(), self.loop_count, client.clone());
                                            }
                                        }

                                        // Loop Count
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new("Loops:").size(11.0).color(COLOR_TEXT_MUTED));
                                            ui.add(egui::DragValue::new(&mut self.loop_count).range(1..=100));
                                        });

                                        // Milestone 6.7: Macro Recorder Toggle
                                        let is_recording = client.is_macro_recording();
                                        let macro_btn_text = if is_recording {
                                            let count = client.macro_action_count();
                                            format!("⏹ Stop Macro ({count})")
                                        } else {
                                            "● Record Macro".to_string()
                                        };
                                        let macro_btn = egui::Button::new(
                                            RichText::new(macro_btn_text).strong().color(Color32::WHITE),
                                        )
                                        .fill(if is_recording { COLOR_DANGER } else { Color32::from_rgb(140, 35, 55) })
                                        .min_size(Vec2::new(105.0, 24.0));

                                        if ui.add(macro_btn)
                                            .on_hover_text("Record manual movements, camera pans, chat, and teleports into an automated Rhai test script")
                                            .clicked()
                                        {
                                             if is_recording {
                                                 let generated_code = client.stop_macro_recording();
                                                 self.editor_code = generated_code;
                                                 self.script_name_input = format!("recorded_macro_{}.rhai", chrono::Local::now().format("%Y%m%d_%H%M%S"));
                                                 self.selected_script_index = None;
                                                 self.status_message = Some("Macro recorded and loaded into editor! Click 'Save' or '▶ Run Test'.".to_string());
                                             } else {
                                                 client.start_macro_recording();
                                                 self.status_message = Some("● Recording macro: perform manual actions in any tab...".to_string());
                                             }
                                        }

                                        // Status Badge
                                        let (badge_color, badge_text) = match self.runner.status {
                                            ScriptStatus::Idle => (COLOR_TEXT_MUTED, "⚪ Idle"),
                                            ScriptStatus::Running => (COLOR_ACCENT, "⏳ Running..."),
                                            ScriptStatus::Paused => (COLOR_WARNING, "⏸ Paused"),
                                            ScriptStatus::Succeeded => (COLOR_SUCCESS, "🟢 Succeeded"),
                                            ScriptStatus::Failed => (COLOR_DANGER, "🔴 Failed"),
                                            ScriptStatus::Stopped => (COLOR_WARNING, "⏹ Stopped"),
                                        };
                                        ui.label(RichText::new(badge_text).color(badge_color).strong().size(12.0));
                                    });
                                });

                                ui.add_space(4.0);

                                // ── ADVANCED EDITOR TOOLBAR (DIRECTLY ABOVE SCRIPT) ──
                                ui.horizontal(|ui| {
                                    // Syntax Check Button
                                    if ui.button("✓ Check Syntax").clicked() {
                                        self.validate_syntax();
                                    }

                                    // Live Realtime Linter Badge
                                    if let Some(ref diag) = self.lint_diagnostic {
                                        let (icon, color) = if diag.is_error { ("🔴", COLOR_DANGER) } else { ("ℹ️", COLOR_WARNING) };
                                        let msg = if diag.is_error {
                                            format!("{icon} Line {}, Col {}: {}", diag.line, diag.col, diag.message)
                                        } else {
                                            format!("{icon} {}", diag.message)
                                        };
                                        inner_box_frame().show(ui, |ui| {
                                            ui.label(RichText::new(msg).size(11.0).color(color).monospace());
                                        });
                                    } else if !self.editor_code.is_empty() {
                                        inner_box_frame().show(ui, |ui| {
                                            ui.label(RichText::new("🟢 Syntax Clean").size(11.0).color(COLOR_SUCCESS));
                                        });
                                    }

                                    // Find & Replace Toggle
                                    let find_btn = if self.show_find_replace { "🔍 Find [-]" } else { "🔍 Find [+]" };
                                    if ui.selectable_label(self.show_find_replace, find_btn).clicked() {
                                        self.show_find_replace = !self.show_find_replace;
                                    }

                                    // Insert Snippet Menu
                                    egui::ComboBox::from_id_salt("insert_snippet_dropdown")
                                        .selected_text("➕ Insert Snippet...")
                                        .show_ui(ui, |ui| {
                                            if ui.button("Assertions Template").clicked() {
                                                self.insert_snippet(SNIPPET_ASSERTIONS);
                                            }
                                            if ui.button("Waypoints & Movement Loop").clicked() {
                                                self.insert_snippet(SNIPPET_WAYPOINTS);
                                            }
                                            if ui.button("Inworld Snapshot Capture").clicked() {
                                                self.insert_snippet(SNIPPET_SNAPSHOT);
                                            }
                                            if ui.button("Teleport & Wait Until Inworld").clicked() {
                                                self.insert_snippet(SNIPPET_LOGIN_TELEPORT);
                                            }
                                            if ui.button("Chat & Speech Broadcast").clicked() {
                                                self.insert_snippet(SNIPPET_CHAT);
                                            }
                                        });

                                    ui.separator();

                                    // Font Size Selector
                                    ui.label(RichText::new("Font:").size(11.0).color(COLOR_TEXT_MUTED));
                                    for size in [11.0, 13.0, 15.0, 17.0] {
                                        let label = format!("{:.0}", size);
                                        if ui.selectable_label(self.font_size == size, label).clicked() {
                                            self.font_size = size;
                                        }
                                    }

                                    ui.checkbox(&mut self.show_line_numbers, "Line Numbers");
                                });

                                // ── Optional Find & Replace Bar ──
                                if self.show_find_replace {
                                    ui.add_space(2.0);
                                    inner_box_frame().show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new("Find:").size(11.0).strong());
                                            let find_edit = egui::TextEdit::singleline(&mut self.find_text)
                                                .desired_width(160.0)
                                                .hint_text("Search query...");
                                            if ui.add(find_edit).changed() {
                                                if !self.find_text.is_empty() {
                                                    self.last_find_count = self.editor_code.match_indices(&self.find_text).count();
                                                } else {
                                                    self.last_find_count = 0;
                                                }
                                            }

                                            if !self.find_text.is_empty() {
                                                ui.label(
                                                    RichText::new(format!("{} match(es)", self.last_find_count))
                                                        .size(11.0)
                                                        .color(if self.last_find_count > 0 { COLOR_ACCENT } else { COLOR_WARNING }),
                                                );
                                            }

                                            ui.add_space(6.0);
                                            ui.label(RichText::new("Replace:").size(11.0).strong());
                                            ui.add(egui::TextEdit::singleline(&mut self.replace_text).desired_width(160.0).hint_text("Replacement..."));

                                            if ui.button("Replace Next").clicked() && !self.find_text.is_empty() {
                                                if let Some(idx) = self.editor_code.find(&self.find_text) {
                                                    self.editor_code.replace_range(idx..idx + self.find_text.len(), &self.replace_text);
                                                    self.last_find_count = self.editor_code.match_indices(&self.find_text).count();
                                                }
                                            }

                                            if ui.button("Replace All").clicked() && !self.find_text.is_empty() {
                                                self.editor_code = self.editor_code.replace(&self.find_text, &self.replace_text);
                                                self.last_find_count = 0;
                                            }

                                            if ui.button("❌ Close").clicked() {
                                                self.show_find_replace = false;
                                            }
                                        });
                                    });
                                }

                                // ── Optional Syntax Validation Banner ──
                                let mut dismiss_syntax_status = false;
                                if let Some((valid, ref msg)) = self.syntax_validation_status {
                                    ui.add_space(2.0);
                                    let (bg_color, fg_color, icon) = if valid {
                                        (Color32::from_rgb(20, 50, 30), COLOR_SUCCESS, "✓")
                                    } else {
                                        (Color32::from_rgb(60, 25, 25), COLOR_DANGER, "⚠")
                                    };

                                    egui::Frame::none()
                                        .fill(bg_color)
                                        .rounding(Rounding::same(4.0))
                                        .inner_margin(egui::Margin::symmetric(8.0, 4.0))
                                        .show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                ui.label(RichText::new(format!("{icon} {msg}")).size(11.0).color(fg_color).monospace());
                                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                    if ui.small_button("Dismiss").clicked() {
                                                        dismiss_syntax_status = true;
                                                    }
                                                });
                                            });
                                        });
                                }
                                if dismiss_syntax_status {
                                    self.syntax_validation_status = None;
                                }

                                ui.add_space(4.0);

                                // ── FULL-SIZE SCRIPT EDITING CONTAINER ──
                                let font_size = self.font_size;
                                let search_query = if self.show_find_replace && !self.find_text.is_empty() {
                                    Some(self.find_text.as_str())
                                } else {
                                    None
                                };

                                // Allocate 100% of remaining panel height (reserving 26px for bottom stats bar)
                                let remaining_editor_h = (ui.available_height() - 28.0).max(120.0);

                                let mut code_changed = false;
                                inner_box_frame().show(ui, |ui| {
                                    ui.set_min_size(Vec2::new(ui.available_width(), remaining_editor_h));

                                    ScrollArea::both()
                                        .id_salt("script_editor_scroll")
                                        .auto_shrink([false, false])
                                        .min_scrolled_height(remaining_editor_h)
                                        .max_height(remaining_editor_h)
                                        .show(ui, |ui| {
                                            ui.horizontal(|ui| {
                                                if self.show_line_numbers {
                                                    let line_count = self.editor_code.lines().count().max(1);
                                                    let error_line = self.lint_diagnostic.as_ref().filter(|d| d.is_error).map(|d| d.line);
                                                    let mut gutter_job = egui::text::LayoutJob::default();
                                                    for n in 1..=line_count {
                                                        let is_err = error_line == Some(n);
                                                        let prefix = if is_err { "🔴" } else { "  " };
                                                        let text = format!("{prefix}{:>3} \n", n);
                                                        let color = if is_err {
                                                            COLOR_DANGER
                                                        } else {
                                                            Color32::from_rgb(100, 115, 140)
                                                        };
                                                        gutter_job.append(
                                                            &text,
                                                            0.0,
                                                            egui::TextFormat {
                                                                font_id: FontId::monospace(font_size),
                                                                color,
                                                                ..Default::default()
                                                            },
                                                        );
                                                    }

                                                    ui.add(egui::Label::new(gutter_job).selectable(false));
                                                    ui.separator();
                                                }

                                                let mut layouter = |ui: &Ui, text: &str, wrap_width: f32| {
                                                    let mut layout_job = highlight_rhai_script(text, font_size, search_query);
                                                    layout_job.wrap.max_width = wrap_width;
                                                    ui.fonts(|f| f.layout_job(layout_job))
                                                };

                                                let text_edit = egui::TextEdit::multiline(&mut self.editor_code)
                                                    .font(FontId::monospace(font_size))
                                                    .desired_width(f32::INFINITY)
                                                    .min_size(Vec2::new(ui.available_width(), remaining_editor_h))
                                                    .lock_focus(true)
                                                    .layouter(&mut layouter);

                                                let text_res = ui.add(text_edit);
                                                if text_res.changed() {
                                                    code_changed = true;
                                                }
                                            });
                                        });
                                });

                                if code_changed {
                                    self.run_realtime_linter();
                                }

                                // ── SCRIPT STATS FOOTER ──
                                ui.add_space(2.0);
                                ui.separator();
                                ui.horizontal(|ui| {
                                    let line_cnt = self.editor_code.lines().count().max(1);
                                    let char_cnt = self.editor_code.chars().count();
                                    let word_cnt = self.editor_code.split_whitespace().count();
                                    ui.label(RichText::new(format!("Lines: {line_cnt} | Chars: {char_cnt} | Words: {word_cnt}")).size(10.0).color(COLOR_TEXT_MUTED));

                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        ui.label(RichText::new("UTF-8 | Rhai Script Engine").size(10.0).color(COLOR_TEXT_MUTED));
                                    });
                                });
                            });
                        },
                    );
                });

                // 3. Right Console & Execution Output Panel
                if self.show_console && !self.is_maximized {
                    // Draggable Splitter Handle between Editor & Console
                    render_splitter(ui, &mut self.console_width, 220.0, 650.0, false, avail_h);

                    ui.push_id("panel_console", |ui| {
                        ui.allocate_ui_with_layout(
                            Vec2::new(self.console_width, avail_h),
                            egui::Layout::top_down(egui::Align::Min),
                            |ui| {
                                card_frame().show(ui, |ui| {
                                    ui.set_min_size(Vec2::new(self.console_width - 24.0, avail_h - 24.0));
                                    ui.horizontal(|ui| {
                                        if !self.batch_results.is_empty() {
                                            if ui.selectable_label(!self.console_view_matrix, "📜 Live Logs").clicked() {
                                                self.console_view_matrix = false;
                                            }
                                            let mat_label = format!("🚀 Batch Matrix ({}/{})", self.batch_results.len(), self.catalog.scripts.len());
                                            if ui.selectable_label(self.console_view_matrix, mat_label).clicked() {
                                                self.console_view_matrix = true;
                                            }
                                        } else {
                                            ui.label(RichText::new("📊 Execution Output").strong().size(12.0));
                                        }

                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            let passed = self.runner.report.passed_assertions;
                                            let failed = self.runner.report.failed_assertions;
                                            let assert_summary = format!("✅ {passed} | ❌ {failed}");
                                            ui.label(
                                                RichText::new(assert_summary)
                                                    .size(11.0)
                                                    .monospace()
                                                    .color(if failed > 0 { COLOR_DANGER } else if passed > 0 { COLOR_SUCCESS } else { COLOR_TEXT_MUTED }),
                                            );
                                        });
                                    });

                                    ui.add_space(4.0);

                                    let console_scroll_h = (ui.available_height() - 8.0).max(100.0);
                                    inner_box_frame().show(ui, |ui| {
                                        ui.set_min_size(Vec2::new(ui.available_width(), console_scroll_h));
                                        ScrollArea::vertical()
                                            .id_salt("script_console_scroll")
                                            .auto_shrink([false, false])
                                            .min_scrolled_height(console_scroll_h)
                                            .max_height(console_scroll_h)
                                            .stick_to_bottom(!self.console_view_matrix)
                                            .show(ui, |ui| {
                                                if self.console_view_matrix && !self.batch_results.is_empty() {
                                                    let total_passed = self.batch_results.iter().filter(|r| r.status == ScriptStatus::Succeeded).count();
                                                    let total_failed = self.batch_results.iter().filter(|r| r.status == ScriptStatus::Failed).count();
                                                    let total_duration: u64 = self.batch_results.iter().map(|r| r.duration_ms).sum();

                                                    inner_box_frame().show(ui, |ui| {
                                                        ui.horizontal(|ui| {
                                                            let status_text = if self.batch_running {
                                                                format!("⏳ Running Suite... ({}/{})", self.batch_index + 1, self.catalog.scripts.len())
                                                            } else if total_failed == 0 {
                                                                format!("✅ All {} Scripts Passed", self.batch_results.len())
                                                            } else {
                                                                format!("⚠️ {total_failed} Failed, {total_passed} Passed")
                                                            };
                                                            let color = if self.batch_running {
                                                                COLOR_WARNING
                                                            } else if total_failed == 0 {
                                                                COLOR_SUCCESS
                                                            } else {
                                                                COLOR_DANGER
                                                            };
                                                            ui.label(RichText::new(status_text).strong().size(12.0).color(color));
                                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                                ui.label(RichText::new(format!("Total: {total_duration}ms")).size(11.0).monospace().color(COLOR_TEXT_MUTED));
                                                            });
                                                        });
                                                    });

                                                    ui.add_space(6.0);
                                                    for (idx, r) in self.batch_results.iter().enumerate() {
                                                        let (icon, color) = match r.status {
                                                            ScriptStatus::Succeeded => ("🟢", COLOR_SUCCESS),
                                                            ScriptStatus::Failed => ("🔴", COLOR_DANGER),
                                                            _ => ("⏳", COLOR_WARNING),
                                                        };

                                                        inner_box_frame().show(ui, |ui| {
                                                            ui.horizontal(|ui| {
                                                                ui.label(RichText::new(format!("{icon} {}. {}", idx + 1, r.name)).strong().size(11.0));
                                                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                                    ui.label(RichText::new(format!("{}ms", r.duration_ms)).size(10.0).monospace().color(COLOR_TEXT_MUTED));
                                                                    ui.label(RichText::new(format!("✅ {} | ❌ {}", r.passed_assertions, r.failed_assertions)).size(10.0).monospace().color(color));
                                                                });
                                                            });

                                                            if let Some(ref err) = r.error_msg {
                                                                ui.add_space(2.0);
                                                                ui.label(RichText::new(err).color(COLOR_DANGER).size(10.0).monospace());
                                                            }
                                                        });
                                                        ui.add_space(3.0);
                                                    }
                                                } else if self.runner.report.logs.is_empty() {
                                                    ui.label(
                                                        RichText::new("No execution output yet.\nClick '▶ Run Test' above the script to start automated execution.")
                                                            .color(COLOR_TEXT_MUTED)
                                                            .size(11.0),
                                                    );
                                                } else {
                                                    for log in &self.runner.report.logs {
                                                        let (tag_color, tag) = match log.level {
                                                            LogLevel::Info => (Color32::from_rgb(100, 180, 255), "INFO"),
                                                            LogLevel::Pass => (COLOR_SUCCESS, "PASS"),
                                                            LogLevel::Fail => (COLOR_DANGER, "FAIL"),
                                                            LogLevel::Warn => (COLOR_WARNING, "WARN"),
                                                            LogLevel::Leap => (Color32::from_rgb(220, 130, 255), "LEAP"),
                                                        };
                                                        ui.horizontal(|ui| {
                                                            ui.label(RichText::new(&log.timestamp).size(10.0).color(COLOR_TEXT_MUTED).monospace());
                                                            ui.label(RichText::new(format!("[{tag}]")).size(10.0).color(tag_color).strong().monospace());
                                                            ui.label(RichText::new(&log.message).size(11.0).color(Color32::from_rgb(220, 225, 235)).monospace());
                                                        });
                                                    }
                                                }
                                            });
                                    });
                                });
                            },
                        );
                    });
                }
            });
        });
    }

    /// Compile-check current script using Rhai's AST compiler.
    fn validate_syntax(&mut self) {
        self.run_realtime_linter();
        if let Some(ref diag) = self.lint_diagnostic {
            if diag.is_error {
                self.syntax_validation_status = Some((false, format!("Line {}, Col {}: {}", diag.line, diag.col, diag.message)));
            } else {
                self.syntax_validation_status = Some((true, diag.message.clone()));
            }
        } else {
            self.syntax_validation_status = Some((true, "Syntax verification passed! AST compiled successfully with 0 errors.".to_string()));
        }
    }

    /// Realtime background linter executed on keystroke.
    fn run_realtime_linter(&mut self) {
        self.last_linted_code = self.editor_code.clone();
        if self.editor_code.trim().is_empty() {
            self.lint_diagnostic = None;
            return;
        }

        let engine = rhai::Engine::new();
        match engine.compile(&self.editor_code) {
            Err(err) => {
                let pos = err.position();
                let line = pos.line().unwrap_or(1);
                let col = pos.position().unwrap_or(1);
                self.lint_diagnostic = Some(LintDiagnostic {
                    line,
                    col,
                    message: format!("{err}"),
                    is_error: true,
                });
            }
            Ok(_) => {
                // Semantic check: missing assertions
                if !self.editor_code.contains("assert_") && !self.editor_code.trim().is_empty() {
                    self.lint_diagnostic = Some(LintDiagnostic {
                        line: 1,
                        col: 1,
                        message: "Notice: Script does not contain any assertions ('assert_true', 'assert_eq')".to_string(),
                        is_error: false,
                    });
                } else {
                    self.lint_diagnostic = None;
                }
            }
        }
    }

    /// Append snippet to editor code.
    fn insert_snippet(&mut self, snippet: &str) {
        if !self.editor_code.ends_with('\n') && !self.editor_code.is_empty() {
            self.editor_code.push('\n');
        }
        self.editor_code.push_str(snippet);
        self.syntax_validation_status = None;
    }

    /// Trigger execution of all catalog scripts in automated sequence.
    pub fn start_batch_suite(&mut self, client: &LeapClient) {
        if self.catalog.scripts.is_empty() {
            return;
        }
        self.batch_running = true;
        self.batch_index = 0;
        self.batch_results.clear();
        self.console_view_matrix = true;

        let first_name = self.catalog.scripts[0].clone();
        if let Some(code) = self.catalog.load_script(&first_name) {
            self.script_name_input = first_name.clone();
            self.editor_code = code.clone();
            self.runner.start(first_name, code, 1, client.clone());
        }
    }

    /// Generate a structured Markdown report summarizing the full batch test suite execution.
    pub fn generate_batch_report_markdown(&self) -> String {
        let mut out = String::new();
        out.push_str("# Aurora-Viewer LEAP Automated Batch Test Suite Report\n\n");
        let now = chrono::Local::now().format("%Y-%m-%d %H:%M:%S").to_string();
        out.push_str(&format!("- **Timestamp**: {}\n", now));
        let total = self.batch_results.len();
        let passed = self.batch_results.iter().filter(|r| r.status == ScriptStatus::Succeeded).count();
        let failed = self.batch_results.iter().filter(|r| r.status == ScriptStatus::Failed).count();
        let total_ms: u64 = self.batch_results.iter().map(|r| r.duration_ms).sum();
        out.push_str(&format!("- **Total Scripts**: {}\n", total));
        out.push_str(&format!("- **Passed**: {}\n", passed));
        out.push_str(&format!("- **Failed**: {}\n", failed));
        out.push_str(&format!("- **Total Execution Time**: {} ms\n\n", total_ms));

        out.push_str("| # | Script Name | Status | Passed | Failed | Duration | Diagnostics |\n");
        out.push_str("| :--- | :--- | :---: | :---: | :---: | :---: | :--- |\n");

        for (idx, r) in self.batch_results.iter().enumerate() {
            let status_str = match r.status {
                ScriptStatus::Succeeded => "✅ PASS",
                ScriptStatus::Failed => "❌ FAIL",
                _ => "⏳ INCOMPLETE",
            };
            let err = r.error_msg.as_deref().unwrap_or("-").replace('\n', " ");
            out.push_str(&format!(
                "| {} | `{}` | {} | {} | {} | {} ms | {} |\n",
                idx + 1,
                r.name,
                status_str,
                r.passed_assertions,
                r.failed_assertions,
                r.duration_ms,
                err
            ));
        }
        out
    }
}

/// Helper to render an interactive vertical divider handle between resizable panels.
fn render_splitter(
    ui: &mut Ui,
    width_state: &mut f32,
    min_width: f32,
    max_width: f32,
    drag_affects_left: bool,
    height: f32,
) {
    let (rect, response) = ui.allocate_exact_size(
        Vec2::new(6.0, height),
        egui::Sense::click_and_drag(),
    );

    if response.hovered() || response.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
    }

    if response.dragged() {
        let delta = ui.input(|i| i.pointer.delta().x);
        if drag_affects_left {
            *width_state = (*width_state + delta).clamp(min_width, max_width);
        } else {
            *width_state = (*width_state - delta).clamp(min_width, max_width);
        }
    }

    let line_color = if response.dragged() {
        COLOR_ACCENT
    } else if response.hovered() {
        Color32::from_rgb(120, 150, 200)
    } else {
        Color32::from_rgb(45, 52, 70)
    };

    ui.painter().vline(rect.center().x, rect.y_range(), egui::Stroke::new(2.0_f32, line_color));
}

// ── Built-in Snippets ────────────────────────────────────────────────────────
const SNIPPET_ASSERTIONS: &str = r#"
// Validation & Assertions
let val = 42;
assert_true(val > 0, "Value is positive");
assert_eq(val, 42, "Value equals 42");
assert_ne(val, 0, "Value is not zero");
assert_near(3.1415, 3.14, 0.01, "Pi approximation matches");
"#;

const SNIPPET_WAYPOINTS: &str = r#"
// Waypoints Walking Loop
let waypoints = [
    #{ x: 128.0, y: 132.0, z: 25.0 },
    #{ x: 134.0, y: 132.0, z: 25.0 },
    #{ x: 134.0, y: 128.0, z: 25.0 },
    #{ x: 128.0, y: 128.0, z: 25.0 },
];

for wp in waypoints {
    print("Navigating to waypoint: (" + wp.x + ", " + wp.y + ")");
    agent_teleport(wp.x, wp.y, wp.z);
    sleep(1.0);
}
"#;

const SNIPPET_SNAPSHOT: &str = r#"
// Inworld Viewport Snapshot Capture
print("Capturing native viewport snapshot...");
let snap_file = snapshot_capture("native", 0, 0);
assert_true(snap_file != "", "Snapshot file was saved successfully");
print("Snapshot saved to: " + snap_file);
"#;

const SNIPPET_LOGIN_TELEPORT: &str = r#"
// Teleport & Inworld State Verification
let state = login_get_state();
print("Current Login State: " + state);

if state == "STATE_STARTED" {
    print("Avatar in world. Teleporting to region coordinates...");
    agent_teleport(128.0, 128.0, 25.0);
    sleep(1.5);
    let pos = agent_get_pos();
    assert_true(pos.x > 0.0, "Valid X coordinate");
}
"#;

const SNIPPET_CHAT: &str = r#"
// Public Chat Broadcast
chat_say("Aurora automated test runner check-in.");
sleep(0.5);
"#;
