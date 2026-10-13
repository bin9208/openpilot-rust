#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct SceneCamera {
    pub active: bool,
    pub position_x_m: f64,
    pub position_y_m: f64,
    pub position_z_m: f64,
    pub right_x: f64,
    pub right_y: f64,
    pub right_z: f64,
    pub up_x: f64,
    pub up_y: f64,
    pub up_z: f64,
    pub forward_x: f64,
    pub forward_y: f64,
    pub forward_z: f64,
    pub center_y: f64,
    pub focal_x: f64,
    pub focal_y: f64,
}
