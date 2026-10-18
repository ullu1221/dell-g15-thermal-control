//! Alienware WMAX hwmon hardware sensor & fan controller.
//! Enforces strict bounds checking: offsets must be within [0, 100]%.

use std::fs;
use std::path::{Path, PathBuf};
use crate::sysfs::{read_sysfs_int, write_sysfs, SysfsError};

#[derive(Debug, Clone, Copy, Default)]
pub struct TelemetryData {
    pub cpu_rpm: Option<u32>,
    pub gpu_rpm: Option<u32>,
    pub cpu_boost: Option<u8>,
    pub gpu_boost: Option<u8>,
    pub cpu_temp_c: Option<i32>,
    pub gpu_temp_c: Option<i32>,
}

/// Locates the active Alienware WMI hwmon path under platform-profile or class/hwmon.
pub fn find_alienware_hwmon() -> Option<PathBuf> {
    // 1. Primary path under resolved platform-profile device symlink
    let wmax_path = crate::sysfs::get_wmax_path();
    if let Some(parent) = wmax_path.parent() {
        let primary = parent.join("device/hwmon");
        if primary.exists() {
            if let Ok(entries) = fs::read_dir(&primary) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.file_name().and_then(|n| n.to_str()).is_some_and(|s| s.starts_with("hwmon")) {
                        return Some(p);
                    }
                }
            }
        }
    }

    // 2. Fallback to platform-profile-0 device/hwmon
    let p0 = Path::new("/sys/class/platform-profile/platform-profile-0/device/hwmon");
    if p0.exists() {
        if let Ok(entries) = fs::read_dir(p0) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.file_name().and_then(|n| n.to_str()).is_some_and(|s| s.starts_with("hwmon")) {
                    return Some(p);
                }
            }
        }
    }

    // 2. Global fallback scan under /sys/class/hwmon/hwmon*
    let fallback = Path::new("/sys/class/hwmon");
    if fallback.exists() {
        if let Ok(entries) = fs::read_dir(fallback) {
            for entry in entries.flatten() {
                let p = entry.path();
                let name_file = p.join("name");
                let mut buf = [0u8; 64];
                if let Ok(name) = crate::sysfs::read_sysfs(&name_file, &mut buf) {
                    if name.contains("alienware_wmi") {
                        return Some(p);
                    }
                }
            }
        }
    }

    None
}

/// Reads current hardware telemetry (fan RPMs, boosts, and temperatures).
pub fn read_telemetry() -> TelemetryData {
    let mut data = TelemetryData::default();
    let hwmon = match find_alienware_hwmon() {
        Some(h) => h,
        None => return data,
    };

    if let Ok(rpm) = read_sysfs_int(&hwmon.join("fan1_input")) {
        if (0..=10000).contains(&rpm) {
            data.cpu_rpm = Some(rpm as u32);
        }
    }
    if let Ok(rpm) = read_sysfs_int(&hwmon.join("fan2_input")) {
        if (0..=10000).contains(&rpm) {
            data.gpu_rpm = Some(rpm as u32);
        }
    }
    if let Ok(boost) = read_sysfs_int(&hwmon.join("fan1_boost")) {
        if (0..=100).contains(&boost) {
            data.cpu_boost = Some(boost as u8);
        }
    }
    if let Ok(boost) = read_sysfs_int(&hwmon.join("fan2_boost")) {
        if (0..=100).contains(&boost) {
            data.gpu_boost = Some(boost as u8);
        }
    }
    if let Ok(mdeg) = read_sysfs_int(&hwmon.join("temp1_input")) {
        if (-40_000..=150_000).contains(&mdeg) {
            if let Ok(deg) = i32::try_from(mdeg / 1000) {
                data.cpu_temp_c = Some(deg);
            }
        }
    }
    if let Ok(mdeg) = read_sysfs_int(&hwmon.join("temp2_input")) {
        if (-40_000..=150_000).contains(&mdeg) {
            if let Ok(deg) = i32::try_from(mdeg / 1000) {
                data.gpu_temp_c = Some(deg);
            }
        }
    }

    data
}

/// Helper to format a bounded u8 into a stack buffer without heap allocation.
fn format_u8(val: u8, buf: &mut [u8; 4]) -> &str {
    use std::io::Write;
    let mut cursor = std::io::Cursor::new(&mut buf[..]);
    let _ = write!(cursor, "{val}");
    let len = cursor.position() as usize;
    std::str::from_utf8(&buf[..len]).unwrap_or("0")
}

/// Sets fan boost offsets defensively. Enforces 0 <= boost <= 100. Zero allocation.
pub fn set_fan_boost(cpu_boost: Option<u8>, gpu_boost: Option<u8>) -> Result<(), SysfsError> {
    let hwmon = find_alienware_hwmon().ok_or(SysfsError::NotFound)?;
    let mut buf = [0u8; 4];

    if let Some(b) = cpu_boost {
        let bounded = b.min(100);
        let path = hwmon.join("fan1_boost");
        write_sysfs(&path, format_u8(bounded, &mut buf))?;
    }

    if let Some(b) = gpu_boost {
        let bounded = b.min(100);
        let path = hwmon.join("fan2_boost");
        write_sysfs(&path, format_u8(bounded, &mut buf))?;
    }

    Ok(())
}
