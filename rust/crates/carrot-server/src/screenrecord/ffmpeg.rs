use super::{catalog, Failure};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
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
    let program = locate(program).ok_or_else(|| Failure::http(503, "ffmpeg not available"))?;
    let mut child = Command::new(program)
        .args(["-hide_banner", "-loglevel", "error", "-y", "-ss", "1", "-i"])
        .arg(&path)
        .args(["-vframes", "1", "-vf", "scale=320:-1"])
        .arg(&output)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|_| Failure::Internal)?;
    let mut stdout = child.stdout.take().ok_or(Failure::Internal)?;
    let mut stderr = child.stderr.take().ok_or(Failure::Internal)?;
    let stdout = thread::spawn(move || {
        let mut bytes = Vec::new();
        stdout.read_to_end(&mut bytes).map(|_| bytes)
    });
    let stderr = thread::spawn(move || {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).map(|_| bytes)
    });
    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ok(status),
            Ok(None) if start.elapsed() < Duration::from_secs(90) => {
                thread::sleep(Duration::from_millis(5))
            }
            Ok(None) | Err(_) => {
                let _ = child.kill();
                let _ = child.wait();
                break Err(Failure::Internal);
            }
        }
    };
    let stdout = stdout
        .join()
        .map_err(|_| Failure::Internal)?
        .map_err(|_| Failure::Internal)?;
    let stderr = stderr
        .join()
        .map_err(|_| Failure::Internal)?
        .map_err(|_| Failure::Internal)?;
    let status = status?;
    let stderr = output_text(stderr)?;
    let stdout = output_text(stdout)?;
    if !status.success()
        || !output.is_file()
        || !fs::metadata(&output).is_ok_and(|metadata| metadata.len() > 0)
    {
        return Err(Failure::http(
            500,
            if !stderr.is_empty() {
                &stderr
            } else if !stdout.is_empty() {
                &stdout
            } else {
                "screenrecord thumbnail generation failed"
            },
        ));
    }
    Ok(output)
}
