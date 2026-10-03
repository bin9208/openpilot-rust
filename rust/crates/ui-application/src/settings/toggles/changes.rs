use super::*;
use crate::{context::Confirmation, params::Read};
use openpilot_ui_framework::widget::DialogResult;
impl Toggles {
    pub(super) fn process_changes(&mut self) -> Result<(), Error> {
        loop {
            let change = self.changes.borrow_mut().pop_front();
            let Some(change) = change else {
                break;
            };
            match change {
                Change::Personality(index) => self
                    .context
                    .params
                    .put("LongitudinalPersonality", index.to_string().as_bytes())?,
                Change::Toggle("ExperimentalMode", value) => self.experimental(value)?,
                Change::Toggle(key, value) => {
                    self.context.params.put_bool(key, value)?;
                    if copy::DEFINITIONS
                        .iter()
                        .any(|item| item.key == key && item.restart)
                    {
                        self.context.params.put_bool("OnroadCycleRequested", true)?;
                    }
                }
            }
        }
        Ok(())
    }
    fn experimental(&mut self, value: bool) -> Result<(), Error> {
        if value && !self.context.params.boolean("ExperimentalModeConfirmed")? {
            let weak = self.handle.as_ref().ok_or(Error::Contract(
                "experimental confirmation requires attached toggle layout",
            ))?;
            let weak = weak
                .upgrade()
                .ok_or(Error::Contract("toggle layout no longer alive"))?
                .downgrade();
            let item = self.item("ExperimentalMode")?;
            let text = format!(
                "<h1>{}</h1><br><p>{}</p>",
                item.title.get(),
                item.description.get()
            );
            let context = self.context.clone();
            let callback = Callback::new(move |result| {
                let result = (|| -> Result<(), crate::Error> {
                    let Some(handle) = weak.upgrade() else {
                        return Ok(());
                    };
                    let mut widget = handle.get_mut::<Toggles>()?;
                    if result == DialogResult::Confirm {
                        context.params.put_bool("ExperimentalMode", true)?;
                        context.params.put_bool("ExperimentalModeConfirmed", true)?;
                    } else {
                        widget.toggle("ExperimentalMode")?.toggle.set_value(false);
                    }
                    widget.update_icon()?;
                    Ok(())
                })();
                if let Err(error) = result {
                    context.actions.push(Action::Failure(error));
                }
            });
            self.context.actions.push(Action::Confirm(Confirmation {
                text,
                confirm: self.context.tr("Enable"),
                cancel: self.context.tr("Cancel"),
                rich: true,
                callback,
            }));
        } else {
            self.update_icon()?;
            self.context.params.put_bool("ExperimentalMode", value)?;
        }
        Ok(())
    }
}
