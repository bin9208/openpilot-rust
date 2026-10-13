//! Source world geometry, OBJ resources and metric labels.
mod coins;
pub mod helpers;
mod labels;
pub mod obj;
pub use coins::*;
pub use labels::*;
pub mod point_cache;

mod policy;
mod projection;
mod resources;
mod strip;
mod vehicle;
mod vehicle_geometry;
pub use resources::{ModelLoadTiming, WorldResources};

mod badges;
mod frame;
mod metric_layout;
mod radar_labels;
mod tpms;
pub use metric_layout::{metric_lines, MetricLabel, MetricLine};

#[expect(
    clippy::cast_precision_loss,
    reason = "The source design width is the exact integer 1920"
)]
const DESIGN_WIDTH: f64 = crate::config::DESIGN_WIDTH as f64;
#[expect(
    clippy::cast_precision_loss,
    reason = "The source design height is the exact integer 480"
)]
const DESIGN_HEIGHT: f64 = crate::config::DESIGN_HEIGHT as f64;
