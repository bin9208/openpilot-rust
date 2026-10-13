#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct ModelRiskPoint {
    pub t_s: f64,
    pub brake_disengage: f64,
    pub gas_disengage: f64,
    pub steer_override: f64,
    pub hard_brake_3: f64,
    pub hard_brake_4: f64,
    pub hard_brake_5: f64,
    pub gas_press: f64,
    pub brake_press: f64,
}
