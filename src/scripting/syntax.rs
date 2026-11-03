//! Rhai and LEAP Scripting Syntax Highlighter for egui TextEdit.

use egui::text::LayoutJob;
use egui::{Color32, FontId, TextFormat};

/// Color palette for Rhai script syntax highlighting (VSCode Dark+ inspired).
pub struct SyntaxColors {
    pub default_text: Color32,
    pub keyword: Color32,
    pub builtin_fn: Color32,
    pub assertion_fn: Color32,
    pub string: Color32,
    pub number: Color32,
    pub comment: Color32,
    pub types: Color32,
    pub operator: Color32,
    pub search_match_bg: Color32,
    pub search_match_fg: Color32,
}

impl Default for SyntaxColors {
    fn default() -> Self {
        Self {
            default_text: Color32::from_rgb(220, 225, 235), // Crisp off-white
            keyword: Color32::from_rgb(197, 134, 192),      // Lilac / Magenta
            builtin_fn: Color32::from_rgb(220, 220, 170),   // Warm gold / yellow
            assertion_fn: Color32::from_rgb(78, 201, 176), // Bright teal / turquoise
            string: Color32::from_rgb(206, 145, 120),       // Warm coral / orange
            number: Color32::from_rgb(181, 206, 168),       // Pale mint green
            comment: Color32::from_rgb(106, 153, 85),       // Sage green
            types: Color32::from_rgb(86, 156, 214),         // Studio blue
            operator: Color32::from_rgb(170, 180, 195),      // Slate muted gray
            search_match_bg: Color32::from_rgb(110, 85, 20), // Glowing amber match highlight
            search_match_fg: Color32::WHITE,
        }
    }
}

/// Tokenize and build an egui `LayoutJob` for interactive syntax highlighting.
pub fn highlight_rhai_script(
    code: &str,
    font_size: f32,
    search_query: Option<&str>,
) -> LayoutJob {
    let colors = SyntaxColors::default();
    let font_id = FontId::monospace(font_size);

    let mut job = LayoutJob::default();
    let chars: Vec<char> = code.chars().collect();
    let len = chars.len();
    let mut i = 0;

    let search_lower = search_query
        .filter(|q| !q.is_empty())
        .map(|q| q.to_lowercase());

    while i < len {
        // 1. Line Comment `// ...`
        if i + 1 < len && chars[i] == '/' && chars[i + 1] == '/' {
            let start = i;
            while i < len && chars[i] != '\n' {
                i += 1;
            }
            let s: String = chars[start..i].iter().collect();
            append_token(&mut job, &s, font_id.clone(), colors.comment, false, None);
            continue;
        }

        // 2. Block Comment `/* ... */`
        if i + 1 < len && chars[i] == '/' && chars[i + 1] == '*' {
            let start = i;
            i += 2;
            while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            if i + 1 < len {
                i += 2; // skip `*/`
            }
            let s: String = chars[start..i].iter().collect();
            append_token(&mut job, &s, font_id.clone(), colors.comment, false, None);
            continue;
        }

        // 3. String Literals `"..."` and `'...'`
        if chars[i] == '"' || chars[i] == '\'' {
            let quote = chars[i];
            let start = i;
            i += 1;
            while i < len && chars[i] != quote {
                if chars[i] == '\\' && i + 1 < len {
                    i += 2; // skip escaped quote
                } else if chars[i] == '\n' {
                    break; // unclosed string on newline
                } else {
                    i += 1;
                }
            }
            if i < len && chars[i] == quote {
                i += 1;
            }
            let s: String = chars[start..i].iter().collect();
            append_token(&mut job, &s, font_id.clone(), colors.string, false, None);
            continue;
        }

        // 4. Numbers (decimal, float, hex)
        if chars[i].is_ascii_digit() || (chars[i] == '.' && i + 1 < len && chars[i + 1].is_ascii_digit()) {
            let start = i;
            if chars[i] == '0' && i + 1 < len && (chars[i + 1] == 'x' || chars[i + 1] == 'X') {
                i += 2;
                while i < len && chars[i].is_ascii_hexdigit() {
                    i += 1;
                }
            } else {
                while i < len && (chars[i].is_ascii_digit() || chars[i] == '.' || chars[i] == '_') {
                    i += 1;
                }
            }
            let s: String = chars[start..i].iter().collect();
            append_token(&mut job, &s, font_id.clone(), colors.number, false, None);
            continue;
        }

        // 5. Identifiers (keywords, functions, types, variables)
        if chars[i].is_alphabetic() || chars[i] == '_' {
            let start = i;
            while i < len && (chars[i].is_alphanumeric() || chars[i] == '_') {
                i += 1;
            }
            let ident: String = chars[start..i].iter().collect();

            // Check if matches active search query
            let is_search_match = search_lower
                .as_ref()
                .map(|q| ident.to_lowercase().contains(q))
                .unwrap_or(false);

            let bg = if is_search_match {
                Some(colors.search_match_bg)
            } else {
                None
            };

            let color = match ident.as_str() {
                // Rhai Keywords
                "let" | "const" | "if" | "else" | "fn" | "return" | "for" | "in" | "while"
                | "loop" | "break" | "continue" | "import" | "as" | "true" | "false" | "null"
                | "try" | "catch" | "throw" | "switch" | "case" | "default" => colors.keyword,

                // Assertion functions
                "assert_eq" | "assert_ne" | "assert_true" | "assert_false" | "assert_near" => {
                    colors.assertion_fn
                }

                // LEAP Subsystem Functions & DSL Builtins
                "agent_get_pos" | "agent_step" | "agent_teleport" | "agent_stop"
                | "agent_set_flying" | "login_get_state" | "login_wait_until_inworld"
                | "login_authenticate" | "login_logout" | "chat_say" | "snapshot_capture"
                | "inventory_get_root" | "print" | "sleep" | "leap_request" | "leap_send" => {
                    colors.builtin_fn
                }

                // Types
                "INT" | "FLOAT" | "String" | "Array" | "Map" | "bool" | "Dynamic" => colors.types,

                // Standard identifier / variable
                _ => colors.default_text,
            };

            append_token(&mut job, &ident, font_id.clone(), color, false, bg);
            continue;
        }

        // 6. Operators & Punctuation
        let op_color = match chars[i] {
            '+' | '-' | '*' | '/' | '%' | '=' | '!' | '<' | '>' | '&' | '|' | '^' => colors.operator,
            '{' | '}' | '(' | ')' | '[' | ']' | ';' | ',' | ':' | '.' => Color32::from_rgb(160, 170, 185),
            _ => colors.default_text,
        };

        let ch_str = chars[i].to_string();
        append_token(&mut job, &ch_str, font_id.clone(), op_color, false, None);
        i += 1;
    }

    job
}

fn append_token(
    job: &mut LayoutJob,
    text: &str,
    font_id: FontId,
    color: Color32,
    italics: bool,
    background: Option<Color32>,
) {
    let mut format = TextFormat::simple(font_id, color);
    format.italics = italics;
    if let Some(bg) = background {
        format.background = bg;
    }
    job.append(text, 0.0, format);
}
