//! Script catalog, template seeding, and disk persistence.

use std::fs;
use std::path::PathBuf;

pub struct ScriptCatalog {
    pub dir_path: PathBuf,
    pub scripts: Vec<String>,
}

impl ScriptCatalog {
    pub fn new() -> Self {
        let dir_path = PathBuf::from("scripts");
        let mut catalog = Self {
            dir_path,
            scripts: Vec::new(),
        };
        catalog.ensure_dir_and_templates();
        catalog.refresh();
        catalog
    }

    pub fn ensure_dir_and_templates(&self) {
        if !self.dir_path.exists() {
            let _ = fs::create_dir_all(&self.dir_path);
        }

        // Seed default templates if they don't exist
        let t1 = self.dir_path.join("01_login_and_verify.rhai");
        if !t1.exists() {
            let _ = fs::write(t1, include_str!("../../scripts/01_login_and_verify.rhai"));
        }

        let t2 = self.dir_path.join("02_movement_and_waypoints.rhai");
        if !t2.exists() {
            let _ = fs::write(t2, include_str!("../../scripts/02_movement_and_waypoints.rhai"));
        }

        let t3 = self.dir_path.join("03_snapshot_regression.rhai");
        if !t3.exists() {
            let _ = fs::write(t3, include_str!("../../scripts/03_snapshot_regression.rhai"));
        }

        let t4 = self.dir_path.join("04_inventory_inspection.rhai");
        if !t4.exists() {
            let _ = fs::write(t4, include_str!("../../scripts/04_inventory_inspection.rhai"));
        }

        let t5 = self.dir_path.join("05_camera_and_views.rhai");
        if !t5.exists() {
            let _ = fs::write(t5, include_str!("../../scripts/05_camera_and_views.rhai"));
        }
    }

    pub fn refresh(&mut self) {
        let mut list = Vec::new();
        if let Ok(entries) = fs::read_dir(&self.dir_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() && path.extension().and_then(|s| s.to_str()) == Some("rhai") {
                    if let Some(file_name) = path.file_name().and_then(|s| s.to_str()) {
                        list.push(file_name.to_string());
                    }
                }
            }
        }
        list.sort();
        self.scripts = list;
    }

    pub fn load_script(&self, name: &str) -> Option<String> {
        let path = self.dir_path.join(name);
        fs::read_to_string(path).ok()
    }

    pub fn save_script(&mut self, name: &str, content: &str) -> Result<(), String> {
        let clean_name = if name.ends_with(".rhai") {
            name.to_string()
        } else {
            format!("{name}.rhai")
        };
        let path = self.dir_path.join(&clean_name);
        fs::write(path, content).map_err(|e| format!("Failed to save script: {e}"))?;
        self.refresh();
        Ok(())
    }

    pub fn delete_script(&mut self, name: &str) -> Result<(), String> {
        let path = self.dir_path.join(name);
        if path.exists() {
            fs::remove_file(path).map_err(|e| format!("Failed to delete script: {e}"))?;
            self.refresh();
        }
        Ok(())
    }
}
