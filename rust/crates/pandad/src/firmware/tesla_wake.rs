//! Vehicle-scoped optional wake firmware selection from source pandad.py.
use openpilot_cereal::car_capnp::car_params;

fn supported(value: &str) -> bool {
    matches!(value, "TESLA_MODEL_3" | "TESLA_MODEL_Y")
}

pub fn enabled(
    opt_in: bool,
    persistent: Option<&[u8]>,
    current: Option<&[u8]>,
    selected: Option<&[u8]>,
) -> bool {
    if !opt_in {
        return false;
    }
    let selected = match selected.map(std::str::from_utf8).transpose() {
        Ok(value) => value,
        Err(_) => return false,
    };
    let Some(raw) = persistent
        .filter(|bytes| !bytes.is_empty())
        .or(current.filter(|bytes| !bytes.is_empty()))
    else {
        return selected.is_some_and(supported);
    };
    let Ok(message) =
        capnp::serialize::read_message(&mut &raw[..], capnp::message::ReaderOptions::new())
    else {
        return false;
    };
    let Ok(params) = message.get_root::<car_params::Reader>() else {
        return false;
    };
    params.get_brand().ok().and_then(|text| text.to_str().ok()) == Some("tesla")
        && params
            .get_car_fingerprint()
            .ok()
            .and_then(|text| text.to_str().ok())
            .is_some_and(supported)
}
