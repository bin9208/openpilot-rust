use crate::{
    bus::{Settings, Value},
    DEFAULT_TETHERING_PASSWORD, TETHERING_IP_ADDRESS,
};
use dbus::arg::{PropMap, RefArg, Variant};

pub fn value<T: RefArg + 'static>(value: T) -> Value {
    Variant(Box::new(value))
}
fn group<const N: usize>(items: [(&str, Value); N]) -> PropMap {
    items
        .into_iter()
        .map(|(key, value)| (key.into(), value))
        .collect()
}
pub fn connection(ssid: &str, password: &str, hidden: bool) -> Settings {
    let mut settings = Settings::from([
        (
            "connection".into(),
            group([
                ("type", value("802-11-wireless".to_owned())),
                ("uuid", value(uuid::Uuid::new_v4().to_string())),
                ("id", value(format!("openpilot connection {ssid}"))),
                ("autoconnect-retries", value(0_i32)),
            ]),
        ),
        (
            "802-11-wireless".into(),
            group([
                ("ssid", value(ssid.as_bytes().to_vec())),
                ("hidden", value(hidden)),
                ("mode", value("infrastructure".to_owned())),
            ]),
        ),
        (
            "ipv4".into(),
            group([
                ("method", value("auto".to_owned())),
                ("dns-priority", value(600_i32)),
            ]),
        ),
        (
            "ipv6".into(),
            group([("method", value("ignore".to_owned()))]),
        ),
    ]);
    if !password.is_empty() {
        settings.insert(
            "802-11-wireless-security".into(),
            group([
                ("key-mgmt", value("wpa-psk".to_owned())),
                ("auth-alg", value("open".to_owned())),
                ("psk", value(password.to_owned())),
            ]),
        );
    }
    settings
}
pub fn hotspot(ssid: &str) -> Settings {
    Settings::from([
        (
            "connection".into(),
            group([
                ("type", value("802-11-wireless".to_owned())),
                ("uuid", value(uuid::Uuid::new_v4().to_string())),
                ("id", value("Hotspot".to_owned())),
                ("autoconnect-retries", value(0_i32)),
                ("interface-name", value("wlan0".to_owned())),
                ("autoconnect", value(false)),
            ]),
        ),
        (
            "802-11-wireless".into(),
            group([
                ("band", value("bg".to_owned())),
                ("mode", value("ap".to_owned())),
                ("ssid", value(ssid.as_bytes().to_vec())),
            ]),
        ),
        (
            "802-11-wireless-security".into(),
            group([
                ("group", value(vec!["ccmp".to_owned()])),
                ("key-mgmt", value("wpa-psk".to_owned())),
                ("pairwise", value(vec!["ccmp".to_owned()])),
                ("proto", value(vec!["rsn".to_owned()])),
                ("psk", value(DEFAULT_TETHERING_PASSWORD.to_owned())),
            ]),
        ),
        (
            "ipv4".into(),
            group([
                ("method", value("shared".to_owned())),
                (
                    "address-data",
                    value(vec![group([
                        ("address", value(TETHERING_IP_ADDRESS.to_owned())),
                        ("prefix", value(24_u32)),
                    ])]),
                ),
                ("gateway", value(TETHERING_IP_ADDRESS.to_owned())),
                ("never-default", value(true)),
            ]),
        ),
        (
            "ipv6".into(),
            group([("method", value("ignore".to_owned()))]),
        ),
    ])
}
