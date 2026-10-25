//! Planner 비교의 실제 명령 순서·로그·첫 실패 전파를 소유하는 native controller.
use crate::{
    qa_process::{CommandSpec, Runner},
    seal::Result,
};
use serde::Serialize;
use std::{fmt, fs, io::Write, path::Path};

mod cli;
pub mod memory;
pub mod plan;
pub mod scheduler;
pub use cli::cli;

#[derive(Clone, Debug)]
pub struct Check {
    pub name: String,
    pub command: CommandSpec,
    pub output_option: String,
    /// 이 명령의 실제 예상 증가량. 과거의 고정 25/35 GiB 정책은 사용하지 않는다.
    pub growth_bytes: u64,
    /// IPC 비교는 native 시각 endpoint를 실제로 거친 프레임 수까지 요구한다.
    pub scheduler_calls: Option<usize>,
}

impl Check {
    pub fn invocation(&self, output: &Path, root: &Path) -> Result<CommandSpec> {
        if self.name.is_empty()
            || !self
                .name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        {
            return Err("invalid Planner check name".into());
        }
        let mut command = self.command.clone();
        let mut argv = command.effective_argv();
        argv.push(self.output_option.clone().into());
        argv.push(output.join(&self.name).into_os_string());
        command.os_argv = Some(argv);
        command.cwd = Some(root.to_owned());
        command.merge_output = true;
        Ok(command)
    }
}

#[derive(Debug, Serialize)]
pub struct CommandRecord {
    pub name: String,
    pub argv: Vec<String>,
    pub returncode: i32,
    pub free_before: u64,
    pub growth_bytes: u64,
}

#[derive(Debug)]
pub struct CheckFailure {
    pub name: String,
    pub returncode: i32,
}
impl fmt::Display for CheckFailure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Planner check {} exited with {}",
            self.name, self.returncode
        )
    }
}
impl std::error::Error for CheckFailure {}

pub fn write_json(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    fs::write(path, bytes)?;
    Ok(())
}

/// 실제 Runner를 통해 각 leaf를 실행한다. 원본처럼 실패 행을 먼저 기록하고
/// 즉시 반환하므로 다음 leaf나 최종 PASS receipt에 도달하지 않는다.
pub fn run_checks(
    checks: &[Check],
    output: &Path,
    root: &Path,
    runner: &mut dyn Runner,
) -> Result<Vec<CommandRecord>> {
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(output)?;
    let mut records = Vec::new();
    for check in checks {
        let mut command = check.invocation(output, root)?;
        let scheduler_evidence = output.join(format!("{}-scheduler.tsv", check.name));
        if check.scheduler_calls.is_some() {
            command.env.insert(
                "PLANNER_NATIVE_SCHEDULER_EVIDENCE".into(),
                scheduler_evidence
                    .to_str()
                    .ok_or("scheduler evidence path must be UTF-8")?
                    .into(),
            );
        }
        let argv = command
            .effective_argv()
            .into_iter()
            .map(|arg| {
                arg.into_string()
                    .map_err(|_| "Planner evidence requires UTF-8 argv".into())
            })
            .collect::<Result<Vec<_>>>()?;
        let space = openpilot_disk_space::inspect(output, check.growth_bytes)?;
        space.require()?;
        let result = runner.run_to_file(&command, &output.join(format!("{}.log", check.name)))?;
        records.push(CommandRecord {
            name: check.name.clone(),
            argv,
            returncode: result.code,
            free_before: space.free_bytes,
            growth_bytes: check.growth_bytes,
        });
        write_json(&output.join("commands.json"), &records)?;
        println!("{} {}", check.name, result.code);
        std::io::stdout().flush()?;
        if result.code != 0 {
            return Err(Box::new(CheckFailure {
                name: check.name.clone(),
                returncode: result.code,
            }));
        }
        if let Some(expected) = check.scheduler_calls {
            scheduler::verify_evidence(&scheduler_evidence, expected)?;
        }
    }
    Ok(records)
}
