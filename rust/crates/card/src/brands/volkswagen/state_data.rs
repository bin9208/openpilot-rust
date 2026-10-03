use super::Error;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub type Values = BTreeMap<String, f64>;
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
pub enum Stock {
    Inactive(bool),
    Values(Values),
}
impl Stock {
    pub(super) fn values(&self) -> Result<&Values, Error> {
        match self {
            Self::Values(v) => Ok(v),
            Self::Inactive(_) => Err(Error::Stock("eps_stock_values")),
        }
    }
}
#[derive(Serialize, Deserialize)]
pub struct Extras {
    pub frame: u64,
    pub eps_init_complete: bool,
    pub button_states: BTreeMap<String, bool>,
    pub esp_hold_confirmation: bool,
    pub long_control_inhibit: bool,
    pub upscale_lead_car_signal: bool,
    pub eps_stock_values: Stock,
    pub klr_stock_values: Values,
    pub ea_hud_stock_values: Values,
    pub ea_control_stock_values: Values,
    pub travel_assist_available: bool,
    pub left_blinker_active: bool,
    pub right_blinker_active: bool,
    pub curvature: f64,
    pub cruise_recovery_timer: u64,
    pub ldw_stock_values: Option<Values>,
    pub gra_stock_values: Option<Values>,
    pub acc_type: Option<f64>,
    pub steering_pressed_cnt: u32,
    pub left_blinker_cnt: u32,
    pub right_blinker_cnt: u32,
    pub left_blinker_prev: bool,
    pub right_blinker_prev: bool,
}
impl Default for Extras {
    fn default() -> Self {
        Self {
            frame: 0,
            eps_init_complete: false,
            button_states: [
                "setCruise",
                "resumeCruise",
                "accelCruise",
                "decelCruise",
                "cancel",
                "gapAdjustCruise",
            ]
            .into_iter()
            .map(|key| (key.to_owned(), false))
            .collect(),
            esp_hold_confirmation: false,
            long_control_inhibit: false,
            upscale_lead_car_signal: false,
            eps_stock_values: Stock::Inactive(false),
            klr_stock_values: Values::new(),
            ea_hud_stock_values: Values::new(),
            ea_control_stock_values: Values::new(),
            travel_assist_available: false,
            left_blinker_active: false,
            right_blinker_active: false,
            curvature: 0.,
            cruise_recovery_timer: 0,
            ldw_stock_values: None,
            gra_stock_values: None,
            acc_type: None,
            steering_pressed_cnt: 0,
            left_blinker_cnt: 0,
            right_blinker_cnt: 0,
            left_blinker_prev: false,
            right_blinker_prev: false,
        }
    }
}
