//! Landmarks model & storage — persists Home, Last, and custom landmarks.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Landmark {
    pub name: String,
    pub region: String,
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub slurl: String,
    #[serde(default)]
    pub is_special: bool,
}

impl Landmark {
    pub fn new(name: &str, region: &str, x: f64, y: f64, z: f64) -> Self {
        let clean_region = region.trim();
        let region_name = if clean_region.is_empty() || clean_region.eq_ignore_ascii_case("last") || clean_region.eq_ignore_ascii_case("home") {
            "Horizons Info Hub"
        } else {
            clean_region
        };
        let slurl = format!("secondlife://{}/{:.0}/{:.0}/{:.0}", region_name, x, y, z);
        Self {
            name: name.to_string(),
            region: region_name.to_string(),
            x,
            y,
            z,
            slurl,
            is_special: false,
        }
    }

    pub fn special(name: &str, region: &str, x: f64, y: f64, z: f64) -> Self {
        let mut lm = Self::new(name, region, x, y, z);
        lm.is_special = true;
        lm
    }
}

pub struct LandmarkStore {
    pub landmarks: Vec<Landmark>,
    file_path: PathBuf,
}

impl LandmarkStore {
    pub fn load() -> Self {
        let file_path = PathBuf::from("saved_landmarks.json");
        let mut landmarks = Vec::new();

        if file_path.exists() {
            if let Ok(data) = std::fs::read_to_string(&file_path) {
                if let Ok(saved) = serde_json::from_str::<Vec<Landmark>>(&data) {
                    landmarks = saved;
                }
            }
        }

        // Sanitize any existing landmarks that have non-SLurl or placeholder slurls
        for lm in &mut landmarks {
            if !lm.slurl.starts_with("secondlife://") {
                let reg = if lm.region.is_empty() || lm.region == "Last" || lm.region == "Home" {
                    "Horizons Info Hub".to_string()
                } else {
                    lm.region.clone()
                };
                lm.slurl = format!("secondlife://{}/{:.0}/{:.0}/{:.0}", reg, lm.x, lm.y, lm.z);
                lm.region = reg;
            }
        }

        // Ensure default Home and Last entries exist if not present
        if !landmarks.iter().any(|lm| lm.name == "🏠 Home Location") {
            landmarks.insert(0, Landmark::special("🏠 Home Location", "Horizons Info Hub", 141.0, 127.0, 23.0));
        }
        if !landmarks.iter().any(|lm| lm.name == "📍 Last Location") {
            let idx = if landmarks.len() > 1 { 1 } else { landmarks.len() };
            landmarks.insert(idx, Landmark::special("📍 Last Location", "Horizons Info Hub", 141.0, 127.0, 23.0));
        }

        Self { landmarks, file_path }
    }

    pub fn save(&self) {
        if let Ok(json) = serde_json::to_string_pretty(&self.landmarks) {
            let _ = std::fs::write(&self.file_path, json);
        }
    }

    pub fn update_last(&mut self, region: &str, x: f64, y: f64, z: f64) {
        let clean = region.trim();
        if clean.is_empty() || clean.eq_ignore_ascii_case("last") || clean.eq_ignore_ascii_case("unknown") {
            return;
        }
        let slurl = format!("secondlife://{clean}/{:.0}/{:.0}/{:.0}", x, y, z);
        if let Some(lm) = self.landmarks.iter_mut().find(|l| l.name == "📍 Last Location") {
            lm.region = clean.to_string();
            lm.x = x;
            lm.y = y;
            lm.z = z;
            lm.slurl = slurl;
        } else {
            let mut lm = Landmark::special("📍 Last Location", clean, x, y, z);
            lm.slurl = slurl;
            self.landmarks.push(lm);
        }
        self.save();
    }

    pub fn update_home(&mut self, region: &str, x: f64, y: f64, z: f64) {
        let clean = region.trim();
        if clean.is_empty() || clean.eq_ignore_ascii_case("home") || clean.eq_ignore_ascii_case("unknown") {
            return;
        }
        let slurl = format!("secondlife://{clean}/{:.0}/{:.0}/{:.0}", x, y, z);
        if let Some(lm) = self.landmarks.iter_mut().find(|l| l.name == "🏠 Home Location") {
            lm.region = clean.to_string();
            lm.x = x;
            lm.y = y;
            lm.z = z;
            lm.slurl = slurl;
        } else {
            let mut lm = Landmark::special("🏠 Home Location", clean, x, y, z);
            lm.slurl = slurl;
            self.landmarks.push(lm);
        }
        self.save();
    }

    pub fn add(&mut self, name: &str, region: &str, x: f64, y: f64, z: f64) {
        let lm = Landmark::new(name, region, x, y, z);
        self.landmarks.push(lm);
        self.save();
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.landmarks.len() && !self.landmarks[index].is_special {
            self.landmarks.remove(index);
            self.save();
        }
    }

    pub fn home_slurl(&self) -> String {
        self.landmarks.iter()
            .find(|l| l.name == "🏠 Home Location")
            .map(|l| l.slurl.clone())
            .unwrap_or_else(|| "secondlife://Horizons Info Hub/141/127/23".to_string())
    }

    pub fn last_slurl(&self) -> String {
        self.landmarks.iter()
            .find(|l| l.name == "📍 Last Location")
            .map(|l| l.slurl.clone())
            .unwrap_or_else(|| "secondlife://Horizons Info Hub/141/127/23".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_landmark_slurl_generation() {
        let lm = Landmark::new("Test Place", "Achel", 140.0, 150.0, 26.0);
        assert_eq!(lm.slurl, "secondlife://Achel/140/150/26");
    }
}
