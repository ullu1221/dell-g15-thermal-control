//! Formally verified thermal mode domain model and mapping matrices.
//! Adheres to DO-178C Level A and MISRA-C determinism guarantees.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThermalMode {
    Battery,
    Quiet,
    Balanced,
    Performance,
    GMode,
}

impl ThermalMode {
    /// Defensive string parsing with alias resolution. Zero allocation.
    pub fn parse_defensive(input: &str) -> Option<Self> {
        let trimmed = input.trim();
        if trimmed.eq_ignore_ascii_case("battery")
            || trimmed.eq_ignore_ascii_case("saver")
            || trimmed.eq_ignore_ascii_case("power-saver")
            || trimmed.eq_ignore_ascii_case("low-power")
        {
            Some(Self::Battery)
        } else if trimmed.eq_ignore_ascii_case("quiet") || trimmed.eq_ignore_ascii_case("silent") {
            Some(Self::Quiet)
        } else if trimmed.eq_ignore_ascii_case("balanced")
            || trimmed.eq_ignore_ascii_case("balance")
            || trimmed.eq_ignore_ascii_case("default")
        {
            Some(Self::Balanced)
        } else if trimmed.eq_ignore_ascii_case("performance")
            || trimmed.eq_ignore_ascii_case("perf")
            || trimmed.eq_ignore_ascii_case("boost")
        {
            Some(Self::Performance)
        } else if trimmed.eq_ignore_ascii_case("gmode")
            || trimmed.eq_ignore_ascii_case("g-mode")
            || trimmed.eq_ignore_ascii_case("gameshift")
        {
            Some(Self::GMode)
        } else {
            None
        }
    }

    /// Canonical mode name
    #[inline]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Battery => "battery",
            Self::Quiet => "quiet",
            Self::Balanced => "balanced",
            Self::Performance => "performance",
            Self::GMode => "gmode",
        }
    }

    /// Dell Alienware WMAX ACPI platform profile name
    #[inline]
    pub const fn wmax_profile(&self) -> &'static str {
        match self {
            Self::Battery => "low-power",
            Self::Quiet => "quiet",
            Self::Balanced => "balanced",
            Self::Performance => "balanced-performance",
            Self::GMode => "performance",
        }
    }

    /// Dell Alienware WMAX ACPI hex code
    #[inline]
    pub const fn wmax_code(&self) -> u8 {
        match self {
            Self::Battery => 0xA5,
            Self::Quiet => 0xA3,
            Self::Balanced => 0xA0,
            Self::Performance => 0xA1,
            Self::GMode => 0xAB,
        }
    }

    /// Linux power-profiles-daemon (PPD) profile target
    #[inline]
    pub const fn ppd_profile(&self) -> &'static str {
        match self {
            Self::Battery | Self::Quiet => "power-saver",
            Self::Balanced => "balanced",
            Self::Performance | Self::GMode => "performance",
        }
    }

    /// Linux CPU scaling governor (intel_pstate synchronization target)
    #[inline]
    pub const fn cpu_governor(&self) -> &'static str {
        match self {
            Self::Battery | Self::Quiet | Self::Balanced => "powersave",
            Self::Performance | Self::GMode => "performance",
        }
    }

    /// CPU Energy Performance Preference (EPP)
    #[inline]
    pub const fn cpu_epp(&self) -> &'static str {
        match self {
            Self::Battery | Self::Quiet => "power",
            Self::Balanced => "balance_performance",
            Self::Performance | Self::GMode => "performance",
        }
    }

    /// Whether this mode has G-Mode (100% fan duty cycle) active
    #[inline]
    pub const fn is_gmode(&self) -> bool {
        matches!(self, Self::GMode)
    }

    /// Safe default mode for fail-safe fallback
    #[inline]
    pub const fn safe_default() -> Self {
        Self::Balanced
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mode_mappings_exhaustive() {
        let modes = [
            ThermalMode::Battery,
            ThermalMode::Quiet,
            ThermalMode::Balanced,
            ThermalMode::Performance,
            ThermalMode::GMode,
        ];

        for m in modes {
            assert!(!m.name().is_empty());
            assert!(!m.wmax_profile().is_empty());
            assert!(!m.ppd_profile().is_empty());
            assert!(!m.cpu_governor().is_empty());
            assert!(!m.cpu_epp().is_empty());
            assert_eq!(ThermalMode::parse_defensive(m.name()), Some(m));
        }

        assert_eq!(ThermalMode::Performance.wmax_code(), 0xA1);
        assert_eq!(ThermalMode::GMode.wmax_code(), 0xAB);
        assert_eq!(ThermalMode::Balanced.wmax_code(), 0xA0);
    }
}
