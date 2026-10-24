//! Compile-time native source identities and explicitly historical original inputs.
//! These are provenance pins, never canned generated assets or fixture answers.
use super::{hash, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, path::Path};

#[derive(Serialize)]
pub(super) struct Provenance {
    implementation: &'static str,
    source_root: &'static str,
    source_sha256: BTreeMap<&'static str, String>,
    executable_sha256: String,
    reference_python_sha256: BTreeMap<&'static str, &'static str>,
}
const NATIVE: &[(&str, &[u8])] = &[
    (
        "rust/crates/dev-tools/src/dbc_assets.rs",
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/dbc_assets.rs")),
    ),
    (
        "rust/crates/dev-tools/src/dbc_assets/provenance.rs",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/dbc_assets/provenance.rs"
        )),
    ),
    (
        "rust/crates/dev-tools/src/dbc_generator.rs",
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/dbc_generator.rs")),
    ),
    (
        "rust/crates/dev-tools/src/dbc_generator/chrysler.rs",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/dbc_generator/chrysler.rs"
        )),
    ),
    (
        "rust/crates/dev-tools/src/dbc_generator/cli.rs",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/dbc_generator/cli.rs"
        )),
    ),
    (
        "rust/crates/dev-tools/src/dbc_generator/mando.rs",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/dbc_generator/mando.rs"
        )),
    ),
    (
        "rust/crates/dev-tools/src/dbc_generator/mando_signals.rs",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/dbc_generator/mando_signals.rs"
        )),
    ),
    (
        "rust/crates/dev-tools/src/dbc_generator/pins.rs",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/dbc_generator/pins.rs"
        )),
    ),
    (
        "rust/crates/dev-tools/src/dbc_generator/procedural.rs",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/dbc_generator/procedural.rs"
        )),
    ),
    (
        "rust/crates/dev-tools/src/argparse_scan.rs",
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/argparse_scan.rs")),
    ),
    (
        "rust/crates/dev-tools/src/argparse_scan/python_decimal.rs",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/argparse_scan/python_decimal.rs"
        )),
    ),
    (
        "rust/crates/dev-tools/src/path_resolve.rs",
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/path_resolve.rs")),
    ),
    (
        "rust/crates/dev-tools/src/python_fs.rs",
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/src/python_fs.rs")),
    ),
    (
        "rust/crates/dev-tools/src/control_tools_policy_check/json.rs",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/control_tools_policy_check/json.rs"
        )),
    ),
    (
        "rust/crates/log-format/src/json/mod.rs",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../log-format/src/json/mod.rs"
        )),
    ),
    (
        "rust/crates/log-format/src/json/parse.rs",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../log-format/src/json/parse.rs"
        )),
    ),
    (
        "rust/crates/log-format/src/json/view.rs",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../log-format/src/json/view.rs"
        )),
    ),
    (
        "rust/crates/log-format/src/json/string.rs",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../log-format/src/json/string.rs"
        )),
    ),
    (
        "rust/crates/log-format/src/json/write.rs",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../log-format/src/json/write.rs"
        )),
    ),
    (
        "rust/crates/dev-tools/src/bin/prepare_card_assets.rs",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/bin/prepare_card_assets.rs"
        )),
    ),
    (
        "rust/crates/dev-tools/src/bin/prepare_radarcan_assets.rs",
        include_bytes!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/src/bin/prepare_radarcan_assets.rs"
        )),
    ),
    (
        "rust/crates/dev-tools/Cargo.toml",
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/Cargo.toml")),
    ),
    (
        "rust/Cargo.lock",
        include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.lock")),
    ),
];
const HISTORICAL: &[(&str, &str)] = &[
    (
        "__init__.py",
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    ),
    (
        "generator/chrysler/_stellantis_common_ram.py",
        "e01a57265224d0923bd405837d9387d1cf555aaf35131cfc42074a3e70b015cd",
    ),
    (
        "generator/generator.py",
        "cbba0de854469d42d096451d358153c3c4ffe86da59bcbf350a2c40364685aa5",
    ),
    (
        "generator/hyundai/hyundai_canfd_corner_radar_180.py",
        "56dec23778712c851df80f4bc0284baa2beff4d5a5f974563dbae77187491dec",
    ),
    (
        "generator/hyundai/hyundai_canfd_corner_radar_235.py",
        "82317feab3c1df6070e24fc760b8cd18f2b10ebf9aecd082b3c6aadf2d222677",
    ),
    (
        "generator/hyundai/hyundai_canfd_corner_radar_430.py",
        "be8f8490b2d35fe372e5e648324788f4c4a8f51ffd59e9c386f22157fd5fe91b",
    ),
    (
        "generator/hyundai/hyundai_canfd_radar.py",
        "c17310e9ec6562c7380db96cf5ade510f130a5c64f0c9775a2fc4d11ab1838ea",
    ),
    (
        "generator/hyundai/hyundai_kia_denso_front_radar.py",
        "b001f6c06c868cc0c73cb2d710249b62bc7ad3828d1debccad56869c9cd19ce1",
    ),
    (
        "generator/hyundai/hyundai_kia_mando_corner_radar.py",
        "d44df047ceb15bced5d172cc6a5daa070ed8bfee46d2c68094c61bfd7f4748b6",
    ),
    (
        "generator/hyundai/hyundai_kia_mando_front_radar.py",
        "22a4723be7c43e13da934db2012e666b3692123b32ff3c3e46a7d55d949f9ada",
    ),
    (
        "generator/rivian/rivian_mando_front_radar.py",
        "9d3222dd9649deee56c258ec9186ca088be08553d7c7cba33a97e88f631c85ec",
    ),
    (
        "generator/tesla/_radar_common.py",
        "abbd789fec23799e60555824decb76f8ff7643b2e8b4ead64ec1ba754efdf3f5",
    ),
    (
        "generator/tesla/tesla_radar_bosch.py",
        "5a2be60178e5205e5b145f404bf81a8bb21174cea4d05c8d72f385c6b6653930",
    ),
    (
        "generator/tesla/tesla_radar_continental.py",
        "1d33e5ff4518e481194fc601da289acbef3337ab343ad13b25295f3781d33c62",
    ),
];

pub(super) fn native() -> Result<Provenance> {
    let executable = std::env::current_exe()
        .map_err(|error| super::io_error(error, Path::new("current executable")))?;
    Ok(Provenance {
        implementation: "native",
        source_root: concat!(env!("CARGO_MANIFEST_DIR"), "/../../.."),
        source_sha256: NATIVE
            .iter()
            .map(|&(name, bytes)| (name, format!("{:x}", Sha256::digest(bytes))))
            .collect(),
        executable_sha256: hash(&executable)?,
        reference_python_sha256: HISTORICAL.iter().copied().collect(),
    })
}
