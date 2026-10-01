use super::advanced::{Action, AdvancedNetworkSettings};
use crate::{
    callback::Callback,
    keyboard::Keyboard,
    widget::{DialogResult, Frame, NavigationRequest},
    Error,
};
use openpilot_wifi::{Command, MeteredType};
impl AdvancedNetworkSettings {
    pub(super) fn perform(&mut self, action: Action, frame: &Frame<'_>) -> Result<(), Error> {
        match action {
            Action::Tether => {
                let checked = self.toggle_value(0)?;
                self.tether_enabled.set(false);
                if checked {
                    self.metered_enabled.set(false);
                }
                self.context
                    .session
                    .send(Command::SetTetheringActive(checked))?;
            }
            Action::Roaming | Action::CellMetered => {
                let (key, index) = match action {
                    Action::Roaming => ("GsmRoaming", 3),
                    _ => ("GsmMetered", 5),
                };
                let value = self.toggle_value(index)?;
                self.params
                    .put_bool(key, value)
                    .map_err(|error| Error::Io(std::io::Error::other(error)))?;
            }
            Action::WifiMetered(selected) => {
                let value = match selected {
                    1 => MeteredType::Yes,
                    2 => MeteredType::No,
                    _ => MeteredType::Unknown,
                };
                self.metered_enabled.set(false);
                self.context
                    .session
                    .send(Command::SetCurrentNetworkMetered(value))?;
            }
            Action::Apn | Action::Password => self.edit_text(action, frame)?,
            Action::Hidden => self.hidden(frame)?,
        }
        Ok(())
    }
    fn edit_text(&mut self, action: Action, frame: &Frame<'_>) -> Result<(), Error> {
        let (minimum, title, subtitle, value) = match action {
            Action::Apn => (
                0,
                self.context.text("Enter APN"),
                self.context.text("leave blank for automatic configuration"),
                String::from_utf8(
                    self.params
                        .get("GsmApn")
                        .map_err(|error| Error::Io(std::io::Error::other(error)))?
                        .unwrap_or_default(),
                )
                .map_err(|_| Error::Contract("APN is not UTF-8"))?,
            ),
            Action::Password => (
                8,
                self.context.text("Enter new tethering password"),
                String::new(),
                self.context.session.snapshot().tethering_password,
            ),
            Action::Tether
            | Action::Roaming
            | Action::CellMetered
            | Action::WifiMetered(_)
            | Action::Hidden => return Err(Error::Contract("action has no text editor")),
        };
        let mut keyboard = self.keyboard.get_mut::<Keyboard>()?;
        keyboard.reset(Some(minimum));
        keyboard.set_title(&title, &subtitle);
        keyboard.set_text(&value);
        let weak = self.keyboard.downgrade();
        let context = self.context.clone();
        let params = self.params.clone();
        let enabled = self.password_enabled.clone();
        let errors = self.errors.clone();
        keyboard.callback = Some(Callback::new(move |result| {
            if result != DialogResult::Confirm {
                return;
            }
            let outcome = (|| {
                let handle = weak
                    .upgrade()
                    .ok_or(Error::Contract("network keyboard dropped"))?;
                let text = handle.get::<Keyboard>()?.text();
                match action {
                    Action::Apn => {
                        let value = text.trim_matches(crate::text::whitespace);
                        if value.is_empty() {
                            params.remove("GsmApn")
                        } else {
                            params.put("GsmApn", value.as_bytes())
                        }
                        .map_err(|error| Error::Io(std::io::Error::other(error)))?;
                    }
                    Action::Password => {
                        context.session.send(Command::SetTetheringPassword(text))?;
                        enabled.set(false);
                    }
                    _ => return Err(Error::Contract("invalid editor completion")),
                }
                Ok(())
            })();
            if let Err(error) = outcome {
                errors.borrow_mut().push(error);
            }
        }));
        frame
            .navigation
            .push(NavigationRequest::Push(self.keyboard.clone()));
        Ok(())
    }
    fn hidden(&mut self, frame: &Frame<'_>) -> Result<(), Error> {
        let mut keyboard = self.keyboard.get_mut::<Keyboard>()?;
        keyboard.reset(Some(1));
        keyboard.set_title(&self.context.text("Enter SSID"), "");
        let weak = self.keyboard.downgrade();
        let context = self.context.clone();
        let errors = self.errors.clone();
        let navigation = frame.navigation.clone();
        keyboard.callback = Some(Callback::new(move |result| {
            if result != DialogResult::Confirm {
                return;
            }
            let outcome = (|| {
                let handle = weak
                    .upgrade()
                    .ok_or(Error::Contract("network keyboard dropped"))?;
                let mut keyboard = handle.get_mut::<Keyboard>()?;
                let ssid = keyboard.text();
                if ssid.is_empty() {
                    return Ok(());
                }
                keyboard.reset(Some(0));
                keyboard.set_title(
                    &context.text("Enter password"),
                    &context.text("for \"{}\"").replace("{}", &ssid),
                );
                let weak = handle.downgrade();
                let context = context.clone();
                let errors = errors.clone();
                keyboard.callback = Some(Callback::new(move |result| {
                    if result != DialogResult::Confirm {
                        return;
                    }
                    let outcome = (|| {
                        let handle = weak
                            .upgrade()
                            .ok_or(Error::Contract("hidden keyboard dropped"))?;
                        let password = handle.get::<Keyboard>()?.text();
                        context.session.send(Command::Connect {
                            ssid: ssid.clone(),
                            password,
                            hidden: true,
                        })
                    })();
                    if let Err(error) = outcome {
                        errors.borrow_mut().push(error);
                    }
                }));
                drop(keyboard);
                navigation.push(NavigationRequest::Push(handle));
                Ok(())
            })();
            if let Err(error) = outcome {
                errors.borrow_mut().push(error);
            }
        }));
        frame
            .navigation
            .push(NavigationRequest::Push(self.keyboard.clone()));
        Ok(())
    }
}
