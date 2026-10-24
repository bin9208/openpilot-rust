//! Actual logger -> collector -> full Event route and rotating-disk assertions.
//! No synthetic logMessage publisher/barrier is allowed in this controller.
use super::super::{
    fixtures, hash,
    peer::{capacity, Owned},
    require, save, Request, Result,
};
use openpilot_cereal::log_capnp::event;
use openpilot_msgq::{Publisher, Subscriber};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Cursor,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::atomic::AtomicBool,
    time::{Duration, Instant},
};
const PHRASES: [&str; 3] = [
    "dropped 1 non iframe packets",
    "preserving ",
    "encoderd packet has a older segment!!!",
];
const SOURCE: &str = "--internal-loggerd-source-collector";

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn publication_requires_valid_exact_union_and_complete_json() {
        let mut message = capnp::message::Builder::new_default();
        let mut root = message.init_root::<event::Builder>();
        root.set_valid(true);
        root.set_error_log_message("{\"msg\":\"source-error\"}");
        let raw = capnp::serialize::write_message_to_words(&message);
        assert!(publication(&raw, false).is_err());
        assert_eq!(
            publication(&raw, true).unwrap(),
            "{\"msg\":\"source-error\"}"
        );
        message
            .get_root::<event::Builder>()
            .unwrap()
            .set_valid(false);
        assert!(publication(&capnp::serialize::write_message_to_words(&message), true).is_err());
    }
    #[test]
    fn pipeline_does_not_reduce_full_record_identity_to_message_text() {
        let published = PHRASES
            .map(|msg| serde_json::to_string(&json!({"msg":msg,"ctx":{"id":1}})).unwrap())
            .to_vec();
        let errors = vec![published.last().unwrap().clone()];
        let mut routed = published.clone();
        routed.push("{\"msg\":\"pipeline-drain\"}".into());
        let disk = PHRASES.join("\n");
        assert!(route_check(&published, &errors, &routed, &errors, &disk).is_ok());
        routed[1] = serde_json::to_string(&json!({"msg":PHRASES[1],"ctx":{"id":2}})).unwrap();
        assert!(route_check(&published, &errors, &routed, &errors, &disk).is_err());
    }
}
pub(super) fn source_worker(args: &[std::ffi::OsString]) -> Option<Result<()>> {
    (args.first().is_some_and(|v| v == SOURCE)).then(|| {
        require(
            args.len() == 3,
            "source collector requires endpoint and log root",
        )?;
        crate::managed_entry_collector_source::run(
            args[1].to_str().ok_or("collector endpoint not UTF-8")?,
            Path::new(&args[2]),
        )
    })
}
fn spawn(command: &mut Command, root: &Path, name: &str) -> Result<Owned> {
    // Arm the existing main-controller child guard before any fallible work.
    Ok(Owned(Some(
        command
            .stdin(Stdio::null())
            .stdout(File::create(root.join(format!("{name}.stdout")))?)
            .stderr(File::create(root.join(format!("{name}.stderr")))?)
            .spawn()?,
    )))
}
fn publication(raw: &[u8], error: bool) -> Result<String> {
    let mut cursor = Cursor::new(raw);
    let message = capnp::serialize::read_message(&mut cursor, Default::default())?;
    require(
        cursor.position() == raw.len() as u64,
        "collector packet has trailing bytes",
    )?;
    let root = message.get_root::<event::Reader<'_>>()?;
    require(root.get_valid(), "collector publication invalid")?;
    let text = match (error, root.which()?) {
        (false, event::LogMessage(v)) | (true, event::ErrorLogMessage(v)) => {
            v?.to_str()?.to_owned()
        }
        _ => return Err("collector publication union differs".into()),
    };
    let parsed: Value = serde_json::from_str(&text)?;
    require(
        parsed.is_object() && parsed["msg"].is_string(),
        "collector publication JSON/msg absent",
    )?;
    Ok(text)
}
fn receive(sub: &mut Subscriber, error: bool, output: &Path, index: usize) -> Result<String> {
    let raw = sub
        .receive(Duration::from_secs(10))?
        .ok_or("missing logger diagnostic publication")?;
    fs::write(
        output.join(format!(
            "{index:04}-{}.capnp",
            if error { "error" } else { "log" }
        )),
        &raw,
    )?;
    publication(&raw, error)
}
fn message(text: &str) -> Result<String> {
    let value: Value = serde_json::from_str(text)?;
    Ok(value["msg"]
        .as_str()
        .ok_or("diagnostic message absent")?
        .to_owned())
}
fn route_check(
    published: &[String],
    errors: &[String],
    routed: &[String],
    routed_errors: &[String],
    disk: &str,
) -> Result<()> {
    let expected = publication_route_check(published, errors, routed, routed_errors)?;
    for record in expected {
        require(
            disk.contains(&message(record)?),
            "warning/error absent from collector disk",
        )?;
    }
    require(
        !disk.contains("pipeline-drain"),
        "DEBUG drain marker leaked to collector disk",
    )
}
pub(super) fn publication_route_check<'a>(
    published: &'a [String],
    errors: &[String],
    routed: &[String],
    routed_errors: &[String],
) -> Result<Vec<&'a String>> {
    let mut expected = Vec::new();
    for record in published {
        let msg = message(record)?;
        if PHRASES.iter().any(|phrase| msg.contains(phrase)) {
            expected.push(record);
        }
    }
    require(
        expected.len() == 3,
        "exactly three source warning/error records required",
    )?;
    require(
        expected.iter().all(|row| routed.contains(row)),
        "full collector JSON missing from rlog",
    )?;
    require(
        errors.len() == 1 && errors.last() == published.last() && routed_errors == errors,
        "full error publication/rlog equality differs",
    )?;
    require(
        routed
            .iter()
            .any(|row| message(row).is_ok_and(|v| v == "pipeline-drain")),
        "pipeline marker absent from route",
    )?;
    Ok(expected)
}
pub(super) fn drive(request: &Request) -> Result<Value> {
    require(
        ["original", "rust"].contains(&request.name.as_str()) && request.runner.len() == 1,
        "pipeline side/collector binding invalid",
    )?;
    let output = &request.output;
    fs::create_dir_all(output.parent().ok_or("pipeline parent absent")?)?;
    fs::create_dir(output)?;
    let prefix = std::env::var("OPENPILOT_PREFIX")?;
    let endpoint_path = PathBuf::from(format!("/tmp/logmessage{prefix}"));
    let endpoint = format!("ipc://{}", endpoint_path.display());
    let disk_root = output
        .join("home")
        .join(format!(".comma{prefix}"))
        .join("log");
    fs::create_dir_all(&disk_root)?;
    let original = request.name == "original";
    let mut collector_command = if original {
        let mut command = Command::new(std::env::current_exe()?);
        command.arg(SOURCE).arg(&endpoint).arg(&disk_root);
        command
    } else {
        Command::new(&request.runner[0])
    };
    collector_command
        .env("HOME", output.join("home"))
        .env_remove("ROUTE_UPLOAD_QA_IPC_ROOT");
    let mut collector = spawn(&mut collector_command, output, "collector")?;
    let shm = PathBuf::from(format!("/dev/shm/msgq_{prefix}"));
    let deadline = Instant::now() + Duration::from_secs(20);
    while !["logMessage", "errorLogMessage"]
        .iter()
        .all(|s| shm.join(s).exists())
    {
        collector.alive()?;
        require(
            Instant::now() < deadline,
            "collector readiness exceeded source bound",
        )?;
        std::thread::sleep(Duration::from_millis(10));
    }
    for service in ["logMessage", "errorLogMessage"] {
        require(
            fs::metadata(shm.join(service))?.len() > capacity(service)? as u64,
            "collector queue capacity differs",
        )?;
    }
    let mut log = Subscriber::for_runtime("logMessage", false, capacity("logMessage")?)?;
    let mut error =
        Subscriber::for_runtime("errorLogMessage", false, capacity("errorLogMessage")?)?;
    let context = zmq::Context::new();
    let push = context.socket(zmq::PUSH)?;
    push.set_linger(0)?;
    push.set_sndtimeo(10_000)?;
    push.connect(&endpoint)?;
    // Authenticate the live normal endpoint with the outer supervisor before
    // launching either logger. No post-teardown pathname can establish ownership.
    crate::qa_endpoint::acknowledge(
        &endpoint_path,
        &PathBuf::from(
            std::env::var_os("LOGGERD_PIPELINE_ENDPOINT_OWNER")
                .ok_or("pipeline endpoint owner missing")?,
        ),
        &prefix,
        &AtomicBool::new(false),
    )?;
    let params = output.join("params").join(&prefix);
    fs::create_dir_all(&params)?;
    for (key, value) in [
        ("RecordRoadCam", "0"),
        ("RecordFront", "0"),
        ("RecordAudio", "0"),
        ("RouteCount", "42"),
    ] {
        fs::write(params.join(key), value)?;
    }
    let mut publishers = BTreeMap::new();
    for service in ["roadEncodeData", "userBookmark", "carState"] {
        publishers.insert(
            service,
            Publisher::for_runtime(service, capacity(service)?)?,
        );
    }
    let mut command = Command::new(&request.binary);
    command
        .env("PARAMS_ROOT", output.join("params"))
        .env("LOG_ROOT", output.join("routes"))
        .env("LOGGERD_TEST", "1")
        .env("LOGGERD_SEGMENT_LENGTH", "60")
        .env_remove("ROUTE_UPLOAD_QA_IPC_ROOT");
    let mut logger = spawn(&mut command, output, "logger")?;
    let deadline = Instant::now() + Duration::from_secs(20);
    while !params.join("CurrentRoute").exists() {
        logger.alive()?;
        require(
            Instant::now() < deadline,
            "pipeline CurrentRoute exceeded source bound",
        )?;
        std::thread::sleep(Duration::from_millis(5));
    }
    let route = fs::read_to_string(params.join("CurrentRoute"))?;
    require(
        !route.is_empty()
            && route
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-'),
        "unsafe pipeline route identifier",
    )?;
    let fixture = fs::read(&request.fixtures)?;
    let inputs = [
        (
            "roadEncodeData",
            fixtures::encoded(&fixture, "roadEncodeData", 70, 0, Some(false))?,
        ),
        (
            "roadEncodeData",
            fixtures::encoded(&fixture, "roadEncodeData", 70, 1, Some(true))?,
        ),
        ("userBookmark", fixtures::event("userBookmark", 0)?),
        (
            "roadEncodeData",
            fixtures::encoded(&fixture, "roadEncodeData", 0, 2, Some(true))?,
        ),
    ];
    let mut input_seals = Vec::new();
    fs::create_dir(output.join("inputs"))?;
    for (index, (service, bytes)) in inputs.iter().enumerate() {
        let path = format!("{index:04}-{service}.capnp");
        fs::write(output.join("inputs").join(&path), bytes)?;
        input_seals.push(json!({"service":service,"path":path,"sha256":hash(bytes)}));
        let publisher = publishers
            .get_mut(service)
            .ok_or("pipeline publisher absent")?;
        publisher.send(bytes)?;
        let deadline = Instant::now() + Duration::from_secs(10);
        while !publisher.readers_caught_up() {
            logger.alive()?;
            collector.alive()?;
            require(
                Instant::now() < deadline,
                "pipeline readers exceeded source bound",
            )?;
            std::thread::sleep(Duration::from_millis(1));
        }
    }
    save(&output.join("inputs/index.json"), &json!(input_seals))?;
    let packets = output.join("publications");
    fs::create_dir(&packets)?;
    let mut published = Vec::new();
    loop {
        require(
            published.len() < 10_000,
            "pipeline publication evidence bound exceeded",
        )?;
        let row = receive(&mut log, false, &packets, published.len())?;
        let done = message(&row)?.contains(PHRASES[2]);
        published.push(row);
        if done {
            break;
        }
    }
    let errors = vec![receive(&mut error, true, &packets, 0)?];
    require(
        errors.last() == published.last(),
        "collector error does not equal final complete log JSON",
    )?;
    push.send(b"\x0a{\"msg\":\"pipeline-drain\"}".as_slice(), 0)?;
    let marker = receive(&mut log, false, &packets, published.len())?;
    require(
        message(&marker)? == "pipeline-drain",
        "source pipeline drain marker differs",
    )?;
    std::thread::sleep(Duration::from_millis(200));
    rustix::process::kill_process(logger.pid()?, rustix::process::Signal::TERM)?;
    require(
        logger.wait(Duration::from_secs(20))? == 0,
        "pipeline logger shutdown failed",
    )?;
    rustix::process::kill_process(collector.pid()?, rustix::process::Signal::INT)?;
    let collector_exit = collector.wait(Duration::from_secs(10))?;
    require(
        collector_exit == if original { -2 } else { 0 },
        "collector SIGINT exit differs from source adapter/native lifecycle",
    )?;
    let segment = output.join("routes").join(format!("{route}--0"));
    for entry in fs::read_dir(&segment)? {
        require(
            !entry?.path().extension().is_some_and(|v| v == "lock"),
            "pipeline lock retained",
        )?;
    }
    let (raw, decoded) =
        crate::bootlog_artifact_check::decode_zstd(&fs::read(segment.join("rlog.zst"))?)?;
    fs::write(output.join("rlog.capnp"), &raw)?;
    save(&output.join("decoded.json"), &decoded)?;
    let mut routed: BTreeMap<&str, Vec<String>> =
        BTreeMap::from([("logMessage", Vec::new()), ("errorLogMessage", Vec::new())]);
    for row in decoded
        .as_array()
        .ok_or("pipeline full Event list absent")?
    {
        for service in ["logMessage", "errorLogMessage"] {
            if let Some(value) = row.get(service) {
                routed.get_mut(service).ok_or("route service absent")?.push(
                    value
                        .as_str()
                        .ok_or("route diagnostic is not complete text")?
                        .to_owned(),
                );
            }
        }
    }
    let mut disk = String::new();
    for entry in fs::read_dir(&disk_root)? {
        let path = entry?.path();
        if path
            .file_name()
            .is_some_and(|v| v.to_string_lossy().starts_with("swaglog."))
        {
            disk.push_str(&fs::read_to_string(path)?);
            disk.push('\n');
        }
    }
    route_check(
        &published,
        &errors,
        &routed["logMessage"],
        &routed["errorLogMessage"],
        &disk,
    )?;
    save(&output.join("publications.json"), &json!(published))?;
    save(&output.join("routed.json"), &json!(routed))?;
    let result = json!({"logger_sha256":crate::seal::digest(&request.binary)?,"collector":request.name,"published_records":published.len(),"expected_warning_error_records":3,"routed_error_records":1,"native_cereal_and_disk_verified":true,"collector_exit":collector_exit,"collector_original_adapter":original,"route":route,"drain_raw":marker});
    save(&output.join("report.json"), &result)?;
    Ok(result)
}
