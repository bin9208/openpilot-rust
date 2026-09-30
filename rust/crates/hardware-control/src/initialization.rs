use crate::{
    io_policy::{check_output, gpio_init, gpio_set, shell, strings, sudo_write},
    AmplifierAction, Command, Error, HardwareControl, Platform,
};

impl HardwareControl {
    pub fn set_power_save(
        &mut self,
        platform: &mut impl Platform,
        enabled: bool,
    ) -> Result<(), Error> {
        if self.model.is_none() {
            return Ok(());
        }
        if self.amplifier_enabled(platform)? {
            platform.amplifier(AmplifierAction::Shutdown(enabled))?;
            if !enabled {
                platform.amplifier(AmplifierAction::Initialize(
                    self.model.as_deref().unwrap_or(""),
                ))?;
            }
        }
        for core in 4..8 {
            sudo_write(
                platform,
                &format!("/sys/devices/system/cpu/cpu{core}/online"),
                if enabled { "0" } else { "1" },
            )?;
        }
        for policy in ["0", "4"] {
            if enabled && policy == "4" {
                continue;
            }
            sudo_write(
                platform,
                &format!("/sys/devices/system/cpu/cpufreq/policy{policy}/scaling_governor"),
                if enabled { "ondemand" } else { "performance" },
            )?;
            if !enabled {
                sudo_write(
                    platform,
                    &format!("/sys/devices/system/cpu/cpufreq/policy{policy}/scaling_max_freq"),
                    "1689600",
                )?;
            }
        }
        self.affine_irq(platform, 7, "kgsl-3d0")?;
        for action in [
            "a5",
            "cci",
            "cpas_camnoc",
            "cpas-cdm",
            "csid",
            "ife",
            "csid-lite",
            "ife-lite",
        ] {
            self.affine_irq(platform, 6, action)?;
        }
        Ok(())
    }

    pub fn initialize_hardware(&mut self, platform: &mut impl Platform) -> Result<(), Error> {
        if self.model.is_none() {
            return Ok(());
        }
        if self.amplifier_enabled(platform)? {
            platform.amplifier(AmplifierAction::Initialize(
                self.model.as_deref().unwrap_or(""),
            ))?;
        }
        shell(platform, "sudo chmod a+w /dev/kmsg")?;
        gpio_init(platform, 49, true)?;
        gpio_set(platform, 49, true)?;
        sudo_write(platform, "/proc/irq/default_smp_affinity", "f")?;
        self.affine_irq(platform, 1, "msm_vidc")?;
        self.affine_irq(platform, 1, "i2c_geni")?;
        self.affine_irq(platform, 5, "fts_ts")?;
        self.affine_irq(platform, 5, "msm_drm")?;
        for (name, value) in [
            ("min_pwrlevel", "1"),
            ("max_pwrlevel", "1"),
            ("force_bus_on", "1"),
            ("force_clk_on", "1"),
            ("force_rail_on", "1"),
            ("idle_timer", "1000"),
            ("devfreq/governor", "performance"),
            ("max_clock_mhz", "710"),
        ] {
            sudo_write(platform, &format!("/sys/class/kgsl/kgsl-3d0/{name}"), value)?;
        }
        for name in [
            "soc:qcom,cpubw",
            "soc:qcom,memlat-cpu0",
            "soc:qcom,memlat-cpu4",
        ] {
            sudo_write(
                platform,
                &format!("/sys/class/devfreq/{name}/governor"),
                "performance",
            )?;
        }
        sudo_write(platform, "/sys/kernel/debug/msm_vidc/clock_scaling", "N")?;
        sudo_write(
            platform,
            "/sys/kernel/debug/msm_vidc/disable_thermal_mitigation",
            "Y",
        )?;
        self.affine_irq(platform, 3, "spi_geni")?;
        match check_output(platform, &["pgrep", "-f", "spi0"]) {
            Ok(bytes) => {
                let pid =
                    String::from_utf8(bytes).map_err(|error| Error::Other(error.to_string()))?;
                let pid = pid.trim_matches(|c: char| {
                    c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)
                });
                platform.command(&Command::Call(strings(&[
                    "sudo", "chrt", "-f", "-p", "1", pid,
                ])))?;
                platform.command(&Command::Call(strings(&[
                    "sudo", "taskset", "-pc", "3", pid,
                ])))?;
            }
            Err(error @ Error::Command { .. }) => platform.print(&error.to_string())?,
            Err(error) => return Err(error),
        }
        Ok(())
    }
}
