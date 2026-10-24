//! Extract unchanged camera reference methods through controlled native I/O boundaries.
use crate::{
    argparse_scan::{scan_os, OptionSpec},
    seal::Result,
};
use std::{fs, io::Write, path::Path};

#[derive(Clone, Copy)]
pub enum Kind {
    Requests,
    Packets,
}

fn read(path: &Path) -> Result<String> {
    Ok(fs::read_to_string(path)?
        .replace("\r\n", "\n")
        .replace('\r', "\n"))
}
fn position(source: &str, needle: &str) -> Result<usize> {
    source
        .find(needle)
        .ok_or_else(|| format!("camera source missing {needle:?}").into())
}
fn section<'a>(source: &'a str, first: &str, last: &str) -> Result<&'a str> {
    let start = position(source, first)?;
    let end = start + position(&source[start..], last)?;
    Ok(&source[start..end])
}
pub fn generate(root: &Path, kind: Kind) -> Result<String> {
    let source = read(&root.join("openpilot/system/camerad/cameras/spectra.cc"))?;
    let signatures: &[&str] = match kind {
        Kind::Requests => &[
            "bool SpectraCamera::handle_camera_event(",
            "bool SpectraCamera::validateEvent(",
            "void SpectraCamera::clearAndRequeue(",
            "bool SpectraCamera::waitForFrameReady(",
            "bool SpectraCamera::processFrame(",
            "bool SpectraCamera::syncFirstFrame(",
        ],
        Kind::Packets => &[
            "static cam_cmd_power *power_set_wait(",
            "void SpectraCamera::sensors_poke(",
            "void SpectraCamera::sensors_i2c(",
            "int SpectraCamera::sensors_init(",
            "void SpectraCamera::configCSIPHY(",
            "void add_patch(",
            "void SpectraCamera::config_ife(",
            "void SpectraCamera::config_bps(",
            "int get_bps_blob_index(",
        ],
    };
    let mut methods = Vec::new();
    for signature in signatures {
        let start = position(&source, signature)?;
        let mut end = start + position(&source[start..], "\n}")? + 2;
        if matches!(kind, Kind::Packets) && source.as_bytes().get(end) == Some(&b';') {
            end += 1;
        }
        methods.push(&source[start..end]);
    }
    let template = match kind {
        Kind::Requests => "rust/tools/camerad_requests_source.cc.in",
        Kind::Packets => "rust/tools/camerad_packets_source.cc.in",
    };
    let mut output = read(&root.join(template))?;
    if matches!(kind, Kind::Packets) {
        let ife = read(&root.join("openpilot/system/camerad/cameras/ife.h"))?;
        let ife = &ife[position(&ife, "int build_common_ife_bps(")?..];
        let linearization = section(
            &source,
            "  uint32_t bl = sensor->black_level",
            "\n\n  assert(sensor->gamma_lut_rgb",
        )?;
        let port = section(
            &source,
            "  struct cam_isp_in_port_info in_port_info =",
            "\n\n  struct cam_isp_resource",
        )?;
        let resource = section(
            &source,
            "  struct cam_icp_acquire_dev_info icp_info =",
            "\n  auto h = device_acquire",
        )?;
        output = output
            .replace("@IFE@", ife)
            .replace("@LINEARIZATION@", linearization)
            .replace("@ACQUIRE_PORT@", port)
            .replace("@ACQUIRE_BPS@", resource);
    }
    output = output.replace("@METHODS@", &methods.join("\n\n"));
    // CPython str.rstrip also accepts the four ASCII information separators.
    Ok(format!(
        "{}\n",
        output
            .trim_end_matches(|ch: char| ch.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&ch))
    ))
}

pub fn main_for(kind: Kind) {
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    let parsed = match scan_os(&args, &[OptionSpec::value("--source", true)]) {
        Ok(parsed) => parsed,
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(2);
        }
    };
    if parsed.help {
        println!("Usage: generate-camerad-reference --source PATH");
        return;
    }
    let result = generate(Path::new(&parsed.values["--source"]), kind)
        .and_then(|text| Ok(std::io::stdout().lock().write_all(text.as_bytes())?));
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
