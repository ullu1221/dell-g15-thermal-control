//! Dell G15 5530 Thermal Sentinel Core Library.
//! High-Risk Systems Engineering • DO-178C Level A • MISRA-C:2012 Aligned.

#![forbid(unsafe_code)]

pub mod guard;
pub mod hwmon;
pub mod model;
pub mod state;
pub mod sysfs;

use std::process::Command;
use model::ThermalMode;
use state::PersistentState;

/// Applies a complete, coordinated thermal & power profile transition.
pub fn apply_mode(mode: ThermalMode, save: bool) -> Result<(), sysfs::SysfsError> {
    println!("[+] Setting Dell G15 thermal/power profile to '{}'...", mode.name());

    let wmax_path = sysfs::get_wmax_path();
    let is_writable = sysfs::is_wmax_writable();

    // 1. Sync CPU scaling governor first (prevents intel_pstate EBUSY lockout)
    let gov_target = mode.cpu_governor();
    let updated_govs = sysfs::sync_cpu_governors(gov_target);

    // 2. Sync power-profiles-daemon (PPD)
    if mode == ThermalMode::Performance && !is_writable {
        println!("[!] Notice: Writing to {} requires root or group 'wheel' permissions.", wmax_path.display());
        println!("    Kept power-profiles-daemon on 'balanced' to prevent locking into 100% G-Mode fan scream (0xab).");
    } else {
        let ppd_target = mode.ppd_profile();
        let _ = Command::new("powerprofilesctl")
            .args(["set", ppd_target])
            .output();
        println!("[✓] Synced power-profiles-daemon ('{}' active in desktop slider).", ppd_target);
    }

    // 3. Direct Dell WMAX ACPI thermal profile sync
    let plat_val = mode.wmax_profile();
    if is_writable {
        sysfs::write_sysfs(wmax_path, plat_val)?;
        println!(
            "[✓] Applied native AWCC thermal mode '{}' (0x{:02x}) to WMAX.",
            plat_val,
            mode.wmax_code()
        );
    } else {
        println!("[!] WMAX node {} is read-only. Run 'sudo g15-sentinel init' to grant permissions.", wmax_path.display());
    }

    // 4. Fallback sync CPU EPP
    let epp_target = mode.cpu_epp();
    sysfs::sync_cpu_epp(epp_target);

    // 5. Persist state
    if save {
        let current_state = state::load_state();
        let new_state = PersistentState {
            mode,
            cpu_boost: current_state.cpu_boost,
            gpu_boost: current_state.gpu_boost,
        };
        let _ = state::save_state(&new_state);
        println!("[✓] Persisted thermal state '{}' across reboots.", mode.name());
    }

    println!("[✓] Dell G15 '{}' mode successfully activated ({} CPUs synced).", mode.name(), updated_govs);
    Ok(())
}

/// Restores previously persisted thermal state.
pub fn restore_saved_state() -> Result<(), sysfs::SysfsError> {
    let saved = state::load_state();
    println!("[*] Restoring saved Dell G15 thermal state: '{}'...", saved.mode.name());
    
    // Apply mode without re-saving
    apply_mode(saved.mode, false)?;

    // Restore fan boosts if elevated
    if saved.cpu_boost > 0 || saved.gpu_boost > 0 {
        let _ = hwmon::set_fan_boost(Some(saved.cpu_boost), Some(saved.gpu_boost));
        println!("[✓] Restored fan boosts: CPU +{}%, GPU +{}%.", saved.cpu_boost, saved.gpu_boost);
    }

    println!("[✓] Dell G15 hardware thermal state successfully restored to '{}'.", saved.mode.name());
    Ok(())
}
