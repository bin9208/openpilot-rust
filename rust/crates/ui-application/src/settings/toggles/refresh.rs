use super::*;
use crate::params::Read;
impl Toggles {
    pub fn refresh(&mut self) -> Result<(), Error> {
        self.context.refresh_params()?;
        let description = self.context.tr(copy::EXPERIMENTAL);
        let (car, longitudinal, engaged) = {
            let ui = self.context.ui.borrow();
            (ui.slow.car, ui.slow.has_longitudinal_control, ui.engaged)
        };
        if let Some(car) = car {
            if longitudinal {
                self.toggle("ExperimentalMode")?.state.enabled = true.into();
                self.item("ExperimentalMode")?.description = description.into();
                self.personality()?.state.enabled = true.into();
            } else {
                let action = self.toggle("ExperimentalMode")?;
                action.state.enabled = false.into();
                action.toggle.set_value(false);
                self.context.params.remove("ExperimentalMode")?;
                let unavailable=self.context.tr("Experimental mode is currently unavailable on this car since the car's stock ACC is used for longitudinal control.");
                let mut long_description = format!(
                    "{unavailable} {}",
                    self.context
                        .tr("openpilot longitudinal control may come in a future update.")
                );
                if car.alpha_longitudinal_available {
                    long_description = if self.is_release {
                        format!("{unavailable} {}",self.context.tr("An alpha version of openpilot longitudinal control can be tested, along with Experimental mode, on non-release branches."))
                    } else {
                        self.context.tr("Enable the openpilot longitudinal control (alpha) toggle to allow Experimental mode.")
                    };
                }
                self.item("ExperimentalMode")?.description =
                    format!("<b>{long_description}</b><br><br>{description}").into();
            }
        } else {
            self.item("ExperimentalMode")?.description = description.into();
        }
        self.update_icon()?;
        for definition in copy::DEFINITIONS {
            let value = self.context.params.boolean(definition.key)?;
            self.toggle(definition.key)?.toggle.set_value(value);
        }
        for definition in copy::DEFINITIONS {
            if definition.restart && !self.locked.contains(definition.key) {
                self.toggle(definition.key)?.state.enabled = (!engaged).into();
            }
        }
        Ok(())
    }
}
