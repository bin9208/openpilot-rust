use super::*;
use crate::params::{datetime, typed};
impl Software {
    pub(super) fn refresh(&mut self) -> Result<(), Error> {
        let context = self.context.clone();
        let onroad = context.ui.borrow().started;
        self.item(0)?.state.visible = onroad.into();
        let description = context.params.string("UpdaterCurrentDescription")?;
        let notes = context
            .params
            .bytes("UpdaterCurrentReleaseNotes")?
            .unwrap_or_default();
        self.item(1)?
            .action_mut::<TextAction>()
            .ok_or(Error::Contract("software version"))?
            .text = description.into();
        self.item(1)?.description = String::from_utf8_lossy(&notes).into_owned().into();
        self.item(2)?.state.visible = (!onroad).into();
        let mut state = context.params.string("UpdaterState")?;
        if state.is_empty() {
            state = "idle".into();
        }
        let failed = typed::integer_value(context.params.as_ref(), "UpdateFailedCount", false)?
            .is_some_and(|value| value != "0" && !value.starts_with('-'));
        let fetch = context.params.boolean("UpdaterFetchAvailable")?;
        let available = context.params.boolean("UpdateAvailable")?;
        if state != "idle" {
            self.waiting = None;
            let text = self
                .state_text
                .iter()
                .find(|(key, _)| *key == state)
                .map_or(state.clone(), |(_, value)| value.clone());
            self.button(2)?.state.enabled = false.into();
            self.button(2)?.value = text.into();
        } else {
            let (value, text) = if failed {
                (context.tr("failed to check for update"), "CHECK")
            } else if fetch {
                (context.tr("update available"), "DOWNLOAD")
            } else {
                let raw = context.params.string("LastUpdateTime")?;
                let value = if let Some(date) = datetime::parse(&raw) {
                    context
                        .tr("up to date, last checked {}")
                        .replace("{}", &time::ago(&context, date)?)
                } else {
                    context.tr("up to date, last checked never")
                };
                (value, "CHECK")
            };
            self.button(2)?.value = value.into();
            self.button(2)?.text = context.tr(text).into();
            if self
                .waiting
                .is_some_and(|start| (context.now_monotonic)() - start > 10.0)
            {
                self.waiting = None;
            }
            let enabled = self.waiting.is_none();
            self.button(2)?.state.enabled = enabled.into();
        }
        self.button(4)?.value = context.params.string("UpdaterTargetBranch")?.into();
        self.item(3)?.state.visible = (!onroad && available).into();
        if available {
            let description = context.params.string("UpdaterNewDescription")?;
            let notes = context
                .params
                .bytes("UpdaterNewReleaseNotes")?
                .unwrap_or_default();
            self.button(3)?.text = context.tr("INSTALL").into();
            self.button(3)?.value = description.into();
            self.item(3)?.description = String::from_utf8_lossy(&notes).into_owned().into();
            self.button(3)?.state.enabled = true.into();
        }
        Ok(())
    }
}
