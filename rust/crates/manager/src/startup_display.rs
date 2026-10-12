use openpilot_registration::{Error, Spinner};
use openpilot_startup_ui::{children, native_children::NativeLaunch};

pub struct Display {
    pub launch: NativeLaunch,
    spinner: Option<children::Spinner<NativeLaunch>>,
}
impl Display {
    pub fn new(launch: NativeLaunch) -> Self {
        Self {
            launch,
            spinner: None,
        }
    }
}
impl Spinner for Display {
    fn start(&mut self) -> Result<(), Error> {
        if self.spinner.is_none() {
            self.spinner = Some(children::Spinner::new(self.launch.clone()));
        }
        Ok(())
    }
    fn update(&mut self, text: &str) -> Result<(), Error> {
        self.start()?;
        match &mut self.spinner {
            Some(spinner) => spinner
                .update(text)
                .map(|_| ())
                .map_err(|error| Error::Spinner(error.to_string())),
            None => Err(Error::Spinner("registration display unavailable".into())),
        }
    }
    fn close(&mut self) -> Result<(), Error> {
        if let Some(mut spinner) = self.spinner.take() {
            spinner.close();
        }
        Ok(())
    }
}
