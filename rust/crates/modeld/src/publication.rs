use openpilot_cereal::log_capnp::model_data_v2::ConfidenceClass;

#[derive(Debug, Default)]
pub struct PublishState {
    disengage: [f32; 25],
    brake5: [f32; 5],
    brake3: [f32; 2],
}

pub struct PublicationStatus {
    pub hard_brake: bool,
    pub confidence: ConfidenceClass,
}

impl PublishState {
    pub fn update(&mut self, meta: &[f32; 55], frame_id: u32) -> PublicationStatus {
        self.brake5.rotate_left(1);
        self.brake5[4] = meta[6];
        self.brake3.rotate_left(1);
        self.brake3[1] = meta[4];
        let hard_brake = self
            .brake5
            .iter()
            .zip([0.05, 0.05, 0.15, 0.15, 0.15])
            .all(|(&value, limit)| value > limit)
            && self.brake3.iter().all(|&value| value > 0.7);
        if frame_id.is_multiple_of(40) {
            let any: [f32; 5] = std::array::from_fn(|i| {
                1.0 - (1.0 - meta[2 + i * 6]) * (1.0 - meta[1 + i * 6]) * (1.0 - meta[3 + i * 6])
            });
            self.disengage.copy_within(5.., 0);
            self.disengage[20] = any[0];
            for i in 1..5 {
                self.disengage[20 + i] = (any[i] - any[i - 1]) / (1.0 - any[i - 1]);
            }
        }
        let score: f64 = (0..5)
            .map(|i| f64::from(self.disengage[i * 5 + 4 - i]) / 5.0)
            .sum();
        let confidence = if score < 0.01165 {
            ConfidenceClass::Green
        } else if score < 0.06157 {
            ConfidenceClass::Yellow
        } else {
            ConfidenceClass::Red
        };
        PublicationStatus {
            hard_brake,
            confidence,
        }
    }
}
