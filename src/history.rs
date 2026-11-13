//! Remembers which sessions were resumed through ccs, for frequency ranking.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

#[derive(Default, Serialize, Deserialize)]
pub struct History {
    /// session id -> (times resumed, last resumed unix secs)
    pub entries: HashMap<String, (u32, u64)>,
}

fn path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("ccs")
        .join("history.json")
}

impl History {
    pub fn load() -> Self {
        fs::read(path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn count(&self, id: &str) -> u32 {
        self.entries.get(id).map_or(0, |e| e.0)
    }

    pub fn last(&self, id: &str) -> u64 {
        self.entries.get(id).map_or(0, |e| e.1)
    }

    pub fn record(&mut self, id: &str, now: u64) {
        let e = self.entries.entry(id.to_string()).or_insert((0, 0));
        e.0 += 1;
        e.1 = now;
        let p = path();
        if let Some(dir) = p.parent() {
            let _ = fs::create_dir_all(dir);
        }
        if let Ok(bytes) = serde_json::to_vec(self) {
            let _ = fs::write(p, bytes);
        }
    }
}
