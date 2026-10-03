use super::*;
use crate::context::{
    actions::{Selection, UpdaterAction},
    Action, Confirmation,
};
use openpilot_ui_framework::widget::DialogResult;
impl Software {
    pub(super) fn change(&mut self, change: Change) -> Result<(), Error> {
        let context = self.context.clone();
        match change {
            Change::Download => {
                self.button(2)?.state.enabled = false.into();
                let action = if self.button(2)?.text.get() == context.tr("CHECK") {
                    UpdaterAction::Check
                } else {
                    UpdaterAction::Download
                };
                self.waiting = Some((context.now_monotonic)());
                context.actions.push(Action::Updater(action));
            }
            Change::Install => {
                self.button(3)?.state.enabled = false.into();
                context.params.put_bool("DoReboot", true)?;
            }
            Change::Uninstall => {
                let callback_context = context.clone();
                let callback = Callback::new(move |result| {
                    if result == DialogResult::Confirm {
                        if let Err(error) = callback_context.params.put_bool("DoUninstall", true) {
                            callback_context.actions.push(Action::Failure(error));
                        }
                    }
                });
                context.actions.push(Action::Confirm(Confirmation {
                    text: context.tr("Are you sure you want to uninstall?"),
                    confirm: context.tr("Uninstall"),
                    cancel: context.tr("Cancel"),
                    rich: false,
                    callback,
                }));
            }
            Change::Branch => self.select_branch()?,
        }
        Ok(())
    }
    fn select_branch(&mut self) -> Result<(), Error> {
        let context = self.context.clone();
        let current = context.params.string("GitBranch")?;
        let mut options: Vec<String> = context
            .params
            .string("UpdaterAvailableBranches")?
            .split(',')
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect();
        for preferred in [
            current.as_str(),
            "devel-staging",
            "devel",
            "nightly",
            "nightly-dev",
            "master",
        ] {
            if let Some(index) = options.iter().position(|value| value == preferred) {
                let value = options.remove(index);
                options.insert(0, value);
            }
        }
        let selected = context.params.string("UpdaterTargetBranch")?;
        let weak = self
            .handle
            .as_ref()
            .and_then(WeakWidgetHandle::upgrade)
            .ok_or(Error::Contract("software handle"))?
            .downgrade();
        let callback_context = context.clone();
        let callback = Callback::new(move |selection: Option<String>| {
            let Some(selection) = selection.filter(|s| !s.is_empty()) else {
                return;
            };
            let result = (|| -> Result<(), crate::Error> {
                callback_context
                    .params
                    .put("UpdaterTargetBranch", selection.as_bytes())?;
                if let Some(widget) = weak.upgrade() {
                    widget.get_mut::<Software>()?.button(4)?.value = selection.into();
                }
                callback_context
                    .actions
                    .push(Action::Updater(UpdaterAction::Check));
                Ok(())
            })();
            if let Err(error) = result {
                callback_context.actions.push(Action::Failure(error));
            }
        });
        context.actions.push(Action::Select(Selection {
            title: context.tr("Select a branch"),
            options,
            selected,
            callback,
        }));
        Ok(())
    }
}
