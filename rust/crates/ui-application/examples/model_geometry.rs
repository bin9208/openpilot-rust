use openpilot_ui_application::onroad::{
    model_renderer::{
        points::{ModelPoint, SamplePoint},
        projection::{Projection, Ribbon},
    },
    path_geometry::{self, PathRibbon},
    road_markings,
};
use openpilot_ui_framework::geometry::{Point, Rect};
use serde::Deserialize;
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Kind {
    Sample,
    Path,
    Lanes,
    Blindspot,
    Quads,
    BigRibbon,
    SmallRibbon,
    Point,
}
#[derive(Deserialize)]
struct Case {
    kind: Kind,
    line: Vec<ModelPoint>,
    transform: [[f64; 3]; 3],
    clip: [f32; 4],
    distances: Vec<f64>,
    width: f64,
    shift: f64,
    z_start: f64,
    z_end: f64,
    end: usize,
    distance: f64,
    invert: bool,
}
fn points(points: Vec<Point>) -> Vec<[f32; 2]> {
    points.into_iter().map(|p| [p.x, p.y]).collect()
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cases: Vec<Case> = serde_json::from_reader(std::io::stdin())?;
    let mut outputs = Vec::with_capacity(cases.len());
    for case in cases {
        let [x, y, width, height] = case.clip;
        let projection = Projection {
            transform: case
                .transform
                .map(|row| row.map(openpilot_ui_framework::text_layout::float)),
            clip: Rect {
                x,
                y,
                width,
                height,
            },
        };
        let output = match case.kind {
            Kind::Sample => {
                serde_json::to_value(path_geometry::sample_path(&case.line, &case.distances)?)?
            }
            Kind::Path => serde_json::to_value(points(path_geometry::project_path(
                &projection,
                &case
                    .line
                    .iter()
                    .copied()
                    .map(SamplePoint::from)
                    .collect::<Vec<_>>(),
                PathRibbon {
                    width: case.width,
                    height: [case.z_start, case.z_end],
                    allow_invert: case.invert,
                },
            )?))?,
            Kind::Lanes => serde_json::to_value(
                road_markings::project_lane_segments(
                    &projection,
                    &road_markings::lane_dash_segments(&case.line, case.distance)?,
                    case.width,
                )
                .into_iter()
                .map(points)
                .collect::<Vec<_>>(),
            )?,
            Kind::Blindspot => serde_json::to_value(points(
                road_markings::project_blindspot_barrier(&projection, &case.line, case.shift),
            ))?,
            Kind::Quads => serde_json::to_value(
                road_markings::blindspot_barrier_quads(&road_markings::project_blindspot_barrier(
                    &projection,
                    &case.line,
                    case.shift,
                ))
                .into_iter()
                .map(|quad| quad.map(|p| [p.x, p.y]))
                .collect::<Vec<_>>(),
            )?,
            Kind::Point => serde_json::to_value(
                projection.point(case.line.first().copied().unwrap_or_default().into()),
            )?,
            Kind::BigRibbon | Kind::SmallRibbon => {
                let big = matches!(case.kind, Kind::BigRibbon);
                serde_json::to_value(points(projection.ribbon(
                    &case.line,
                    Ribbon {
                        half_width: case.width,
                        height: case.z_start,
                        shift: if big { case.shift } else { 0. },
                        end: case.end,
                        end_distance: big.then_some(case.distance),
                        allow_invert: case.invert,
                    },
                )?))?
            }
        };
        outputs.push(output);
    }
    serde_json::to_writer(std::io::stdout(), &outputs)?;
    Ok(())
}
