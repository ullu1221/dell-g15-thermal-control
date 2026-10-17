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

    if command -v cargo &>/dev/null && [[ -d "$SCRIPT_DIR/native" ]]; then
        echo "[*] Compiling High-Risk HPC Static Native Engine (g15-sentinel)..."
        TARGET_FLAG=""
        BIN_PATH="$SCRIPT_DIR/native/target/release/g15-thermal-sentinel"
        if rustup target list 2>/dev/null | grep -q "x86_64-unknown-linux-musl (installed)"; then
            TARGET_FLAG="--target x86_64-unknown-linux-musl"
            BIN_PATH="$SCRIPT_DIR/native/target/x86_64-unknown-linux-musl/release/g15-thermal-sentinel"
        fi
        (cd "$SCRIPT_DIR/native" && cargo build --release $TARGET_FLAG -q) || true
        if [[ -f "$BIN_PATH" ]]; then
            cp "$BIN_PATH" "$HOME/.local/bin/g15-sentinel"
            echo "  -> Installed High-Risk HPC Binary: $HOME/.local/bin/g15-sentinel"
        fi
    fi

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

    if command -v cargo &>/dev/null && [[ -d "$SCRIPT_DIR/native" ]]; then
        echo "      Compiling & installing /usr/local/bin/g15-sentinel (HPC Native Engine)..."
        TARGET_FLAG=""
        BIN_PATH="$SCRIPT_DIR/native/target/release/g15-thermal-sentinel"
        if rustup target list 2>/dev/null | grep -q "x86_64-unknown-linux-musl (installed)"; then
            TARGET_FLAG="--target x86_64-unknown-linux-musl"
            BIN_PATH="$SCRIPT_DIR/native/target/x86_64-unknown-linux-musl/release/g15-thermal-sentinel"
        fi
        (cd "$SCRIPT_DIR/native" && cargo build --release $TARGET_FLAG -q) || true
        if [[ -f "$BIN_PATH" ]]; then
            install -m 755 "$BIN_PATH" /usr/local/bin/g15-sentinel
            echo "  -> Installed /usr/local/bin/g15-sentinel"
        fi
    elif [[ -f "/usr/local/bin/g15-sentinel" ]]; then
        echo "      Detected existing /usr/local/bin/g15-sentinel (HPC Native Engine active)."
    elif [[ -n "${SUDO_USER:-}" ]] && [[ -f "$(getent passwd "$SUDO_USER" | cut -d: -f6)/.local/bin/g15-sentinel" ]]; then
        USER_BIN="$(getent passwd "$SUDO_USER" | cut -d: -f6)/.local/bin/g15-sentinel"
        install -m 755 "$USER_BIN" /usr/local/bin/g15-sentinel
        echo "  -> Installed pre-existing $USER_BIN to /usr/local/bin/g15-sentinel"
    fi

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

    for f in /sys/devices/system/cpu/cpu*/cpufreq/scaling_governor /sys/devices/system/cpu/cpu*/cpufreq/energy_performance_preference; do
        if [[ -f "$f" ]]; then
            chmod 664 "$f" || true
            chgrp wheel "$f" || true
        fi
    done

    echo "[3/4] Installing boot initialization service..."
    install -m 644 "$SCRIPT_DIR/system/dell-g15-thermal.service" /etc/systemd/system/dell-g15-thermal.service
    systemctl daemon-reload
    systemctl enable --now dell-g15-thermal.service

    echo "[4/4] Restoring saved thermal state & power profile..."
    if [[ -x /usr/local/bin/g15-sentinel ]]; then
        /usr/local/bin/g15-sentinel restore 2>/dev/null || true
    elif [[ -x /usr/local/bin/dell-g15-thermal ]]; then
        /usr/local/bin/dell-g15-thermal restore 2>/dev/null || true
    elif command -v powerprofilesctl &>/dev/null; then
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
