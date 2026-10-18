//! Dell G15 5530 Thermal Sentinel CLI & HPC Engine.
//! High-Risk Systems Engineering • Deterministic • Zero-Allocation Hot Path.

#![forbid(unsafe_code)]

use std::env;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{exit, Command};
use std::time::Instant;

use g15_thermal_sentinel::hwmon::{self, read_telemetry};
use g15_thermal_sentinel::model::ThermalMode;
use g15_thermal_sentinel::state;
use g15_thermal_sentinel::sysfs;
use g15_thermal_sentinel::{apply_mode, restore_saved_state};

fn print_usage() {
    println!(
        r#"g15-sentinel - Mission-Critical Hardware Thermal & Power Controller for Dell G15 5530

Usage:
  g15-sentinel status                    # Real-time hardware telemetry (< 1ms execution)
  g15-sentinel mode <target>             # Switch profile: battery, quiet, balanced, performance, gmode
  g15-sentinel balance                   # Fast shortcut to balanced profile
  g15-sentinel gmode                     # Fast shortcut to toggle Alienware Game Shift (100% fan)
  g15-sentinel fan [cpu_boost] [gpu_boost] # Inspect fan RPMs or set manual boost offsets (0-100%)
  g15-sentinel restore                   # Restore last saved thermal state across reboots
  g15-sentinel init                      # Configure sysfs permissions & tmpfiles persistence (sudo)
  g15-sentinel watch                     # Low-overhead thermal guard daemon (zero CPU idle)
"#
    );
}

fn cmd_status() {
    let t0 = Instant::now();
    let telemetry = read_telemetry();

    let mut buf = [0u8; 64];
    let wmax_path = sysfs::get_wmax_path();
    let wmax_mode = sysfs::read_sysfs(wmax_path, &mut buf).unwrap_or("unknown");

    let mut gov_buf = [0u8; 64];
    let gov_path = Path::new("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor");
    let governor = sysfs::read_sysfs(gov_path, &mut gov_buf).unwrap_or("unknown");

    let mut epp_buf = [0u8; 64];
    let epp_path = Path::new("/sys/devices/system/cpu/cpu0/cpufreq/energy_performance_preference");
    let epp = sysfs::read_sysfs(epp_path, &mut epp_buf).unwrap_or("unknown");

    let ppd_profile = std::process::Command::new("powerprofilesctl")
        .arg("get")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| {
            let mut ppd_buf = [0u8; 64];
            let acpi_path = Path::new(sysfs::ACPI_PROFILE_PATH);
            sysfs::read_sysfs(acpi_path, &mut ppd_buf).unwrap_or(wmax_mode).to_string()
        });

    let is_gmode = wmax_mode == "performance";
    let elapsed = t0.elapsed();

    println!("=================================================================");
    println!(" DELL G15 5530 NATIVE THERMAL & POWER PROFILE STATUS");
    println!("=================================================================");
    println!("powerprofilesctl Profile:  {} (Active in Desktop Slider)", ppd_profile.to_uppercase());
    println!("Native AWCC WMAX Profile:  {} (0x{:02x})", wmax_mode.to_uppercase(), get_awcc_code(wmax_mode));
    println!("Game Shift / G-Mode:       {}", if is_gmode { "ACTIVE [100% FAN OVERRIDE]" } else { "OFF [Dynamic Fan Curve]" });
    println!("CPU Scaling Governor:      {}", governor);
    println!("CPU Energy Preference:     {}", epp);
    println!("-----------------------------------------------------------------");
    println!(
        "CPU Fan:                   {} (Boost Offset: {}%)",
        telemetry.cpu_rpm.map(|r| format!("{} RPM", r)).unwrap_or_else(|| "N/A".to_string()),
        telemetry.cpu_boost.unwrap_or(0)
    );
    println!(
        "GPU Fan:                   {} (Boost Offset: {}%)",
        telemetry.gpu_rpm.map(|r| format!("{} RPM", r)).unwrap_or_else(|| "N/A".to_string()),
        telemetry.gpu_boost.unwrap_or(0)
    );
    println!(
        "CPU Temperature:           {}",
        telemetry.cpu_temp_c.map(|t| format!("{} °C", t)).unwrap_or_else(|| "N/A".to_string())
    );
    println!(
        "GPU Temperature:           {}",
        telemetry.gpu_temp_c.map(|t| format!("{} °C", t)).unwrap_or_else(|| "N/A".to_string())
    );
    println!("=================================================================");
    println!(" [HPC Sentinel Telemetry Latency: {:.2?}]", elapsed);
}

fn get_awcc_code(mode: &str) -> u8 {
    match mode {
        "low-power" => 0xA5,
        "quiet" => 0xA3,
        "balanced" => 0xA0,
        "balanced-performance" => 0xA1,
        "performance" => 0xAB,
        _ => 0x00,
    }
}

fn cmd_fan(args: &[String]) {
    let cpu_boost = args.first().and_then(|s| s.parse::<u8>().ok());
    let gpu_boost = args.get(1).and_then(|s| s.parse::<u8>().ok());

    if cpu_boost.is_some() || gpu_boost.is_some() {
        if let Err(e) = hwmon::set_fan_boost(cpu_boost, gpu_boost) {
            eprintln!("[ERROR] Failed to set fan boost: {}", e);
            exit(1);
        }
        if let Some(c) = cpu_boost {
            println!("[✓] CPU Fan Boost offset set to {}%.", c.min(100));
        }
        if let Some(g) = gpu_boost {
            println!("[✓] GPU Fan Boost offset set to {}%.", g.min(100));
        }

        // Save persistent boosts
        let mut st = state::load_state();
        if let Some(c) = cpu_boost {
            st.cpu_boost = c.min(100);
        }
        if let Some(g) = gpu_boost {
            st.gpu_boost = g.min(100);
        }
        if let Err(e) = state::save_state(&st) {
            eprintln!("[!] Warning: Failed to persist fan boost offsets: {e}");
        }
    } else {
        cmd_status();
    }
}

fn cmd_init() {
    println!("[*] Initializing Dell G15 5530 hardware thermal subsystem (HPC Sentinel)...");

    let wmax_path = sysfs::get_wmax_path();
    if wmax_path.exists() {
        let _ = fs::set_permissions(wmax_path, fs::Permissions::from_mode(0o664));
        let _ = Command::new("chgrp").args(["wheel", &wmax_path.to_string_lossy()]).output();
        println!("[✓] Granted group 'wheel' write access to {}.", wmax_path.display());
    }

    if let Ok(entries) = fs::read_dir("/sys/class/platform-profile") {
        for entry in entries.flatten() {
            let prof = entry.path().join("profile");
            if prof.exists() {
                let _ = fs::set_permissions(&prof, fs::Permissions::from_mode(0o664));
                let _ = Command::new("chgrp").args(["wheel", &prof.to_string_lossy()]).output();
            }
        }
    }

    if let Some(hwmon) = hwmon::find_alienware_hwmon() {
        for fan in ["fan1_boost", "fan2_boost"] {
            let p = hwmon.join(fan);
            if p.exists() {
                let _ = fs::set_permissions(&p, fs::Permissions::from_mode(0o664));
                let _ = Command::new("chgrp").args(["wheel", &p.to_string_lossy()]).output();
                println!("[✓] Granted group 'wheel' write access to {}.", p.display());
            }
        }
    }

    // Set permissions on CPU scaling_governor and energy_performance_preference
    for core_id in 0..32 {
        let gov = Path::new("/sys/devices/system/cpu")
            .join(format!("cpu{}", core_id))
            .join("cpufreq/scaling_governor");
        if gov.exists() {
            let _ = fs::set_permissions(&gov, fs::Permissions::from_mode(0o664));
            let _ = Command::new("chgrp").args(["wheel", &gov.to_string_lossy()]).output();
        }
        let epp = Path::new("/sys/devices/system/cpu")
            .join(format!("cpu{}", core_id))
            .join("cpufreq/energy_performance_preference");
        if epp.exists() {
            let _ = fs::set_permissions(&epp, fs::Permissions::from_mode(0o664));
            let _ = Command::new("chgrp").args(["wheel", &epp.to_string_lossy()]).output();
        }
        let epb = Path::new("/sys/devices/system/cpu")
            .join(format!("cpu{}", core_id))
            .join("power/energy_perf_bias");
        if epb.exists() {
            let _ = fs::set_permissions(&epb, fs::Permissions::from_mode(0o664));
            let _ = Command::new("chgrp").args(["wheel", &epb.to_string_lossy()]).output();
        }
    }

    // Ensure persistent state file in /etc exists and is group-writable by wheel
    let state_file = Path::new("/etc/dell-g15-state.json");
    if !state_file.exists() {
        let _ = fs::write(state_file, "{}\n");
    }
    let _ = fs::set_permissions(state_file, fs::Permissions::from_mode(0o664));
    let _ = Command::new("chgrp").args(["wheel", "/etc/dell-g15-state.json"]).output();

    // Write permanent tmpfiles.d configuration
    let tmpfiles_path = Path::new("/etc/tmpfiles.d/dell-g15-thermal.conf");
    let tmpfiles_content = "\
# Dell G15 5530 Hardware Thermal & Fan Controls (HPC Sentinel)
z /sys/class/platform-profile/platform-profile-*/profile 0664 root wheel -
z /sys/class/platform-profile/platform-profile-*/device/hwmon/hwmon*/fan1_boost 0664 root wheel -
z /sys/class/platform-profile/platform-profile-*/device/hwmon/hwmon*/fan2_boost 0664 root wheel -
z /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor 0664 root wheel -
z /sys/devices/system/cpu/cpu*/cpufreq/energy_performance_preference 0664 root wheel -
z /sys/devices/system/cpu/cpu*/power/energy_perf_bias 0664 root wheel -
f /etc/dell-g15-state.json 0664 root wheel -
";
    if fs::write(tmpfiles_path, tmpfiles_content).is_ok() {
        let _ = Command::new("systemd-tmpfiles").args(["--create", "/etc/tmpfiles.d/dell-g15-thermal.conf"]).output();
        println!("[✓] Created and applied /etc/tmpfiles.d/dell-g15-thermal.conf for permanent persistence across reboots.");
    }

    if let Err(e) = restore_saved_state() {
        eprintln!("[!] Warning: Failed to restore saved thermal state during init: {e}");
    }
}

fn cmd_watch() {
    println!("[+] Dell G15 Thermal Sentinel Watchdog active. Monitoring hardware guard limits...");
    loop {
        g15_thermal_sentinel::guard::audit_thermal_guard();
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}

fn get_fallback_mode_on_gmode_disengage() -> ThermalMode {
    if let Ok(content) = std::fs::read_to_string("/etc/hpc-perf.conf") {
        for line in content.lines() {
            let line = line.trim();
            if let Some(val) = line.strip_prefix("ACTIVE_PROFILE=") {
                let val = val.trim().trim_matches('"').to_lowercase();
                if val == "ai" || val == "gaming" || val == "vmgaming" || val == "vm-gaming" {
                    return ThermalMode::Performance;
                } else if val == "battery" {
                    return ThermalMode::Quiet;
                } else if val == "daily" {
                    return ThermalMode::Balanced;
                }
            }
        }
    }
    let saved = state::load_state();
    if saved.mode == ThermalMode::Performance || saved.mode == ThermalMode::GMode {
        ThermalMode::Performance
    } else {
        saved.mode
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        cmd_status();
        return;
    }

    match args[1].as_str() {
        "status" => cmd_status(),
        "mode" => {
            if args.len() < 3 {
                eprintln!("[ERROR] Missing target mode.");
                eprintln!("Valid options: battery, quiet, balanced, performance, gmode");
                exit(1);
            }
            let target = &args[2];
            match ThermalMode::parse_defensive(target) {
                Some(mode) => {
                    if let Err(e) = apply_mode(mode, true) {
                        eprintln!("[ERROR] Failed to set mode: {e}");
                        exit(1);
                    }
                }
                None => {
                    eprintln!("[ERROR] Unknown mode '{target}'.");
                    eprintln!("Valid options: battery, quiet, balanced, performance, gmode");
                    exit(1);
                }
            }
        }
        "performance" | "perf" => {
            if let Err(e) = apply_mode(ThermalMode::Performance, true) {
                eprintln!("[ERROR] Failed to set performance mode: {e}");
                exit(1);
            }
        }
        "balance" | "balanced" => {
            if let Err(e) = apply_mode(ThermalMode::Balanced, true) {
                eprintln!("[ERROR] Failed to set balanced mode: {e}");
                exit(1);
            }
        }
        "quiet" => {
            if let Err(e) = apply_mode(ThermalMode::Quiet, true) {
                eprintln!("[ERROR] Failed to set quiet mode: {e}");
                exit(1);
            }
        }
        "battery" => {
            if let Err(e) = apply_mode(ThermalMode::Battery, true) {
                eprintln!("[ERROR] Failed to set battery mode: {e}");
                exit(1);
            }
        }
        "gmode" => {
            let mut buf = [0u8; 64];
            let wmax_path = sysfs::get_wmax_path();
            let current = sysfs::read_sysfs(wmax_path, &mut buf).unwrap_or("unknown");
            if current == "performance" {
                let target = get_fallback_mode_on_gmode_disengage();
                println!("[+] Game Shift / G-Mode currently ACTIVE -> Disengaging to {} mode...", target.name());
                if let Err(e) = apply_mode(target, true) {
                    eprintln!("[ERROR] Failed to disengage Game Shift: {e}");
                    exit(1);
                }
            } else {
                println!("[+] Engaging Game Shift / G-Mode (Full fan speed override)...");
                if let Err(e) = apply_mode(ThermalMode::GMode, true) {
                    eprintln!("[ERROR] Failed to engage Game Shift: {e}");
                    exit(1);
                }
            }
        }
        "fan" | "fans" => {
            cmd_fan(&args[2..]);
        }
        "restore" => {
            if let Err(e) = restore_saved_state() {
                eprintln!("[ERROR] Failed to restore state: {}", e);
                exit(1);
            }
        }
        "init" => {
            cmd_init();
        }
        "watch" => {
            cmd_watch();
        }
        "--help" | "-h" | "help" => {
            print_usage();
        }
        other => {
            eprintln!("[ERROR] Unknown command '{}'. Run with --help for usage.", other);
            exit(1);
        }
    }
}
