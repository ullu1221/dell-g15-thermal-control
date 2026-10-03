#!/usr/bin/env bash
# ==============================================================================
# Dell G15 5530 Thermal Controller & Power Profile Bridge Installer
# ==============================================================================

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

echo "================================================================="
echo " Dell G15 5530 Thermal Controller & PPD Bridge Installer"
echo "================================================================="

# User installation (no sudo required)
install_user() {
    echo "[*] Performing user installation into ~/.local/bin..."
    mkdir -p "$HOME/.local/bin"
    ln -sf "$SCRIPT_DIR/scripts/dell-g15-thermal" "$HOME/.local/bin/dell-g15-thermal"
    chmod +x "$SCRIPT_DIR/scripts/dell-g15-thermal"
    echo "  -> Linked: $HOME/.local/bin/dell-g15-thermal"

    if [[ ":$PATH:" != *":$HOME/.local/bin:"* ]]; then
        echo "  [!] Notice: ~/.local/bin is not in your current PATH. Add it to ~/.bashrc:"
        echo "      export PATH=\"\$HOME/.local/bin:\$PATH\""
    else
        echo "  [✓] ~/.local/bin is already in your PATH."
    fi

    echo ""
    echo "  -> Testing command:"
    "$HOME/.local/bin/dell-g15-thermal" status
    echo ""
    echo "================================================================="
    echo " [SUCCESS] Installed to ~/.local/bin/dell-g15-thermal"
    echo "================================================================="
    echo "[INFO] For system-wide udev rules (passwordless register writes) and the"
    echo "       boot initialization service, run: sudo ./install.sh --system"
}

# System installation (sudo required)
install_system() {
    if [[ $EUID -ne 0 ]]; then
        echo "[INFO] Elevation required for system installation. Re-running with sudo..."
        exec sudo "$0" "--system"
    fi

    echo "[1/4] Installing /usr/local/bin/dell-g15-thermal..."
    install -m 755 "$SCRIPT_DIR/scripts/dell-g15-thermal" /usr/local/bin/dell-g15-thermal

    # Also link to user's .local/bin if SUDO_USER is set
    if [[ -n "${SUDO_USER:-}" && "$SUDO_USER" != "root" ]]; then
        USER_HOME=$(getent passwd "$SUDO_USER" | cut -d: -f6)
        if [[ -d "$USER_HOME" ]]; then
            mkdir -p "$USER_HOME/.local/bin"
            ln -sf /usr/local/bin/dell-g15-thermal "$USER_HOME/.local/bin/dell-g15-thermal"
            chown -h "$SUDO_USER:$SUDO_USER" "$USER_HOME/.local/bin/dell-g15-thermal" 2>/dev/null || true
            echo "  -> Linked /usr/local/bin/dell-g15-thermal to $USER_HOME/.local/bin/dell-g15-thermal"
        fi
    fi

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

    echo "[3/4] Installing boot initialization service..."
    install -m 644 "$SCRIPT_DIR/system/dell-g15-thermal.service" /etc/systemd/system/dell-g15-thermal.service
    systemctl daemon-reload
    systemctl enable --now dell-g15-thermal.service

    echo "[4/4] Setting power-profiles-daemon default to balanced..."
    if command -v powerprofilesctl &>/dev/null; then
        powerprofilesctl set balanced 2>/dev/null || true
    fi

    echo "================================================================="
    echo " [SUCCESS] System-wide thermal bridge installed and active!"
    echo "================================================================="
    echo ""
    /usr/local/bin/dell-g15-thermal status
}

case "${1:-}" in
    --user)
        install_user
        ;;
    --system)
        install_system
        ;;
    *)
        if [[ $EUID -eq 0 ]]; then
            install_system
        else
            install_user
        fi
        ;;
esac
