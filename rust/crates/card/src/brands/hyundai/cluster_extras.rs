//! Extra HDA2-style items on the CCNC cluster of camera-SCC CAN-FD cars
//! (`CcncClusterDisplay` = 1), plus a raw-value probe on `CanfdDebug` >= 100.
//!
//! Items, with the value meanings the DBC lists:
//! - speed-limit sign (0x162 SPEEDLIMIT, SPEEDLIMIT_FLASH 2 normal / 4 red):
//!   carrot's navigation limit while the camera reads no sign;
//! - speed-limit assist arrow (0x161 SLA_ICON 1/2 white up/down, 3/4 green):
//!   which way the set speed is from that limit;
//! - auto set-speed popup (0x1E0 HDA_AutoSetSpd*): the set speed changed with
//!   no driver button;
//! - second car ahead (0x162 FF_*_ALT; the DBC lists only boxes and cones);
//! - lane line distance (0x161 LANELINE_*_POSITION, 0.1 m: the owner car's
//!   camera sends 10..20 for 0.9..1.7 m and mostly 15);
//! - forward-collision picture (0x161 FCA_ICON 2 red, FCA_IMAGE 2 visible) on
//!   openpilot's FCW;
//! - lane view zoom (0x161 LANE_ZOOM 0 "ZOOM", stock 1) during a lane change.
//!
//! Probe: `CanfdDebug` = code * 100 + value forces one signal so the meaning of
//! a value can be seen on the cluster (values 1..99 keep their old meaning).

use super::{
    lead::{self, Lead},
    wire::{get, set, Values},
    Error,
};
use openpilot_cereal::log_capnp::event;
use openpilot_control_policy::math::clip;
use openpilot_messaging::state::State as Subscribers;

/// The farther of leadOne / leadTwo when it is at least 5 m behind the nearer
/// one and within 1.8 m of the path: (distance, lateral from the path).
pub fn second_lead(leads: &[Lead], path: (&[f64], &[f64])) -> Result<Option<(f64, f64)>, Error> {
    let mut valid: Vec<&Lead> = leads
        .iter()
        .filter(|lead| {
            lead.status
                && lead.d_rel > 0.
                && [lead.d_rel, lead.y_rel, lead.v_rel]
                    .iter()
                    .all(|value| value.is_finite())
        })
        .collect();
    valid.sort_by(|a, b| a.d_rel.total_cmp(&b.d_rel));
    let [first, second, ..] = valid[..] else {
        return Ok(None);
    };
    if second.d_rel - first.d_rel < 5. {
        return Ok(None);
    }
    let lateral = lead::lateral(second, path)?;
    Ok((lateral.abs() <= 1.8).then_some((second.d_rel, lateral)))
}

/// One forced signal from `CanfdDebug` (code * 100 + value).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Probe {
    pub code: u8,
    pub value: u8,
}

pub const PROBE_LANE_ZOOM: u8 = 1;
pub const PROBE_FCA_IMAGE: u8 = 2;
pub const PROBE_FCA_ICON: u8 = 3;
pub const PROBE_SLA_ICON: u8 = 4;
pub const PROBE_SPEEDLIMIT_FLASH: u8 = 5;
pub const PROBE_SIGNS: u8 = 6;
pub const PROBE_FF_DETECT_ALT: u8 = 7;
pub const PROBE_AUTO_SET_STA: u8 = 8;
pub const PROBE_AUTO_SET_UPDATE: u8 = 9;
pub const PROBE_LANE_POSITION: u8 = 10;
pub const PROBE_LF_LATERAL: u8 = 11;
pub const PROBE_FF_DETECT: u8 = 12;
pub const PROBE_LAST: u8 = PROBE_FF_DETECT;

impl Probe {
    #[must_use]
    pub fn from_debug(debug: i32) -> Option<Self> {
        let code = u8::try_from(debug / 100).ok()?;
        let value = u8::try_from(debug % 100).ok()?;
        (1..=PROBE_LAST)
            .contains(&code)
            .then_some(Self { code, value })
    }

    fn value_for(probe: Option<Self>, code: u8) -> Option<f64> {
        probe
            .filter(|probe| probe.code == code)
            .map(|probe| f64::from(probe.value))
    }
}

/// Speed limit shown when the camera has none: a speed camera ahead first,
/// otherwise the road limit (carrotMan, km/h).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct NavLimit {
    pub road: f64,
    pub camera: f64,
    pub camera_distance: f64,
}

impl NavLimit {
    /// From a carrotMan received within the last second.
    #[must_use]
    pub fn read(subscribers: &Subscribers, now: f64) -> Option<Self> {
        let topic = subscribers.topic("carrotMan").ok()?;
        if !(topic.seen
            && topic.alive
            && topic.valid
            && (0. ..=1.).contains(&(now - topic.receive_time)))
        {
            return None;
        }
        let event::Which::CarrotMan(Ok(cm)) = topic.event().ok()?.which().ok()? else {
            return None;
        };
        Some(Self {
            road: f64::from(cm.get_n_road_limit_speed()),
            camera: f64::from(cm.get_x_spd_limit()),
            camera_distance: f64::from(cm.get_x_spd_dist()),
        })
    }

    #[must_use]
    pub fn kph(&self) -> Option<f64> {
        let camera = (self.camera > 0. && self.camera_distance > 0.).then_some(self.camera);
        camera
            .or((self.road > 0.).then_some(self.road))
            .filter(|limit| (5. ..=250.).contains(limit))
    }
}

pub struct ExtrasInput {
    /// `CcncClusterDisplay`.
    pub display: bool,
    pub probe: Option<Probe>,
    pub nav: Option<NavLimit>,
    pub v_ego_kph: f64,
    pub set_kph: f64,
    pub cruise: bool,
    pub fcw: bool,
    /// Model lane lines 1 and 2 at the car (device frame y, left negative).
    pub lane_y: Option<[f64; 2]>,
    pub lane_probs: Option<[f64; 2]>,
    pub lane_change: bool,
    /// Second car ahead in our lane: (distance, lateral from the path).
    pub second_lead: Option<(f64, f64)>,
}

/// Lane distance is sent only while the model sees the line this well.
const LANE_PROB: f64 = 0.5;
/// Set speed this far from the limit shows the assist arrow.
const SLA_MARGIN_KPH: f64 = 2.;
/// Auto set-speed popup: 2 s, not within 1.5 s of a driver button.
const POPUP_FRAMES: u32 = 200;
const BUTTON_QUIET_FRAMES: u32 = 150;
/// Probe objects.
const PROBE_DISTANCE: f64 = 35.;
const PROBE_LIMIT: f64 = 80.;

#[derive(Default)]
pub struct Extras {
    frame: u32,
    last_button: Option<u32>,
    last_set: Option<f64>,
    popup_until: u32,
    popup_value: f64,
}

impl Extras {
    /// Every controller frame: track set-speed changes the driver did not make.
    pub fn observe(&mut self, frame: u32, set_kph: f64, cruise: bool, button: bool) {
        self.frame = frame;
        if button {
            self.last_button = Some(frame);
        }
        let quiet = self
            .last_button
            .is_none_or(|last| frame.wrapping_sub(last) > BUTTON_QUIET_FRAMES);
        let valid = cruise && set_kph.is_finite() && set_kph > 0.;
        if let (Some(last), true) = (self.last_set, valid) {
            if quiet && (set_kph - last).abs() >= 1. {
                self.popup_until = frame.wrapping_add(POPUP_FRAMES);
                self.popup_value = set_kph;
            }
        }
        self.last_set = valid.then_some(set_kph);
        if !valid {
            self.popup_until = frame;
        }
    }

    fn popup(&self) -> Option<f64> {
        let left = self.popup_until.wrapping_sub(self.frame);
        (left > 0 && left <= POPUP_FRAMES).then_some(self.popup_value)
    }

    /// LFAHDA_CLUSTER (0x1E0): auto set-speed status, update flag and value.
    #[must_use]
    pub fn auto_set(&self, i: &ExtrasInput, metric: bool) -> Option<[f64; 3]> {
        if let Some(value) = Probe::value_for(i.probe, PROBE_AUTO_SET_STA) {
            return Some([value, 1., PROBE_LIMIT]);
        }
        if let Some(value) = Probe::value_for(i.probe, PROBE_AUTO_SET_UPDATE) {
            return Some([1., value, PROBE_LIMIT]);
        }
        if !i.display {
            return None;
        }
        let speed = self.popup()?;
        let shown = if metric { speed } else { speed / 1.609_344 };
        Some([1., 1., clip(shown.round(), 0., 255.)])
    }
}

pub fn apply_auto_set(data: &mut Values, values: Option<[f64; 3]>) {
    if let Some([status, update, value]) = values {
        set(
            data,
            &[
                ("HDA_AutoSetSpdSta", status),
                ("HDA_AutoSetSpdUpdtSta", update),
                ("HDA_AutoSetSpdVal", value),
            ],
        );
    }
}

fn sla_icon(i: &ExtrasInput) -> f64 {
    let Some(limit) = i.nav.and_then(|nav| nav.kph()) else {
        return 0.;
    };
    if !i.set_kph.is_finite() || i.set_kph <= 0. {
        return 0.;
    }
    let colour = if i.cruise { 2. } else { 0. };
    if i.set_kph > limit + SLA_MARGIN_KPH {
        2. + colour
    } else if i.set_kph < limit - SLA_MARGIN_KPH {
        1. + colour
    } else {
        0.
    }
}

/// ADRV_0x161 items.
pub fn adrv161(data: &mut Values, i: &ExtrasInput) -> Result<(), Error> {
    if i.display {
        data.insert("SLA_ICON".into(), sla_icon(i));
        if i.fcw {
            set(data, &[("FCA_ICON", 2.), ("FCA_IMAGE", 2.)]);
        }
        if i.lane_change {
            data.insert("LANE_ZOOM".into(), 0.);
        }
        if let (Some(lanes), Some(probs)) = (i.lane_y, i.lane_probs) {
            for (index, name) in ["LANELINE_LEFT_POSITION", "LANELINE_RIGHT_POSITION"]
                .into_iter()
                .enumerate()
            {
                if probs[index] >= LANE_PROB && lanes[index].is_finite() {
                    data.insert(
                        name.into(),
                        clip((lanes[index].abs() * 10.).round(), 0., 63.),
                    );
                }
            }
        }
    }
    for (code, names) in [
        (PROBE_LANE_ZOOM, &["LANE_ZOOM"][..]),
        (PROBE_FCA_IMAGE, &["FCA_IMAGE"][..]),
        (PROBE_FCA_ICON, &["FCA_ICON"][..]),
        (PROBE_SLA_ICON, &["SLA_ICON"][..]),
        (
            PROBE_LANE_POSITION,
            &["LANELINE_LEFT_POSITION", "LANELINE_RIGHT_POSITION"][..],
        ),
    ] {
        if let Some(value) = Probe::value_for(i.probe, code) {
            for name in names {
                get(data, name)?;
                data.insert((*name).into(), value);
            }
        }
    }
    Ok(())
}

/// CCNC_0x162 items; runs after the front lead and side slots are set.
pub fn ccnc162(data: &mut Values, i: &ExtrasInput) -> Result<(), Error> {
    if i.display {
        if get(data, "SPEEDLIMIT")? == 0. {
            if let Some(limit) = i.nav.and_then(|nav| nav.kph()) {
                let over = i.v_ego_kph.is_finite() && i.v_ego_kph > limit + 0.5;
                set(
                    data,
                    &[
                        ("SPEEDLIMIT", limit.round()),
                        ("SPEEDLIMIT_FLASH", if over { 4. } else { 2. }),
                    ],
                );
            }
        }
        if get(data, "FF_DETECT_ALT")? == 0. {
            if let Some((distance, lateral)) = i.second_lead {
                set(
                    data,
                    &[
                        ("FF_DETECT_ALT", 2.),
                        ("FF_DISTANCE_ALT", clip(distance, 0.1, 204.5)),
                        ("FF_LATERAL_ALT", clip(lateral, -6.4, 6.3)),
                    ],
                );
            }
        }
    }
    let Some(probe) = i.probe else {
        return Ok(());
    };
    let value = f64::from(probe.value);
    match probe.code {
        PROBE_SPEEDLIMIT_FLASH => {
            if get(data, "SPEEDLIMIT")? == 0. {
                data.insert("SPEEDLIMIT".into(), PROBE_LIMIT);
            }
            data.insert("SPEEDLIMIT_FLASH".into(), value);
        }
        PROBE_SIGNS => {
            data.insert("SIGNS".into(), value);
        }
        PROBE_FF_DETECT_ALT => set(
            data,
            &[
                ("FF_DETECT_ALT", value),
                ("FF_DISTANCE_ALT", PROBE_DISTANCE),
                ("FF_LATERAL_ALT", 0.),
            ],
        ),
        PROBE_LF_LATERAL => set(
            data,
            &[
                ("LF_DETECT", 3.),
                ("LF_DETECT_DISTANCE", PROBE_DISTANCE),
                ("LF_DETECT_LATERAL", value / 10.),
            ],
        ),
        PROBE_FF_DETECT => {
            if get(data, "FF_DETECT")? == 0. {
                set(data, &[("FF_DISTANCE", 25.), ("FF_LATERAL", 0.)]);
            }
            data.insert("FF_DETECT".into(), value);
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values(names: &[&str]) -> Values {
        names.iter().map(|name| ((*name).into(), 0.)).collect()
    }

    fn input() -> ExtrasInput {
        ExtrasInput {
            display: true,
            probe: None,
            nav: Some(NavLimit {
                road: 60.,
                camera: 0.,
                camera_distance: 0.,
            }),
            v_ego_kph: 50.,
            set_kph: 80.,
            cruise: true,
            fcw: false,
            lane_y: Some([-1.2, 1.9]),
            lane_probs: Some([0.9, 0.2]),
            lane_change: false,
            second_lead: Some((40., 0.3)),
        }
    }

    const S161: &[&str] = &[
        "SLA_ICON",
        "FCA_ICON",
        "FCA_IMAGE",
        "LANE_ZOOM",
        "LANELINE_LEFT_POSITION",
        "LANELINE_RIGHT_POSITION",
    ];
    const S162: &[&str] = &[
        "SPEEDLIMIT",
        "SPEEDLIMIT_FLASH",
        "SIGNS",
        "FF_DETECT",
        "FF_DISTANCE",
        "FF_LATERAL",
        "FF_DETECT_ALT",
        "FF_DISTANCE_ALT",
        "FF_LATERAL_ALT",
        "LF_DETECT",
        "LF_DETECT_DISTANCE",
        "LF_DETECT_LATERAL",
    ];

    #[test]
    fn probe_decodes_code_and_value() {
        assert_eq!(Probe::from_debug(0), None);
        assert_eq!(Probe::from_debug(5), None);
        assert_eq!(Probe::from_debug(99), None);
        assert_eq!(Probe::from_debug(102), Some(Probe { code: 1, value: 2 }));
        assert_eq!(Probe::from_debug(731), Some(Probe { code: 7, value: 31 }));
        assert_eq!(Probe::from_debug(1300), None);
        assert_eq!(Probe::from_debug(-1), None);
    }

    #[test]
    fn camera_limit_first_then_road_limit() {
        let mut nav = NavLimit {
            road: 60.,
            camera: 50.,
            camera_distance: 300.,
        };
        assert_eq!(nav.kph(), Some(50.));
        nav.camera_distance = 0.;
        assert_eq!(nav.kph(), Some(60.));
        nav.road = 0.;
        assert_eq!(nav.kph(), None);
    }

    #[test]
    fn sign_fills_only_an_empty_stock_sign_and_flashes_red_over_it() {
        let mut data = values(S162);
        data.insert("SPEEDLIMIT_FLASH".into(), 2.);
        ccnc162(&mut data, &input()).unwrap();
        assert_eq!(data["SPEEDLIMIT"], 60.);
        assert_eq!(data["SPEEDLIMIT_FLASH"], 2.);
        assert_eq!(data["FF_DETECT_ALT"], 2.);
        assert_eq!(data["FF_DISTANCE_ALT"], 40.);

        let mut fast = input();
        fast.v_ego_kph = 70.;
        let mut data = values(S162);
        ccnc162(&mut data, &fast).unwrap();
        assert_eq!(data["SPEEDLIMIT_FLASH"], 4.);

        let mut data = values(S162);
        data.insert("SPEEDLIMIT".into(), 40.);
        data.insert("SPEEDLIMIT_FLASH".into(), 2.);
        ccnc162(&mut data, &fast).unwrap();
        assert_eq!(data["SPEEDLIMIT"], 40.);
        assert_eq!(data["SPEEDLIMIT_FLASH"], 2.);
    }

    #[test]
    fn hud_items_follow_the_inputs() {
        let mut data = values(S161);
        data.insert("LANE_ZOOM".into(), 1.);
        data.insert("LANELINE_RIGHT_POSITION".into(), 15.);
        let mut i = input();
        i.fcw = true;
        i.lane_change = true;
        adrv161(&mut data, &i).unwrap();
        // Set 80 over a 60 limit while cruising: green down.
        assert_eq!(data["SLA_ICON"], 4.);
        assert_eq!(data["FCA_ICON"], 2.);
        assert_eq!(data["FCA_IMAGE"], 2.);
        assert_eq!(data["LANE_ZOOM"], 0.);
        assert_eq!(data["LANELINE_LEFT_POSITION"], 12.);
        // Right line under the probability floor keeps the stock value.
        assert_eq!(data["LANELINE_RIGHT_POSITION"], 15.);

        let mut i = input();
        i.cruise = false;
        i.set_kph = 50.;
        let mut data = values(S161);
        adrv161(&mut data, &i).unwrap();
        assert_eq!(data["SLA_ICON"], 1.);
    }

    #[test]
    fn display_off_changes_nothing_but_the_probe() {
        let mut i = input();
        i.display = false;
        i.fcw = true;
        let mut data = values(S161);
        let before = data.clone();
        adrv161(&mut data, &i).unwrap();
        assert_eq!(data, before);

        i.probe = Probe::from_debug(101);
        adrv161(&mut data, &i).unwrap();
        assert_eq!(data["LANE_ZOOM"], 1.);

        i.probe = Probe::from_debug(1018);
        adrv161(&mut data, &i).unwrap();
        assert_eq!(data["LANELINE_LEFT_POSITION"], 18.);
        assert_eq!(data["LANELINE_RIGHT_POSITION"], 18.);

        i.probe = Probe::from_debug(1105);
        let mut data = values(S162);
        ccnc162(&mut data, &i).unwrap();
        assert_eq!(data["LF_DETECT"], 3.);
        assert_eq!(data["LF_DETECT_LATERAL"], 0.5);
        assert_eq!(data["SPEEDLIMIT"], 0.);
    }

    #[test]
    fn auto_set_popup_only_without_a_driver_button() {
        let i = input();
        let mut extras = Extras::default();
        extras.observe(0, 80., true, false);
        extras.observe(1, 70., true, false);
        assert_eq!(extras.auto_set(&i, true), Some([1., 1., 70.]));
        extras.observe(200, 70., true, false);
        assert_eq!(extras.auto_set(&i, true), Some([1., 1., 70.]));
        extras.observe(201, 70., true, false);
        assert_eq!(extras.auto_set(&i, true), None);

        // A driver button 1 s before the change: no popup.
        extras.observe(300, 70., true, true);
        extras.observe(400, 80., true, false);
        assert_eq!(extras.auto_set(&i, true), None);

        // Engaging (no previous set speed) is not an automatic change.
        let mut extras = Extras::default();
        extras.observe(0, 0., false, false);
        extras.observe(1, 80., true, false);
        assert_eq!(extras.auto_set(&i, true), None);
    }
}
