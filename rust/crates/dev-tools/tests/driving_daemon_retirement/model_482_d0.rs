//! #482 D0: 고정 PR packet의 실제 Cereal decode와 전체 raw bit 분류만 수행한다.
use openpilot_cereal::log_capnp::event;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env,
    error::Error,
    fs::{self, File, OpenOptions},
    io::{Cursor, Write},
    os::unix::fs::{FileExt, MetadataExt, OpenOptionsExt},
    path::PathBuf,
};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const PACKET_SHA: &str = "5c9ad58bd983e76375378fae1e27dce6267a9529c25d5707d6413ad03704dfad";
const RAW_SHA: &str = "9afb2defff4ff29e5bf70e54db32d0d531d5e2b44507c8de652ee2c0dfcb67fe";
const RAW_BYTES: usize = 10_304;
const PR_HEAD: &str = "bacda68cf229eae5f5f757774e943afeb287c0c8";
const PR_SOURCE: &str = "e0e55f32161f3613bfd93bd23a18816abb159b04";
const FIXTURE_SOURCE: &str = "e25a14386582858dae25c6daca5ced3bfb4bacab";
const FIXTURE_SHA: &str = "afe1c795d7f609801eb4cd5a5a7748c3c996929403a3c0fa102f232f2a8d248d";

fn sha(raw: &[u8]) -> String {
    format!("{:x}", Sha256::digest(raw))
}
fn identity(m: &fs::Metadata) -> (u64, u64, u64, i64, i64, i64, i64) {
    (
        m.dev(),
        m.ino(),
        m.len(),
        m.mtime(),
        m.mtime_nsec(),
        m.ctime(),
        m.ctime_nsec(),
    )
}
struct Held {
    path: PathBuf,
    file: File,
    identity: (u64, u64, u64, i64, i64, i64, i64),
    bytes: Vec<u8>,
}
impl Held {
    fn open(path: PathBuf, length: usize, expected: &str) -> Result<Self> {
        let fd = rustix::fs::open(
            &path,
            rustix::fs::OFlags::RDONLY
                | rustix::fs::OFlags::NOFOLLOW
                | rustix::fs::OFlags::NONBLOCK
                | rustix::fs::OFlags::CLOEXEC,
            rustix::fs::Mode::empty(),
        )?;
        let file = File::from(fd);
        let m = file.metadata()?;
        if !m.is_file() || m.len() != length as u64 || path.canonicalize()? != path {
            return Err("D0 input kind/size/canonical path differs".into());
        }
        let mut bytes = vec![0; length];
        file.read_exact_at(&mut bytes, 0)?;
        if sha(&bytes) != expected {
            return Err("D0 immutable input SHA differs".into());
        }
        let held = Self {
            path,
            file,
            identity: identity(&m),
            bytes,
        };
        held.verify()?;
        Ok(held)
    }
    fn verify(&self) -> Result<()> {
        let named = fs::symlink_metadata(&self.path)?;
        let mut current = vec![0; self.bytes.len()];
        self.file.read_exact_at(&mut current, 0)?;
        if !named.is_file()
            || named.file_type().is_symlink()
            || identity(&named) != self.identity
            || identity(&self.file.metadata()?) != self.identity
            || current != self.bytes
        {
            return Err("D0 held input changed".into());
        }
        Ok(())
    }
}
fn decode(packet: &[u8]) -> Result<(u32, Vec<u8>)> {
    if packet.is_empty() || packet.len() > 32_768 {
        return Err("D0 packet length outside bound".into());
    }
    let mut cursor = Cursor::new(packet);
    let mut options = capnp::message::ReaderOptions::new();
    options.traversal_limit_in_words = Some(65_536);
    options.nesting_limit = 32;
    let message = capnp::serialize::read_message(&mut cursor, options)?;
    if cursor.position() != packet.len() as u64 {
        return Err("D0 packet contains trailing bytes".into());
    }
    let root = message.get_root::<event::Reader>()?;
    let event::Which::ModelV2(model) = root.which()? else {
        return Err("D0 packet union is not modelV2".into());
    };
    let model = model?;
    let raw = model.get_raw_predictions()?;
    if raw.len() != RAW_BYTES {
        return Err("D0 raw prediction must contain all 10304 bytes".into());
    }
    Ok((model.get_frame_id(), raw.to_vec()))
}
fn class(bits: u32) -> &'static str {
    match (bits & 0x7fff_ffff, bits & 0x7f80_0000, bits & 0x007f_ffff) {
        (0, _, _) => {
            if bits >> 31 == 0 {
                "positive_zero"
            } else {
                "negative_zero"
            }
        }
        (_, 0, _) => "subnormal",
        (_, 0x7f80_0000, 0) => "infinity",
        (_, 0x7f80_0000, _) => {
            if bits & 0x0040_0000 == 0 {
                "signaling_nan"
            } else {
                "quiet_nan"
            }
        }
        _ => "finite_normal",
    }
}
fn census(actual: &[u8], frozen: &[u8]) -> Result<Value> {
    if actual.len() != RAW_BYTES || frozen.len() != RAW_BYTES {
        return Err("D0 comparison requires two complete 10304-byte arrays".into());
    }
    let mut pairs = BTreeMap::<String, usize>::new();
    let mut differences = Vec::new();
    let mut equal = 0;
    for (index, (a, b)) in actual
        .chunks_exact(4)
        .zip(frozen.chunks_exact(4))
        .enumerate()
    {
        let a = u32::from_le_bytes(a.try_into()?);
        let b = u32::from_le_bytes(b.try_into()?);
        let (ac, bc) = (class(a), class(b));
        *pairs.entry(format!("{ac}/{bc}")).or_default() += 1;
        if a == b {
            equal += 1;
            continue;
        }
        let kind = if a & 0x7fff_ffff == 0 && b & 0x7fff_ffff == 0 {
            "signed_zero"
        } else if a & 0x7f80_0000 != 0x7f80_0000 && b & 0x7f80_0000 != 0x7f80_0000 {
            "finite_nonzero"
        } else {
            "nonfinite"
        };
        differences.push(json!({"index":index,"actual_bits":format!("{a:08x}"),
            "frozen_bits":format!("{b:08x}"),"actual_class":ac,"frozen_class":bc,"kind":kind}));
    }
    Ok(
        json!({"words":RAW_BYTES/4,"equal_words":equal,"different_words":differences.len(),"byte_equal":actual==frozen,
        "actual_raw_sha256":sha(actual),"frozen_raw_sha256":sha(frozen),
        "class_pairs":pairs,"differences":differences,"all_differences_included":true}),
    )
}

#[test]
#[ignore = "explicit Actions D0: fixed PR packet and independent raw fixture; no model execution"]
fn pinned_pr1344_model_v2_raw_prediction_full_bit_census() -> Result<()> {
    assert_eq!(env::var("GITHUB_ACTIONS")?, "true");
    assert_eq!(env::var("GITHUB_REPOSITORY")?, "bin9208/openpilot-rust");
    let current_source = env::var("EXPECTED_SOURCE")?;
    assert!(current_source.len() == 40 && current_source.bytes().all(|b| b.is_ascii_hexdigit()));
    let input = PathBuf::from(env::var_os("MODEL482_D0_INPUT").ok_or("D0 input missing")?);
    assert_eq!(input.canonicalize()?, input);
    let packet = Held::open(input.join("packet.bin"), 19_152, PACKET_SHA)?;
    let frozen = Held::open(input.join("frozen-raw.bin"), RAW_BYTES, RAW_SHA)?;
    let (frame, actual) = decode(&packet.bytes)?;

    // 실제 decoder의 거부 경계를 확인하며 frozen 기대값을 생성하거나 교체하지 않는다.
    assert!(decode(&[]).is_err());
    assert!(decode(&packet.bytes[..packet.bytes.len() - 1]).is_err());
    let mut trailing = packet.bytes.clone();
    trailing.extend_from_slice(&[0; 8]);
    assert_eq!(
        decode(&trailing).unwrap_err().to_string(),
        "D0 packet contains trailing bytes"
    );
    let mut wrong_union = capnp::message::Builder::new_default();
    wrong_union.init_root::<event::Builder>().init_car_state();
    let wrong_union = capnp::serialize::write_message_to_words(&wrong_union);
    assert_eq!(
        decode(&wrong_union).unwrap_err().to_string(),
        "D0 packet union is not modelV2"
    );
    let mut short_raw = capnp::message::Builder::new_default();
    short_raw
        .init_root::<event::Builder>()
        .init_model_v2()
        .set_raw_predictions(&[0; 4]);
    let short_raw = capnp::serialize::write_message_to_words(&short_raw);
    assert_eq!(
        decode(&short_raw).unwrap_err().to_string(),
        "D0 raw prediction must contain all 10304 bytes"
    );
    assert!(census(&actual[..RAW_BYTES - 4], &frozen.bytes).is_err());
    // 실제 IEEE754 경계의 분류를 확인하되 데이터 비교의 예상 차이 수는 고정하지 않는다.
    assert_eq!(class(0x8000_0000), "negative_zero");
    assert_eq!(class(0x7f80_0001), "signaling_nan");
    assert_eq!(class(0x7fc0_0001), "quiet_nan");
    let comparison = census(&actual, &frozen.bytes)?;
    assert_eq!(
        comparison["equal_words"].as_u64().unwrap()
            + comparison["different_words"].as_u64().unwrap(),
        2576
    );
    packet.verify()?;
    frozen.verify()?;
    let report = json!({"schema":1,"diagnostic":"model-482-pr1344-packet-d0",
        "current_decoder_source":current_source,"pr_head":PR_HEAD,"pr_merge_source":PR_SOURCE,
        "pr_run":40173790842_u64,"pr_job":121111148638_u64,"pr_artifact":12592585842_u64,
        "packet_sha256":PACKET_SHA,"packet_bytes":19152,"actual_model_v2_frame_id":frame,
        "fixture_source":FIXTURE_SOURCE,"fixture_manifest_sha256":FIXTURE_SHA,"fixture_frame_index":0,
        "comparison":comparison,"decoder_rejection_controls":6,
        "classification_uses_only_integer_bits":true,"diagnostic_complete":true,
        "original_executed":false,"model_executed":false,"native_daemon_executed":false,
        "historical_provider_proven":false,"first_producer_observed":false,
        "full128_acceptance":false,"historical_half_snan5_resolved":false,
        "model_or_historical_acceptance":false,"retirement_credit":0});
    let output = PathBuf::from(env::var_os("MODEL482_D0_REPORT").ok_or("D0 report missing")?);
    assert_eq!(
        output
            .parent()
            .ok_or("D0 report parent missing")?
            .canonicalize()?,
        output.parent().unwrap()
    );
    let mut raw_file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(output.with_file_name("decoded-raw.bin"))?;
    raw_file.write_all(&actual)?;
    raw_file.sync_all()?;
    let bytes = serde_json::to_vec_pretty(&report)?;
    assert!(bytes.len() <= 4 * 1024 * 1024);
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&output)?;
    file.write_all(&bytes)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    packet.verify()?;
    frozen.verify()?;
    println!(
        "MODEL482_D0_COMPLETE words=2576 different_words={}",
        report["comparison"]["different_words"]
    );
    Ok(())
}
