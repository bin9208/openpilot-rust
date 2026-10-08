use super::Scene;
use openpilot_msgq::VisionStream;
use openpilot_ui_application::params::Read;
use openpilot_ui_application::{
    context::Context,
    onroad::camera::{CameraView, Config, Transform},
    state::Status,
};
use openpilot_ui_framework::{canvas::Canvas, widget::WidgetHandle};
use serde::Deserialize;
#[derive(Deserialize)]
pub struct Options {
    pub stream: u8,
    #[serde(default)]
    pub transform: u8,
    #[serde(default)]
    pub switches: Vec<(u32, u8)>,
    #[serde(default)]
    pub engaged: bool,
}
fn stream(value: u8) -> Result<VisionStream, Box<dyn std::error::Error>> {
    Ok(match value {
        0 => VisionStream::Road,
        1 => VisionStream::Driver,
        2 => VisionStream::WideRoad,
        3 => VisionStream::Map,
        _ => return Err("invalid camera stream".into()),
    })
}
pub fn create(
    context: &Context,
    canvas: &mut Canvas,
    scene: &Scene,
) -> Result<WidgetHandle, Box<dyn std::error::Error>> {
    let options = scene.camera.as_ref().ok_or("camera options missing")?;
    if let Some(driver) = &scene.driver {
        if scene.config.big {
            return Ok(WidgetHandle::new(
                openpilot_ui_application::onroad::driver_camera::Dialog::with_camera(
                    context.clone(),
                    canvas,
                    "rustvision",
                )?,
            ));
        }
        if driver.navigation {
            return Ok(WidgetHandle::new(
                openpilot_ui_application::mici::onroad::driver_camera::dialog::with_camera(
                    context.clone(),
                    canvas,
                    "rustvision",
                )?,
            ));
        }
        return Ok(WidgetHandle::new(
            openpilot_ui_application::mici::onroad::driver_camera::Preview::with_camera(
                context.clone(),
                canvas,
                "rustvision",
                20.0,
                driver.setup,
            )?,
        ));
    }
    let mut widget = CameraView::new(
        context.clone(),
        Config {
            name: "rustvision".into(),
            stream: stream(options.stream)?,
            compact: !scene.config.big,
        },
        canvas,
    )?;
    widget.transform = match options.transform {
        1 => Transform::DriverLarge,
        2 => Transform::DriverCompact,
        3 => Transform::Matrix([[1.3, 0.0, 0.1], [0.0, 1.1, -0.3], [0.0, 0.0, 1.0]]),
        _ => Transform::Fit,
    };
    widget.background = Some(u32::from_le_bytes([22, 33, 44, 255]));
    Ok(WidgetHandle::new(widget))
}
pub fn before(
    context: &Context,
    widget: &WidgetHandle,
    scene: &Scene,
    index: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let options = scene.camera.as_ref().ok_or("camera options missing")?;
    context.ui.borrow_mut().status = if options.engaged {
        Status::Engaged
    } else {
        Status::Disengaged
    };
    for (frame, kind) in &options.switches {
        if *frame == index {
            widget
                .get_mut::<CameraView>()?
                .switch_stream(stream(*kind)?)?;
        }
    }
    if let Some(driver) = &scene.driver {
        super::product_driver::before(context, widget, driver, index)?;
    }
    let directory = std::path::PathBuf::from(std::env::var("UI_CAMERA_SYNC")?);
    std::fs::write(directory.join(format!("{index}.ready")), b"ready")?;
    let start = std::time::Instant::now();
    while !directory.join(format!("{index}.allow")).exists() {
        if start.elapsed() > std::time::Duration::from_secs(10) {
            return Err("owned camera coordinator timeout".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    Ok(())
}
pub fn snapshot(
    context: &Context,
    widget: &WidgetHandle,
    scene: &Scene,
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    if let Some(driver) = &scene.driver {
        return super::product_driver::snapshot(
            context,
            widget,
            scene.config.big,
            driver.navigation,
        );
    }
    if scene.road.is_some() {
        let widget = widget.get::<openpilot_ui_application::onroad::augmented::Road>()?;
        let camera = &widget.camera;
        return Ok(
            serde_json::json!({"frame":camera.frame().map(|frame|frame.frame_id),"stream":camera.stream() as u8,"streams":camera.available_streams.iter().map(|stream|*stream as u8).collect::<Vec<_>>(),"mode":widget.road_view_mode()?,"position":context.params.string("DevicePosition")?}),
        );
    }
    let widget = widget.get::<CameraView>()?;
    Ok(
        serde_json::json!({"frame":widget.frame().map(|frame|frame.frame_id),"stream":widget.stream() as u8,"streams":widget.available_streams.iter().map(|stream|*stream as u8).collect::<Vec<_>>()}),
    )
}
