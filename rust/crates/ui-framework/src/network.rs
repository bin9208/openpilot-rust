//! Network widgets from system/ui/widgets/network.py; transport is openpilot-wifi.
mod model;
mod session;
mod wifi;
mod wifi_render;
pub use model::{Model, Phase};
pub use session::{Context, WifiBackend, WifiSession};
pub use wifi::WifiManagerUi;
mod advanced;
mod advanced_actions;
mod panel;
pub use advanced::AdvancedNetworkSettings;
pub use panel::{NavButton, NetworkUi, Panel};
