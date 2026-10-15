//! Causal neural housing identity; inference is supplied by the selected owner.
use crate::{
    detector::{gray_view, roi},
    pixels::Rgb,
};
use openpilot_opencv_runtime as cv;
use openpilot_plannerd::signal::tracker::overlap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
pub type Error = Box<dyn std::error::Error>;
#[derive(Clone, Deserialize, Serialize)]
pub struct Proposal {
    #[serde(rename = "box")]
    pub bounds: [f64; 4],
    pub score: f64,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct Color {
    pub label: String,
    pub score: f64,
}
pub trait Network {
    fn detect(&mut self, rgb: &Rgb, rect: [i32; 4]) -> Result<Vec<Proposal>, Error>;
    fn classify(&mut self, rgb: &Rgb, bounds: [f64; 4]) -> Result<Color, Error>;
}
#[derive(Clone)]
struct Template {
    pixels: Vec<u8>,
    mask: Vec<u8>,
    width: u32,
    height: u32,
    x: i32,
    y: i32,
}
impl Template {
    fn snapshot(&self) -> Value {
        let array = |data: &[u8]| json!({"shape":[self.height,self.width],"dtype":"uint8","sha256":format!("{:x}",Sha256::digest(data))});
        json!([array(&self.pixels), array(&self.mask), [self.x, self.y]])
    }
}
#[derive(Clone)]
struct Track {
    proposal: Proposal,
    id: u64,
    detected_at: f64,
    source: &'static str,
    template: Option<Template>,
}
pub struct Sequence {
    period: f64,
    last_detection: f64,
    previous_time: Option<f64>,
    tracks: Vec<Track>,
    next_id: u64,
}
impl Default for Sequence {
    fn default() -> Self {
        Self {
            period: 0.2,
            last_detection: f64::NEG_INFINITY,
            previous_time: None,
            tracks: Vec::new(),
            next_id: 1,
        }
    }
}
impl Sequence {
    pub fn snapshot(&self) -> Value {
        json!({"period":self.period,"last_detection":self.last_detection,"previous_time":self.previous_time,"next_id":self.next_id,"tracks":self.tracks.iter().map(|t| json!({"box":t.proposal.bounds,"score":t.proposal.score,"id":t.id,"detected_at":t.detected_at,"source":t.source,"template":t.template.as_ref().map(Template::snapshot)})).collect::<Vec<_>>()})
    }
    fn template(gray: &[u8], width: u32, height: u32, bounds: [f64; 4]) -> Option<Template> {
        let [a, b, c, d] = bounds.map(|v| v as i32);
        let [x, y, x2, y2] = [
            (a - 6).max(0),
            (b - 6).max(0),
            (c + 6).min(width as i32),
            (d + 6).min(height as i32),
        ];
        let pixels = roi(gray, width as usize, height as usize, [x, y, x2, y2])?;
        let w = (x2 - x) as u32;
        let h = (y2 - y) as u32;
        let mut mask = vec![255_u8; pixels.len()];
        for yy in (b - y).max(0)..(d - y).min(h as i32) {
            for xx in (a - x).max(0)..(c - x).min(w as i32) {
                mask[yy as usize * w as usize + xx as usize] = 0;
            }
        }
        let selected = pixels
            .iter()
            .zip(&mask)
            .filter_map(|(p, m)| (*m != 0).then_some(f64::from(*p)))
            .collect::<Vec<_>>();
        if selected.len() < 20 {
            return None;
        }
        let mean = selected.iter().sum::<f64>() / selected.len() as f64;
        let std = (selected
            .iter()
            .map(|v| (v - mean) * (v - mean))
            .sum::<f64>()
            / selected.len() as f64)
            .sqrt();
        if std < 2. {
            return None;
        }
        Some(Template {
            pixels,
            mask,
            width: w,
            height: h,
            x,
            y,
        })
    }
    pub fn process(
        &mut self,
        rgb: &Rgb,
        timestamp: f64,
        network: &mut impl Network,
    ) -> Result<Value, Error> {
        if self
            .previous_time
            .is_some_and(|last| !(timestamp - last > 0. && timestamp - last <= 0.25))
        {
            self.tracks.clear();
            self.last_detection = f64::NEG_INFINITY;
        }
        self.previous_time = Some(timestamp);
        let image = cv::rgb_to_gray(rgb.view()?)?;
        let gray = image.view().data();
        let (w, h) = (rgb.width as i32, rgb.height as i32);
        let mut tracked = Vec::new();
        for old in &self.tracks {
            if timestamp - old.detected_at > 0.5 {
                continue;
            }
            let Some(t) = &old.template else {
                continue;
            };
            let [x, y, x2, y2] = [
                (t.x - 12).max(0),
                (t.y - 12).max(0),
                (t.x + t.width as i32 + 12).min(w),
                (t.y + t.height as i32 + 12).min(h),
            ];
            let Some(search) = roi(gray, w as usize, h as usize, [x, y, x2, y2]) else {
                continue;
            };
            let scores = cv::match_template_ccorr(
                gray_view(&search, (x2 - x) as u32, (y2 - y) as u32)?,
                gray_view(&t.pixels, t.width, t.height)?,
                Some(gray_view(&t.mask, t.width, t.height)?),
            )?;
            let mut best = 0;
            let quality = |i: usize| {
                if scores[i].is_finite() {
                    scores[i]
                } else {
                    -1.
                }
            };
            for i in 1..scores.len() {
                if quality(i) > quality(best) {
                    best = i;
                }
            }
            if f64::from(quality(best)) < 0.90 {
                continue;
            }
            let columns = (x2 - x - t.width as i32 + 1) as usize;
            let dx = x + (best % columns) as i32 - t.x;
            let dy = y + (best / columns) as i32 - t.y;
            let mut track = old.clone();
            track.proposal.bounds = std::array::from_fn(|i| {
                old.proposal.bounds[i] + f64::from(if i % 2 == 0 { dx } else { dy })
            });
            let [a, b, c, d] = track.proposal.bounds;
            if !(0. <= a && a < c && c <= f64::from(w) && 0. <= b && b < d && d <= f64::from(h)) {
                continue;
            }
            track.source = "tracked";
            tracked.push(track);
        }
        let interval = if tracked.is_empty() {
            self.period.min(0.1)
        } else {
            self.period
        };
        let detected = timestamp - self.last_detection >= interval - 1e-9;
        if detected {
            self.last_detection = timestamp;
            let proposals = network.detect(
                rgb,
                [
                    (0.2 * f64::from(w)) as i32,
                    (0.04 * f64::from(h)) as i32,
                    (0.85 * f64::from(w)) as i32,
                    (0.6 * f64::from(h)) as i32,
                ],
            )?;
            let mut used = Vec::new();
            let mut merged = Vec::new();
            for proposal in &proposals {
                let mut candidate: Option<&Track> = None;
                for t in &tracked {
                    if used.contains(&t.id) || overlap(t.proposal.bounds, proposal.bounds) < 0.2 {
                        continue;
                    }
                    if candidate.is_none_or(|old| {
                        overlap(t.proposal.bounds, proposal.bounds)
                            > overlap(old.proposal.bounds, proposal.bounds)
                    }) {
                        candidate = Some(t);
                    }
                }
                let id = if let Some(old) = candidate {
                    used.push(old.id);
                    old.id
                } else {
                    let id = self.next_id;
                    self.next_id += 1;
                    id
                };
                merged.push(Track {
                    proposal: proposal.clone(),
                    id,
                    detected_at: timestamp,
                    source: "detected",
                    template: None,
                });
            }
            merged.extend(tracked.into_iter().filter(|t| {
                !used.contains(&t.id)
                    && proposals
                        .iter()
                        .all(|p| overlap(t.proposal.bounds, p.bounds) < 0.2)
            }));
            tracked = merged;
        }
        tracked.truncate(20);
        self.tracks = tracked;
        let mut result = Vec::new();
        for t in &mut self.tracks {
            t.template = Self::template(gray, rgb.width, rgb.height, t.proposal.bounds);
            let color = network.classify(rgb, t.proposal.bounds)?;
            result.push(json!({"id":t.id,"box":t.proposal.bounds,"source":t.source,"neural_age":timestamp-t.detected_at,"label":color.label,"score":color.score}));
        }
        Ok(json!({"timestamp":timestamp,"detected":detected,"tracks":result}))
    }
}
