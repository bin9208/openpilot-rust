//! Offline native replay. Analysis inversion never writes vehicle parameters.
pub mod identity;
use crate::{
    controller::{Controller, Input, Options, Output},
    point::Point,
    Error,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Inversion {
    #[default]
    Recorded,
    Normal,
    Flipped,
}

pub fn apply_inversion(points: &mut [Point], recorded_flipped: bool, choice: Inversion) {
    let desired = match choice {
        Inversion::Recorded => recorded_flipped,
        Inversion::Normal => false,
        Inversion::Flipped => true,
    };
    if desired != recorded_flipped {
        for point in points
            .iter_mut()
            .filter(|p| p.source.rsplit('.').next() == Some("frontRadar"))
        {
            point.y_rel = -point.y_rel;
            point.yv_rel = -point.yv_rel;
        }
    }
}

pub struct Replay {
    controller: Controller,
    choice: Inversion,
}
impl Replay {
    pub fn new(options: Options, choice: Inversion) -> Self {
        Self {
            controller: Controller::new(options),
            choice,
        }
    }
    pub fn update(&mut self, mut input: Input, recorded_flipped: bool) -> Result<Output, Error> {
        apply_inversion(&mut input.points, recorded_flipped, self.choice);
        self.controller.update(&input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn recorded_flip_is_preserved_and_analysis_affects_only_front() {
        let mut points: Vec<_> = ["frontRadar", "cornerLeft", "scc"]
            .into_iter()
            .map(|source| Point {
                source: source.into(),
                y_rel: -2.,
                yv_rel: -0.5,
                ..Point::default()
            })
            .collect();
        apply_inversion(&mut points, true, Inversion::Recorded);
        assert_eq!(points[0].y_rel, -2.);
        apply_inversion(&mut points, true, Inversion::Normal);
        assert_eq!((points[0].y_rel, points[0].yv_rel), (2., 0.5));
        assert_eq!(points[1].y_rel, -2.);
        assert_eq!(points[2].y_rel, -2.);
        apply_inversion(&mut points, false, Inversion::Flipped);
        assert_eq!(points[0].y_rel, -2.);
    }
}

pub mod log;

pub mod raw;

pub mod dbc;

#[cfg(feature = "native-skip-miri")]
pub mod package;

#[cfg(test)]
mod display_tests {
    use super::*;
    use capnp::{message::ReaderOptions, serialize};
    use openpilot_cereal::log_capnp::event;
    use openpilot_radarcan::{
        data::Data,
        point::{Point as RadarPoint, Source},
    };

    #[test]
    fn display_wire_objects_remain_separate_and_analysis_flip_is_applied_once() {
        let mut data = Data {
            points: vec![RadarPoint {
                track_id: 7,
                y_rel: 1.25,
                yv_rel: 0.5,
                ..RadarPoint::default()
            }],
            display_points: vec![
                RadarPoint {
                    track_id: 7,
                    y_rel: -3.5,
                    yv_rel: -0.25,
                    ..RadarPoint::default()
                },
                RadarPoint {
                    track_id: 8,
                    y_rel: 2.5,
                    radar_source: Source::Corner235,
                    ..RadarPoint::default()
                },
            ],
            has_display_data: true,
            display_points_present: true,
            points_present: true,
            ..Data::default()
        };
        data.set_flip(true);
        data.set_flip(true);
        let bytes = openpilot_radarcan::wire::encode(&data, true, 100).unwrap();
        let message = serialize::read_message(bytes.as_slice(), ReaderOptions::new()).unwrap();
        let event::LiveTracks(tracks) = message
            .get_root::<event::Reader<'_>>()
            .unwrap()
            .which()
            .unwrap()
        else {
            panic!("wrong event")
        };
        let tracks = tracks.unwrap();
        assert!(tracks.get_has_display_data());
        assert!(tracks.get_radar_track_flipped());
        let mut control = crate::wire::points(tracks).unwrap();
        let mut display = crate::wire::display_points(tracks).unwrap();
        assert_eq!(control.len(), 1);
        assert_eq!(display.len(), 2);
        assert_eq!((control[0].y_rel, display[0].y_rel), (-1.25, 3.5));
        apply_inversion(&mut control, true, Inversion::Recorded);
        apply_inversion(&mut display, true, Inversion::Recorded);
        assert_eq!((control[0].y_rel, display[0].y_rel), (-1.25, 3.5));
        apply_inversion(&mut control, true, Inversion::Normal);
        apply_inversion(&mut display, true, Inversion::Normal);
        assert_eq!(
            (control[0].y_rel, display[0].y_rel, display[0].yv_rel),
            (1.25, -3.5, -0.25)
        );
        assert_eq!(display[1].y_rel, 2.5);
    }
}
