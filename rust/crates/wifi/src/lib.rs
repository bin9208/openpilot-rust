mod actions;
mod bus;
mod engine;
mod error;
mod initialize;
mod manager;
mod monitor;
mod networks;
mod settings;
pub mod state;
mod tether;
pub mod transition;
mod types;
pub use error::Error;
pub use manager::{Config, WifiManager};
pub use types::*;

pub fn normalize_ssid(ssid: &str) -> String {
    ssid.replace('\u{2019}', "'")
}

pub const TETHERING_IP_ADDRESS: &str = "192.168.43.1";
pub const DEFAULT_TETHERING_PASSWORD: &str = "swagswagcomma";

pub fn get_security_type(flags: u32, wpa_flags: u32, rsn_flags: u32) -> SecurityType {
    let properties = wpa_flags | rsn_flags;
    let supports_wpa = 0x1 | 0x2 | 0x10 | 0x20 | 0x100;
    if flags == 0 || (flags & 2 != 0 && properties & supports_wpa == 0) {
        SecurityType::Open
    } else if flags & 1 != 0 && properties & supports_wpa != 0 && properties & 0x200 == 0 {
        SecurityType::Wpa
    } else {
        SecurityType::Unsupported
    }
}
