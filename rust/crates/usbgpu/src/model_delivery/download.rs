use super::Error;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Clone, Copy)]
pub enum DownloadKind {
    Model,
    Precompiled,
}

#[derive(Clone, Copy)]
pub enum Event {
    Progress { downloaded: u64, total: u64 },
    Verifying,
}

pub fn sha256(path: &Path) -> Result<String, Error> {
    let mut file = File::open(path)?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0; 1024 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hash.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hash.finalize()))
}

fn size(path: &Path) -> Result<u64, Error> {
    match path.metadata() {
        Ok(value) if value.is_file() => Ok(value.len()),
        Ok(_) => Ok(0),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(0),
        Err(error) => Err(error.into()),
    }
}

/// Preserve the two source download policies, including partial files on failure.
///
/// # Errors
/// Returns verification, storage, HTTP or filesystem failures without activation.
pub fn download(
    agent: &ureq::Agent,
    url: &str,
    target: &Path,
    expected_size: u64,
    expected_sha: &str,
    kind: DownloadKind,
    progress: &mut impl FnMut(u64, u64),
) -> Result<PathBuf, Error> {
    download_observed(
        agent,
        url,
        target,
        expected_size,
        expected_sha,
        kind,
        &mut |event| match event {
            Event::Progress { downloaded, total } => progress(downloaded, total),
            Event::Verifying => {}
        },
    )
}

pub fn download_observed(
    agent: &ureq::Agent,
    url: &str,
    target: &Path,
    expected_size: u64,
    expected_sha: &str,
    kind: DownloadKind,
    observe: &mut impl FnMut(Event),
) -> Result<PathBuf, Error> {
    let parent = target
        .parent()
        .ok_or_else(|| Error::Invalid("artifact has no parent".into()))?;
    fs::create_dir_all(parent)?;
    if size(target)? == expected_size && target.is_file() {
        if matches!(kind, DownloadKind::Model) {
            observe(Event::Verifying);
        }
        if sha256(target)? == expected_sha {
            return Ok(target.to_path_buf());
        }
        if matches!(kind, DownloadKind::Model) {
            fs::remove_file(target)?;
        }
    }
    let mut partial = target.as_os_str().to_os_string();
    partial.push(".part");
    let partial = PathBuf::from(partial);
    let mut offset = size(&partial)?;
    if offset >= expected_size && partial.is_file() {
        if offset == expected_size && matches!(kind, DownloadKind::Model) {
            observe(Event::Verifying);
        }
        if offset == expected_size && sha256(&partial)? == expected_sha {
            fs::rename(&partial, target)?;
            return Ok(target.to_path_buf());
        }
        fs::remove_file(&partial)?;
        offset = 0;
    }
    let capacity = rustix::fs::statvfs(parent).map_err(std::io::Error::from)?;
    let available = capacity.f_bavail.saturating_mul(capacity.f_frsize);
    let required = expected_size
        .saturating_sub(offset)
        .saturating_add(256 << 20);
    if available < required {
        return Err(Error::Invalid(format!(
            "insufficient storage (need {required} bytes)"
        )));
    }
    let user_agent = match kind {
        DownloadKind::Model => "carrot-modeld/1",
        DownloadKind::Precompiled => "carrot-precompiled/1",
    };
    let mut request = agent
        .get(url)
        .header("Accept-Encoding", "identity")
        .header("User-Agent", user_agent);
    if offset != 0 {
        request = request.header("Range", format!("bytes={offset}-"));
    }
    if matches!(kind, DownloadKind::Model) {
        observe(Event::Progress {
            downloaded: offset,
            total: expected_size,
        });
    }
    let response = request.call()?;
    let append = offset != 0 && response.status().as_u16() == 206;
    if append {
        let range = response
            .headers()
            .get("Content-Range")
            .and_then(|value| value.to_str().ok())
            .unwrap_or("");
        if !range.starts_with(&format!("bytes {offset}-")) {
            return Err(Error::Invalid("incorrect artifact resume response".into()));
        }
    } else {
        if response.status().as_u16() != 200 {
            return Err(Error::Invalid(format!(
                "unexpected download status {}",
                response.status().as_u16()
            )));
        }
        if matches!(kind, DownloadKind::Precompiled) {
            offset = 0;
        }
    }
    let chunk = match kind {
        DownloadKind::Model => 4 << 20,
        DownloadKind::Precompiled => 1 << 20,
    };
    let mut buffer = vec![0; chunk];
    let mut reader = response.into_body().into_reader();
    let mut file = OpenOptions::new()
        .write(true)
        .create(true)
        .append(append)
        .truncate(!append)
        .open(&partial)?;
    loop {
        let mut filled = 0;
        while filled < buffer.len() {
            let count = reader.read(&mut buffer[filled..])?;
            if count == 0 {
                break;
            }
            filled += count;
        }
        if filled == 0 {
            break;
        }
        offset = offset
            .checked_add(
                u64::try_from(filled)
                    .map_err(|_| Error::Invalid("artifact length overflow".into()))?,
            )
            .ok_or_else(|| Error::Invalid("artifact length overflow".into()))?;
        if matches!(kind, DownloadKind::Precompiled) && offset > expected_size {
            return Err(Error::Invalid("artifact exceeds declared size".into()));
        }
        file.write_all(&buffer[..filled])?;
        observe(Event::Progress {
            downloaded: offset,
            total: expected_size,
        });
    }
    file.flush()?;
    file.sync_all()?;
    let actual = size(&partial)?;
    if actual != expected_size {
        return Err(Error::Invalid(format!(
            "model size mismatch: expected {expected_size}, got {actual}"
        )));
    }
    if matches!(kind, DownloadKind::Model) {
        observe(Event::Verifying);
    }
    if sha256(&partial)? != expected_sha {
        fs::remove_file(&partial)?;
        return Err(Error::Invalid("artifact sha256 mismatch".into()));
    }
    fs::rename(partial, target)?;
    Ok(target.to_path_buf())
}
