use super::{catalog, Failure};
use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::Duration,
};

fn executable(path: &Path) -> bool {
    path.exists() && !path.is_dir() && rustix::fs::access(path, rustix::fs::Access::EXEC_OK).is_ok()
}

fn locate(program: &Path) -> Option<PathBuf> {
    if program.components().count() > 1 {
        return executable(program).then(|| program.to_path_buf());
    }
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths)
            .map(|path| path.join(program))
            .find(|path| executable(path))
    })
}

fn output_text(bytes: Vec<u8>) -> Result<String, Failure> {
    String::from_utf8(bytes)
        .map(|text| text.replace("\r\n", "\n").replace('\r', "\n"))
        .map_err(|_| Failure::Internal)
}

pub(crate) struct Completed {
    pub(crate) status: std::process::ExitStatus,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}
pub(crate) fn run(
    program: &Path,
    args: &[OsString],
    timeout: Duration,
) -> Result<Completed, Failure> {
    let program = locate(program).ok_or_else(|| Failure::http(503, "ffmpeg not available"))?;
    let mut child = Command::new(program)
        .args(["-hide_banner", "-loglevel", "error", "-y"])
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| Failure::Internal)?;
    let captured = openpilot_process_supervision::capture_output(&mut child, timeout)
        .map_err(|_| Failure::Internal)?;
    let stdout = output_text(captured.stdout)?;
    let stderr = output_text(captured.stderr)?;
    Ok(Completed {
        status: captured.status,
        stdout,
        stderr,
    })
}

pub fn thumbnail(
    directories: &[PathBuf],
    cache: &Path,
    program: &Path,
    id: &str,
    wall: i64,
) -> Result<PathBuf, Failure> {
    let path = super::find_file(directories, id, wall)?;
    let directory = cache.join("screen_thumb");
    fs::create_dir_all(&directory).map_err(|_| Failure::Internal)?;
    let output = directory.join(format!("{}.jpg", catalog::token(&crate::Value::text(id))));
    if output.is_file() && fs::metadata(&output).is_ok_and(|metadata| metadata.len() > 0) {
        return Ok(output);
    }
    let args = [
        OsString::from("-ss"),
        OsString::from("1"),
        OsString::from("-i"),
        path.into_os_string(),
        OsString::from("-vframes"),
        OsString::from("1"),
        OsString::from("-vf"),
        OsString::from("scale=320:-1"),
        output.clone().into_os_string(),
    ];
    let result = run(program, &args, Duration::from_secs(90))?;
    if !result.status.success()
        || !output.is_file()
        || !fs::metadata(&output).is_ok_and(|metadata| metadata.len() > 0)
    {
        return Err(Failure::http(
            500,
            if !result.stderr.is_empty() {
                &result.stderr
            } else if !result.stdout.is_empty() {
                &result.stdout
            } else {
                "screenrecord thumbnail generation failed"
            },
        ));
    }
    Ok(output)
}
