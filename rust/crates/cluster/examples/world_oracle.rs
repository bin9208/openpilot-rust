use openpilot_cluster::{
    config,
    models::ClusterUiState,
    renderer::world::{self, helpers, obj::ObjMesh},
    renderer::{ClusterUiRenderer, Options, Rect},
    scene::{CameraSpec, RadarPointMarker, Vec3, VehicleBox},
};
use serde::Deserialize;
use serde_json::json;
use std::{
    io::{self, BufRead, Write},
    path::PathBuf,
};
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Theme {
    Dark,
    Light,
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Input {
    Vehicle {
        vehicle: VehicleBox,
        metric: bool,
        theme: Theme,
        color_mode: i64,
    },
    Scale {
        distance: f64,
    },
    Mode {
        mode: i64,
    },
    Camera {
        camera: CameraSpec,
        point: Vec3,
    },
    Rectangles {
        left: Rect,
        right: Rect,
        bounds: helpers::Bounds,
    },
    Radar {
        point: RadarPointMarker,
        metric: bool,
    },
    Policy {
        state: Box<ClusterUiState>,
        options: Options,
    },
    Obj {
        path: PathBuf,
    },
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut output = io::stdout().lock();
    for line in io::stdin().lock().lines() {
        let query: Input = serde_json::from_str(&line?)?;
        let result = match query {
            Input::Vehicle {
                vehicle,
                metric,
                theme,
                color_mode,
            } => {
                let theme = match theme {
                    Theme::Dark => config::dark_theme(),
                    Theme::Light => config::light_theme(),
                };
                json!({"distance":world::vehicle_distance_label(&vehicle,metric),"speed":world::vehicle_speed_label(&vehicle,metric),
                    "distance_m":world::vehicle_distance_m(&vehicle),"lead_one":world::vehicle_has_lead_one_metrics(&vehicle),
                    "color":helpers::vehicle_metric_color(&vehicle,&theme,color_mode),
                    "coins":[world::camera_overlay_vehicle_coin_colors(&vehicle,false,false),world::camera_overlay_vehicle_coin_colors(&vehicle,true,false),
                             world::camera_overlay_vehicle_coin_colors(&vehicle,false,true),world::camera_overlay_vehicle_coin_colors(&vehicle,true,true)],
                    "stopped":world::camera_overlay_vehicle_is_stopped(&vehicle)})
            }
            Input::Scale { distance } => json!(helpers::world_label_scale(distance)),
            Input::Mode { mode } => json!([
                world::radar_info_shows_vehicle(mode),
                world::radar_info_shows_radar_points(mode),
                world::radar_info_shows_speed(mode),
                world::radar_info_shows_distance(mode)
            ]),
            Input::Camera { camera, point } => {
                json!({"forward":helpers::camera_forward(&camera).map(|value|[value.x,value.y,value.z]),"depth":helpers::camera_depth_m(point,&camera)})
            }
            Input::Rectangles {
                left,
                right,
                bounds,
            } => {
                json!({"overlap":helpers::rectangles_overlap(left,right),"inside":helpers::label_rect_inside_bounds(left,bounds)})
            }
            Input::Radar { point, metric } => {
                json!({"distance":world::radar_point_distance_label(&point,metric),"speed":world::radar_point_speed_label(&point,metric)})
            }
            Input::Policy { state, options } => {
                let renderer = ClusterUiRenderer::new(std::path::Path::new("."), options);
                json!({"shift":renderer.world_view_shift_x(&state),"design_shift":renderer.world_view_shift_design_x(&state),"left":renderer.turn_signal_center_x_offset(&state,"left"),"right":renderer.turn_signal_center_x_offset(&state,"right"),"bounds":renderer.world_label_bounds(10.,20.,30.,40.)})
            }
            Input::Obj { path } => {
                let mesh = ObjMesh::load(&path)?;
                json!({"vertices":mesh.vertices.into_iter().map(f64::from).collect::<Vec<_>>(),"normals":mesh.normals.into_iter().map(f64::from).collect::<Vec<_>>(),"colors":mesh.colors})
            }
        };
        serde_json::to_writer(&mut output, &result)?;
        writeln!(output)?;
        output.flush()?;
    }
    Ok(())
}
