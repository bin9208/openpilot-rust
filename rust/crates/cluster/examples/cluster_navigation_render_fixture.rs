use openpilot_cluster::models::ClusterUiState;
use openpilot_cluster::renderer::tokens::{NAVI_TRAFFIC_PANEL_RIGHT, NAVI_TRAFFIC_PANEL_Y};
use openpilot_cluster::renderer::{ClusterUiRenderer, Options, StatsProvider, SystemStats};
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
    surface: String,
    options: Options,
    state: ClusterUiState,
    monotonic: f64,
    wall_time: f64,
    #[serde(default)]
    replay: openpilot_cluster::renderer::hud::ReplayControls,
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
        return Err("cluster_navigation_render_fixture ROOT INPUT OUTPUT".into());
    };
    let root = PathBuf::from(root);
    let output = PathBuf::from(output);
    std::fs::create_dir_all(&output)?;
    let request: Request = serde_json::from_slice(&std::fs::read(input)?)?;
    let mut renderer: Option<ClusterUiRenderer> = None;
    let mut dimensions = None;
    let mut captures = Vec::new();
    for case in request.cases {
        let size = (case.options.width, case.options.height);
        if dimensions != Some(size) {
            if let Some(mut renderer) = renderer.take() {
                renderer.close()?;
            }
            let mut next = ClusterUiRenderer::new(&root, case.options.clone());
            next.set_stats_provider(Box::new(FixedStats(request.system_stats.clone())));
            next.open(true)?;
            renderer = Some(next);
            dimensions = Some(size);
        }
        let renderer = renderer.as_mut().ok_or("renderer missing")?;
        renderer.set_theme_mode(&case.options.theme_mode);
        renderer.set_screen_mode(case.options.screen_mode);
        renderer.set_panel_layout(case.options.panel_layout);
        renderer.set_display_preferences(&case.options.language, case.options.is_metric);
        for warm in 0..2 {
            let frame = renderer.capture_draw(|renderer| draw_case(renderer, &case))?;
            let path = output.join(format!("{}-{warm}.png", case.name));
            renderer.write_png(&frame, &path)?;
            captures.push(serde_json::json!({"name": case.name, "warm": warm, "width": frame.width, "height": frame.height, "path": path}));
        }
    }
    if let Some(mut renderer) = renderer {
        renderer.close()?;
    }
    std::fs::write(
        output.join("native-result.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"frames": captures.len(), "captures": captures}),
        )?,
    )?;
    Ok(())
}

fn draw_case(
    renderer: &mut ClusterUiRenderer,
    case: &Case,
) -> openpilot_cluster::renderer::Result<()> {
    renderer.gfx.clear(renderer.current_theme().bg)?;
    renderer.gfx.push_matrix()?;
    let result = (|| {
        if !matches!(case.surface.as_str(), "alerts" | "hud" | "replay") {
            renderer.gfx.scale(
                f64::from(case.options.width) / 1920.0,
                f64::from(case.options.height) / 480.0,
                1.0,
            )?;
        }
        draw_surface(renderer, case)
    })();
    let pop = renderer.gfx.pop_matrix();
    result.and(pop)
}

fn draw_surface(
    renderer: &mut ClusterUiRenderer,
    case: &Case,
) -> openpilot_cluster::renderer::Result<()> {
    use openpilot_cluster::renderer::{FrameClock, Rect};
    match case.surface.as_str() {
        "replay" => renderer.draw_route_replay_controls(&case.replay),
        "hud" => renderer.draw_hud(
            &case.state,
            FrameClock {
                monotonic: case.monotonic,
                perf_counter: case.monotonic,
                wall_time: case.wall_time,
            },
            None,
        ),
        "status" => {
            renderer.draw_drive_status(&case.state)?;
            renderer.draw_git_status(
                case.state.git_status.as_ref(),
                case.state.network_address.as_deref(),
                case.state.actual_fps,
            )?;
            renderer.draw_cluster_core_usage(case.state.cluster_core_usage_text.as_deref(), 1920.0)
        }
        "speed" => renderer.draw_speed_block(&case.state, 0.0),
        "route" => renderer.draw_route_overlay(case.state.route_overlay.as_ref()),
        "plot-full" | "plot-right" | "plot-compact" => {
            let bounds = match case.surface.as_str() {
                "plot-full" => Rect::new(500.0, 18.0, 1392.0, 444.0),
                "plot-right" => Rect::new(1124.0, 1.0, 792.0, 478.0),
                _ => Rect::new(100.0, 20.0, 650.0, 440.0),
            };
            renderer.draw_debug_plot(case.state.debug_plot.as_ref(), bounds, case.monotonic)
        }
        "system" => renderer.draw_system_dashboard_panel(&case.state),
        "trip" => renderer.draw_trip_report_panel_contents(&case.state),
        "live-debug" => renderer.draw_live_debug_panel(&case.state),
        "dashboard" => renderer.draw_navi_dashboard(&case.state),
        "live" => renderer.draw_navi_live_panel(&case.state, case.wall_time),
        "debug" => renderer.draw_navi_debug_panel(case.state.navi_debug.as_ref()),
        "traffic" => renderer.draw_navi_traffic_light_panel(
            case.state
                .navi_debug
                .as_ref()
                .and_then(|value| value.traffic_light.as_ref())
                .ok_or(openpilot_cluster::renderer::Error::Contract(
                    "fixture traffic missing",
                ))?,
            NAVI_TRAFFIC_PANEL_RIGHT,
            NAVI_TRAFFIC_PANEL_Y,
            case.monotonic,
        ),
        _ => draw_widgets(renderer, case),
    }
}
fn draw_widgets(
    renderer: &mut ClusterUiRenderer,
    case: &Case,
) -> openpilot_cluster::renderer::Result<()> {
    use openpilot_cluster::renderer::hud::SignalSide;
    match case.surface.as_str() {
        "alerts" => renderer.draw_alert_overlay(case.state.alert.as_ref()),
        "clock" => renderer.draw_center_clock(
            &case.state,
            openpilot_cluster::renderer::tokens::TOP_CLOCK_CENTER_X,
        ),
        "gauges" => renderer
            .draw_accel_block(&case.state)
            .and_then(|()| renderer.draw_steering_output_block(&case.state)),
        "signals" => {
            renderer.draw_turn_signal(
                SignalSide::Left,
                case.state.left_signal,
                case.state.debug_ui_visible,
                0.0,
            )?;
            renderer.draw_turn_signal(
                SignalSide::Right,
                case.state.right_signal,
                case.state.debug_ui_visible,
                0.0,
            )
        }
        _ => Err(openpilot_cluster::renderer::Error::Contract(
            "fixture surface unknown",
        )),
    }
}
