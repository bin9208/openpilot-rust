#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, Default)]
pub struct ClusterAlert {
    pub text1: String,
    pub text2: String,
    pub size: i64,
    pub status: i64,
    pub alert_type: String,
}
