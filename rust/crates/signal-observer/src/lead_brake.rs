//! Bounded display/log-only central lead-lamp observation; never a control input.
use crate::pixels::Rgb;
use openpilot_opencv_runtime as cv;
type Error = Box<dyn std::error::Error>;
use serde_json::{json, Value};
#[derive(Clone, Debug, serde::Deserialize)]
pub struct Context {
    pub lead: bool,
    pub radar: bool,
    pub id: i64,
    pub distance: f64,
    pub lateral: f64,
    pub probability: f64,
    pub vision_x: f64,
    pub vision_y: f64,
    pub yaw: f64,
    pub steer: f64,
    pub speed: f64,
    pub calibrated: bool,
    pub height: f64,
    pub rpy: [f64; 3],
}
#[derive(Default)]
pub struct Detector {
    id: Option<i64>,
    bounds: Option<[i32; 4]>,
    since: f64,
    last: Option<f64>,
    count: u32,
}
impl Detector {
    pub fn reset(&mut self) {
        *self = Self::default();
    }
    pub fn unknown(&mut self, reason: &str) -> Value {
        self.reset();
        json!({"state":"unknown","reason":reason,"box":null})
    }
    pub fn process(&mut self, rgb: &Rgb, stamp: f64, c: &Context) -> Result<Value, Error> {
        if ![stamp, c.distance, c.lateral, c.yaw, c.steer, c.speed]
            .iter()
            .all(|v| v.is_finite())
        {
            return Ok(self.unknown("invalid_context"));
        }
        if !c.lead
            || !c.radar
            || c.id < 0
            || !(3. ..=35.).contains(&c.distance)
            || c.probability < 0.8
        {
            return Ok(self.unknown("no_matched_lead"));
        }
        if c.yaw.abs() > 0.06 || (c.steer.abs() > 15. && c.speed.abs() > 2.) {
            return Ok(self.unknown("turning"));
        }
        let depth = c.distance + 1.52;
        if !(c.vision_x + c.vision_y).is_finite()
            || (c.vision_x - depth).abs() > 2_f64.max(0.15 * c.distance)
            || (c.vision_y - c.lateral).abs() > 0.5
        {
            return Ok(self.unknown("vision_radar_mismatch"));
        }
        if !c.calibrated
            || (rgb.width, rgb.height) != (1344, 760)
            || !c.rpy.iter().all(|v| v.is_finite())
            || !(0.7..=2.2).contains(&c.height)
        {
            return Ok(self.unknown("uncalibrated"));
        }
        rgb.view()?;
        let rotation = openpilot_calibrationd::orientation::rotation(c.rpy);
        let projection: [[f64; 3]; 3] = std::array::from_fn(|i| {
            std::array::from_fn(|j| match i {
                0 => 1141.5 * rotation[1][j] + 672. * rotation[0][j],
                1 => 1141.5 * rotation[2][j] + 380. * rotation[0][j],
                _ => rotation[0][j],
            })
        });
        let mut lo = [f64::INFINITY; 2];
        let mut hi = [f64::NEG_INFINITY; 2];
        for lateral in [-0.27, 0.27] {
            for z in [1.25, 1.9] {
                let point = [depth, c.lateral + lateral, c.height - z];
                let d: [f64; 3] = std::array::from_fn(|i| {
                    projection[i][0] * point[0]
                        + projection[i][1] * point[1]
                        + projection[i][2] * point[2]
                });
                if d[2] <= 0. {
                    return Ok(self.unknown("projection"));
                }
                let xy = [d[0] / d[2], d[1] / d[2]];
                for i in 0..2 {
                    lo[i] = lo[i].min(xy[i]);
                    hi[i] = hi[i].max(xy[i]);
                }
            }
        }
        let [x1, y1, x2, y2] = [
            lo[0].floor() as i32,
            lo[1].floor() as i32,
            hi[0].ceil() as i32,
            hi[1].ceil() as i32,
        ];
        if x1 < 0
            || y1 < 0
            || x1 >= x2
            || y1 >= y2
            || x2 > 1344
            || y2 > 760
            || i64::from(x2 - x1) * i64::from(y2 - y1) > 24000
        {
            return Ok(self.unknown("crop_budget"));
        }
        let patch = rgb.crop([x1, y1, x2, y2]).ok_or("invalid brake crop")?;
        let (w, h) = (patch.width as usize, patch.height as usize);
        let mut mask: Vec<u8> = patch
            .data
            .chunks_exact(3)
            .map(|p| {
                if p[0] >= 160
                    && i32::from(p[0]) - i32::from(p[1]) >= 45
                    && i32::from(p[0]) - i32::from(p[2]) >= 45
                {
                    255
                } else {
                    0
                }
            })
            .collect();
        // OpenCV MORPH_CLOSE with a 1x3 all-ones kernel and its default borders.
        let dilated: Vec<u8> = (0..mask.len())
            .map(|i| {
                let x = i % w;
                mask[i - x + x.saturating_sub(1)..i - x + (x + 2).min(w)]
                    .iter()
                    .copied()
                    .max()
                    .unwrap()
            })
            .collect();
        for (i, v) in mask.iter_mut().enumerate() {
            let x = i % w;
            *v = *dilated[i - x + x.saturating_sub(1)..i - x + (x + 2).min(w)]
                .iter()
                .min()
                .unwrap();
        }
        let view = cv::ImageView::new(
            &mask,
            cv::ImageLayout::new(cv::Dimensions::new(w as u32, h as u32)?, cv::Format::Gray)?,
        )?;
        let candidates: Vec<[i32; 4]> = cv::external_contours(view)?
            .into_iter()
            .filter_map(|c| {
                let b = c.bounds;
                let ratio = f64::from(b.width) / f64::from(b.height.max(1));
                let physical = f64::from(b.width) * depth / 1141.5;
                (c.area >= 4.
                    && (2.2..=18.).contains(&ratio)
                    && (0.07..=0.7).contains(&physical)
                    && b.x > 0
                    && b.y > 0
                    && i64::from(b.x) + i64::from(b.width) < w as i64
                    && i64::from(b.y) + i64::from(b.height) < h as i64)
                    .then_some([
                        b.x + x1,
                        b.y + y1,
                        b.x + x1 + b.width as i32,
                        b.y + y1 + b.height as i32,
                    ])
            })
            .collect();
        if candidates.len() != 1 {
            return Ok(self.unknown("absent_or_ambiguous"));
        }
        let bounds = candidates[0];
        let continuous = self.id == Some(c.id)
            && self
                .last
                .is_some_and(|last| 0. < stamp - last && stamp - last <= 0.25)
            && self.bounds.is_some_and(|old| {
                (bounds[0] - old[0]).abs() <= 8.max(bounds[2] - bounds[0])
                    && (bounds[1] - old[1]).abs() <= 6.max(bounds[3] - bounds[1])
            });
        if !continuous {
            self.since = stamp;
            self.count = 0;
        }
        self.id = Some(c.id);
        self.bounds = Some(bounds);
        self.last = Some(stamp);
        self.count += 1;
        let confirmed = self.count >= 3 && stamp - self.since >= 0.15;
        Ok(
            json!({"state":if confirmed {"candidate"} else {"unknown"},"reason":if confirmed {"central_lamp"} else {"confirming"},"box":bounds,"lead_id":c.id,"distance":c.distance}),
        )
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Admission {
    Off,
    Skip,
    Busy,
    Work,
}
#[derive(Default)]
pub struct Budget {
    session: String,
    next: f64,
}
impl Budget {
    pub fn admit(&mut self, enabled: bool, session: &str, now: f64, work_start: f64) -> Admission {
        if !enabled {
            *self = Self::default();
            return Admission::Off;
        }
        if self.session != session {
            self.session = session.into();
            self.next = 0.;
        }
        if now < self.next {
            return Admission::Skip;
        }
        self.next = now + 0.1;
        if now - work_start < 0.045 {
            Admission::Work
        } else {
            Admission::Busy
        }
    }
    pub fn overrun(&mut self, now: f64) {
        self.next = now + 0.5;
    }
}

#[cfg(feature = "daemon")]
pub mod runtime;

#[cfg(test)]
mod tests {
    use super::*;
    fn image(lamp: bool) -> crate::pixels::Rgb {
        cv::initialize(1).unwrap();
        let mut data = vec![0; 1344 * 760 * 3];
        if lamp {
            for y in 342..348 {
                for x in 660..684 {
                    data[(y * 1344 + x) * 3] = 255;
                }
            }
        }
        crate::pixels::Rgb {
            width: 1344,
            height: 760,
            data,
        }
    }
    fn context() -> Context {
        Context {
            lead: true,
            radar: true,
            id: 3,
            distance: 8.,
            lateral: 0.,
            probability: 0.99,
            vision_x: 9.52,
            vision_y: 0.,
            yaw: 0.,
            steer: 0.,
            speed: 0.,
            calibrated: true,
            height: 1.286,
            rpy: [0.; 3],
        }
    }
    #[test]
    fn central_lamp_needs_three_causal_frames_and_missing_is_unknown() {
        let mut detector = Detector::default();
        let rgb = image(true);
        for t in [1., 1.1] {
            assert_eq!(
                detector.process(&rgb, t, &context()).unwrap()["state"],
                "unknown"
            );
        }
        assert_eq!(
            detector.process(&rgb, 1.2, &context()).unwrap()["state"],
            "candidate"
        );
        assert_eq!(
            detector.process(&image(false), 1.25, &context()).unwrap()["state"],
            "unknown"
        );
    }
    #[test]
    fn replacing_lead_repeated_frame_or_gap_cannot_inherit_confirmation() {
        let rgb = image(true);
        let mut detector = Detector::default();
        for _ in 0..8 {
            assert_eq!(
                detector.process(&rgb, 1., &context()).unwrap()["state"],
                "unknown"
            );
        }
        for t in [1.1, 1.2] {
            detector.process(&rgb, t, &context()).unwrap();
        }
        let mut next = context();
        next.id = 4;
        assert_eq!(
            detector.process(&rgb, 1.3, &next).unwrap()["state"],
            "unknown"
        );
        assert_eq!(
            detector.process(&rgb, 1.6, &next).unwrap()["state"],
            "unknown"
        );
        for change in 0..5 {
            let mut ctx = context();
            match change {
                0 => ctx.yaw = 0.2,
                1 => ctx.vision_y = 2.,
                2 => ctx.lead = false,
                3 => ctx.calibrated = false,
                _ => ctx.distance = f64::NAN,
            }
            assert_eq!(
                detector.process(&rgb, 2., &ctx).unwrap()["state"],
                "unknown"
            );
        }
    }
    #[test]
    fn off_and_busy_iterations_do_not_admit_extra_work_or_catch_up() {
        let mut budget = Budget::default();
        assert_eq!(budget.admit(false, "s", 1., 0.), Admission::Off);
        assert_eq!(budget.admit(true, "s", 2., 1.9), Admission::Busy);
        assert_eq!(budget.admit(true, "s", 2.05, 2.05), Admission::Skip);
        assert_eq!(budget.admit(true, "s", 2.11, 2.11), Admission::Work);
        budget.overrun(2.12);
        assert_eq!(budget.admit(true, "s", 2.3, 2.3), Admission::Skip);
        assert_eq!(budget.admit(true, "next", 2.31, 2.31), Admission::Work);
    }
}
