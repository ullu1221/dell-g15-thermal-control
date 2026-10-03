# Deep Problem Analysis: Dell G15 5530 Thermal & Power Profile Architecture Under Linux

## 1. Executive Summary

When running Linux on modern Dell G-Series gaming laptops (specifically the **Dell G15 5530**, Raptor Lake HX + RTX 3050/4050/4060 Mobile), users encounter two severe, compounding issues:

1. **The Reboot Jet-Fan Scream (5500 RPM on startup)**: Upon rebooting, the laptop's Embedded Controller (EC) immediately engages Game Shift (G-Mode), pinning both CPU and GPU fans to 100% duty cycle, generating high acoustic noise even while idle on desktop.
2. **The Missing 4th Thermal Mode & Profile Collapse**: While the Windows Alienware Command Center (AWCC) provides **4 distinct thermal slider profiles** (Battery Saver, Quiet, Balanced, Performance) plus a separate Game Shift toggle, Linux only exposes **3 profiles**, and selecting "Performance" forces jet-engine fan scream instead of unlocking high power limits with dynamic acoustic curves.

This document presents a root-cause technical analysis based on kernel driver source code, module disassembly, ACPI bytecode traces, and D-Bus specifications.

---

## 2. Root Cause 1: The Reboot G-Mode Activation

### 2.1 State Persistence in `power-profiles-daemon`
Under Linux distributions running systemd (Arch Linux, Fedora, Ubuntu, Debian), power management is coordinated by `power-profiles-daemon` (PPD). PPD maintains state persistence across reboots via `/var/lib/power-profiles-daemon/state.ini`:

```ini
[State]
battery_aware=true
CpuDriver=intel_pstate
PlatformDriver=platform_profile
Profile=performance
```

If the system was placed into `performance` mode (either manually, via an optimization script, or automatically by a game launcher such as Lutris or Steam GameMode), PPD persists `Profile=performance`.

During the next system boot:
1. `power-profiles-daemon.service` starts at multi-user initialization.
2. It reads `Profile=performance` from `state.ini`.
3. It writes `"performance"` to the sysfs node `/sys/firmware/acpi/platform_profile`.

### 2.2 The Linux Kernel `alienware-wmi` Quirk
Under the hood, `/sys/firmware/acpi/platform_profile` dispatches the command to the registered platform profile handlers. For Dell G-Series laptops, the handler is `alienware-wmi` (`drivers/platform/x86/dell/alienware-wmi-wmax.c`).

In the kernel driver, Dell G-Series laptops match the `g_series_quirks` structure:

```c
static const struct quirk_entry g_series_quirks = {
    .num_zones = 1,
    .hdmi_mux = false,
    .gmode = true,   /* HARDCODED G-MODE QUIRK */
};
```

When probing platform profiles, the driver executes:
```c
if (quirks->gmode || force_gmode) {
    profile[PLATFORM_PROFILE_PERFORMANCE] = AWCC_PROFILE_SPECIAL_GMODE; /* 0xAB */
    set_bit(PLATFORM_PROFILE_PERFORMANCE, profile_choices);
}
```

And during profile activation (`awcc_platform_profile_set`):
```asm
; Disassembled from alienware-wmi.ko:
2012: cmp $0x5, %ebx          ; Check if target is PLATFORM_PROFILE_PERFORMANCE (index 5)
2015: je  2071                ; If performance, jump to G-Mode handler
...
2028: mov $0x25, %esi         ; WMAX Method 0x25 (Game Shift Status)
2036: movl $0x1, 0x14(%rsp)   ; Arg1 = 0x01 (ACTIVATE GAME SHIFT)
203e: call wmax_wmi_execute   ; Fire ACPI WMAX call to Embedded Controller!
```

### 2.3 The Consequence
Because `performance` is hard-linked to Game Shift (`0xab` / WMAX method `0x25, 0x01`), every time PPD boots up with `performance` in its state file, it fires the ACPI call to enable Game Shift. The hardware Embedded Controller instantly ramps both CPU and GPU fans to 100% duty cycle (~5500 RPM), creating an intolerable startup experience.

---

## 3. Root Cause 2: Kernel Multi-Driver Profile Collision

In modern Linux kernels (6.12+, 6.13, 7.x), the platform profile subsystem supports multiple concurrent drivers. On the Dell G15 5530, **two separate drivers** register platform profile providers:

1. **`platform-profile-0` (`alienware-wmi`)**:
   Exposed choices:
   ```
   low-power quiet balanced balanced-performance performance custom
   ```
   - `low-power`: AWCC Battery Saver (`0xa5`)
   - `quiet`: AWCC Quiet (`0xa3`)
   - `balanced`: AWCC Balanced (`0xa0`)
   - `balanced-performance`: AWCC Performance (`0xa1` — Full TDP/TGP Boost + **Dynamic Smart Fans**)
   - `performance`: AWCC Game Shift (`0xab` — **100% Fan Override**)

2. **`platform-profile-1` (`dell-pc`)**:
   Exposed choices:
   ```
   cool quiet balanced performance
   ```

### 3.1 The Intersection Collapse
The global kernel ACPI platform profile interface (`/sys/firmware/acpi/platform_profile_choices`) computes the **mathematical intersection** of all active profile handlers.

Because `dell-pc` only implements `{cool, quiet, balanced, performance}`:
$$\text{Intersection} = \{\text{alienware-wmi}\} \cap \{\text{dell-pc}\} = \{\text{quiet}, \text{balanced}, \text{performance}\}$$

The aggregate interface collapses down to only 3 choices:
```bash
$ cat /sys/firmware/acpi/platform_profile_choices
quiet balanced performance
```

Both `low-power` (`0xa5` Battery Saver) and `balanced-performance` (`0xa1` True Dynamic Performance) are completely hidden and stripped away from user-space tools!

---

## 4. Root Cause 3: The FreeDesktop D-Bus Specification Barrier

Desktop environments (KDE Plasma 6, GNOME 47, Cinnamon) control system power profiles via the FreeDesktop standard D-Bus interface:
- **Bus Name**: `net.hadess.PowerProfiles`
- **Object Path**: `/net/hadess/PowerProfiles`
- **Interface**: `net.hadess.PowerProfiles`

```bash
$ busctl introspect net.hadess.PowerProfiles /net/hadess/PowerProfiles net.hadess.PowerProfiles
NAME                  TYPE      SIGNATURE RESULT/VALUE
.ActiveProfile        property  s         "balanced"
.Profiles             property  aa{sv}    3 [ "power-saver", "balanced", "performance" ]
```

The FreeDesktop specification **strictly limits** power profiles to three enumerated strings:
1. `power-saver`
2. `balanced`
3. `performance`

### Why Desktop Sliders Cannot Display 4 Stops
The KDE Plasma battery applet slider (Powerdevil) and GNOME power menu do not query sysfs directly; they strictly bind to `net.hadess.PowerProfiles`. The UI widgets are hardcoded with a 3-way radio button or 3-stop slider.

If an administrator uninstalls `power-profiles-daemon` or replaces it with an incompatible daemon:
- The KDE Plasma battery slider vanishes or displays an error.
- Steam GameScope cannot manage performance states.
- Feral GameMode (`gamemoded`) fails to hold the performance profile during gameplay.
- Lutris game launch power switches fail.

---

## 5. Summary of the Architectural Dilemma

| Dimension | Windows AWCC | Standard Linux Kernel (Unmodified) | What Users Need |
| :--- | :--- | :--- | :--- |
| **Profile Stops** | 4 Stops on Slider + 1 Toggle | 3 Stops on Slider | 4 Modes Accessible + G-Mode Toggle |
| **Performance Behavior** | Full Boost + Dynamic Smart Fans (`0xa1`) | Jet Engine 100% Fans (`0xab`) | Full Boost + Dynamic Smart Fans (`0xa1`) |
| **G-Mode Trigger** | Dedicated F9 / Game Shift Key | Bound to standard "Performance" | Dedicated CLI / Key Shortcut Toggle |
| **Reboot Behavior** | Silent / Restores Quiet or Balanced | 100% Fan Roar on Boot | Silent / Safe Balanced Boot |
| **Desktop Compatibility**| Proprietary UWP UI | Standard PPD D-Bus API | 100% Unbroken Desktop Slider & Script API |

The solution requires a **multi-layer orchestration bridge** that satisfies the 3-state D-Bus contract while unlocking the 5 native hardware states directly at the WMAX ACPI layer.
