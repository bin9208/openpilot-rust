use super::ModelPathPoint;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LaneMarking {
    pub offset: f64,
    pub color: [u8; 3],
    pub style: String,
    pub visible: bool,
    pub width: i64,
    pub model_points: Vec<ModelPathPoint>,
    pub model_lateral_shift_m: f64,
}
impl Default for LaneMarking {
    fn default() -> Self {
        Self {
            offset: 0.0,
            color: [255, 255, 255],
            style: "solid".into(),
            visible: true,
            width: 5,
            model_points: Vec::new(),
            model_lateral_shift_m: 0.0,
        }
    }
}
