#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize, Default)]
pub struct TpmsInfo {
    pub fl: Option<f64>,
    pub fr: Option<f64>,
    pub rl: Option<f64>,
    pub rr: Option<f64>,
}
