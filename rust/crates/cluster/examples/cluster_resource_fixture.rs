use openpilot_cluster::{
    models::{ClusterUiState, NaviMediaFrame, RouteOverlay},
    renderer::{ClusterUiRenderer, MediaFit, Options, Point, Rect, StatsProvider, SystemStats},
};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
struct Request {
    system_stats: SystemStats,
    cases: Vec<Case>,
}
#[derive(Deserialize)]
struct Case {
    name: String,
    action: String,
    options: Options,
    state: ClusterUiState,
    overlay: Option<RouteOverlay>,
    #[serde(default)]
    frame: Option<NaviMediaFrame>,
    now: f64,
    setters: bool,
    reopen: bool,
}
struct FixedStats(SystemStats);
impl StatsProvider for FixedStats {
    fn sample(&mut self, _now: f64) -> SystemStats {
        self.0.clone()
    }
    fn close(&mut self) {}
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    let [_, root, input, output] = args.as_slice() else {
        return Err("cluster_resource_fixture ROOT INPUT OUTPUT".into());
    };
    let root = PathBuf::from(root);
    let output = PathBuf::from(output);
    std::fs::create_dir_all(&output)?;
    let request: Request = serde_json::from_slice(&std::fs::read(input)?)?;
    let mut renderer: Option<ClusterUiRenderer> = None;
    let mut captures = Vec::new();
    let mut previous = None;
    for case in request.cases {
        if renderer.is_none() {
            let mut value = ClusterUiRenderer::new(&root, case.options.clone());
            value.set_stats_provider(Box::new(FixedStats(request.system_stats.clone())));
            value.open(true)?;
            value.set_profile_enabled(true);
            renderer = Some(value);
        }
        let value = renderer.as_mut().ok_or("renderer missing")?;
        if case.reopen {
            value.close()?;
            value.open(true)?;
            previous = None;
        }
        if case.setters {
            value.set_theme_mode(&case.options.theme_mode);
            value.set_screen_mode(case.options.screen_mode);
            value.set_panel_layout(case.options.panel_layout);
            value.set_display_preferences(&case.options.language, case.options.is_metric);
        }
        value.clear_profile_samples();
        if case.action == "trip" {
            value.prepare_trip_report_cache(&case.state, case.now)?;
        }
        let mut texture = None;
        if case.action == "route" {
            texture = value
                .route_video_texture_for_overlay(case.overlay.as_ref().ok_or("route missing")?)?;
        }
        let fitted = if case.action == "media" {
            value.navi_media_fitted_size(
                case.frame.as_ref(),
                Rect::new(30.25, 18.75, 530.5, 270.25),
                false,
            )?
        } else {
            None
        };
        let reuse = texture.and_then(|texture| {
            previous
                .filter(|(_, width, height)| *width == texture.width && *height == texture.height)
                .map(|(id, _, _)| id == texture.id)
        });
        previous = texture.map(|texture| (texture.id, texture.width, texture.height));
        let frame = value.capture_draw(|value| {
            value.gfx.clear(value.current_theme().bg)?;
            if let Some(texture) = texture {
                value.gfx.texture_pro(
                    texture,
                    Rect::new(
                        0.0,
                        0.0,
                        f64::from(texture.width),
                        f64::from(texture.height),
                    ),
                    Rect::new(30.25, 18.75, 530.5, 270.25),
                    Point::default(),
                    0.0,
                    [255, 255, 255],
                )?;
            }
            if case.action == "trip" {
                value.draw_trip_report_panel(&case.state)?;
            }
            if case.action == "media" {
                value.draw_navi_media(
                    case.frame.as_ref(),
                    Rect::new(30.25, 18.75, 530.5, 270.25),
                    MediaFit::default(),
                )?;
            }
            Ok(())
        })?;
        let path = output.join(format!("{}-0.png", case.name));
        value.write_png(&frame, &path)?;
        captures.push(serde_json::json!({"name":case.name,"warm":0,"width":frame.width,"height":frame.height,"path":path,"fitted":fitted,"reuse_previous":reuse,"profile_names":value.profile_samples().iter().map(|(name,_)|name).collect::<Vec<_>>() }));
    }
    if let Some(mut value) = renderer {
        value.close()?;
    }
    std::fs::write(
        output.join("native-result.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"frames":captures.len(),"captures":captures}),
        )?,
    )?;
    Ok(())
}
