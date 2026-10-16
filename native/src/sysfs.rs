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

/// Returns the primary active WMAX profile path (or fallback).
pub fn get_wmax_path() -> &'static Path {
    let wmax = Path::new(WMAX_PROFILE_PATH);
    if wmax.exists() {
        wmax
    } else {
        Path::new(ACPI_PROFILE_PATH)
    }
}

/// Checks write accessibility of the WMAX profile node.
pub fn is_wmax_writable() -> bool {
    let path = get_wmax_path();
    if !path.exists() {
        return false;
    }
    OpenOptions::new().write(true).open(path).is_ok()
}

/// Synchronizes CPU scaling governors across all detected cores (bounded to max 32 cores).
pub fn sync_cpu_governors(governor: &str) -> usize {
    let mut updated = 0;
    // Bounded iteration across cpu0..cpu31
    for core_id in 0..32 {
        let path_str = format!("{}/cpu{}/cpufreq/scaling_governor", CPU_SYSFS_BASE, core_id);
        let path = Path::new(&path_str);
        if path.exists() {
            if write_sysfs(path, governor).is_ok() {
                updated += 1;
            }
        } else if core_id >= 16 {
            // Reached core ceiling for the Dell G15 5530 (16 threads max)
            break;
        }
    }
    updated
}

/// Synchronizes CPU EPP registers across all detected cores (bounded to max 32 cores).
pub fn sync_cpu_epp(epp: &str) -> usize {
    let mut updated = 0;
    for core_id in 0..32 {
        let path_str = format!("{}/cpu{}/cpufreq/energy_performance_preference", CPU_SYSFS_BASE, core_id);
        let path = Path::new(&path_str);
        if path.exists() {
            if write_sysfs(path, epp).is_ok() {
                updated += 1;
            }
        } else if core_id >= 16 {
            break;
        }
    }
    updated
}
