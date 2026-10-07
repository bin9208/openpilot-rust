use super::*;
use crate::context::Confirmation;
use openpilot_ui_framework::widget::DialogResult;
impl Device {
    pub(super) fn change(&mut self, change: Change) -> Result<(), Error> {
        let weak = self
            .handle
            .as_ref()
            .and_then(WeakWidgetHandle::upgrade)
            .ok_or(Error::Contract("device layout handle"))?
            .downgrade();
        let context = self.context.clone();
        if matches!(change, Change::Language) {
            let callback = Callback::new(move |()| {
                if let Some(widget) = weak.upgrade() {
                    let result = (|| -> Result<(), crate::Error> {
                        widget.get_mut::<Self>()?.description()?;
                        Ok(())
                    })();
                    if let Err(error) = result {
                        context.actions.push(Action::Failure(error));
                    }
                }
            });
            self.context.actions.push(Action::SelectLanguage(callback));
            return Ok(());
        }
        let (alert, title, prompt) = match change {
            Change::Reset => (
                "Disengage to Reset Calibration",
                "Reset",
                "Are you sure you want to reset calibration?",
            ),
            Change::Reboot => (
                "Disengage to Reboot",
                "Reboot",
                "Are you sure you want to reboot?",
            ),
            Change::Shutdown => (
                "Disengage to Power Off",
                "Power Off",
                "Are you sure you want to power off?",
            ),
            Change::Language => return Err(Error::Contract("device change already handled")),
        };
        if context.ui.borrow().engaged {
            context.actions.push(Action::Alert(context.tr(alert)));
            return Ok(());
        }
        let callback = Callback::new(move |result| {
            if result != DialogResult::Confirm || context.ui.borrow().engaged {
                return;
            }
            let result = (|| -> Result<(), crate::Error> {
                match change {
                    Change::Reset => {
                        for key in [
                            "CalibrationParams",
                            "LiveTorqueParameters",
                            "LiveParameters",
                            "LiveParametersV2",
                            "LiveDelay",
                        ] {
                            context.params.remove(key)?;
                        }
                        context.params.put_bool("OnroadCycleRequested", true)?;
                        if let Some(widget) = weak.upgrade() {
                            widget.get_mut::<Self>()?.description()?;
                        }
                    }
                    Change::Reboot => context.params.put_bool_nonblocking("DoReboot", true)?,
                    Change::Shutdown => context.params.put_bool_nonblocking("DoShutdown", true)?,
                    Change::Language => {
                        return Err(crate::Error::Contract("unexpected device confirmation"))
                    }
                }
                Ok(())
            })();
            if let Err(error) = result {
                context.actions.push(Action::Failure(error));
            }
        });
        self.context.actions.push(Action::Confirm(Confirmation {
            text: self.context.tr(prompt),
            confirm: self.context.tr(title),
            cancel: self.context.tr("Cancel"),
            rich: false,
            callback,
        }));
        Ok(())
    }
}
