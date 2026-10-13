use openpilot_cluster::{
    models::ClusterUiState,
    renderer::{world::WorldResources, ClusterUiRenderer, Options},
    scene::ClusterScene,
};
use serde::Deserialize;
use std::path::PathBuf;
#[derive(Deserialize)]
struct Request {
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    assets_root: PathBuf,
    options: Options,
    state: ClusterUiState,
    scene: ClusterScene,
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, _root, input, output] = args.as_slice() else {
        return Err("cluster_world_render_fixture ROOT INPUT OUTPUT".into());
    };
    let output = PathBuf::from(output);
    std::fs::create_dir_all(&output)?;
    let request: Request = serde_json::from_slice(&std::fs::read(input)?)?;
    let mut renderer: Option<ClusterUiRenderer> = None;
    let mut provider = None;
    let mut captures = Vec::new();
    let mut resources = Vec::new();
    let mut closures = Vec::new();
    let mut model_lifecycle = None;
    for case in request.cases {
        if provider.as_ref() != Some(&case.assets_root) {
            if let Some(mut previous) = renderer.take() {
                previous.close()?;
                closures.push(serde_json::json!({"model":previous.world_resources().model_loaded(),"strip_cache":previous.world_resources().point_cache_len()}));
                assert!(!previous.world_resources().model_loaded());
                assert_eq!(previous.world_resources().point_cache_len(), 0);
            }
            let mut next = ClusterUiRenderer::new(&case.assets_root, case.options.clone());
            next.set_profile_enabled(true);
            next.open(true)?;
            if model_lifecycle.is_none() && next.world_resources().model_loaded() {
                model_lifecycle = Some(probe_model_lifecycle(
                    &mut next,
                    &case.assets_root,
                    &output,
                )?);
            }
            assert_eq!(next.world_resources().model_loaded(),case.assets_root.join("openpilot/selfdrive/carrot/cluster/assets/models/cybertruck/cybertruck_cluster.obj").is_file());
            assert_eq!(
                next.tpms_car_texture().is_some(),
                case.assets_root
                    .join("openpilot/selfdrive/carrot/cluster/assets/images/tpms_toy_car.png")
                    .is_file()
            );
            renderer = Some(next);
            provider = Some(case.assets_root.clone());
        }
        let renderer = renderer.as_mut().ok_or("world renderer missing")?;
        renderer.set_theme_mode(&case.options.theme_mode);
        renderer.set_screen_mode(case.options.screen_mode);
        renderer.set_panel_layout(case.options.panel_layout);
        renderer.set_display_preferences(&case.options.language, case.options.is_metric);
        let mut warm_cache = None;
        for warm in 0..2 {
            let frame = renderer.capture_draw(|renderer| {
                renderer.gfx.clear(renderer.current_theme().bg)?;
                renderer.draw_scene(&case.scene, &case.state)?;
                renderer.draw_tpms_status(&case.state)
            })?;
            let path = output.join(format!("{}-{warm}.png", case.name));
            renderer.write_png(&frame, &path)?;
            let cache = renderer.world_resources().point_cache_len();
            if let Some(first) = warm_cache {
                assert_eq!(first, cache);
            } else {
                warm_cache = Some(cache);
            }
            captures.push(serde_json::json!({"name":case.name,"warm":warm,"width":frame.width,"height":frame.height,"path":path}));
        }
        resources.push(serde_json::json!({"name":case.name,"model":renderer.world_resources().model_loaded(),"icon":renderer.tpms_car_texture().is_some(),"strip_cache":renderer.world_resources().point_cache_len(),"profile":renderer.profile_samples()}));
        renderer.clear_profile_samples();
    }
    if let Some(mut renderer) = renderer {
        renderer.close()?;
        assert!(!renderer.world_resources().model_loaded());
        assert_eq!(renderer.world_resources().point_cache_len(), 0);
        closures.push(serde_json::json!({"model":renderer.world_resources().model_loaded(),"strip_cache":renderer.world_resources().point_cache_len()}));
    }
    std::fs::write(
        output.join("native-result.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"frames":captures.len(),"captures":captures,"resources":resources,"closed_model":true,"closed_strip_cache":true,"closures":closures,"model_lifecycle":model_lifecycle}),
        )?,
    )?;
    Ok(())
}

fn probe_model_lifecycle(
    renderer: &mut ClusterUiRenderer,
    assets: &std::path::Path,
    output: &std::path::Path,
) -> openpilot_cluster::renderer::Result<serde_json::Value> {
    let missing = output.join("missing-model-provider");
    assert!(!missing.exists());
    let mut probe = WorldResources::default();
    assert!(probe.load_model(&mut renderer.gfx, &missing)?.is_empty());
    assert!(!probe.model_loaded());
    assert!(probe.load_model(&mut renderer.gfx, assets)?.is_empty());
    assert!(!probe.model_loaded());
    probe.close_resources();
    let stages = probe.load_model(&mut renderer.gfx, assets)?;
    assert!(probe.model_loaded());
    let names: Vec<_> = stages.iter().map(|stage| stage.name).collect();
    assert_eq!(
        names,
        [
            "vehicle_model.parse_obj",
            "vehicle_model.upload_mesh",
            "vehicle_model.load_from_mesh"
        ]
    );
    probe.close_resources();
    assert!(!probe.model_loaded());
    Ok(
        serde_json::json!({"missing_first":true,"same_owner_retry_blocked":true,"close_resets_attempt":true,"real_model_reloaded":true,"stages":names,"closed_model":true}),
    )
}
