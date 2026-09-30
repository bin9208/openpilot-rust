use crate::{
    io_policy::{check_output, gpio_init, gpio_set, shell},
    Error, HardwareControl, Platform,
};

const BACKLIGHT: &str = "/sys/class/backlight/panel0-backlight";

fn integer(value: f64) -> Result<String, Error> {
    if !value.is_finite() {
        return Err(Error::Other(
            "cannot convert non-finite float to integer".into(),
        ));
    }
    Ok(if value.abs() < 1.0 {
        "0".into()
    } else {
        format!("{:.0}", value.trunc())
    })
}

impl HardwareControl {
    pub fn set_display_power(&self, platform: &mut impl Platform, on: bool) {
        if self.model.is_some() {
            match platform.write(&format!("{BACKLIGHT}/bl_power"), if on { "0" } else { "4" }) {
                Ok(()) | Err(_) => {}
            }
        }
    }
    pub fn set_screen_brightness(&self, platform: &mut impl Platform, percentage: f64) {
        if self.model.is_none() {
            return;
        }
        let result = (|| {
            let text = platform.read(&format!("{BACKLIGHT}/max_brightness"))?;
            let maximum = text
                .trim()
                .parse::<f64>()
                .map_err(|error| Error::Other(error.to_string()))?;
            platform.write(
                &format!("{BACKLIGHT}/brightness"),
                &integer(percentage * (maximum / 100.0))?,
            )
        })();
        match result {
            Ok(()) | Err(_) => {}
        }
    }
    pub fn set_ir_power(&self, platform: &mut impl Platform, percent: i32) -> Result<(), Error> {
        if self.model.is_none() || self.model.as_deref() == Some("tizi") {
            return Ok(());
        }
        let value = integer((f64::from(percent) / 100.0) * 300.0)?;
        platform.write("/sys/class/leds/led:switch_2/brightness", "0\n")?;
        platform.write(
            "/sys/class/leds/led:torch_2/brightness",
            &format!("{value}\n"),
        )?;
        platform.write(
            "/sys/class/leds/led:switch_2/brightness",
            &format!("{value}\n"),
        )
    }
    pub fn reboot(&self, platform: &mut impl Platform) -> Result<(), Error> {
        if self.model.is_none() {
            return platform.print("REBOOT!");
        }
        check_output(platform, &["sudo", "reboot"])?;
        Ok(())
    }
    pub fn shutdown(&self, platform: &mut impl Platform) -> Result<(), Error> {
        if self.model.is_none() {
            return platform.print("SHUTDOWN!");
        }
        shell(platform, "sudo poweroff")
    }
    pub fn uninstall(&self, platform: &mut impl Platform) -> Result<(), Error> {
        if self.model.is_none() {
            return platform.print("uninstall");
        }
        platform.touch("/data/__system_reset__")?;
        platform.sync()?;
        self.reboot(platform)
    }
    pub fn reset_internal_panda(&self, platform: &mut impl Platform) -> Result<(), Error> {
        if self.model.is_none() {
            return Ok(());
        }
        gpio_init(platform, 124, true)?;
        gpio_init(platform, 134, true)?;
        gpio_set(platform, 124, true)?;
        gpio_set(platform, 134, false)?;
        platform.sleep(0.01)?;
        gpio_set(platform, 124, false)
    }
    pub fn recover_internal_panda(&self, platform: &mut impl Platform) -> Result<(), Error> {
        if self.model.is_none() {
            return Ok(());
        }
        gpio_init(platform, 124, true)?;
        gpio_init(platform, 134, true)?;
        gpio_set(platform, 124, true)?;
        gpio_set(platform, 134, true)?;
        platform.sleep(0.01)?;
        gpio_set(platform, 124, false)?;
        platform.sleep(0.01)?;
        gpio_set(platform, 134, false)
    }
    pub fn booted(&self, platform: &mut impl Platform) -> Result<bool, Error> {
        if self.model.is_none() {
            return Ok(true);
        }
        let state = match check_output(
            platform,
            &[
                "/bin/sh",
                "-c",
                "sudo cat /sys/kernel/debug/msm_vidc/core0/info",
            ],
        ) {
            Ok(bytes) => String::from_utf8(bytes).unwrap_or_default(),
            Err(_) => String::new(),
        };
        Ok(!state.contains("Core state: 0") || platform.monotonic()? >= 120.0)
    }
}
