//! Causal image correspondence; previous pixels establish identity, never color.
use crate::{
    detector::{Detection, detect, gray_view, roi},
    pixels::{Rgb, illuminated_green, lamp_evidence_with},
};
use openpilot_opencv_runtime as cv;
use openpilot_plannerd::signal::tracker::{Frame, Tracker, overlap};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Image(#[from] cv::Error),
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}
#[derive(Debug, Default)]
pub struct Process {
    tracker: Tracker,
    previous_gray: Option<(u32, u32, Vec<u8>)>,
}
impl Process {
    pub fn snapshot(&self) -> Value {
        let mut state = self.tracker.snapshot();
        state["previous_gray"] = self.previous_gray.as_ref().map_or(Value::Null, |(width,height,data)| json!({"width":width,"height":height,"sha256":format!("{:x}",Sha256::digest(data))}));
        state
    }
    pub fn process(
        &mut self,
        rgb: &Rgb,
        timestamp: f64,
        allow_green: bool,
    ) -> Result<Value, Error> {
        let gray = cv::rgb_to_gray(rgb.view()?)?;
        let gray = gray.view().data();
        let mut detections = detect(rgb, gray, true, false)?;
        let state = self.tracker.snapshot();
        if let (Some((width, height, previous)), Some(last)) =
            (&self.previous_gray, state["last_timestamp"].as_f64())
        {
            if *width == rgb.width
                && *height == rgb.height
                && timestamp > last
                && timestamp - last <= 0.5
            {
                let bridge = timestamp - last > 0.25;
                for track in state["tracks"].as_array().into_iter().flatten() {
                    let pending_red = track["pending"] == "red"
                        && track["observations"].as_u64().unwrap_or(0) >= 2
                        && track["evidence"]["source"] == "night_lamp_core";
                    if (!track["seen_red"].as_bool().unwrap_or(false) && !pending_red)
                        || track["last_seen"].as_f64() != Some(last)
                    {
                        continue;
                    }
                    let b: [f64; 4] = serde_json::from_value(track["box"].clone())?;
                    if !bridge && detections.iter().any(|d| overlap(b, d.bounds_f64()) > 0.25) {
                        continue;
                    }
                    let [x1, y1, x2, y2] = b.map(|v| v as i32);
                    let w = x2 - x1;
                    let h = y2 - y1;
                    let pad = 6.max(h / 2);
                    let [ax, ay, bx, by] = [
                        (x1 - pad).max(0),
                        (y1 - pad).max(0),
                        (x2 + pad).min(*width as i32),
                        (y2 + pad).min(*height as i32),
                    ];
                    let margin = 10.max(w / 5);
                    let [sx, sy, ex, ey] = [
                        (ax - margin).max(0),
                        (ay - margin).max(0),
                        (bx + margin).min(*width as i32),
                        (by + margin).min(*height as i32),
                    ];
                    let Some(template) = roi(
                        previous,
                        *width as usize,
                        *height as usize,
                        [ax, ay, bx, by],
                    ) else {
                        continue;
                    };
                    if ey - sy < by - ay || ex - sx < bx - ax {
                        continue;
                    }
                    let tw = (bx - ax) as usize;
                    let th = (by - ay) as usize;
                    let mut mask = vec![1_u8; tw * th];
                    for y in (y1 - ay).max(0)..(y2 - ay).min(th as i32) {
                        for x in (x1 - ax).max(0)..(x2 - ax).min(tw as i32) {
                            mask[y as usize * tw + x as usize] = 0;
                        }
                    }
                    let selected = template
                        .iter()
                        .zip(&mask)
                        .filter_map(|(v, m)| (*m != 0).then_some(f64::from(*v)))
                        .collect::<Vec<_>>();
                    let mean = selected.iter().sum::<f64>() / selected.len().max(1) as f64;
                    let std = (selected
                        .iter()
                        .map(|v| (v - mean) * (v - mean))
                        .sum::<f64>()
                        / selected.len().max(1) as f64)
                        .sqrt();
                    let masked = selected.len() >= 24 && std >= 8.;
                    if !masked && bridge {
                        continue;
                    }
                    let Some(search) =
                        roi(gray, *width as usize, *height as usize, [sx, sy, ex, ey])
                    else {
                        continue;
                    };
                    let mask_view = if masked {
                        Some(gray_view(&mask, tw as u32, th as u32)?)
                    } else {
                        None
                    };
                    let mut scores = cv::match_template(
                        gray_view(&search, (ex - sx) as u32, (ey - sy) as u32)?,
                        gray_view(&template, tw as u32, th as u32)?,
                        mask_view,
                    )?;
                    for score in &mut scores {
                        if !score.is_finite() {
                            *score = -1.;
                        }
                    }
                    let mut best = 0;
                    for i in 1..scores.len() {
                        if scores[i] > scores[best] {
                            best = i;
                        }
                    }
                    let score = f64::from(scores[best]);
                    if score < if masked { 0.85 } else { 0.75 } {
                        continue;
                    }
                    let sw = (ex - sx) as usize - tw + 1;
                    let (lx, ly) = (best % sw, best / sw);
                    let dx = sx + lx as i32 - ax;
                    let dy = sy + ly as i32 - ay;
                    let bounds = [x1 + dx, y1 + dy, x2 + dx, y2 + dy];
                    let evidence = lamp_evidence_with(rgb, bounds, false)?;
                    let mut red_motion = false;
                    if !bridge
                        && evidence.raw == "red"
                        && track["pending"] == "red"
                        && score >= 0.9
                        && track["evidence"]["source"] == "night_lamp_core"
                    {
                        for y in ly.saturating_sub(2)..(ly + 3).min(scores.len() / sw) {
                            for x in lx.saturating_sub(2)..(lx + 3).min(sw) {
                                scores[y * sw + x] = -1.;
                            }
                        }
                        red_motion = score
                            - f64::from(scores.iter().copied().fold(f32::NEG_INFINITY, f32::max))
                            >= 0.05;
                    }
                    if bridge {
                        detections
                            .retain(|d| overlap(bounds.map(f64::from), d.bounds_f64()) <= 0.35);
                    }
                    let mut d = Detection::new(bounds, evidence, "causal_template");
                    d.match_score = Some(score);
                    if bridge || red_motion {
                        d.continuity_id = track["ident"].as_u64();
                    }
                    if red_motion {
                        d.red_motion_verified = Some(true);
                    }
                    detections.push(d);
                }
            }
        }
        if !allow_green {
            for d in &mut detections {
                if d.evidence.raw == "green" {
                    d.evidence.raw = "unknown";
                    d.green_rejected = Some("motion_context");
                }
            }
            self.tracker.reject_green();
        }
        for d in &mut detections {
            if d.evidence.raw == "green" && !illuminated_green(rgb, d.bounds)? {
                d.evidence.raw = "unknown";
                d.green_rejected = Some("unlit_or_border");
            }
        }
        self.previous_gray = Some((rgb.width, rgb.height, gray.to_vec()));
        Ok(self.tracker.update(&Frame {
            timestamp,
            detections: detections
                .iter()
                .map(serde_json::to_value)
                .collect::<Result<_, _>>()?,
        })?)
    }
}
