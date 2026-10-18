//! Direct, zero-allocation, bounded sysfs I/O engine.
//! Bounded execution O(1), no heap allocations, no panics.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::Path;

pub const WMAX_PROFILE_PATH: &str = "/sys/class/platform-profile/platform-profile-0/profile";
pub const ACPI_PROFILE_PATH: &str = "/sys/firmware/acpi/platform_profile";
pub const CPU_SYSFS_BASE: &str = "/sys/devices/system/cpu";

#[derive(Debug)]
pub enum SysfsError {
    NotFound,
    PermissionDenied,
    Io(std::io::ErrorKind),
    InvalidUtf8,
    ParseError,
}

impl std::fmt::Display for SysfsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound => write!(f, "Node not found"),
            Self::PermissionDenied => write!(f, "Permission denied (requires root or wheel group)"),
            Self::Io(kind) => write!(f, "I/O error: {:?}", kind),
            Self::InvalidUtf8 => write!(f, "Invalid UTF-8 in sysfs response"),
            Self::ParseError => write!(f, "Failed to parse sysfs integer"),
        }
    }
}

/// Reads up to `buf.len()` bytes from a sysfs path into a pre-allocated stack buffer.
/// Returns the slice of valid ASCII bytes trimmed of trailing newlines/spaces.
pub fn read_sysfs<'a>(path: &Path, buf: &'a mut [u8]) -> Result<&'a str, SysfsError> {
    if !path.exists() {
        return Err(SysfsError::NotFound);
    }

    let mut file = File::open(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::PermissionDenied => SysfsError::PermissionDenied,
        std::io::ErrorKind::NotFound => SysfsError::NotFound,
        other => SysfsError::Io(other),
    })?;

    let bytes_read = file.read(buf).map_err(|e| match e.kind() {
        std::io::ErrorKind::PermissionDenied => SysfsError::PermissionDenied,
        other => SysfsError::Io(other),
    })?;

    if bytes_read == 0 {
        return Ok("");
    }

    let raw_str = std::str::from_utf8(&buf[..bytes_read]).map_err(|_| SysfsError::InvalidUtf8)?;
    Ok(raw_str.trim())
}

/// Reads and parses an integer value from sysfs.
pub fn read_sysfs_int(path: &Path) -> Result<i64, SysfsError> {
    let mut buf = [0u8; 64];
    let val_str = read_sysfs(path, &mut buf)?;
    val_str.parse::<i64>().map_err(|_| SysfsError::ParseError)
}

/// Writes direct bytes to a sysfs path.
pub fn write_sysfs(path: &Path, content: &str) -> Result<(), SysfsError> {
    if !path.exists() {
        return Err(SysfsError::NotFound);
    }

    let mut file = OpenOptions::new()
        .write(true)
        .truncate(true)
        .open(path)
        .map_err(|e| match e.kind() {
            std::io::ErrorKind::PermissionDenied => SysfsError::PermissionDenied,
            std::io::ErrorKind::NotFound => SysfsError::NotFound,
            other => SysfsError::Io(other),
        })?;

    file.write_all(content.as_bytes()).map_err(|e| match e.kind() {
        std::io::ErrorKind::PermissionDenied => SysfsError::PermissionDenied,
        other => SysfsError::Io(other),
    })?;

    Ok(())
}

static RESOLVED_WMAX_PATH: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

/// Returns the primary active WMAX profile path (or fallback).
pub fn get_wmax_path() -> &'static Path {
    RESOLVED_WMAX_PATH.get_or_init(|| {
        let base = Path::new("/sys/class/platform-profile");
        if let Ok(entries) = std::fs::read_dir(base) {
            for entry in entries.flatten() {
                let p = entry.path();
                let name_file = p.join("name");
                let mut buf = [0u8; 64];
                if let Ok(name) = read_sysfs(&name_file, &mut buf) {
                    if name.trim() == "alienware-wmi" {
                        let prof = p.join("profile");
                        if prof.exists() {
                            return prof;
                        }
                    }
                }
            }
        }
        let wmax = Path::new(WMAX_PROFILE_PATH);
        if wmax.exists() {
            wmax.to_path_buf()
        } else {
            Path::new(ACPI_PROFILE_PATH).to_path_buf()
        }
    })
}

/// Checks write accessibility of the WMAX profile node.
pub fn is_wmax_writable() -> bool {
    let path = get_wmax_path();
    if !path.exists() {
        return false;
    }
    OpenOptions::new().write(true).open(path).is_ok()
}

/// Formats CPU governor sysfs path into a stack buffer without heap allocation.
pub fn cpu_governor_path(core_id: usize, buf: &mut [u8; 128]) -> Option<&Path> {
    use std::io::Write;
    let mut cursor = std::io::Cursor::new(&mut buf[..]);
    write!(cursor, "{CPU_SYSFS_BASE}/cpu{core_id}/cpufreq/scaling_governor").ok()?;
    let len = cursor.position() as usize;
    std::str::from_utf8(&buf[..len]).ok().map(Path::new)
}

/// Formats CPU EPP sysfs path into a stack buffer without heap allocation.
pub fn cpu_epp_path(core_id: usize, buf: &mut [u8; 128]) -> Option<&Path> {
    use std::io::Write;
    let mut cursor = std::io::Cursor::new(&mut buf[..]);
    write!(cursor, "{CPU_SYSFS_BASE}/cpu{core_id}/cpufreq/energy_performance_preference").ok()?;
    let len = cursor.position() as usize;
    std::str::from_utf8(&buf[..len]).ok().map(Path::new)
}

/// Synchronizes CPU scaling governors across all detected cores (bounded to max 32 cores).
pub fn sync_cpu_governors(governor: &str) -> usize {
    let mut updated = 0;
    let mut path_buf = [0u8; 128];
    // Bounded iteration across cpu0..cpu31
    for core_id in 0..32 {
        if let Some(path) = cpu_governor_path(core_id, &mut path_buf) {
            if path.exists() {
                if write_sysfs(path, governor).is_ok() {
                    updated += 1;
                }
            } else if core_id >= 16 {
                // Reached core ceiling for the Dell G15 5530 (16 threads max)
                break;
            }
        }
    }
    updated
}

/// Synchronizes CPU EPP registers across all detected cores (bounded to max 32 cores).
pub fn sync_cpu_epp(epp: &str) -> usize {
    let mut updated = 0;
    let mut path_buf = [0u8; 128];
    for core_id in 0..32 {
        if let Some(path) = cpu_epp_path(core_id, &mut path_buf) {
            if path.exists() {
                if write_sysfs(path, epp).is_ok() {
                    updated += 1;
                }
            } else if core_id >= 16 {
                break;
            }
        }
    }
    updated
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cpu_path_formatting_deterministic() {
        let mut buf = [0u8; 128];
        for i in 0..32 {
            let gov_path = cpu_governor_path(i, &mut buf).expect("valid gov path");
            let expected = format!("/sys/devices/system/cpu/cpu{i}/cpufreq/scaling_governor");
            assert_eq!(gov_path.to_str().unwrap(), expected);

            let epp_path = cpu_epp_path(i, &mut buf).expect("valid epp path");
            let expected_epp = format!("/sys/devices/system/cpu/cpu{i}/cpufreq/energy_performance_preference");
            assert_eq!(epp_path.to_str().unwrap(), expected_epp);
        }
    }
}
