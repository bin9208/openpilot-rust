#[path = "support/runtime_context.rs"]
pub mod fixture;
use openpilot_msgq::VisionStream;
use openpilot_ui_application::{onroad::calibration::Calibration, params::Read};
use openpilot_ui_framework::{
    application::{Application, ApplicationConfig},
    geometry::Rect,
};
use serde::Deserialize;
use std::{io::Read as _, path::Path};
#[derive(Deserialize)]
struct Step {
    rect: Rect,
    stream: u8,
    speed: f64,
    messages: Vec<Vec<u8>>,
    #[serde(default)]
    reset: bool,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, root, output] = args.as_slice() else {
        return Err("calibration_geometry ROOT OUTPUT".into());
    };
    let root = Path::new(root);
    let output = Path::new(output);
    let mut input = String::new();
    std::io::stdin().read_to_string(&mut input)?;
    let steps: Vec<Step> = serde_json::from_str(&input)?;
    let app = Application::new(ApplicationConfig::for_runtime(
        root,
        "Calibration geometry oracle",
    )?)?;
    let (context, _) = fixture::context(root, output, &app)?;
    let mut calibration = Calibration::default();
    let mut values = Vec::new();
    for step in steps {
        if step.reset {
            calibration = Calibration::default();
        }
        context
            .messages
            .borrow_mut()
            .state
            .update(0.0, &step.messages)?;
        calibration.update(&context)?;
        let stream = match step.stream {
            0 => VisionStream::Road,
            2 => VisionStream::WideRoad,
            _ => return Err("invalid calibration stream".into()),
        };
        let (camera, model) = calibration.matrices(step.rect, stream, step.speed, context.big)?;
        values.push(serde_json::json!({"camera":camera,"model":model,"position":context.params.string("DevicePosition")?}));
    }
    println!("CALIBRATION_RESULT {}", serde_json::to_string(&values)?);
    Ok(())
}
