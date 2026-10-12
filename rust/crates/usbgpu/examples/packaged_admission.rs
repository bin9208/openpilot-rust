use openpilot_usbgpu::{
    model::{self, Paths},
    model_delivery::pinned_manifest,
    worker_artifact,
};
use std::{
    fs::{FileTimes, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::MetadataExt,
    path::PathBuf,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let assets = PathBuf::from(args.next().ok_or("missing installed assets path")?);
    let cache = PathBuf::from(args.next().ok_or("missing owned cache path")?);
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    let paths = Paths {
        models: cache.join("source-models"),
        cache,
        assets,
    };
    if !model::status(&paths)?.compiled {
        return Err("genuine baseline model is not ready".into());
    }
    for _ in 0..9 {
        if !model::status(&paths)?.compiled {
            return Err("unchanged admitted model became unavailable".into());
        }
    }
    let model = paths
        .cache
        .join("precompiled")
        .join(pinned_manifest().sha256)
        .join("model.pkl");
    let mut file = OpenOptions::new().read(true).write(true).open(&model)?;
    let before = file.metadata()?;
    let modified = before.modified()?;
    file.seek(SeekFrom::Start(16))?;
    let mut original = [0];
    file.read_exact(&mut original)?;
    file.seek(SeekFrom::Start(16))?;
    file.write_all(&[original[0] ^ 1])?;
    file.set_times(FileTimes::new().set_modified(modified))?;
    let changed = file.metadata()?;
    let changed_ready = model::status(&paths);
    let changed_worker = worker_artifact::prepare(&model, &paths.assets, [1344, 760], None);
    file.seek(SeekFrom::Start(16))?;
    file.write_all(&original)?;
    file.set_times(FileTimes::new().set_modified(modified))?;
    let changed_ready = changed_ready?.compiled;
    let worker_rejected = changed_worker.is_err();
    let restored = model::status(&paths)?.compiled;
    if changed_ready || !worker_rejected || !restored {
        return Err("same-size mutation escaped readiness/worker admission".into());
    }
    serde_json::to_writer_pretty(
        std::io::stdout().lock(),
        &serde_json::json!({
        "path":model,"unchanged_polls":9,"changed_ready":changed_ready,
        "worker_rejected":worker_rejected,"restored_ready":restored,
        "same_inode":before.ino()==changed.ino(),"same_device":before.dev()==changed.dev(),
        "same_size":before.len()==changed.len(),
        "same_mtime":before.mtime()==changed.mtime()&&before.mtime_nsec()==changed.mtime_nsec(),
        "before_ctime":[before.ctime(),before.ctime_nsec()],
        "changed_ctime":[changed.ctime(),changed.ctime_nsec()],
        "scope":"owned mutable cache; genuine pinned PKL; no GPU/device open"}),
    )?;
    Ok(())
}
