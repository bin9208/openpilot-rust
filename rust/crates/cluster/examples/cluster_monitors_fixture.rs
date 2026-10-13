use openpilot_cluster::monitors::{
    read_cluster_threads, read_git_status, read_head_branch, resolve_git_dir, valid_address,
    CoreHistory, GitCommands, GitOutput, ThreadCpuSample,
};
use serde::Deserialize;
use std::{
    collections::VecDeque,
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Deserialize)]
struct Request {
    addresses: Vec<Vec<u8>>,
    sequences: Vec<Sequence>,
    proc_roots: Vec<PathBuf>,
    repos: Vec<PathBuf>,
    git: Vec<GitCase>,
}
#[derive(Deserialize)]
struct Sequence {
    clock_ticks: i64,
    updates: Vec<Update>,
}
#[derive(Deserialize)]
struct Update {
    now: f64,
    samples: Vec<ThreadCpuSample>,
}
#[derive(Deserialize)]
struct GitCase {
    repo: Option<PathBuf>,
    outputs: Vec<GitOutput>,
}
struct Commands {
    outputs: VecDeque<GitOutput>,
    calls: Vec<serde_json::Value>,
}
impl GitCommands for Commands {
    fn run(&mut self, _repo: &Path, args: &[&str], timeout: Duration) -> GitOutput {
        self.calls
            .push(serde_json::json!({"args":args,"timeout":timeout.as_secs_f64()}));
        self.outputs.pop_front().expect("unexpected Git command")
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let request: Request = serde_json::from_reader(std::io::stdin())?;
    let addresses: Vec<_> = request
        .addresses
        .iter()
        .map(|value| valid_address(value))
        .collect();
    let histories: Vec<_> = request
        .sequences
        .into_iter()
        .map(|sequence| {
            let mut history = CoreHistory::new(sequence.clock_ticks);
            sequence
                .updates
                .into_iter()
                .map(|update| history.observe(&update.samples, update.now))
                .collect::<Vec<_>>()
        })
        .collect();
    let scans: Vec<_> = request
        .proc_roots
        .iter()
        .map(|root| read_cluster_threads(root, 42))
        .collect();
    let repos: Vec<_> = request.repos.iter().map(|root| serde_json::json!({"head":read_head_branch(root),"git_dir":resolve_git_dir(root)})).collect();
    let git: Vec<_> = request
        .git
        .into_iter()
        .map(|case| {
            let mut commands = Commands {
                outputs: case.outputs.into(),
                calls: Vec::new(),
            };
            let status =
                read_git_status(case.repo.as_deref(), &mut commands, Duration::from_secs(4));
            assert!(commands.outputs.is_empty(), "unconsumed Git output");
            serde_json::json!({"status":status,"calls":commands.calls})
        })
        .collect();
    serde_json::to_writer(
        std::io::stdout(),
        &serde_json::json!({"addresses":addresses,"histories":histories,"scans":scans,"repos":repos,"git":git}),
    )?;
    Ok(())
}
