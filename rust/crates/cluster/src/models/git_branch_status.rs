use super::GitBranchStatusState;
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct GitBranchStatus {
    pub branch: String,
    pub state: GitBranchStatusState,
    pub detail: String,
}
impl Default for GitBranchStatus {
    fn default() -> Self {
        Self {
            branch: String::new(),
            state: GitBranchStatusState::Unknown,
            detail: String::new(),
        }
    }
}
