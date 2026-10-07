use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
pub struct Button {
    pub kind: u16,
    pub pressed: bool,
}

#[derive(Debug, Serialize)]
pub struct MainToggle {
    pub main_button: u16,
    pub pressed_at: Option<f64>,
    pub triggered: bool,
}

impl MainToggle {
    pub fn new(main_button: u16) -> Self {
        Self {
            main_button,
            pressed_at: None,
            triggered: false,
        }
    }

    pub fn update(&mut self, buttons: &[Button], engagement: (bool, f64)) -> bool {
        let (engaged, now) = engagement;
        for button in buttons {
            if button.kind != self.main_button {
                continue;
            }
            if button.pressed {
                if self.pressed_at.is_none() {
                    self.pressed_at = Some(now);
                    self.triggered = false;
                }
            } else {
                self.pressed_at = None;
                self.triggered = false;
            }
        }
        if self.pressed_at.is_some_and(|pressed| now - pressed >= 2.) && !self.triggered && !engaged
        {
            self.triggered = true;
            true
        } else {
            false
        }
    }
}
