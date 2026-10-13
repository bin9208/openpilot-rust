use openpilot_cluster::camera::LiveRoadCamera;
use openpilot_msgq::VisionStream;
use openpilot_startup_ui::{
    config::Config,
    draw::Draw,
    geometry::Rect,
    renderer::{Renderer, Window},
};
use serde::Deserialize;
use std::{
    path::Path,
    time::{Duration, Instant},
};

#[derive(Deserialize)]
struct Scene {
    width: f32,
    height: f32,
    destination: Rect,
    steps: Vec<Step>,
    #[serde(default)]
    external: bool,
}
#[derive(Deserialize)]
struct Step {
    now: f64,
    wide: bool,
    select: bool,
    draw: bool,
}
fn stream(stream: VisionStream) -> u8 {
    match stream {
        VisionStream::Road => 0,
        VisionStream::Driver => 1,
        VisionStream::WideRoad => 2,
        VisionStream::Map => 3,
    }
}
fn synchronize(directory: &Path, index: usize) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::write(directory.join(format!("{index}.ready")), b"ready")?;
    let started = Instant::now();
    while !directory.join(format!("{index}.allow")).is_file() {
        if started.elapsed() > Duration::from_secs(15) {
            return Err("camera fixture peer timed out".into());
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, root, scene, output, synchronization] = args.as_slice() else {
        return Err("cluster_camera_fixture ROOT SCENE OUTPUT SYNCHRONIZATION".into());
    };
    let scene: Scene = serde_json::from_slice(&std::fs::read(scene)?)?;
    let assets = Path::new(root).join("openpilot/selfdrive/assets");
    let mut renderer = Renderer::new_window(
        Window {
            config: Config {
                big: false,
                large_viewport: false,
                pc: true,
                scale: 1.0,
            },
            dimensions: (scene.width, scene.height),
            title: "Cluster camera QA".into(),
            spinner: false,
            language: "en".into(),
        },
        &assets,
    )?;
    let descriptors = std::fs::read_dir("/proc/self/fd")?.count();
    let mut camera = LiveRoadCamera::for_platform("rustcluster", &mut renderer, scene.external)?;
    let mut traces = Vec::new();
    for (index, step) in scene.steps.iter().enumerate() {
        synchronize(Path::new(synchronization), index)?;
        if step.select {
            camera.select_stream_at(step.wide, step.now)?;
        }
        renderer.begin();
        renderer.clear(u32::from_le_bytes([22, 33, 44, 255]))?;
        let drawn = if step.draw {
            camera.draw_at(&mut renderer, scene.destination, step.now)?
        } else {
            false
        };
        let mut available: Vec<_> = camera
            .input
            .available_streams()
            .iter()
            .copied()
            .map(stream)
            .collect();
        available.sort_unstable();
        let frame = camera.input.frame().map(|frame| serde_json::json!({
            "id":frame.metadata.frame_id,"width":frame.metadata.width,"height":frame.metadata.height,
            "stride":frame.metadata.stride,"uv_offset":frame.metadata.uv_offset,
            "sof":frame.metadata.timestamp_sof,"eof":frame.metadata.timestamp_eof,
        }));
        traces.push(serde_json::json!({"drawn":drawn,"stream":stream(camera.input.stream()),"streams":available,"frame":frame}));
        if scene.external {
            let row = traces.last_mut().ok_or("camera trace missing")?;
            row["zero_copy"] = serde_json::json!(camera.zero_copy());
        }
        renderer.screenshot(&Path::new(output).join(format!("frame-{index:03}.png")))?;
        renderer.end();
    }
    camera.close(&mut renderer)?;
    renderer.release_dynamic_textures();
    let cleanup = serde_json::json!({"frame":camera.input.frame().is_some(),"streams":camera.input.available_streams().len()});
    if scene.external {
        std::fs::write(
            Path::new(output).join("fd-delta.json"),
            serde_json::to_vec(&serde_json::json!({
                "delta":i64::try_from(std::fs::read_dir("/proc/self/fd")?.count())? - i64::try_from(descriptors)?,
            }))?,
        )?;
    }
    std::fs::write(
        Path::new(output).join("trace.json"),
        serde_json::to_vec_pretty(&serde_json::json!({"steps":traces,"cleanup":cleanup}))?,
    )?;
    Ok(())
}
