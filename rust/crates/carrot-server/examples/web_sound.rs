use num_traits::ToPrimitive;
use openpilot_carrot_server::{
    web_sound::{Button, Input, Policy, Settings},
    Value,
};
use std::{
    io::{self, BufRead},
    path::PathBuf,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    for line in io::stdin().lock().lines() {
        let input = Value::parse(&line?)?;
        let result = match input.get("action").string()?.as_str() {
            "settings" => {
                let params = openpilot_params::Params::for_runtime_at(&PathBuf::from(
                    input.get("params").string()?,
                ))?;
                let settings = Settings::read(&params);
                Value::object([
                    ("volume", Value::Float(settings.volume)),
                    ("engage_volume", Value::Float(settings.engage_volume)),
                    ("sound_directory", Value::text(settings.sound_directory)),
                ])
            }
            "policy" => {
                let mut policy = Policy::new(input.get("tizi").truth());
                let Value::Array(frames) = input.get("frames") else {
                    return Err("expected frames array".into());
                };
                let mut rows = Vec::new();
                for frame in frames {
                    let buttons = match frame.get("button") {
                        Value::Null => Vec::new(),
                        Value::Bool(pressed) => vec![Button {
                            kind: 8,
                            pressed: *pressed,
                        }],
                        _ => return Err("expected button boolean or null".into()),
                    };
                    let settings = Settings::from_values(
                        (
                            (frame.get("volume").float()? / 100.).clamp(0., 2.),
                            (frame.get("engage_volume").float()? / 100.).clamp(0., 2.),
                        ),
                        (
                            &frame.get("language").string()?,
                            &frame.get("fallback_language").string()?,
                        ),
                    );
                    let state = policy.step(Input {
                        now: frame.get("now").float()?,
                        received: frame.get("received").float()?,
                        enabled: frame.get("enabled").truth(),
                        alert: frame
                            .get("alert")
                            .int()?
                            .to_u16()
                            .ok_or("alert outside u16")?,
                        valid: frame.get("valid").truth(),
                        updated: frame.get("updated").truth(),
                        countdown: frame
                            .get("countdown")
                            .int()?
                            .to_i32()
                            .ok_or("countdown outside i32")?,
                        countdown_valid: frame.get("countdown_valid").truth(),
                        countdown_updated: frame.get("countdown_updated").truth(),
                        car_valid: frame.get("car_valid").truth(),
                        buttons: &buttons,
                        settings: Some(settings),
                    });
                    if let Some(state) = state {
                        rows.push(state);
                    }
                }
                Value::Array(rows)
            }
            _ => return Err("unknown fixture action".into()),
        };
        println!("{}", result.encode()?);
    }
    Ok(())
}
