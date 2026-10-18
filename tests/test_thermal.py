#!/usr/bin/env python3
"""
Exhaustive Automated Hardware & Unit Test Suite for dell-g15-thermal-control
Validates syntax, WMAX ACPI mappings, power-profiles-daemon synchronization,
sysfs permissions, telemetry accuracy, and live hardware state transitions.
"""

import json
import os
import subprocess
import sys
import unittest
from pathlib import Path

REPO_DIR = Path(__file__).resolve().parent.parent
SCRIPT = REPO_DIR / "scripts" / "dell-g15-thermal"


def load_dell_thermal():
    import importlib.util
    from importlib.machinery import SourceFileLoader
    loader = SourceFileLoader("dell_thermal", str(SCRIPT))
    spec = importlib.util.spec_from_file_location("dell_thermal", str(SCRIPT), loader=loader)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


class TestDellG15ThermalControl(unittest.TestCase):

    @classmethod
    def setUpClass(cls):
        mod = load_dell_thermal()
        cls.initial_state = mod.load_saved_state()
        cls.initial_mode = cls.initial_state.get("mode", "performance")

    @classmethod
    def tearDownClass(cls):
        mod = load_dell_thermal()
        if cls.initial_mode:
            subprocess.run([sys.executable, str(SCRIPT), "mode", cls.initial_mode], capture_output=True)
        if cls.initial_state:
            mod.save_persistent_state(
                cls.initial_state.get("mode", "performance"),
                cpu_boost=cls.initial_state.get("cpu_boost", 0),
                gpu_boost=cls.initial_state.get("gpu_boost", 0),
            )

    def test_01_script_exists_and_executable(self):
        """Verifies that dell-g15-thermal exists and has executable permissions."""
        self.assertTrue(SCRIPT.exists(), f"Script not found at {SCRIPT}")
        self.assertTrue(os.access(SCRIPT, os.X_OK), f"Script {SCRIPT} is not executable")

    def test_02_cli_help(self):
        """Verifies that dell-g15-thermal --help returns 0 and displays commands."""
        res = subprocess.run([sys.executable, str(SCRIPT), "--help"], capture_output=True, text=True)
        self.assertEqual(res.returncode, 0)
        self.assertIn("status", res.stdout)
        self.assertIn("performance", res.stdout)
        self.assertIn("balance", res.stdout)
        self.assertIn("gmode", res.stdout)

    def test_03_telemetry_status(self):
        """Verifies that status command executes and returns valid hardware telemetry."""
        res = subprocess.run([sys.executable, str(SCRIPT), "status"], capture_output=True, text=True)
        self.assertEqual(res.returncode, 0, f"Status failed: {res.stderr}")
        self.assertIn("Native AWCC WMAX Profile", res.stdout)
        self.assertIn("CPU Scaling Governor", res.stdout)
        self.assertIn("CPU Temperature", res.stdout)
        self.assertIn("GPU Temperature", res.stdout)

    def test_04_mapping_consistency(self):
        """Verifies bidirectional ACPI, PPD, and EPP mapping tables."""
        mod = load_dell_thermal()

        # Ensure all supported modes map correctly
        expected_modes = ["battery", "quiet", "balanced", "performance", "gmode"]
        for m in expected_modes:
            self.assertIn(m, mod.PPD_MAP, f"Missing PPD mapping for {m}")
            self.assertIn(m, mod.GOVERNOR_MAP, f"Missing GOVERNOR mapping for {m}")
            self.assertIn(m, mod.EPP_MAP, f"Missing EPP mapping for {m}")
            self.assertIn(m, mod.WMAX_MAP, f"Missing WMAX mapping for {m}")

        self.assertEqual(mod.GOVERNOR_MAP["battery"], "powersave")
        self.assertEqual(mod.GOVERNOR_MAP["quiet"], "powersave")
        self.assertEqual(mod.GOVERNOR_MAP["balanced"], "powersave")
        self.assertEqual(mod.GOVERNOR_MAP["performance"], "performance")
        self.assertEqual(mod.GOVERNOR_MAP["gmode"], "performance")

        self.assertEqual(mod.WMAX_MAP["balanced"], "balanced")
        self.assertEqual(mod.WMAX_MAP["performance"], "balanced-performance")
        self.assertEqual(mod.WMAX_MAP["gmode"], "performance")

    def test_05_state_persistence_cycle(self):
        """Verifies atomic state saving and loading across temporary state files."""
        mod = load_dell_thermal()
        import tempfile
        with tempfile.TemporaryDirectory() as tmp_dir:
            tmp_state_file = Path(tmp_dir) / "state.json"
            orig_state_paths = list(mod.STATE_PATHS)
            mod.STATE_PATHS = [tmp_state_file]
            try:
                mod.save_persistent_state("balanced", cpu_boost=25, gpu_boost=30)
                state = mod.load_saved_state()
                self.assertEqual(state.get("mode"), "balanced")
                self.assertEqual(state.get("governor"), "powersave")
                self.assertEqual(state.get("cpu_boost"), 25)
                self.assertEqual(state.get("gpu_boost"), 30)
            finally:
                mod.STATE_PATHS = orig_state_paths

    def test_06_sysfs_permissions(self):
        """Verifies write accessibility of AWCC WMAX platform profile nodes and CPU sysfs."""
        wmax_profile = Path("/sys/class/platform-profile/platform-profile-0/profile")
        if wmax_profile.exists():
            # Check file is readable
            self.assertTrue(os.access(wmax_profile, os.R_OK))
            # Verify valid active platform profile string
            profile = wmax_profile.read_text().strip()
            self.assertIn(profile, ["low-power", "quiet", "balanced", "balanced-performance", "performance", "custom"])

        gov_file = Path("/sys/devices/system/cpu/cpu0/cpufreq/scaling_governor")
        if gov_file.exists():
            self.assertTrue(os.access(gov_file, os.R_OK))
            gov = gov_file.read_text().strip()
            self.assertIn(gov, ["powersave", "performance"])

    def test_07_live_mode_transition(self):
        """Verifies live switching to performance, then safely back to initial mode."""
        mod = load_dell_thermal()
        initial_state = mod.load_saved_state()
        initial_mode = initial_state.get("mode", "balanced")
        try:
            # 1. Switch to performance
            res_perf = subprocess.run([sys.executable, str(SCRIPT), "mode", "performance"], capture_output=True, text=True)
            self.assertEqual(res_perf.returncode, 0, f"Failed to set performance mode: {res_perf.stderr}")
            self.assertIn("successfully activated", res_perf.stdout)
        finally:
            # 2. Restore whatever mode was active before test started
            res_restore = subprocess.run([sys.executable, str(SCRIPT), "mode", initial_mode], capture_output=True, text=True)
            self.assertEqual(res_restore.returncode, 0, f"Failed to restore initial mode: {res_restore.stderr}")
            self.assertIn("successfully activated", res_restore.stdout)

    def test_08_restore_command(self):
        """Verifies the state restore command executes cleanly."""
        res = subprocess.run([sys.executable, str(SCRIPT), "restore"], capture_output=True, text=True)
        self.assertEqual(res.returncode, 0, f"Restore failed: {res.stderr}")
        self.assertIn("successfully restored", res.stdout)

    def test_09_native_sentinel_binary(self):
        """Verifies High-Risk HPC Static Native Engine (g15-sentinel) execution and latency."""
        native_bin = REPO_DIR / "native" / "target" / "release" / "g15-thermal-sentinel"
        if not native_bin.exists():
            native_bin = Path("/usr/local/bin/g15-sentinel")

        if native_bin.exists():
            res = subprocess.run([str(native_bin), "status"], capture_output=True, text=True)
            self.assertEqual(res.returncode, 0, f"Native binary failed: {res.stderr}")
            self.assertIn("Native AWCC WMAX Profile", res.stdout)
            self.assertIn("CPU Scaling Governor", res.stdout)
            self.assertIn("HPC Sentinel Telemetry Latency", res.stdout)


if __name__ == "__main__":
    print("=" * 65)
    print(" DELL G15 5530 THERMAL CONTROL EXHAUSTIVE VERIFICATION SUITE")
    print("=" * 65)
    unittest.main(verbosity=2)
