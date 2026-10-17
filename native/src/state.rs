//! Atomic, defensive thermal state persistence across reboots.
//! Conforms to INV-04 (Atomic Transitions) and INV-05 (Zero-Scream Boot Protection).

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};
use crate::model::ThermalMode;

#[derive(Debug, Clone)]
pub struct PersistentState {
    pub mode: ThermalMode,
    pub cpu_boost: u8,
    pub gpu_boost: u8,
}

impl Default for PersistentState {
    fn default() -> Self {
        Self {
            mode: ThermalMode::Balanced,
            cpu_boost: 0,
            gpu_boost: 0,
        }
    }
}

/// Computes the user state file path (`~/.config/dell-g15/state.json`).
pub fn get_user_state_path() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config").join("dell-g15").join("state.json"))
}

/// Fallback system state path (`/var/tmp/dell-g15-state.json`).
pub fn get_system_state_path() -> PathBuf {
    PathBuf::from("/var/tmp/dell-g15-state.json")
}

/// Atomically persists thermal state to disk.
pub fn save_state(state: &PersistentState) -> std::io::Result<()> {
    let target_path = get_user_state_path().unwrap_or_else(get_system_state_path);
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let temp_path = target_path.with_extension("tmp");
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    let json_content = format!(
        "{{\n  \"mode\": \"{}\",\n  \"wmax\": \"{}\",\n  \"ppd\": \"{}\",\n  \"governor\": \"{}\",\n  \"epp\": \"{}\",\n  \"cpu_boost\": {},\n  \"gpu_boost\": {},\n  \"timestamp\": {}\n}}\n",
        state.mode.name(),
        state.mode.wmax_profile(),
        state.mode.ppd_profile(),
        state.mode.cpu_governor(),
        state.mode.cpu_epp(),
        state.cpu_boost,
        state.gpu_boost,
        timestamp
    );

    // Write temp file and atomically rename
    {
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temp_path)?;
        file.write_all(json_content.as_bytes())?;
        file.sync_all()?;
    }

    fs::rename(&temp_path, &target_path)?;

    // Mirror to system state path if permitted
    let sys_path = get_system_state_path();
    if target_path != sys_path {
        let _ = fs::copy(&target_path, &sys_path);
    }

    Ok(())
}

/// Defensively loads state from disk. Reverts to Balanced on corruption or missing file.
pub fn load_state() -> PersistentState {
    let candidates: [Option<PathBuf>; 2] = [get_user_state_path(), Some(get_system_state_path())];

    for path in candidates.into_iter().flatten() {
        if path.exists() {
            if let Ok(mut f) = File::open(&path) {
                let mut buf = [0u8; 1024];
                if let Ok(bytes_read) = f.read(&mut buf) {
                    if bytes_read > 0 {
                        if let Ok(content) = std::str::from_utf8(&buf[..bytes_read]) {
                            if let Some(parsed) = parse_simple_state(content) {
                                return parsed;
                            }
                        }
                    }
                }
            }
        }
    }

    PersistentState::default()
}

/// Zero-allocation, lightweight JSON key-value extractor.
fn parse_simple_state(json: &str) -> Option<PersistentState> {
    let mut mode = ThermalMode::Balanced;
    let mut cpu_boost = 0u8;
    let mut gpu_boost = 0u8;

    for line in json.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("\"mode\":") {
            if let Some(val) = extract_string_val(trimmed) {
                if let Some(m) = ThermalMode::parse_defensive(val) {
                    mode = m;
                }
            }
        } else if trimmed.starts_with("\"cpu_boost\":") {
            if let Some(n) = extract_int_val(trimmed) {
                cpu_boost = n.min(100) as u8;
            }
        } else if trimmed.starts_with("\"gpu_boost\":") {
            if let Some(n) = extract_int_val(trimmed) {
                gpu_boost = n.min(100) as u8;
            }
        }
    }

    Some(PersistentState {
        mode,
        cpu_boost,
        gpu_boost,
    })
}

fn extract_string_val(line: &str) -> Option<&str> {
    let (_, val) = line.split_once(':')?;
    let raw = val.trim().trim_matches(|c| c == '"' || c == ',' || c == ' ');
    Some(raw)
}

fn extract_int_val(line: &str) -> Option<u64> {
    let (_, val) = line.split_once(':')?;
    let raw = val.trim().trim_matches(|c| c == ',' || c == ' ');
    raw.parse::<u64>().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_state_valid() {
        let json = r#"{
  "mode": "performance",
  "wmax": "balanced-performance",
  "ppd": "performance",
  "governor": "performance",
  "epp": "performance",
  "cpu_boost": 45,
  "gpu_boost": 50,
  "timestamp": 123456789
}"#;
        let st = parse_simple_state(json).expect("valid parse");
        assert_eq!(st.mode, ThermalMode::Performance);
        assert_eq!(st.cpu_boost, 45);
        assert_eq!(st.gpu_boost, 50);
    }

    #[test]
    fn test_parse_simple_state_clamping_prevents_truncation() {
        let json = r#"{
  "mode": "quiet",
  "cpu_boost": 257,
  "gpu_boost": 9999
}"#;
        let st = parse_simple_state(json).expect("valid parse");
        assert_eq!(st.mode, ThermalMode::Quiet);
        // 257 must clamp to 100, NOT truncate to 1
        assert_eq!(st.cpu_boost, 100);
        assert_eq!(st.gpu_boost, 100);
    }

    #[test]
    fn test_parse_corrupt_json_fallback() {
        let json = "not valid json at all";
        let st = parse_simple_state(json).expect("valid fallback");
        assert_eq!(st.mode, ThermalMode::Balanced);
        assert_eq!(st.cpu_boost, 0);
        assert_eq!(st.gpu_boost, 0);
    }
}
