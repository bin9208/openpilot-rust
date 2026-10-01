use super::product_input::{Step, KEYS};
use openpilot_ui_application::{
    context::{
        actions::{Selection, UpdaterAction},
        Action, Confirmation, Context,
    },
    mici::widgets::dialog::{Confirmation as MiciConfirmation, InputOptions},
    params::Read,
};
use openpilot_ui_framework::{
    canvas::Canvas,
    keyboard::Keyboard,
    widget::{DialogResult, NavigationQueue, NavigationRequest, WidgetHandle},
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
type Error = Box<dyn std::error::Error>;
pub struct Effects {
    context: Context,
    values: Vec<Value>,
    confirmations: Vec<Confirmation>,
    mici_confirmations: Vec<MiciConfirmation>,
    inputs: Vec<InputOptions>,
    keyboards: Vec<WidgetHandle>,
    selections: Vec<Selection>,
}
impl Effects {
    pub fn new(context: Context) -> Self {
        Self {
            context,
            values: Vec::new(),
            confirmations: Vec::new(),
            mici_confirmations: Vec::new(),
            inputs: Vec::new(),
            keyboards: Vec::new(),
            selections: Vec::new(),
        }
    }
    pub fn before(&mut self, step: Option<&Step>) -> Result<(), Error> {
        if let Some(selected) = step.and_then(|step| step.selection.as_ref()) {
            self.selections
                .last_mut()
                .ok_or("no selection dialog")?
                .selected = selected.clone();
        }
        if let Some(confirm) = step.and_then(|step| step.confirm) {
            if let Some(dialog) = self.selections.pop() {
                dialog.callback.call(confirm.then_some(dialog.selected));
            } else if let Some(dialog) = self.confirmations.pop() {
                dialog.callback.call(if confirm {
                    DialogResult::Confirm
                } else {
                    DialogResult::Cancel
                });
            } else if let Some(dialog) = self.mici_confirmations.pop() {
                if confirm {
                    (dialog.callback)();
                }
            } else if let Some(keyboard) = self.keyboards.pop() {
                let callback = keyboard.get::<Keyboard>()?.callback.clone();
                if let Some(callback) = callback {
                    callback.call(if confirm {
                        DialogResult::Confirm
                    } else {
                        DialogResult::Cancel
                    });
                }
            }
        }
        if let Some(text) = step.and_then(|step| step.input_text.as_ref()) {
            if let Some(input) = self.inputs.pop() {
                if let Some(callback) = input.callback {
                    callback(text.clone());
                }
            } else if let Some(keyboard) = self.keyboards.pop() {
                let callback = {
                    let mut input = keyboard.get_mut::<Keyboard>()?;
                    input.set_text(text);
                    input.callback.clone()
                };
                if let Some(callback) = callback {
                    callback.call(DialogResult::Confirm);
                }
            } else {
                return Err("no input dialog pending".into());
            }
        }
        Ok(())
    }
    pub fn navigation(&mut self, request: NavigationRequest) -> Result<(), Error> {
        match request {
            NavigationRequest::Pop(callback) => {
                self.values.push(json!({"pop":true}));
                if let Some(callback) = callback {
                    callback();
                }
            }
            NavigationRequest::Push(widget) => {
                if let Ok(nav) = widget.get::<openpilot_ui_framework::navigation::NavWidget>() {
                    if (nav.content.as_ref() as &dyn std::any::Any)
                        .is::<openpilot_ui_application::mici::settings::network::wifi::Wifi>()
                    {
                        self.values.push(json!({"page":"Wifi"}));
                        return Ok(());
                    }
                }
                {
                    let keyboard = widget.get::<Keyboard>()?;
                    self.values.push(json!({"keyboard":keyboard.title.text.get(),"text":keyboard.text(),"minimum":keyboard.options.min_length}));
                }
                self.keyboards.push(widget);
            }
            NavigationRequest::Close
            | NavigationRequest::PopAt(_)
            | NavigationRequest::PopTo { .. } => return Err("unexpected product navigation".into()),
        }
        Ok(())
    }
    pub fn drain(
        &mut self,
        navigation: &NavigationQueue,
        canvas: &mut Canvas,
    ) -> Result<(), Error> {
        while let Some(request) = navigation.pop() {
            self.navigation(request)?;
        }
        while let Some(action) = self.context.actions.pop() {
            match action {
                Action::Select(dialog) => {
                    self.values.push(json!({"select":dialog.title,"options":dialog.options,"current":dialog.selected}));
                    self.selections.push(dialog);
                }
                Action::Confirm(dialog) => {
                    self.values.push(json!({"confirm":dialog.text,"button":dialog.confirm,"cancel":dialog.cancel,"rich":dialog.rich}));
                    self.confirmations.push(dialog);
                }
                Action::ShowTouches(value) => self.values.push(json!({"touches":value})),
                Action::ShowFps(value) => self.values.push(json!({"fps":value})),
                Action::MiciAlert { title, description } => self
                    .values
                    .push(json!({"mici_alert":title,"description":description})),
                Action::MiciConfirm(dialog) => {
                    self.values.push(json!({"mici_confirm":dialog.title,"exit":dialog.exit_on_confirm,"red":dialog.red}));
                    self.mici_confirmations.push(dialog);
                }
                Action::MiciInput(input) => {
                    self.values.push(json!({"mici_input":input.hint,"text":input.text,"minimum":input.minimum_length}));
                    self.inputs.push(input);
                }
                Action::Updater(UpdaterAction::Reboot) => {
                    self.context.params.put_bool("DoReboot", true)?
                }
                Action::Updater(action @ (UpdaterAction::Check | UpdaterAction::Download)) => {
                    self.values.push(json!({"updater":format!("{action:?}")}))
                }
                Action::SetLanguage(code) => {
                    canvas.renderer.set_language(&code);
                    self.values.push(json!({"language":code}));
                }
                Action::Alert(value) => self.values.push(json!({"alert":value})),
                Action::Open(page) => self.values.push(json!({"page":format!("{page:?}")})),
                Action::Failure(error) => return Err(error.into()),
                Action::PairingCheck
                | Action::Recording(_)
                | Action::ToggleRecording
                | Action::Bookmark
                | Action::SetOffroadBrightness(_)
                | Action::SetInteractiveTimeout(_)
                | Action::RefreshParams
                | Action::SelectLanguage(_)
                | Action::Exit => return Err("unexpected product effect".into()),
            }
        }
        Ok(())
    }
    pub fn snapshot(&self, raw_keys: impl Iterator<Item = String>) -> Result<Value, Error> {
        self.context.params.flush()?;
        let params = KEYS
            .iter()
            .map(|key| {
                Ok((
                    key.to_string(),
                    self.context
                        .params
                        .bytes(key)?
                        .map(String::from_utf8)
                        .transpose()?,
                ))
            })
            .collect::<Result<BTreeMap<_, _>, Error>>()?;
        let raw_params = raw_keys
            .filter_map(|key| match self.context.params.bytes(&key) {
                Ok(Some(bytes)) => Some(Ok((
                    key,
                    bytes
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect::<String>(),
                ))),
                Ok(None) => None,
                Err(error) => Some(Err(error)),
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;
        Ok(
            json!({"params":params,"effects":self.values,"personality":self.context.ui.borrow().personality,"raw_params":raw_params}),
        )
    }
}
