use openpilot_webcam::{
    runtime::Camerad,
    selection::{CameraKind, CameraSpec},
    Error,
};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Write},
    path::PathBuf,
};

#[derive(serde::Deserialize)]
struct Spec {
    kind: String,
    input: String,
}

fn descriptors() -> Result<BTreeMap<String, PathBuf>, Error> {
    let mut result = BTreeMap::new();
    for entry in fs::read_dir("/proc/self/fd")? {
        let entry = entry?;
        let target = match fs::read_link(entry.path()) {
            Ok(target) => target,
            Err(error) if error.kind() == io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error.into()),
        };
        if target == std::path::Path::new(&format!("/proc/{}/fd", std::process::id())) {
            continue;
        }
        result.insert(entry.file_name().to_string_lossy().into_owned(), target);
    }
    Ok(result)
}

fn specifications(folder: &std::path::Path, input: &str) -> Result<Vec<CameraSpec>, Error> {
    let specs: Vec<Spec> = serde_json::from_slice(&fs::read(input)?)?;
    specs
        .into_iter()
        .map(|spec| {
            let input = PathBuf::from(&spec.input);
            let input = match fs::symlink_metadata(&input) {
                Ok(_) => input.canonicalize()?,
                Err(error) if error.kind() == io::ErrorKind::NotFound => input
                    .parent()
                    .ok_or(Error::Contract("camera fixture has no parent"))?
                    .canonicalize()?
                    .join(
                        input
                            .file_name()
                            .ok_or(Error::Contract("camera fixture has no filename"))?,
                    ),
                Err(error) => return Err(error.into()),
            };
            if !input.starts_with(folder) {
                return Err(Error::Contract("camera fixture outside owned root"));
            }
            let kind = match spec.kind.as_str() {
                "road" => CameraKind::Road,
                "wide" => CameraKind::WideRoad,
                "driver" => CameraKind::Driver,
                _ => return Err(Error::Contract("unknown camera kind")),
            };
            Ok(CameraSpec {
                kind,
                id: input
                    .to_str()
                    .ok_or(Error::Contract("camera fixture path is not Unicode"))?
                    .into(),
                device: input
                    .to_str()
                    .ok_or(Error::Contract("camera fixture path is not Unicode"))?
                    .into(),
            })
        })
        .collect::<Result<Vec<_>, Error>>()
}

fn main() -> Result<(), Error> {
    let mut args = std::env::args().skip(1);
    let folder = PathBuf::from(args.next().ok_or(Error::Contract("missing owned output"))?)
        .canonicalize()?;
    if std::env::var("WEBCAM_OWNED_ROOT").ok().as_deref() != folder.to_str()
        || std::env::var("OPENPILOT_PREFIX")
            .unwrap_or_default()
            .is_empty()
    {
        return Err(Error::Contract(
            "webcam probe requires owned root and namespace",
        ));
    }
    let input = args
        .next()
        .ok_or(Error::Contract("missing specification"))?;
    let specs = specifications(&folder, &input)?;
    let before = descriptors()?;
    let mut daemon = Camerad::prepare(specs.clone())?;
    let allocated = descriptors()?;
    let mut rejected = Vec::new();
    let mut unaligned_clients = Vec::new();
    for spec in &specs {
        let stream = match spec.kind {
            CameraKind::Road => openpilot_msgq::VisionStream::Road,
            CameraKind::WideRoad => openpilot_msgq::VisionStream::WideRoad,
            CameraKind::Driver => openpilot_msgq::VisionStream::Driver,
        };
        let mut client = openpilot_msgq::VisionClient::new("camerad", stream, false)?;
        let result = client.connect();
        if let Err(error) = result {
            rejected
                .push(serde_json::json!({"service":spec.kind.service(),"error":error.to_string()}));
        } else if client
            .layout()
            .is_some_and(|layout| !layout.len.is_multiple_of(8))
        {
            unaligned_clients.push((spec.kind, client));
        }
    }
    let after_client_probe = descriptors()?;
    println!("READY");
    io::stdout().flush()?;
    let mut command = String::new();
    io::stdin().read_line(&mut command)?;
    if command.trim() != "RUN" {
        return Err(Error::Contract("missing worker start"));
    }
    let mut messages = Vec::new();
    let reports = daemon.run(|publication| {
        let target = folder.join(format!("{}-{}.bin", publication.kind.service(), publication.frame_id));
        fs::write(&target, publication.bytes)?;
        messages.push(serde_json::json!({"service":publication.kind.service(), "frame_id":publication.frame_id,"raw":target}));
        Ok(())
    })?;
    let unaligned_observations = unaligned_frames(unaligned_clients)?;
    let after_run = descriptors()?;
    drop(daemon);
    let after_drop = descriptors()?;
    let socket = PathBuf::from(format!(
        "/tmp/{}_visionipc_camerad",
        std::env::var("OPENPILOT_PREFIX").map_err(|_| Error::Contract("missing prefix"))?
    ));
    fs::write(
        folder.join("worker-result.json"),
        serde_json::to_vec_pretty(
            &serde_json::json!({"messages":messages,"cameras_before_caller_cleanup":reports,
                "descriptors":{"before":before,"allocated":allocated,"after_client_probe":after_client_probe,"after_run":after_run,"after_drop":after_drop},
                "native_client_rejections":rejected,"native_unaligned_frames":unaligned_observations,"listener_removed":!socket.exists()}),
        )?,
    )?;
    Ok(())
}

fn unaligned_frames(
    clients: Vec<(CameraKind, openpilot_msgq::VisionClient)>,
) -> Result<Vec<serde_json::Value>, Error> {
    let mut result = Vec::new();
    for (kind, mut client) in clients {
        let frame = client
            .receive(std::time::Duration::ZERO)?
            .ok_or(Error::Contract(
                "unaligned native client did not receive a frame",
            ))?;
        let mut bytes = vec![0; frame.metadata().len];
        frame.copy_into(&mut bytes)?;
        result.push(serde_json::json!({"service":kind.service(),"frame_id":frame.metadata().frame_id,"buffer_frame_id":frame.descriptor()?.buffer_frame_id,"bytes":bytes}));
    }
    Ok(result)
}
