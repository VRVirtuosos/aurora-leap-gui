//! Chat & Communications Tab (`LLChat`).

use std::collections::HashMap;
use std::time::Duration;

use chrono::Local;
use egui::{Color32, RichText, Ui};
use serde_llsd::LLSDValue;

use crate::client::LeapClient;
use crate::codec::{get_i32, get_str};
use crate::ui::theme::{card_frame, inner_box_frame, COLOR_ACCENT, COLOR_SUCCESS, COLOR_TEXT_MUTED, COLOR_WARNING};

pub struct ChatTab {
    message_input: String,
    channel_input: i32,
    chat_type_idx: usize,
    chat_history: Vec<ChatMessage>,
    status_msg: Option<String>,
}

struct ChatMessage {
    timestamp: String,
    from: String,
    channel: i32,
    message: String,
    is_outbound: bool,
}

impl ChatTab {
    pub fn new() -> Self {
        Self {
            message_input: String::new(),
            channel_input: 0,
            chat_type_idx: 1, // Normal
            chat_history: Vec::new(),
            status_msg: None,
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, client: &LeapClient) {
        let is_connected = client.is_connected();

        // Ingest any incoming events from the 'chat' pump into chat_history
        self.poll_chat_events(client);

        ui.vertical(|ui| {
            card_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.heading("💬 Chat Transmission & Monitor (`LLChat`)");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let sub_btn = egui::Button::new(RichText::new("📡 Subscribe to 'chat' Pump").size(12.0));
                        if ui.add_enabled(is_connected, sub_btn).clicked() {
                            match client.listen("chat", "gui_chat_tab") {
                                Ok(true) => self.status_msg = Some("Subscribed to 'chat' pump.".to_string()),
                                Ok(false) => self.status_msg = Some("Already subscribed or duplicate.".to_string()),
                                Err(e) => self.status_msg = Some(format!("Subscribe error: {e}")),
                            }
                        }
                    });
                });

                ui.add_space(8.0);

                // Input Bar
                ui.horizontal(|ui| {
                    ui.label("Message:");
                    let text_edit = ui.add_sized(
                        [ui.available_width() - 320.0, 22.0],
                        egui::TextEdit::singleline(&mut self.message_input).hint_text("Type a message to send to simulator / viewer..."),
                    );

                    ui.label("Channel:");
                    ui.add(egui::DragValue::new(&mut self.channel_input).speed(1));

                    let types = ["Whisper (0)", "Normal (1)", "Shout (2)"];
                    egui::ComboBox::from_id_salt("chat_type_combo")
                        .selected_text(types[self.chat_type_idx])
                        .show_ui(ui, |ui| {
                            for (idx, name) in types.iter().enumerate() {
                                ui.selectable_value(&mut self.chat_type_idx, idx, *name);
                            }
                        });

                    let send_btn = egui::Button::new(RichText::new("Send Chat").strong().color(Color32::WHITE)).fill(COLOR_ACCENT);
                    let should_send = (ui.add_enabled(is_connected && !self.message_input.trim().is_empty(), send_btn).clicked())
                        || (text_edit.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));

                    if should_send && is_connected && !self.message_input.trim().is_empty() {
                        self.send_chat(client);
                    }
                });

                if let Some(ref st) = self.status_msg {
                    ui.add_space(4.0);
                    ui.label(RichText::new(st).color(COLOR_SUCCESS).size(11.0));
                }
            });

            ui.add_space(10.0);

            // Live Chat Feed
            card_frame().show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Live Chat Stream ({})", self.chat_history.len())).strong().size(13.0));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("Clear Chat").clicked() {
                            self.chat_history.clear();
                        }
                    });
                });

                ui.add_space(6.0);

                egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                    for msg in &self.chat_history {
                        inner_box_frame().show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(&msg.timestamp).color(COLOR_TEXT_MUTED).size(10.0));
                                ui.label(
                                    RichText::new(&msg.from)
                                        .strong()
                                        .color(if msg.is_outbound { COLOR_ACCENT } else { COLOR_SUCCESS })
                                        .size(12.0),
                                );
                                if msg.channel != 0 {
                                    ui.label(RichText::new(format!("[ch:{}]", msg.channel)).color(COLOR_WARNING).size(10.0));
                                }
                                ui.label(RichText::new(&msg.message).size(12.0));
                            });
                        });
                        ui.add_space(3.0);
                    }

                    if self.chat_history.is_empty() {
                        ui.label(RichText::new("No chat messages yet. Type a message above and click 'Send Chat'.").color(COLOR_TEXT_MUTED));
                    }
                });
            });
        });
    }

    fn send_chat(&mut self, client: &LeapClient) {
        let msg_text = self.message_input.trim().to_string();
        self.message_input.clear();

        let mut args = HashMap::new();
        args.insert("op".to_string(), LLSDValue::String("sendChat".to_string()));
        args.insert("message".to_string(), LLSDValue::String(msg_text.clone()));
        args.insert("channel".to_string(), LLSDValue::Integer(self.channel_input));
        args.insert("type".to_string(), LLSDValue::Integer(self.chat_type_idx as i32));

        match client.request("LLChat", args, Duration::from_secs(4)) {
            Ok(_) => {
                let timestamp = Local::now().format("%H:%M:%S").to_string();
                self.chat_history.push(ChatMessage {
                    timestamp,
                    from: "You (Outbound)".to_string(),
                    channel: self.channel_input,
                    message: msg_text,
                    is_outbound: true,
                });
                self.status_msg = Some("Message dispatched to viewer LLChat pump.".to_string());
            }
            Err(e) => {
                self.status_msg = Some(format!("Failed to send chat: {e}"));
            }
        }
    }

    fn poll_chat_events(&mut self, client: &LeapClient) {
        let events = client.get_events();
        // Check for new chat pump events
        for ev in events {
            if ev.pump == "chat" {
                if let LLSDValue::Map(_) = ev.data {
                    let from = get_str(&ev.data, "from").unwrap_or("Simulator").to_string();
                    let message = get_str(&ev.data, "message").unwrap_or("").to_string();
                    let channel = get_i32(&ev.data, "channel").unwrap_or(0);

                    // Avoid duplicate consecutive insertions by checking last entry
                    let is_dup = self.chat_history.last().map_or(false, |last| {
                        last.message == message && last.timestamp == ev.timestamp
                    });

                    if !is_dup && !message.is_empty() {
                        self.chat_history.push(ChatMessage {
                            timestamp: ev.timestamp,
                            from,
                            channel,
                            message,
                            is_outbound: false,
                        });
                    }
                }
            }
        }
    }
}
