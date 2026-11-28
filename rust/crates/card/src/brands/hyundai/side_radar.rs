//! `CcncFrontRadarSide`: draw the adjacent-lane cars that the front radar sees
//! (radarState leadLeft / leadRight) in the empty LF / RF slots of CCNC 0x162,
//! so an HDA1 cluster without corner radar shows side cars like stock HDA2.
//!
//! The owner's IONIQ 5 PE HDA1 camera sends 0x162 with only FF filled; LF, RF,
//! LR and RR stay 0 (rlog 00000ce3 segs 6 and 11). A slot that the stock
//! sender already fills (corner-radar cars) is never touched. LR / RR stay
//! empty: the front radar does not see beside or behind the car.
//!
//! 0x162 slot meaning, as read by the cluster replay decoder: DISTANCE is metres
//! ahead, LATERAL an unsigned offset (the slot gives the side), DETECT 3 a grey
//! car.

use super::{
    wire::{get, Values},
    Error,
};
use openpilot_control_policy::math::clip;

/// 0: off, 1: radar side leads, 2: fixed test cars (LF 15 m, RF 30 m).
pub type Mode = u8;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct SideLead {
    pub d_rel: f64,
    /// Lateral offset from the planned path (radard `dPath`).
    pub d_path: f64,
    /// Absolute speed of the side car.
    pub v_lead: f64,
}

/// Distance window the slot is filled for.
pub const MIN_DISTANCE: f64 = 4.;
pub const MAX_DISTANCE: f64 = 60.;
/// radard splits left / centre / right at |dPath| 1.8 m; beyond 5 m it is
/// usually the second lane over or the road edge.
pub const MIN_LATERAL: f64 = 1.8;
pub const MAX_LATERAL: f64 = 5.;
/// A side object slower than this is taken for a parked car or roadside
/// clutter unless we are slow ourselves (a queue beside us in traffic).
pub const STATIONARY_SPEED: f64 = 2.;
pub const SLOW_EGO_SPEED: f64 = 5.;
/// Counted in 0x162 frames (20 Hz): 0.15 s to show, 0.5 s held after a drop.
const SHOW_FRAMES: u32 = 3;
const HOLD_FRAMES: u32 = 10;
/// Grey car (1 / 2 are boxes, 4 a white car).
const GREY_CAR: f64 = 3.;
const TEST_CARS: [(f64, f64); 2] = [(15., 3.), (30., 3.5)];
const SLOTS: [&str; 2] = ["LF", "RF"];

#[must_use]
pub fn usable(lead: &SideLead, v_ego: f64) -> Option<(f64, f64)> {
    let lateral = lead.d_path.abs();
    let values_ok = [lead.d_rel, lead.d_path, lead.v_lead]
        .iter()
        .all(|value| value.is_finite());
    let moving = lead.v_lead.abs() >= STATIONARY_SPEED || v_ego < SLOW_EGO_SPEED;
    (values_ok
        && (MIN_DISTANCE..=MAX_DISTANCE).contains(&lead.d_rel)
        && (MIN_LATERAL..=MAX_LATERAL).contains(&lateral)
        && moving)
        .then_some((lead.d_rel, lateral))
}

#[derive(Default)]
struct Slot {
    seen: u32,
    missing: u32,
    shown: Option<(f64, f64)>,
}

impl Slot {
    fn update(&mut self, sample: Option<(f64, f64)>) -> Option<(f64, f64)> {
        match sample {
            Some(value) => {
                self.seen = self.seen.saturating_add(1);
                self.missing = 0;
                if self.shown.is_some() || self.seen >= SHOW_FRAMES {
                    self.shown = Some(value);
                }
            }
            None => {
                self.seen = 0;
                self.missing = self.missing.saturating_add(1);
                if self.missing > HOLD_FRAMES {
                    self.shown = None;
                }
            }
        }
        self.shown
    }
}

#[derive(Default)]
pub struct SideRadar {
    slots: [Slot; 2],
}

impl SideRadar {
    /// Fill the empty LF / RF slots of one 0x162 frame.
    pub fn apply(
        &mut self,
        data: &mut Values,
        mode: Mode,
        sides: [Option<SideLead>; 2],
        v_ego: f64,
    ) -> Result<(), Error> {
        if mode == 0 {
            *self = Self::default();
            return Ok(());
        }
        for (index, slot) in SLOTS.iter().enumerate() {
            let target = if mode == 2 {
                Some(TEST_CARS[index])
            } else {
                let sample = sides[index].and_then(|lead| usable(&lead, v_ego));
                self.slots[index].update(sample)
            };
            let detect = format!("{slot}_DETECT");
            if get(data, &detect)? != 0. {
                continue;
            }
            let Some((distance, lateral)) = target else {
                continue;
            };
            data.insert(detect.as_str().into(), GREY_CAR);
            data.insert(
                format!("{slot}_DETECT_DISTANCE").as_str().into(),
                clip(distance, 0.1, 204.5),
            );
            data.insert(
                format!("{slot}_DETECT_LATERAL").as_str().into(),
                clip(lateral, 0., 12.7),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame() -> Values {
        let mut data = Values::new();
        for slot in ["LF", "RF"] {
            for suffix in ["DETECT", "DETECT_DISTANCE", "DETECT_LATERAL"] {
                data.insert(format!("{slot}_{suffix}").as_str().into(), 0.);
            }
        }
        data
    }

    fn lead(d_rel: f64, d_path: f64, v_lead: f64) -> Option<SideLead> {
        Some(SideLead {
            d_rel,
            d_path,
            v_lead,
        })
    }

    #[test]
    fn filter_drops_far_close_outer_and_parked() {
        assert_eq!(usable(&lead(20., 3., 15.).unwrap(), 15.), Some((20., 3.)));
        assert_eq!(
            usable(&lead(20., -3.2, 15.).unwrap(), 15.),
            Some((20., 3.2))
        );
        assert!(usable(&lead(3., 3., 15.).unwrap(), 15.).is_none());
        assert!(usable(&lead(89.7, 3., 15.).unwrap(), 15.).is_none());
        assert!(usable(&lead(20., 11.1, 15.).unwrap(), 15.).is_none());
        assert!(usable(&lead(20., 1.0, 15.).unwrap(), 15.).is_none());
        // Parked car while we drive; a stopped queue while we crawl.
        assert!(usable(&lead(20., 3., 0.).unwrap(), 15.).is_none());
        assert!(usable(&lead(20., 3., 0.).unwrap(), 1.).is_some());
        assert!(usable(&lead(f64::NAN, 3., 10.).unwrap(), 15.).is_none());
    }

    #[test]
    fn shows_after_three_frames_and_holds_half_a_second() {
        let mut radar = SideRadar::default();
        let mut data = frame();
        for _ in 0..2 {
            radar
                .apply(&mut data, 1, [lead(20., 3., 15.), None], 15.)
                .unwrap();
            assert_eq!(data["LF_DETECT"], 0.);
        }
        radar
            .apply(&mut data, 1, [lead(20., 3., 15.), None], 15.)
            .unwrap();
        assert_eq!(data["LF_DETECT"], 3.);
        assert_eq!(data["LF_DETECT_DISTANCE"], 20.);
        assert_eq!(data["LF_DETECT_LATERAL"], 3.);
        assert_eq!(data["RF_DETECT"], 0.);
        for _ in 0..10 {
            let mut data = frame();
            radar.apply(&mut data, 1, [None, None], 15.).unwrap();
            assert_eq!(data["LF_DETECT"], 3.);
        }
        let mut data = frame();
        radar.apply(&mut data, 1, [None, None], 15.).unwrap();
        assert_eq!(data["LF_DETECT"], 0.);
    }

    #[test]
    fn stock_slot_wins_and_off_leaves_frame_alone() {
        let mut radar = SideRadar::default();
        let mut data = frame();
        data.insert("RF_DETECT".into(), 3.);
        data.insert("RF_DETECT_DISTANCE".into(), 7.);
        radar.apply(&mut data, 2, [None, None], 15.).unwrap();
        assert_eq!(data["LF_DETECT"], 3.);
        assert_eq!(data["LF_DETECT_DISTANCE"], 15.);
        assert_eq!(data["RF_DETECT_DISTANCE"], 7.);

        let mut data = frame();
        let before = data.clone();
        for _ in 0..5 {
            radar
                .apply(&mut data, 0, [lead(20., 3., 15.); 2], 15.)
                .unwrap();
        }
        assert_eq!(data, before);
    }
}
