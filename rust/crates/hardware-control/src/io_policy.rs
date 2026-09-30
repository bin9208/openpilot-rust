use crate::{Command, Error, HardwareControl, Platform};

pub(crate) fn strings(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}

pub(crate) fn check_output(platform: &mut impl Platform, argv: &[&str]) -> Result<Vec<u8>, Error> {
    let argv = strings(argv);
    let output = platform.command(&Command::Output(argv.clone()))?;
    if output.status != 0 {
        return Err(Error::Command {
            argv,
            status: output.status,
        });
    }
    Ok(output.stdout)
}

pub(crate) fn shell(platform: &mut impl Platform, command: &str) -> Result<(), Error> {
    platform.command(&Command::Shell(command.into()))?;
    Ok(())
}

pub fn sudo_write(platform: &mut impl Platform, path: &str, value: &str) -> Result<(), Error> {
    match platform.write(path, value) {
        Ok(()) => Ok(()),
        Err(error) if error.is_permission() => {
            shell(platform, &format!("sudo chmod a+w {path}"))?;
            match platform.write(path, value) {
                Ok(()) => Ok(()),
                Err(error) if error.is_permission() => {
                    shell(platform, &format!("sudo su -c 'echo {value} > {path}'"))
                }
                Err(error) => Err(error),
            }
        }
        Err(error) => Err(error),
    }
}

pub fn gpio_init(platform: &mut impl Platform, pin: u32, output: bool) -> Result<(), Error> {
    let path = format!("/sys/class/gpio/gpio{pin}/direction");
    match platform.write(&path, if output { "out" } else { "in" }) {
        Ok(()) => Ok(()),
        Err(error) => platform.print(&format!("Failed to set gpio {pin} direction: {error}")),
    }
}

pub fn gpio_set(platform: &mut impl Platform, pin: u32, high: bool) -> Result<(), Error> {
    let path = format!("/sys/class/gpio/gpio{pin}/value");
    match platform.write(&path, if high { "1" } else { "0" }) {
        Ok(()) => Ok(()),
        Err(error) => platform.print(&format!("Failed to set gpio {pin} value: {error}")),
    }
}

impl HardwareControl {
    pub fn affine_irq(
        &mut self,
        platform: &mut impl Platform,
        core: u8,
        action: &str,
    ) -> Result<(), Error> {
        let interrupts = platform.read("/proc/interrupts")?;
        let mut matches = Vec::new();
        for line in interrupts.lines() {
            let irq = line.split(':').next().unwrap_or("").trim();
            if irq.is_empty() || !irq.bytes().all(|byte| byte.is_ascii_digit()) {
                continue;
            }
            if !self.irq_actions.contains_key(irq) {
                let actions = match platform.read(&format!("/sys/kernel/irq/{irq}/actions")) {
                    Ok(text) => text.trim().split(',').map(Into::into).collect(),
                    Err(error) if error.is_missing() => Vec::new(),
                    Err(error) => return Err(error),
                };
                self.irq_actions.insert(irq.into(), actions);
            }
            if self.irq_actions[irq].iter().any(|value| value == action) {
                matches.push(irq.to_owned());
            }
        }
        if matches.is_empty() {
            platform.print(&format!("No IRQs found for '{action}'"))?;
        }
        for irq in matches {
            sudo_write(
                platform,
                &format!("/proc/irq/{irq}/smp_affinity_list"),
                &core.to_string(),
            )?;
        }
        Ok(())
    }
}
