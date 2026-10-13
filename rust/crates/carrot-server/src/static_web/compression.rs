use crate::{static_assets::resolve, Error};
use flate2::{Compression, GzBuilder};
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex},
    time::{SystemTime, UNIX_EPOCH},
};

static LOCKS: LazyLock<Mutex<HashMap<PathBuf, Arc<Mutex<()>>>>> = LazyLock::new(Mutex::default);

fn source_key(path: &Path) -> Option<(SystemTime, u64)> {
    let metadata = fs::metadata(path).ok()?;
    metadata
        .is_file()
        .then(|| Some((metadata.modified().ok()?, metadata.len())))?
}

fn remove_sidecar(path: &Path) -> Result<(), Error> {
    match fs::remove_file(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error.into()),
    }
}

fn atomic_replace(path: &Path, encoded: &[u8]) -> Result<(), Error> {
    let parent = path
        .parent()
        .ok_or_else(|| Error::Source("invalid static asset directory".into()))?;
    let mut temporary = tempfile::Builder::new()
        .prefix(".carrot_asset_")
        .tempfile_in(parent)?;
    temporary.write_all(encoded)?;
    temporary
        .persist(path)
        .map_err(|error| Error::Io(error.error))?;
    Ok(())
}

fn gzip_matches(path: &Path, bytes: &[u8]) -> bool {
    let Ok(file) = fs::File::open(path) else {
        return false;
    };
    let mut decoded = flate2::read::MultiGzDecoder::new(file);
    let mut old = Vec::new();
    decoded.read_to_end(&mut old).is_ok() && old == bytes
}

pub(crate) fn refresh_asset(
    root: &Path,
    path: &str,
    codec: Option<&crate::static_web::brotli::Brotli>,
) -> Result<(), Error> {
    if (!path.ends_with(".js") && !path.ends_with(".css"))
        || path.starts_with("//")
        || path.starts_with("\\\\")
    {
        return Ok(());
    }
    let Some(source) = resolve(root, path) else {
        return Ok(());
    };
    let lock = {
        let mut locks = LOCKS
            .lock()
            .map_err(|_| Error::Source("static asset lock poisoned".into()))?;
        Arc::clone(
            locks
                .entry(source.clone())
                .or_insert_with(|| Arc::new(Mutex::new(()))),
        )
    };
    let _guard = lock
        .lock()
        .map_err(|_| Error::Source("static asset lock poisoned".into()))?;
    let gzip = source.with_file_name(format!(
        "{}.gz",
        source.file_name().unwrap_or_default().to_string_lossy()
    ));
    let brotli = source.with_file_name(format!(
        "{}.br",
        source.file_name().unwrap_or_default().to_string_lossy()
    ));
    for _ in 0..3 {
        let Some(before) = source_key(&source) else {
            continue;
        };
        let Ok(bytes) = fs::read(&source) else {
            continue;
        };
        if source_key(&source) != Some(before) || u64::try_from(bytes.len()).ok() != Some(before.1)
        {
            continue;
        }
        let mtime = u32::try_from(
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        )
        .map_err(|_| Error::Source("gzip timestamp overflow".into()))?;
        let gzip_payload = if gzip_matches(&gzip, &bytes) {
            None
        } else {
            let mut compressor = GzBuilder::new()
                .mtime(mtime)
                .write(Vec::new(), Compression::best());
            compressor.write_all(&bytes)?;
            Some(compressor.finish()?)
        };
        let brotli_payload = match codec {
            Some(codec)
                if !fs::read(&brotli).is_ok_and(|encoded| codec.matches(&encoded, &bytes)) =>
            {
                Some(codec.compress(&bytes)?)
            }
            Some(_) | None => None,
        };
        if source_key(&source) != Some(before) {
            continue;
        }
        if let Some(payload) = gzip_payload {
            atomic_replace(&gzip, &payload)?;
        }
        if source_key(&source) != Some(before) {
            continue;
        }
        match codec {
            None => remove_sidecar(&brotli)?,
            Some(_) => {
                if let Some(payload) = brotli_payload {
                    atomic_replace(&brotli, &payload)?;
                }
            }
        }
        if source_key(&source) == Some(before) {
            return Ok(());
        }
    }
    remove_sidecar(&gzip)?;
    remove_sidecar(&brotli)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{gzip_matches, refresh_asset};
    use crate::static_assets::Assets;
    use std::{fs, sync::Barrier};

    #[test]
    fn stale_brotli_removed_when_optional_codec_is_absent() {
        let directory = tempfile::tempdir().expect("fixture directory");
        let source = b"export const fixture = 1;\r\n";
        fs::write(directory.path().join("app.js"), source).expect("source");
        fs::write(directory.path().join("app.js.br"), b"stale").expect("sidecar");
        refresh_asset(directory.path(), "app.js", None).expect("refresh");
        let gzip_matches = gzip_matches(&directory.path().join("app.js.gz"), source);
        let brotli_exists = directory.path().join("app.js.br").exists();
        println!(
            "ABSENT_CODEC_RESULT: {{\"gzip_matches\":{gzip_matches},\"brotli_exists\":{brotli_exists}}}"
        );
        assert!(gzip_matches && !brotli_exists);
    }

    #[test]
    fn concurrent_refresh_when_requests_and_startup_share_asset_lock() {
        let directory = tempfile::tempdir().expect("fixture directory");
        fs::create_dir(directory.path().join("js")).expect("asset directory");
        let source = b"export const fixture = 1;\r\n";
        let path = directory.path().join("js/app.js");
        fs::write(&path, source).expect("source");
        fs::write(path.with_extension("js.gz"), b"stale").expect("sidecar");
        let barrier = Barrier::new(4);
        std::thread::scope(|scope| {
            let workers: Vec<_> = (0..4)
                .map(|index| {
                    let root = directory.path();
                    let barrier = &barrier;
                    scope.spawn(move || {
                        let assets = Assets::default();
                        barrier.wait();
                        if index % 2 == 0 {
                            assets.precompress(root)
                        } else {
                            assets.refresh(root, "/js/app.js")
                        }
                    })
                })
                .collect();
            for worker in workers {
                worker.join().expect("refresh worker").expect("refresh");
            }
        });
        assert!(gzip_matches(&path.with_extension("js.gz"), source));
        let codec = crate::static_web::brotli::Brotli::load().expect("host Brotli dependency");
        assert!(codec.matches(
            &fs::read(path.with_extension("js.br")).expect("sidecar"),
            source
        ));
        assert_eq!(
            fs::read_dir(path.parent().expect("directory"))
                .expect("directory entries")
                .count(),
            3
        );
    }
}
