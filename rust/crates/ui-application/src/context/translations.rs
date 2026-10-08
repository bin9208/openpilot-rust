use openpilot_ui_framework::multilang::Multilang;
use std::sync::{Arc, Mutex};
/// Rendering and background callbacks resolve the same current language catalog.
#[derive(Clone)]
pub struct Translations(Arc<Mutex<Multilang>>);
impl Translations {
    pub fn new(value: Multilang) -> Self {
        Self(Arc::new(Mutex::new(value)))
    }
    pub fn tr(&self, text: &str) -> String {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .tr(text)
            .into()
    }
    pub fn trn(&self, singular: &str, plural: &str, n: i64) -> String {
        self.0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .trn(singular, plural, n)
            .into()
    }
    pub fn update<R>(&self, update: impl FnOnce(&mut Multilang) -> R) -> R {
        update(&mut self.0.lock().unwrap_or_else(|error| error.into_inner()))
    }
}
