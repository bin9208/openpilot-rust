//! Complete field boundary for the original CarSpecificEvents policy.
use crate::alerts::enum_wire;
use openpilot_cereal::car_capnp::{car_control, car_params, car_state};
use serde::{Deserialize, Serialize};

pub use car_params::NetworkLocation;
pub use car_state::{button_event::Type as ButtonType, GearShifter};

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(from = "String", into = "String")]
pub enum Brand {
    Body,
    Mock,
    Ford,
    Nissan,
    Chrysler,
    Honda,
    Toyota,
    Gm,
    Volkswagen,
    Hyundai,
    Tesla,
    Other(String),
}

impl From<String> for Brand {
    fn from(value: String) -> Self {
        match value.as_str() {
            "body" => Self::Body,
            "mock" => Self::Mock,
            "ford" => Self::Ford,
            "nissan" => Self::Nissan,
            "chrysler" => Self::Chrysler,
            "honda" => Self::Honda,
            "toyota" => Self::Toyota,
            "gm" => Self::Gm,
            "volkswagen" => Self::Volkswagen,
            "hyundai" => Self::Hyundai,
            "tesla" => Self::Tesla,
            _ => Self::Other(value),
        }
    }
}

impl From<Brand> for String {
    fn from(value: Brand) -> Self {
        match value {
            Brand::Body => "body".into(),
            Brand::Mock => "mock".into(),
            Brand::Ford => "ford".into(),
            Brand::Nissan => "nissan".into(),
            Brand::Chrysler => "chrysler".into(),
            Brand::Honda => "honda".into(),
            Brand::Toyota => "toyota".into(),
            Brand::Gm => "gm".into(),
            Brand::Volkswagen => "volkswagen".into(),
            Brand::Hyundai => "hyundai".into(),
            Brand::Tesla => "tesla".into(),
            Brand::Other(value) => value,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InputError {
    #[error("invalid CarSpecificEvents cereal input: {0}")]
    Cereal(#[from] capnp::Error),
    #[error("invalid CarSpecificEvents wire enum: {0}")]
    Enum(#[from] capnp::NotInSchema),
    #[error("invalid CarSpecificEvents brand text: {0}")]
    BrandText(#[from] std::str::Utf8Error),
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CarParams {
    pub brand: Brand,
    pub min_steer_speed: f64,
    pub min_enable_speed: f64,
    pub pcm_cruise: bool,
    pub openpilot_longitudinal_control: bool,
    #[serde(with = "enum_wire")]
    pub network_location: NetworkLocation,
}

impl CarParams {
    pub fn read(reader: car_params::Reader<'_>) -> Result<Self, InputError> {
        Ok(Self {
            brand: reader.get_brand()?.to_str()?.to_owned().into(),
            min_steer_speed: f64::from(reader.get_min_steer_speed()),
            min_enable_speed: f64::from(reader.get_min_enable_speed()),
            pcm_cruise: reader.get_pcm_cruise(),
            openpilot_longitudinal_control: reader.get_openpilot_longitudinal_control(),
            network_location: reader.get_network_location()?,
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CruiseState {
    pub enabled: bool,
    pub available: bool,
    pub standstill: bool,
    pub non_adaptive: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Button {
    #[serde(rename = "type", with = "enum_wire")]
    pub kind: ButtonType,
    pub pressed: bool,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CarState {
    pub v_ego: f64,
    pub v_cruise: f64,
    pub carrot_cruise: i16,
    pub door_open: bool,
    pub seatbelt_unlatched: bool,
    #[serde(with = "enum_wire")]
    pub gear_shifter: GearShifter,
    pub cruise_state: CruiseState,
    pub esp_disabled: bool,
    pub esp_active: bool,
    pub stock_fcw: bool,
    pub stock_aeb: bool,
    pub brake_hold_active: bool,
    pub parking_brake: bool,
    pub acc_faulted: bool,
    pub steering_pressed: bool,
    pub brake_pressed: bool,
    pub brake: f64,
    pub standstill: bool,
    pub gas_pressed: bool,
    pub vehicle_sensors_invalid: bool,
    pub invalid_lkas_setting: bool,
    pub low_speed_alert: bool,
    pub button_enable: bool,
    pub activate_cruise: i16,
    pub button_events: Vec<Button>,
    pub steer_fault_temporary: bool,
    pub steer_fault_permanent: bool,
    pub soft_hold_active: i16,
}

impl CarState {
    pub fn read(reader: car_state::Reader<'_>) -> Result<Self, InputError> {
        let cruise = reader.get_cruise_state()?;
        let buttons = reader.get_button_events()?;
        Ok(Self {
            v_ego: f64::from(reader.get_v_ego()),
            v_cruise: f64::from(reader.get_v_cruise()),
            carrot_cruise: reader.get_carrot_cruise(),
            door_open: reader.get_door_open(),
            seatbelt_unlatched: reader.get_seatbelt_unlatched(),
            gear_shifter: reader.get_gear_shifter()?,
            cruise_state: CruiseState {
                enabled: cruise.get_enabled(),
                available: cruise.get_available(),
                standstill: cruise.get_standstill(),
                non_adaptive: cruise.get_non_adaptive(),
            },
            esp_disabled: reader.get_esp_disabled(),
            esp_active: reader.get_esp_active(),
            stock_fcw: reader.get_stock_fcw(),
            stock_aeb: reader.get_stock_aeb(),
            brake_hold_active: reader.get_brake_hold_active(),
            parking_brake: reader.get_parking_brake(),
            acc_faulted: reader.get_acc_faulted(),
            steering_pressed: reader.get_steering_pressed(),
            brake_pressed: reader.get_brake_pressed(),
            brake: f64::from(reader.get_brake()),
            standstill: reader.get_standstill(),
            gas_pressed: reader.get_gas_pressed(),
            vehicle_sensors_invalid: reader.get_vehicle_sensors_invalid(),
            invalid_lkas_setting: reader.get_invalid_lkas_setting(),
            low_speed_alert: reader.get_low_speed_alert(),
            button_enable: reader.get_button_enable(),
            activate_cruise: reader.get_activate_cruise(),
            button_events: buttons
                .iter()
                .map(|button| {
                    Ok(Button {
                        kind: button.get_type()?,
                        pressed: button.get_pressed(),
                    })
                })
                .collect::<Result<_, InputError>>()?,
            steer_fault_temporary: reader.get_steer_fault_temporary(),
            steer_fault_permanent: reader.get_steer_fault_permanent(),
            soft_hold_active: reader.get_soft_hold_active(),
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CarControl {
    pub enabled: bool,
    pub accel: f64,
}

impl CarControl {
    pub fn read(reader: car_control::Reader<'_>) -> Result<Self, InputError> {
        Ok(Self {
            enabled: reader.get_enabled(),
            accel: f64::from(reader.get_actuators()?.get_accel()),
        })
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CarInputs {
    pub current: CarState,
    pub previous: CarState,
    pub control: CarControl,
}
