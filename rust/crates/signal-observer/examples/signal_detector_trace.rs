use openpilot_signal_observer::{
    detector::{day_proposals, detect, housing_proposals, night_proposals},
    pixels::{dark_scene, Rgb},
};
use serde::Deserialize;
use serde_json::json;
use std::{
    fs,
    io::{self, Read},
    path::PathBuf,
};
#[derive(Deserialize)]
struct Input {
    path: PathBuf,
    width: u32,
    height: u32,
    conservative_green: bool,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    openpilot_opencv_runtime::initialize(1)?;
    let mut text = String::new();
    io::stdin().read_to_string(&mut text)?;
    let cases: Vec<Input> = serde_json::from_str(&text)?;
    let mut output = Vec::new();
    for i in cases {
        let rgb = Rgb {
            width: i.width,
            height: i.height,
            data: fs::read(i.path)?,
        };
        let gray = openpilot_opencv_runtime::rgb_to_gray(rgb.view()?)?;
        let gray = gray.view().data();
        let dark = dark_scene(&rgb)?;
        output.push(json!({"dark":dark,"housing":housing_proposals(&rgb,gray,false,i.conservative_green)?,"forward_housing":housing_proposals(&rgb,gray,true,i.conservative_green)?,"day":day_proposals(&rgb,gray,i.conservative_green)?,"night":night_proposals(&rgb,gray,i.conservative_green,dark)?,"detect_default":detect(&rgb,gray,false,i.conservative_green)?,"detect_day":detect(&rgb,gray,true,i.conservative_green)?}));
    }
    println!("{}", serde_json::to_string(&output)?);
    Ok(())
}
