use super::{select, Candidate, ResultData, MASK_CHANNELS, PROTO_SIZE};
use crate::Error;
use num_traits::ToPrimitive;

pub struct Head<'a> {
    predictions: &'a [f32],
    prototypes: &'a [f32],
    anchors: usize,
}

struct Detection {
    anchor: usize,
    class_id: u8,
    score: f32,
    bounds: [f32; 4],
}

impl Detection {
    fn iou(&self, other: &Self) -> f32 {
        let [left, top, right, bottom] = self.bounds;
        let [other_left, other_top, other_right, other_bottom] = other.bounds;
        if self
            .bounds
            .iter()
            .chain(&other.bounds)
            .any(|value| value.is_nan())
        {
            return f32::NAN;
        }
        let width = (right.min(other_right) - left.max(other_left)).max(0.0);
        let height = (bottom.min(other_bottom) - top.max(other_top)).max(0.0);
        let intersection = width * height;
        let area = (right - left) * (bottom - top);
        let other_area = (other_right - other_left) * (other_bottom - other_top);
        intersection / (area + other_area - intersection + 1e-6)
    }
}

impl<'a> Head<'a> {
    pub fn new(predictions: &'a [f32], prototypes: &'a [f32]) -> Result<Self, Error> {
        if !predictions.len().is_multiple_of(42)
            || prototypes.len() != MASK_CHANNELS * PROTO_SIZE * PROTO_SIZE
        {
            return Err(Error::Invalid("invalid lane output tensor shape"));
        }
        Ok(Self {
            predictions,
            prototypes,
            anchors: predictions.len() / 42,
        })
    }

    fn at(&self, row: usize, anchor: usize) -> f32 {
        self.predictions[row * self.anchors + anchor]
    }

    fn detections(&self, threshold: f32) -> Vec<Detection> {
        let mut output = Vec::new();
        for anchor in 0..self.anchors {
            let mut class_id = 0;
            let mut score = self.at(4, anchor);
            for class in 1..6u8 {
                let next = self.at(4 + usize::from(class), anchor);
                if next.is_nan() || next > score {
                    score = next;
                    class_id = class;
                }
                if score.is_nan() {
                    break;
                }
            }
            if score >= threshold {
                let [x, y, width, height] = std::array::from_fn(|row| self.at(row, anchor));
                output.push(Detection {
                    anchor,
                    class_id,
                    score,
                    bounds: [
                        x - width * 0.5,
                        y - height * 0.5,
                        x + width * 0.5,
                        y + height * 0.5,
                    ],
                });
            }
        }
        output
    }

    pub fn candidates(&self, confidence: f32, iou: f32) -> Result<Vec<Candidate>, Error> {
        let detections = self.detections(confidence);
        let mut kept = Vec::new();
        // These classes never become lane candidates, and NMS is per class.
        for class in [0, 1, 2, 5] {
            let mut order: Vec<_> = detections
                .iter()
                .filter(|detection| detection.class_id == class)
                .collect();
            order.sort_by(|left, right| right.score.total_cmp(&left.score));
            while let Some(&current) = order.first() {
                kept.push(current);
                let mut retained = 0;
                for index in 1..order.len() {
                    let other = order[index];
                    if current.iou(other) <= iou {
                        order[retained] = other;
                        retained += 1;
                    }
                }
                order.truncate(retained);
            }
        }
        kept.sort_by(|left, right| right.score.total_cmp(&left.score));
        let mut candidates = Vec::with_capacity(kept.len());
        for detection in kept {
            if let Some(candidate) = self.mask(detection)? {
                candidates.push(candidate);
            }
        }
        Ok(candidates)
    }

    fn mask(&self, detection: &Detection) -> Result<Option<Candidate>, Error> {
        let mut bounds = [0u32; 4];
        for (output, value) in bounds.iter_mut().zip(detection.bounds) {
            *output = ((value / 416.0) * 104.0)
                .clamp(0.0, 103.0)
                .to_u32()
                .ok_or(Error::Invalid("cannot convert float NaN to integer"))?;
        }
        let [left, top, right, bottom] = bounds;
        if right <= left || bottom <= top {
            return Ok(None);
        }
        let coefficients: [f32; MASK_CHANNELS] =
            std::array::from_fn(|channel| self.at(10 + channel, detection.anchor));
        // Rows above the existing cutoff can never produce an accepted candidate.
        for row in (top.max(52)..=bottom).rev() {
            let mut count = 0;
            let mut total = 0;
            let first = usize::try_from(row * 104 + left)
                .map_err(|_| Error::Invalid("lane mask offset exceeds address space"))?;
            let width = usize::try_from(right - left + 1)
                .map_err(|_| Error::Invalid("lane mask width exceeds address space"))?;
            let mut values = [0.0f32; PROTO_SIZE];
            // Visit contiguous pixels while retaining the original 32-channel
            // accumulation order for each pixel (no reassociation or fused math).
            for (channel, coefficient) in coefficients.iter().enumerate() {
                let start = channel * PROTO_SIZE * PROTO_SIZE + first;
                for (value, sample) in values[..width]
                    .iter_mut()
                    .zip(&self.prototypes[start..start + width])
                {
                    *value += coefficient * sample;
                }
            }
            for (offset, &value) in values[..width].iter().enumerate() {
                if value > 0.0 {
                    count += 1;
                    total += u32::try_from(offset)
                        .map_err(|_| Error::Invalid("lane column exceeds integer range"))?;
                }
            }
            if count > 0 {
                return Ok(Some(Candidate {
                    class_id: detection.class_id,
                    score: detection.score,
                    bottom: row,
                    center: f64::from(left) + f64::from(total) / f64::from(count),
                }));
            }
        }
        Ok(None)
    }

    pub fn result(&self, confidence: f32, iou: f32) -> Result<ResultData, Error> {
        select(&self.candidates(confidence, iou)?)
    }
}
