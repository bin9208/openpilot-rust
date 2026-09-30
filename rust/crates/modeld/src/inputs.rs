use serde::Serialize;

#[derive(Default)]
pub struct DropTracker {
    last_frame: u32,
    warmup_count: u8,
    filtered: f64,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct FrameDrop {
    pub dropped: u32,
    pub prepare_only: bool,
    pub ratio: f64,
}

impl DropTracker {
    pub fn observe(&mut self, frame_id: u32) -> FrameDrop {
        let dropped = frame_id.saturating_sub(self.last_frame).saturating_sub(1);
        let alpha = 0.05 / (10.0 + 0.05);
        self.filtered = (1.0 - alpha) * self.filtered + alpha * f64::from(dropped.min(10));
        if self.warmup_count < 10 {
            self.filtered = 0.0;
            self.warmup_count += 1;
        }
        self.last_frame = frame_id;
        FrameDrop {
            dropped,
            prepare_only: dropped > 0,
            ratio: self.filtered / (1.0 + self.filtered),
        }
    }

    pub fn reset_warmup(&mut self) {
        self.warmup_count = 0;
    }
}

#[derive(Debug, thiserror::Error)]
#[error("invalid driving-model feature count")]
pub struct FeatureCount;

pub struct PolicyInputs {
    previous_desire: [f32; 8],
    packed: Vec<f32>,
}

impl PolicyInputs {
    pub fn new(feature_count: usize) -> Result<Self, FeatureCount> {
        if feature_count == 0 || feature_count > 1024 * 1024 {
            return Err(FeatureCount);
        }
        Ok(Self {
            previous_desire: [0.0; 8],
            packed: vec![0.0; 12 + feature_count],
        })
    }

    pub fn update(&mut self, desire: i32, is_rhd: bool, lateral_time: f64, longitudinal_time: f64) {
        let mut current = [0.0_f32; 8];
        if let Ok(index @ 1..=7) = usize::try_from(desire) {
            current[index] = 1.0;
        }
        for (index, &value) in current.iter().enumerate() {
            self.packed[index] = if value - self.previous_desire[index] > 0.99 {
                value
            } else {
                0.0
            };
        }
        self.previous_desire = current;
        self.packed[8..10].copy_from_slice(if is_rhd { &[0.0, 1.0] } else { &[1.0, 0.0] });
        self.packed[10] = lateral_time as f32;
        self.packed[11] = longitudinal_time as f32;
    }

    pub fn set_features(&mut self, values: &[f32]) -> Result<(), FeatureCount> {
        if values.len() != self.packed.len() - 12 {
            return Err(FeatureCount);
        }
        self.packed[12..].copy_from_slice(values);
        Ok(())
    }

    pub fn packed(&self) -> &[f32] {
        &self.packed
    }
}
