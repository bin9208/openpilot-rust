#[derive(Clone, Debug)]
pub(crate) struct Request {
    pub epoch: u64,
    pub key: String,
    pub sequence: u64,
    pub payload: Vec<u8>,
    pub config: Vec<u8>,
    pub keyframe: bool,
    pub width: i64,
    pub height: i64,
    pub reset: bool,
}
