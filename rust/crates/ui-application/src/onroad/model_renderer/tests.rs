use super::{points::SamplePoint, projection::Projection, settings::Settings};
use crate::{
    params::Read,
    render_diagnostics::{Clock, Timings},
    Error,
};
use std::cell::{Cell, RefCell};
struct Parameters(RefCell<Vec<String>>);
impl Read for Parameters {
    fn bytes(&self, key: &str) -> Result<Option<Vec<u8>>, Error> {
        self.0.borrow_mut().push(key.into());
        Ok(Some(b"14".to_vec()))
    }
}
#[test]
fn parameter_refresh_preserves_order_and_one_second_boundary() {
    let parameters = Parameters(RefCell::new(Vec::new()));
    let mut settings = Settings::default();
    settings.refresh(&parameters, 100.).unwrap();
    let expected = [
        "ShowLaneInfo",
        "ShowRadarInfo",
        "ShowPathMode",
        "ShowPathColor",
        "ShowPathModeLane",
        "ShowPathColorLane",
        "ShowPathColorCruiseOff",
        "CarrotTireTrajectory",
    ];
    assert_eq!(*parameters.0.borrow(), expected);
    parameters.0.borrow_mut().clear();
    settings.refresh(&parameters, 100.999999).unwrap();
    assert!(parameters.0.borrow().is_empty());
    settings.refresh(&parameters, 101.).unwrap();
    assert_eq!(*parameters.0.borrow(), expected);
    assert_eq!(settings.next_refresh, 102.);
}
#[test]
fn single_point_projection_rejects_nonfinite_and_near_zero_depth() {
    let mut projection = Projection {
        clip: openpilot_ui_framework::geometry::Rect {
            x: -10.,
            y: -10.,
            width: 20.,
            height: 20.,
        },
        ..Default::default()
    };
    projection.set_transform([[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]]);
    assert!(projection.point(SamplePoint([1., 2., 0.])).is_none());
    assert!(projection
        .point(SamplePoint([1., 2., 0.00000099]))
        .is_none());
    assert!(projection.point(SamplePoint([f64::NAN, 2., 1.])).is_none());
    assert!(projection
        .point(SamplePoint([1., f64::INFINITY, 1.]))
        .is_none());
    assert_eq!(
        projection.point(SamplePoint([1., 2., -1.])).unwrap().0,
        [-1., -2.]
    );
}
struct Ticks(Cell<i128>);
impl Clock for Ticks {
    fn monotonic_ns(&self) -> i128 {
        let v = self.0.get();
        self.0.set(v + 1_000_000);
        v
    }
    fn thread_ns(&self) -> i128 {
        self.monotonic_ns()
    }
}
#[test]
fn timing_accumulates_repeated_stage_and_propagates_failure() {
    let mut timing = Timings::new(Ticks(Cell::new(0)));
    timing.start();
    timing.call("path", || Ok(())).unwrap();
    assert!(timing
        .call("path", || Err::<(), _>(Error::Contract("failed frame")))
        .is_err());
    let values = timing.finish().unwrap();
    assert_eq!(values.iter().filter(|(k, _)| k == "path_ms").count(), 1);
    assert!(values.iter().any(
        |(k, v)| k == "path_ms" && matches!(v, openpilot_logging::Number::Float(n) if *n == 6.)
    ));
}
