# Deployment, Setup & Configuration Guide

## 1. System Requirements & Prerequisites

- **Hardware**: Dell G15 5530 (Intel 13th Gen Raptor Lake HX + NVIDIA RTX 3050/4050/4060 Laptop GPU).
- **Kernel**: Linux kernel 6.12 or newer (tested on 6.12, 6.13, and 7.2.8-zen1-1-zen).
- **Core Dependencies**:
  - `power-profiles-daemon` (installed by default on Arch, Fedora, Ubuntu, Debian).
  - `python3` (3.10+).
  - User in group `wheel` (for unprivileged sysfs access via udev).

---

## 2. Quick Installation

Clone or navigate to the repository and execute the installer:

```bash
cd /home/p/dell-g15-thermal-control
sudo ./install.sh
```

The installer will:
1. Copy the standalone controller script to `/usr/local/bin/dell-g15-thermal`.
2. Install `/etc/udev/rules.d/99-dell-g15-thermal.rules` and reload udev rules.
3. Install and activate `/etc/systemd/system/dell-g15-thermal.service`.
4. Normalize `power-profiles-daemon` to `balanced` mode to prevent boot-time fan screams.

---

## 3. Manual Installation (Step-by-Step)

If you prefer installing components manually:

### Step 1: Install the Executable Script
```bash
sudo install -m 755 scripts/dell-g15-thermal /usr/local/bin/dell-g15-thermal
```

### Step 2: Install Udev Rules
```bash
sudo install -m 644 system/99-dell-g15-thermal.rules /etc/udev/rules.d/99-dell-g15-thermal.rules
sudo udevadm control --reload-rules
sudo udevadm trigger --subsystem-match=platform-profile --subsystem-match=hwmon
```

### Step 3: Configure Systemd Boot Service
```bash
sudo install -m 644 system/dell-g15-thermal.service /etc/systemd/system/dell-g15-thermal.service
sudo systemctl daemon-reload
sudo systemctl enable --now dell-g15-thermal.service
```

---

## 4. CLI Usage & Command Reference

### 4.1 Inspecting Real-Time Telemetry
Display current power profile, native WMAX profile, CPU/GPU fan RPMs, and temperatures:
```bash
$ dell-g15-thermal status

=================================================================
 DELL G15 5530 NATIVE THERMAL & POWER PROFILE STATUS
=================================================================
powerprofilesctl Profile:  BALANCED (Active in Desktop Slider)
Native AWCC WMAX Profile:  BALANCED (0xa0)
Game Shift / G-Mode:       OFF [Dynamic Fan Curve]
CPU Energy Preference:     balance_performance
-----------------------------------------------------------------
CPU Fan:                   892 RPM (Boost Offset: 0%)
GPU Fan:                   1016 RPM (Boost Offset: 0%)
CPU Temperature:           59 °C
GPU Temperature:           30 °C
=================================================================
```

### 4.2 Changing Thermal Profiles
```bash
# Activate Battery Saver (0xa5 - throttled, passive fans)
dell-g15-thermal mode battery

# Activate Quiet mode (0xa3 - silent acoustics ~800-1200 RPM)
dell-g15-thermal mode quiet

# Activate Balanced mode (0xa0 - adaptive daily driver)
dell-g15-thermal mode balanced
# Or use shortcut:
dell-g15-thermal balance

# Activate True Performance mode (0xa1 - unlocked PL1/PL2, dynamic smart fans)
dell-g15-thermal mode performance
# Or use shortcut:
dell-g15-thermal performance
```

### 4.3 Toggling Game Shift (G-Mode)
Toggles the 100% duty cycle fan overdrive on and off (mimicking the physical F9 key):
```bash
$ dell-g15-thermal gmode
[+] Engaging Game Shift / G-Mode (Full fan speed override)...
[✓] Synced power-profiles-daemon ('performance' active in desktop slider).
[✓] Dell G15 'gmode' mode successfully activated.

$ dell-g15-thermal gmode
[+] Game Shift / G-Mode is currently ACTIVE -> Disengaging to Balanced mode...
[✓] Synced power-profiles-daemon ('balanced' active in desktop slider).
[✓] Dell G15 'balanced' mode successfully activated.
```

### 4.4 Fan Boost Controls
Inspect fan RPMs or apply manual boost offsets:
```bash
# View fan status
dell-g15-thermal fan

# Set CPU fan boost to 50% and GPU fan boost to 50%
dell-g15-thermal fan 50 50

# Reset fan boost offsets to 0%
dell-g15-thermal fan 0 0
```

---

## 5. Desktop Environment Integration

### 5.1 KDE Plasma 6 Custom Shortcuts
To map the physical Game Shift key or custom hotkeys in KDE:
1. Open **System Settings** $\to$ **Shortcuts** $\to$ **Custom Shortcuts** (or **Command Shortcuts**).
2. Click **Add New** $\to$ **Global Shortcut** $\to$ **Command/URL**.
3. Name: `Toggle Game Shift (G-Mode)`
4. Trigger: Press `F9` (or `Fn+F9` depending on BIOS Fn lock).
5. Action: `/usr/local/bin/dell-g15-thermal gmode`
6. Add another shortcut for Performance mode:
   - Name: `Activate Performance Mode`
   - Trigger: `Meta + Shift + P`
   - Action: `/usr/local/bin/dell-g15-thermal mode performance`

### 5.2 GNOME Custom Keybindings
1. Open **Settings** $\to$ **Keyboard** $\to$ **Keyboard Shortcuts** $\to$ **View and Customize Shortcuts** $\to$ **Custom Shortcuts**.
2. Add Command: `/usr/local/bin/dell-g15-thermal gmode` with Shortcut `F9`.

---

## 6. Game Launchers & Automation

### 6.1 Steam Launch Options
To automatically engage True Performance mode when a game launches and revert to Balanced when closing:
```bash
dell-g15-thermal mode performance && %command% ; dell-g15-thermal mode balanced
```

### 6.2 Lutris Pre/Post-Launch Scripts
In Lutris Game Configuration $\to$ **System options**:
- **Pre-launch script**: `/usr/local/bin/dell-g15-thermal mode performance`
- **Post-exit script**: `/usr/local/bin/dell-g15-thermal mode balanced`

### 6.3 Feral Interactive GameMode (`gamemoded`)
Edit `/etc/gamemode.ini` or `~/.config/gamemode.ini`:
```ini
[custom]
start=dell-g15-thermal mode performance
end=dell-g15-thermal mode balanced
```

---

## 7. Troubleshooting & Diagnostics Checklist

| Symptom | Cause | Solution |
| :--- | :--- | :--- |
| **Fans roar at 5500 RPM on reboot** | `state.ini` restored `performance` | Verify `dell-g15-thermal.service` is enabled: `sudo systemctl enable --now dell-g15-thermal.service` |
| **Permission Denied writing to profile** | Udev rules not loaded | Run `sudo udevadm control --reload-rules && sudo udevadm trigger` |
| **`powerprofilesctl` missing** | Daemon not running | Run `sudo systemctl enable --now power-profiles-daemon.service` |
| **`alienware_wmi` not loaded** | Kernel module unloaded | Run `sudo modprobe alienware_wmi` and verify with `lsmod \| grep alienware_wmi` |
