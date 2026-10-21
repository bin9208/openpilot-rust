//! Compare pre-frame native initialization to authenticated original statements.
use crate::{
    model_family_process::{
        car_params, held_tree, layout, publish_report, require, verify_tree, Cancellation,
        FailedCase, Held, Namespace, Process,
    },
    model_family_reference::Original,
    seal::Result,
};
use serde_json::{json, Value};
use std::{ffi::OsString, fs, path::PathBuf, time::Duration};
pub const RETIRED: [(&str, &str); 5] = [
    (
        "rust/tools/check_model_startup.py",
        "7e2204cf3bf929438ef1c0ea0f1d617661072dbb68d55afedacb4dabd7e8cb37",
    ),
    (
        "rust/tools/model_startup_reference.py",
        "19a31217596e3bad9ad682fbd550ab9bdf011189dc595d9f2be04ee4cb47c034",
    ),
    (
        "rust/tools/check_driver_daemon.py",
        "ea1266dc395dab662319bd3637ad90440989db3d2937756dbf07fbb993520f5b",
    ),
    (
        "rust/tools/model_logging_capture.py",
        "f00fa03c10597fce6c53931b45cf8867baded88a75948c664ffd075f3042535d",
    ),
    (
        "rust/tools/model_logging_reference.py",
        "ced29c79268f0df83bd9f741c8c840c6111521909592bfd7ad7b5602f0182910",
    ),
];
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Expect {
    Lazy,
    PreFrame,
}
impl Expect {
    pub fn parse(value: &str) -> Result<Self> {
        match value {
            "lazy" => Ok(Self::Lazy),
            "pre-frame" => Ok(Self::PreFrame),
            _ => Err("startup expectation must be lazy or pre-frame".into()),
        }
    }
}
pub struct Options {
    pub driving: PathBuf,
    pub driver: PathBuf,
    pub driving_peer: PathBuf,
    pub driver_peer: PathBuf,
    pub catalog: PathBuf,
    pub original: PathBuf,
    pub original_sha256: String,
    pub root: PathBuf,
    pub output: PathBuf,
    pub expect: Expect,
    pub expect_absent: bool,
}
fn reference_case(report: &Value, component: &str, width: u64, raw: bool) -> Result<Value> {
    let runs = report["runs"]
        .as_array()
        .ok_or("original startup report has no runs")?;
    require(
        runs.len() == 8 && report["device_validation"] == false,
        "original startup case roster changed",
    )?;
    let found = runs
        .iter()
        .filter(|r| r["component"] == component && r["resolution"][0] == width && r["raw"] == raw)
        .collect::<Vec<_>>();
    require(
        found.len() == 1,
        "original startup camera/raw case missing or duplicated",
    )?;
    let row = found[0];
    let (height, _, _, _, size) = layout(width)?;
    require(
        row["source"]["event"] == "initialized"
            && row["source"]["component"] == component
            && row["source"]["width"] == width
            && row["source"]["height"] == height
            && row["source"]["buffer_len"] == size
            && row["source"]["model_class"] == "ModelState"
            && row["source"]["frames_sent_before_initialization"] == 0
            && row["source_first_frame"]["event"] == "first_frame"
            && row["source_first_frame"]["frame_id"] == 1,
        "original initialization/first-frame evidence changed",
    )?;
    Ok(row.clone())
}
fn execute(options: &Options, mut emit: impl FnMut(&Value) -> Result<()>) -> Result<Value> {
    let cancel = Cancellation::new()?;
    require(
        options.output.is_absolute(),
        "startup output must be absolute",
    )?;
    let original = Original::open(
        &options.root,
        &options.original,
        &options.original_sha256,
        &RETIRED,
        options.expect_absent,
        &[
            "openpilot/selfdrive/modeld/modeld.py",
            "openpilot/selfdrive/modeld/dmonitoringmodeld.py",
            "openpilot/system/camerad/cameras/nv12_info.py",
        ],
    )?;
    let inputs = [
        &options.driving,
        &options.driver,
        &options.driving_peer,
        &options.driver_peer,
    ]
    .into_iter()
    .map(|p| Held::open(p, 512 * 1024 * 1024, true))
    .collect::<Result<Vec<_>>>()?;
    let catalog = held_tree(&options.catalog)?;
    original.catalog_sources(&options.catalog, &["driving", "driver"])?;
    crate::disk_space::inspect(&options.output, 1024 * 1024 * 1024)?.require()?;
    fs::create_dir(&options.output)?;
    let mut reports = Vec::new();
    for component in ["modeld", "dmonitoringmodeld"] {
        for width in [1344, 1928] {
            for raw in [false, true] {
                cancel.check()?;
                original.verify()?;
                let source = reference_case(&original.report, component, width, raw)?;
                let (height, stride, _, uv, size) = layout(width)?;
                let destination = options
                    .output
                    .join(format!("{component}-{width}-raw-{}", u8::from(raw)));
                fs::create_dir(&destination)?;
                let mut failure = FailedCase::new(
                    destination.join("report.json"),
                    json!({"component":component,"resolution":[width,height],"raw":raw,"passed":false,"source":source["source"],"source_first_frame":source["source_first_frame"]}),
                );
                let namespace = Namespace::new()?;
                let params_root = destination.join("params");
                let params = params_root.join(namespace.prefix());
                fs::create_dir_all(&params)?;
                for (name, value) in [
                    ("VEgoStopping", "5"),
                    ("CameraYawTrimDeg", "35"),
                    ("UseWideCamera", "1"),
                    ("SteerActuatorDelay", "0"),
                    ("LatSmoothSec", "0"),
                    ("LongActuatorDelay", "30"),
                ] {
                    fs::write(params.join(name), value)?;
                }
                let mut environment = namespace.environment(&params_root);
                if raw {
                    environment.insert("SEND_RAW_PRED".into(), "0".into());
                }
                let peer_path = if component == "modeld" {
                    &options.driving_peer
                } else {
                    &options.driver_peer
                };
                let mut peer_args = vec![
                    width.to_string().into(),
                    height.to_string().into(),
                    stride.to_string().into(),
                    uv.to_string().into(),
                    size.to_string().into(),
                ];
                if component == "modeld" {
                    peer_args.push("dual".into());
                }
                let mut peer = Process::spawn(
                    peer_path,
                    &peer_args,
                    &environment,
                    &[],
                    &destination.join("peer.log"),
                )?;
                peer.require_line(&["READY"], Duration::from_secs(60), &cancel)?;
                // The original source initialization sets this before the
                // native phase starts in the same namespace.
                if component == "modeld" {
                    fs::write(params.join("UsbGpuLoading"), b"0")?;
                }
                let binary = if component == "modeld" {
                    &options.driving
                } else {
                    &options.driver
                };
                let args = [
                    OsString::from("--trusted-catalog"),
                    options.catalog.as_os_str().to_owned(),
                ];
                let remove = if raw {
                    Vec::new()
                } else {
                    vec!["SEND_RAW_PRED"]
                };
                let mut daemon = Process::spawn(
                    binary,
                    &args,
                    &environment,
                    &remove,
                    &destination.join("native.log"),
                )?;
                daemon.require_line(
                    &[if component == "modeld" {
                        "connected extra cam"
                    } else {
                        "connected with buffer size"
                    }],
                    Duration::from_secs(60),
                    &cancel,
                )?;
                let mut loaded = daemon
                    .line(&["models loaded"], Duration::from_secs(10), &cancel)?
                    .is_some();
                if !loaded {
                    require(
                        daemon.alive()?,
                        "startup daemon exited before model load deadline",
                    )?;
                }
                let before_params = loaded;
                require(
                    !params.join("CarParams").exists(),
                    "CarParams existed before withheld-frame initialization check",
                )?;
                if component == "modeld" {
                    car_params(&params.join("CarParams"), false)?;
                    daemon.require_line(
                        &["modeld got CarParams"],
                        Duration::from_secs(60),
                        &cancel,
                    )?;
                }
                if !loaded {
                    loaded = daemon
                        .line(&["models loaded"], Duration::from_secs(1), &cancel)?
                        .is_some();
                    if !loaded {
                        require(
                            daemon.alive()?,
                            "startup daemon exited while frames were withheld",
                        )?;
                    }
                }
                let expected = options.expect == Expect::PreFrame;
                failure.observed(json!({"component":component,"resolution":[width,height],"raw":raw,"passed":false,"source":source["source"],"source_first_frame":source["source_first_frame"],"native_loaded_before_car_params":before_params,"native_loaded_without_frame":loaded,"frames_sent_to_native":0}));
                require(
                    before_params == expected && loaded == expected,
                    "native startup timing differs from requested original pre-frame/lazy contract",
                )?;
                let exit = daemon.stop(&cancel)?;
                require(exit.success(), "startup daemon did not stop successfully")?;
                require(
                    peer.command("stop", &cancel)? == "OK",
                    "startup peer stop failed",
                )?;
                require(
                    peer.wait(Duration::from_secs(5), &cancel)?.success(),
                    "startup peer did not exit successfully",
                )?;
                namespace.finish()?;
                let report = json!({"component":component,"resolution":[width,height],"raw":raw,"source":source["source"],"source_first_frame":source["source_first_frame"],"native_loaded_before_car_params":before_params,"native_loaded_without_frame":loaded,"frames_sent_to_native":0,"commands":{"native":daemon.receipt(),"peer":peer.receipt()},"native_exit":exit.code(),"expected":if expected{"pre-frame"}else{"lazy"}});
                publish_report(&destination.join("report.json"), &report, || {
                    cancel.check()?;
                    original.verify()
                })?;
                failure.accepted();
                emit(&report)?;
                reports.push(report);
            }
        }
    }
    let report = json!({"runs":reports,"device_validation":false,"original_manifest_sha256":options.original_sha256,"source_absence":options.expect_absent,"inputs":inputs.iter().map(Held::pin).collect::<Vec<_>>()});
    publish_report(&options.output.join("report.json"), &report, || {
        cancel.check()?;
        original.verify()?;
        for input in &inputs {
            input.verify()?;
        }
        verify_tree(&options.catalog, &catalog)?;
        original.catalog_sources(&options.catalog, &["driving", "driver"])
    })?;
    Ok(report)
}
pub fn run(options: &Options) -> Result<Value> {
    execute(options, |_| Ok(()))
}
pub fn cli() -> Result<()> {
    use crate::argparse_scan::{scan_os, OptionSpec};
    use std::io::Write;
    let parsed = scan_os(
        &std::env::args_os().skip(1).collect::<Vec<_>>(),
        &[
            OptionSpec::value("--driving", true),
            OptionSpec::value("--driver", true),
            OptionSpec::value("--catalog", true),
            OptionSpec::value("--output", true),
            OptionSpec::value("--native-driving-peer", true),
            OptionSpec::value("--native-driver-peer", true),
            OptionSpec::value("--native-original", true),
            OptionSpec::value("--native-original-sha256", true),
            OptionSpec::value("--native-root", true),
            OptionSpec::value("--expect", false),
            OptionSpec::value("--source-policy", false),
        ],
    )?;
    if parsed.help {
        std::io::stdout().write_all(b"Usage: check-model-startup --driving ELF --driver ELF --catalog DIR --output DIR --native-driving-peer ELF --native-driver-peer ELF --native-original DIR --native-original-sha256 SHA --native-root DIR [--expect pre-frame|lazy] [--source-policy present|retired-model-family]\n")?;
        return Ok(());
    }
    let value = |n: &str| -> Result<&OsString> {
        parsed
            .values
            .get(n)
            .ok_or_else(|| "startup CLI option missing".into())
    };
    let path = |n: &str| -> Result<PathBuf> {
        let p = PathBuf::from(value(n)?);
        Ok(if p.is_absolute() {
            p
        } else {
            std::env::current_dir()?.join(p)
        })
    };
    let policy = parsed
        .values
        .get("--source-policy")
        .map(|s| s.to_str().ok_or("startup source policy is not UTF-8"))
        .transpose()?
        .unwrap_or("present");
    require(
        matches!(policy, "present" | "retired-model-family"),
        "unknown startup source policy",
    )?;
    let options = Options {
        driving: path("--driving")?,
        driver: path("--driver")?,
        driving_peer: path("--native-driving-peer")?,
        driver_peer: path("--native-driver-peer")?,
        catalog: path("--catalog")?,
        original: path("--native-original")?,
        original_sha256: value("--native-original-sha256")?
            .to_str()
            .ok_or("startup original SHA is not UTF-8")?
            .to_owned(),
        root: path("--native-root")?,
        output: path("--output")?,
        expect: Expect::parse(
            parsed
                .values
                .get("--expect")
                .map(|s| s.to_str().ok_or("startup expectation is not UTF-8"))
                .transpose()?
                .unwrap_or("pre-frame"),
        )?,
        expect_absent: policy == "retired-model-family",
    };
    // As in the original, emit eight case rows. A broken stream aborts before
    // the top-level success receipt; late failures cannot retract earlier rows.
    let mut stdout = std::io::stdout().lock();
    execute(&options, |row| {
        serde_json::to_writer(&mut stdout, row)?;
        stdout.write_all(b"\n")?;
        stdout.flush()?;
        Ok(())
    })?;
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_case_admission_rejects_missing_duplicate_or_early_frame_evidence() {
        let mut rows = Vec::new();
        for component in ["modeld", "dmonitoringmodeld"] {
            for width in [1344, 1928] {
                for raw in [false, true] {
                    let (h, _, _, _, size) = layout(width).unwrap();
                    rows.push(json!({"component":component,"resolution":[width,h],"raw":raw,"source":{"event":"initialized","component":component,"width":width,"height":h,"buffer_len":size,"model_class":"ModelState","frames_sent_before_initialization":0},"source_first_frame":{"event":"first_frame","frame_id":1}}));
                }
            }
        }
        let mut report = json!({"runs":rows,"device_validation":false});
        reference_case(&report, "modeld", 1344, false).unwrap();
        report["runs"][0]["source"]["frames_sent_before_initialization"] = json!(1);
        assert!(reference_case(&report, "modeld", 1344, false).is_err());
        report["runs"][0] = report["runs"][1].clone();
        assert!(reference_case(&report, "modeld", 1344, false).is_err());
        assert!(Expect::parse("auto").is_err());
    }
}
