use crate::parse::{sigmoid, softmax, Normal, ParseError, RawOutputs};

#[derive(Debug)]
pub struct DrivingPrediction {
    pub plan: [[f32; 15]; 33],
    pub plan_std: [[f32; 15]; 33],
    pub pose: Normal<6>,
    pub wide_euler: Normal<3>,
    pub road_transform: Normal<6>,
    pub lanes: Normal<264>,
    pub edges: Normal<132>,
    pub leads: Normal<72>,
    pub lane_prob: [f32; 8],
    pub lead_prob: [f32; 3],
    pub meta: [f32; 55],
    pub desire_state: [f32; 8],
    pub desire_prediction: [f32; 32],
    pub direct_action: Option<[f32; 2]>,
}

impl DrivingPrediction {
    pub fn parse(raw: &RawOutputs<'_>) -> Result<Self, ParseError> {
        let plan = raw.normal::<495>("plan")?;
        let mut desire_state = raw.array("desire_state")?;
        softmax(&mut desire_state);
        let mut desire_prediction = raw.array::<32>("desire_pred")?;
        for row in desire_prediction.chunks_exact_mut(8) {
            softmax(row);
        }
        Ok(Self {
            plan: std::array::from_fn(|i| std::array::from_fn(|j| plan.mean[i * 15 + j])),
            plan_std: std::array::from_fn(|i| std::array::from_fn(|j| plan.std[i * 15 + j])),
            pose: raw.normal("pose")?,
            wide_euler: raw.normal("wide_from_device_euler")?,
            road_transform: raw.normal("road_transform")?,
            lanes: raw.normal("lane_lines")?,
            edges: raw.normal("road_edges")?,
            leads: raw.leads()?,
            lane_prob: raw.array::<8>("lane_lines_prob")?.map(sigmoid),
            lead_prob: raw.array::<3>("lead_prob")?.map(sigmoid),
            meta: raw.array::<55>("meta")?.map(sigmoid),
            desire_state,
            desire_prediction,
            direct_action: if raw.has("action") {
                Some(raw.array("action")?)
            } else {
                None
            },
        })
    }
}

#[derive(Debug)]
pub struct DriverData {
    pub face: Normal<6>,
    pub probabilities: [f32; 8],
}

impl DriverData {
    fn parse(raw: &RawOutputs<'_>, suffix: &str) -> Result<Self, ParseError> {
        let face = raw.normal(&format!("face_descs_{suffix}"))?;
        let mut probabilities = [0.0; 8];
        for (value, name) in probabilities.iter_mut().zip([
            "face_prob",
            "left_eye_prob",
            "right_eye_prob",
            "left_blink_prob",
            "right_blink_prob",
            "sunglasses_prob",
            "using_phone_prob",
            "sleep_prob",
        ]) {
            *value = sigmoid(raw.array::<1>(&format!("{name}_{suffix}"))?[0]);
        }
        Ok(Self {
            face,
            probabilities,
        })
    }
}

#[derive(Debug)]
pub struct DriverPrediction {
    pub left: DriverData,
    pub right: DriverData,
    pub wheel_on_right: f32,
}

impl DriverPrediction {
    pub fn parse(raw: &RawOutputs<'_>) -> Result<Self, ParseError> {
        Ok(Self {
            left: DriverData::parse(raw, "lhd")?,
            right: DriverData::parse(raw, "rhd")?,
            wheel_on_right: sigmoid(raw.array::<1>("wheel_on_right")?[0]),
        })
    }
}
