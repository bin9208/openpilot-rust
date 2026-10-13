use openpilot_usbgpu::{
    model::{self, Paths},
    model_delivery::{self, failure, packaged, precompiled, validation},
    worker_artifact,
};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let assets = PathBuf::from(args.next().ok_or("missing installed assets path")?);
    let cache = PathBuf::from(args.next().ok_or("missing owned cache path")?);
    let worker = PathBuf::from(args.next().ok_or("missing native worker path")?);
    if args.next().is_some() {
        return Err("unexpected argument".into());
    }
    let paths = Paths {
        models: cache.join("source-models"),
        cache,
        assets,
    };
    let selected = model_delivery::pinned_manifest();
    let seeded = packaged::seed(&paths, &selected)?;
    let (source, changed) = model_delivery::ensure(
        &model_delivery::agent(1),
        &selected,
        &paths.cache,
        &mut |_, _| {},
    )?;
    let model = precompiled::ensure(
        &model_delivery::agent(1),
        &model_delivery::agent(1),
        &selected,
        &paths.cache,
        &paths.assets,
        &mut |_, _| {},
    )?
    .ok_or("native packaged model unavailable")?;
    let mut cameras = Vec::new();
    for camera in [[1344, 760], [1928, 1208]] {
        let artifact = worker_artifact::prepare(&model, &paths.assets, camera, None)?;
        cameras.push(
            serde_json::json!({"camera":camera,"checkpoint":artifact.info.checkpoint,
            "frame_size":artifact.info.frame_size,"output_count":artifact.info.output_count,
            "descriptor_sha256":digest(&artifact.descriptor),"warp_sha256":digest(&artifact.warp),
            "manifest_sha256":artifact.manifest_sha256}),
        );
    }
    let status = model::status(&paths)?;
    if !seeded || !status.compiled || status.compile_pending {
        return Err("native source activation/readiness mismatch".into());
    }
    let binding = worker_artifact::bind(&model, &paths.assets)?;
    let key = validation::key(
        &model,
        &validation::Device {
            device: "mici".into(),
            machine_id: "owned-fixture".into(),
            os: "owned-host".into(),
            kernel: "owned-fixture".into(),
            machine: std::env::consts::ARCH.into(),
        },
        &validation::Provider {
            worker: &worker,
            runner: Path::new(&std::env::current_exe()?),
            manifest_sha256: binding.manifest_sha256(),
        },
    )?;
    validation::save(&model, key.as_deref())?;
    if !validation::cached(&model, key.as_deref()) {
        return Err("native provider validation identity was not reusable".into());
    }
    let transient = failure::record(
        &model,
        "f0 out failed: -1",
        failure::Kind::Detail,
        "owned-fixture",
        1.,
    )?;
    let still_ready = model::status(&paths)?.compiled;
    let fatal = failure::record(
        &model,
        "owned persistent kernel failure",
        failure::Kind::Detail,
        "owned-fixture",
        2.,
    )?;
    let rejected_ready = model::status(&paths)?.compiled;
    let retry = precompiled::ensure(
        &model_delivery::agent(1),
        &model_delivery::agent(1),
        &selected,
        &paths.cache,
        &paths.assets,
        &mut |_, _| {},
    )?;
    if transient || !still_ready || !fatal || rejected_ready || retry.is_some() {
        return Err("native provider rejection/retry policy changed".into());
    }
    serde_json::to_writer_pretty(
        std::io::stdout().lock(),
        &serde_json::json!({
        "seeded":seeded,"source":source,"changed":changed,"model":model,"cameras":cameras,
        "transient_rejected":transient,"transient_ready":still_ready,"fatal_rejected":fatal,
        "fatal_ready":rejected_ready,"retry_selected":retry.is_some(),"validation_key":key,
        "scope":"actual source/cache/native artifact APIs; no hardware open or GPU execution"}),
    )?;
    Ok(())
}
