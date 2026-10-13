use openpilot_cluster::{
    models::ClusterUiState,
    renderer::{
        camera_overlay::{camera_for_state, vehicle_draw_key, vehicle_overlay_label},
        ClusterUiRenderer, Options, Rect,
    },
    scene::{ClusterScene, Vec3},
};
use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct Request {
    cases: Vec<Case>,
    points: Vec<Vec3>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    assets_root: PathBuf,
    options: Options,
    state: ClusterUiState,
    scene: ClusterScene,
    wide: bool,
    pitch_deg: f64,
    z_offset: f64,
    render: bool,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, input, output] = args.as_slice() else {
        return Err("cluster_camera_overlay_fixture INPUT OUTPUT".into());
    };
    let request: Request = serde_json::from_slice(&std::fs::read(input)?)?;
    let output = PathBuf::from(output);
    std::fs::create_dir_all(&output)?;
    let mut renderer: Option<ClusterUiRenderer> = None;
    let mut dimensions = None;
    let mut captures = Vec::new();
    let mut helpers = Vec::new();
    let mut buffers = Vec::new();
    let mut closes = Vec::new();
    for case in request.cases {
        let size = (case.options.width, case.options.height);
        if dimensions != Some(size) {
            if let Some(mut renderer) = renderer.take() {
                close(&mut renderer, &mut closes)?;
            }
            let mut next = ClusterUiRenderer::new(&case.assets_root, case.options.clone());
            next.set_profile_enabled(true);
            next.open(true)?;
            renderer = Some(next);
            dimensions = Some(size);
        }
        let renderer = renderer.as_mut().ok_or("camera overlay renderer missing")?;
        configure(renderer, &case);
        let view = renderer
            .camera_overlay_projection(&case.state)
            .ok_or("camera overlay projection missing")?;
        let camera = camera_for_state(&case.state, case.wide);
        let points: Vec<_> = request
            .points
            .iter()
            .map(|point| {
                renderer
                    .camera_overlay_screen_xy(*point, &view, case.scene.scene_shift_x_m)
                    .map(|point| [point.x, point.y])
            })
            .collect();
        let vehicles:Vec<_>=case.scene.vehicles.iter().map(|vehicle|serde_json::json!({"key":vehicle_draw_key(vehicle),"label":vehicle_overlay_label(vehicle,case.state.radar_info_mode,renderer.is_metric)})).collect();
        helpers.push(serde_json::json!({"name":case.name,"projection":view,"camera":camera,"points":points,"vehicles":vehicles}));
        if case.render {
            let before = renderer.camera_overlay_state().buffer_capacities();
            for warm in 0..2 {
                let frame = renderer.capture_draw(|renderer| {
                    renderer.gfx.clear(renderer.current_theme().bg)?;
                    renderer.draw_camera_projected_overlay(
                        &case.scene,
                        &case.state,
                        if warm == 0 { None } else { Some(&view) },
                    )?;
                    renderer.gfx.rectangle(
                        Rect::new(f64::from(size.0) - 20., 5., 10., 10.),
                        [255, 0, 255, 255],
                    )
                })?;
                let path = output.join(format!("{}-{warm}.png", case.name));
                renderer.write_png(&frame, &path)?;
                captures.push(serde_json::json!({"name":case.name,"warm":warm,"width":frame.width,"height":frame.height,"path":path}));
            }
            let (points, visible, native) = renderer.camera_overlay_state().buffer_capacities();
            buffers.push(serde_json::json!({"name":case.name,"points":points,"visible":visible,"native":native,"before":before,"strip_available":renderer.gfx.triangle_strip2d_available(),"profile":renderer.profile_samples()}));
            renderer.clear_profile_samples();
        }
    }
    if let Some(mut renderer) = renderer {
        close(&mut renderer, &mut closes)?;
    }
    write_result(&output, &captures, &helpers, &buffers, &closes)?;
    Ok(())
}
fn configure(renderer: &mut ClusterUiRenderer, case: &Case) {
    renderer.set_theme_mode(&case.options.theme_mode);
    renderer.set_screen_mode(case.options.screen_mode);
    renderer.set_panel_layout(case.options.panel_layout);
    renderer.set_display_preferences(&case.options.language, case.options.is_metric);
    let state = renderer.camera_overlay_state_mut();
    state.wide = case.wide;
    state.pitch_offset_deg = case.pitch_deg;
    state.z_offset_m = case.z_offset;
}
fn close(
    renderer: &mut ClusterUiRenderer,
    closes: &mut Vec<serde_json::Value>,
) -> openpilot_cluster::renderer::Result<()> {
    renderer.close()?;
    let (points, visible, native) = renderer.camera_overlay_state().buffer_capacities();
    assert_eq!((points, visible, native), (0, 0, 0));
    closes.push(serde_json::json!({"points":points,"visible":visible,"native":native}));
    Ok(())
}
fn write_result(
    output: &Path,
    captures: &[serde_json::Value],
    helpers: &[serde_json::Value],
    buffers: &[serde_json::Value],
    closes: &[serde_json::Value],
) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::write(
        output.join("native-result.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"frames":captures.len(),"captures":captures,"helpers":helpers,"buffers":buffers,"closes":closes}),
        )?,
    )?;
    Ok(())
}
