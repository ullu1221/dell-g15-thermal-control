# Alienware Command Center (AWCC) Reverse-Engineering: Dell G15 5530

## 1. Overview & Database Extraction

By inspecting the reverse-engineered Alienware Command Center Linux project (`AWCC-main`), we obtain the exact proprietary hardware specifications and ACPI WMI commands utilized by Dell firmware for the **Dell G15 5530**.

In `database.json`, the Dell G15 5530 device definition is configured as:

```json
"Dell G15 5530": {
    "featureSet": "1111111",
    "thermalModes": "11001111",
    "lightingModes": "111111"
}
```

### 1.1 Device Identification & ACPI Scope
- **Device DMI Name**: `Dell G15 5530`
- **ACPI WMAX Scope**: `\_SB.AMWW.WMAX` (Intel Raptor Lake platform)
- **WMI GUID**: `A70591CE-A997-11DA-B012-B622A1EF5492`

---

## 2. Thermal Mode Bitmask Decoding

In `AWCC-main/include/database.h`, the 8-bit thermal bitmask enum is defined as:

```cpp
enum class ThermalModeSet : std::uint8_t {
    Quiet        = 0b00000001, // bit 0 (0x01)
    Balanced     = 0b00000010, // bit 1 (0x02)
    Performance  = 0b00000100, // bit 2 (0x04)
    BatterySaver = 0b00001000, // bit 3 (0x08)
    Cool         = 0b00010000, // bit 4 (0x10) - Unsupported on G15 5530
    FullSpeed    = 0b00100000, // bit 5 (0x20) - Legacy / Unsupported
    GMode        = 0b01000000, // bit 6 (0x40)
    Manual       = 0b10000000  // bit 7 (0x80)
};
```

When decoded against `"thermalModes": "11001111"` (parsed as MSB $\to$ LSB: `[Manual, GMode, FullSpeed, Cool, BatterySaver, Performance, Balanced, Quiet]`):

| Mode | Bit Index | Supported? | ACPI Set Code | ACPI Get Code | Linux Sysfs Mapping | Acoustic Curve |
| :--- | :---: | :---: | :---: | :---: | :--- | :--- |
| **Battery Saver** | 3 | **YES** | `0xA5` | `0xA5` | `low-power` | Silent, 0 RPM idle |
| **Quiet** | 0 | **YES** | `0xA3` | `0xA3` | `quiet` | Whisper (~800–1200 RPM) |
| **Balanced** | 1 | **YES** | `0xA0` | `0xA0` | `balanced` | Adaptive Dynamic |
| **Performance** | 2 | **YES** | `0xA1` | `0xA1` | `balanced-performance` | **Full Power + Smart Fan** |
| **Game Shift (G-Mode)** | 6 | **YES** | `0xAB` | `0xAB` | `performance` | **100% Locked Overdrive** |
| **Manual** | 7 | **YES** | `0x00` | `0x00` | `custom` | Direct Fan Boost Offset |
| **Cool** | 4 | NO | `0xA2` | `0xA2` | N/A | N/A |
| **Full Speed** | 5 | NO | `0xA4` | `0xA4` | N/A | N/A |

---

## 3. ACPI WMAX Bytecode Specifications

The AWCC software communicates with the firmware Embedded Controller using ACPI method calls sent to `\_SB.AMWW.WMAX`.

### 3.1 Method 0x14: Query Operations (Get)
```
\_SB.AMWW.WMAX 0 0x14 {Arg1, Arg2, 0x00, 0x00}
```
- **Query Active Thermal Mode**:
  `{0x0B, 0x00, 0x00, 0x00}` $\to$ Returns current mode code (`0xA0`, `0xA1`, `0xA3`, `0xA5`, `0xAB`).
- **Query CPU Fan Boost Offset**:
  `{0x0C, 0x32, 0x00, 0x00}` $\to$ Returns CPU fan boost percentage (`0` to `100`).
- **Query GPU Fan Boost Offset**:
  `{0x0C, 0x33, 0x00, 0x00}` $\to$ Returns GPU fan boost percentage (`0` to `100`).

### 3.2 Method 0x15: Control Operations (Set)
```
\_SB.AMWW.WMAX 0 0x15 {Arg1, Arg2, Arg3, 0x00}
```
- **Set Thermal Mode**:
  `{0x01, ModeCode, 0x00, 0x00}`
  - Example: `{0x01, 0xA1, 0x00, 0x00}` sets Performance mode.
- **Set CPU Fan Boost Offset**:
  `{0x02, 0x32, BoostValue, 0x00}`
  - BoostValue ranges from `0` to `100`.
- **Set GPU Fan Boost Offset**:
  `{0x02, 0x33, BoostValue, 0x00}`
  - BoostValue ranges from `0` to `100`.

### 3.3 Method 0x25: Game Shift (G-Mode) Toggle
Game Shift is a dedicated hardware override method in the Dell EC:
```
\_SB.AMWW.WMAX 0 0x25 {Arg1, 0x00, 0x00, 0x00}
```
- **Activate Game Shift**: `{0x01, 0x00, 0x00, 0x00}`
  - Signals the EC to ramp fans to maximum duty cycle (~5500 RPM) and lock high power thresholds.
- **Deactivate Game Shift**: `{0x00, 0x00, 0x00, 0x00}`
  - Releases the 100% duty cycle lock, returning fans to the active thermal profile's curve.
- **Query Game Shift Status**: `{0x02, 0x00, 0x00, 0x00}`
  - Returns `0x01` if Game Shift is active, `0x00` if inactive.

---

## 4. Modern Linux Kernel Native Sysfs Translation

While AWCC on Linux historically required the out-of-tree `acpi_call` kernel module, modern Linux kernels (6.12+ through 7.x) feature an in-tree driver (`alienware-wmi`) that registers these interfaces directly into standard Linux sysfs subsystems.

### 4.1 Platform Profile Sysfs Node
Location: `/sys/class/platform-profile/platform-profile-0/profile`

Writing to this node executes the exact ACPI WMAX calls:
- `echo "low-power" > profile` $\longrightarrow$ Method `0x15, 0x01, 0xA5`
- `echo "quiet" > profile` $\longrightarrow$ Method `0x15, 0x01, 0xA3`
- `echo "balanced" > profile` $\longrightarrow$ Method `0x15, 0x01, 0xA0`
- `echo "balanced-performance" > profile` $\longrightarrow$ Method `0x15, 0x01, 0xA1` (**No fan scream!**)
- `echo "performance" > profile` $\longrightarrow$ Method `0x25, 0x01` (**Game Shift 100% fans**)

### 4.2 Fan Telemetry & Fan Boost Sysfs Nodes
Location: `/sys/class/platform-profile/platform-profile-0/device/hwmon/hwmon4/` (or `/sys/class/hwmon/hwmon*` matching `alienware_wmi`):

| Sysfs Attribute | Type | Description |
| :--- | :---: | :--- |
| `fan1_label` | Read-only | Returns `"CPU Fan"` |
| `fan1_input` | Read-only | Current CPU Fan speed in **RPM** (e.g. `1420`) |
| `fan1_boost` | Read/Write | CPU Fan boost offset (`0` to `100`) $\to$ fires Method `0x15, 0x02, 0x32` |
| `fan2_label` | Read-only | Returns `"GPU Fan"` |
| `fan2_input` | Read-only | Current GPU Fan speed in **RPM** (e.g. `1650`) |
| `fan2_boost` | Read/Write | GPU Fan boost offset (`0` to `100`) $\to$ fires Method `0x15, 0x02, 0x33` |
| `temp1_input` | Read-only | CPU package temperature in millidegrees Celsius (e.g. `58000` $\to$ 58°C) |
| `temp2_input` | Read-only | GPU die temperature in millidegrees Celsius (e.g. `45000` $\to$ 45°C) |

No `acpi_call` module or custom kernel compilation is required. The entire Dell AWCC hardware suite is accessible through standard sysfs file I/O operations when appropriate udev permissions are granted.
