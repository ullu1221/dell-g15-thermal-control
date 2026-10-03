#!/usr/bin/env bash
# ==============================================================================
# Dell G15 5530 Thermal Controller & Power Profile Bridge Installer
# ==============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "================================================================="
echo " Installing Dell G15 5530 Thermal Controller & PPD Bridge"
echo "================================================================="

if [[ $EUID -ne 0 ]]; then
    echo "[INFO] Elevation required. Re-running with sudo..."
    exec sudo "$0" "$@"
fi

# 1. Install CLI binary
echo "[1/4] Installing /usr/local/bin/dell-g15-thermal..."
install -m 755 "$SCRIPT_DIR/scripts/dell-g15-thermal" /usr/local/bin/dell-g15-thermal

# 2. Install Udev Rules
echo "[2/4] Installing Udev rules for unprivileged hardware register access..."
install -m 644 "$SCRIPT_DIR/system/99-dell-g15-thermal.rules" /etc/udev/rules.d/99-dell-g15-thermal.rules
udevadm control --reload-rules 2>/dev/null || true
udevadm trigger --subsystem-match=platform-profile --subsystem-match=hwmon 2>/dev/null || true

# Grant immediate permissions to group wheel for existing files
if [[ -f "/sys/class/platform-profile/platform-profile-0/profile" ]]; then
    chmod 664 /sys/class/platform-profile/platform-profile-0/profile || true
    chgrp wheel /sys/class/platform-profile/platform-profile-0/profile || true
fi

for f in /sys/class/platform-profile/platform-profile-0/device/hwmon/hwmon*/fan*_boost; do
    if [[ -f "$f" ]]; then
        chmod 664 "$f" || true
        chgrp wheel "$f" || true
    fi
done

# 3. Install and Enable Systemd Service
echo "[3/4] Installing boot initialization service..."
install -m 644 "$SCRIPT_DIR/system/dell-g15-thermal.service" /etc/systemd/system/dell-g15-thermal.service
systemctl daemon-reload
systemctl enable --now dell-g15-thermal.service

# 4. Verify power-profiles-daemon state persistence
echo "[4/4] Setting power-profiles-daemon default to balanced..."
if command -v powerprofilesctl &>/dev/null; then
    powerprofilesctl set balanced 2>/dev/null || true
fi

echo "================================================================="
echo " [SUCCESS] Dell G15 5530 Thermal Control Installed Successfully!"
echo "================================================================="
echo ""
/usr/local/bin/dell-g15-thermal status
