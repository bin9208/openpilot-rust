//! The request path calls `encode_location`; these tests retain its prior byte rule.
use super::encode_location;
use stats_alloc::{StatsAlloc, INSTRUMENTED_SYSTEM};
use std::{
    alloc::System,
    ffi::OsStr,
    fs::{self, File},
    hint::black_box,
    io::Write,
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

#[global_allocator]
static GLOBAL: &StatsAlloc<System> = &INSTRUMENTED_SYSTEM;
const CHILD_ENV: &str = "OPENPILOT_CWEB_REDIRECT_ALLOCATION_CHILD";
const WARM_CALLS: usize = 1000;

// The original in-request expression, kept separately from the production helper.
fn previous_encoding(bytes: &[u8]) -> String {
    bytes
        .iter()
        .map(|&byte| {
            if byte.is_ascii_alphanumeric() || byte.is_ascii_punctuation() {
                char::from(byte).to_string()
            } else {
                format!("%{byte:02X}")
            }
        })
        .collect()
}

#[test]
fn redirect_location_encoding_matches_all_original_byte_cases() {
    assert_eq!(encode_location(b""), "");
    for byte in 0..=u8::MAX {
        assert_eq!(
            encode_location(&[byte]),
            previous_encoding(&[byte]),
            "byte {byte}"
        );
    }
    let all: Vec<u8> = (0..=u8::MAX).collect();
    assert_eq!(encode_location(&all), previous_encoding(&all));
    let mixed = b"/foo?x=hello%20world&y=!@#$^*()[]{};:,.~`|\\\x00\x7f\x80\xff";
    assert_eq!(encode_location(mixed), previous_encoding(mixed));
    assert_eq!(encode_location(b"%/?#&+"), "%/?#&+");
    assert_eq!(
        encode_location(b" \t\r\n\x00\x80\xff"),
        "%20%09%0D%0A%00%80%FF"
    );
    let long: Vec<u8> = all.iter().copied().cycle().take(16_384).collect();
    assert_eq!(encode_location(&long), previous_encoding(&long));
}

fn fingerprint(mut state: u64, bytes: &[u8]) -> u64 {
    for &byte in bytes {
        state = (state ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
    }
    state
}

#[test]
fn redirect_location_encoder_uses_one_owned_buffer_per_warmed_call() {
    if std::env::var_os(CHILD_ENV).as_deref() != Some(OsStr::new("1")) {
        let directory = std::env::temp_dir().join(format!(
            "cweb-redirect-allocation-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&directory).unwrap();
        let stdout = directory.join("stdout");
        let stderr = directory.join("stderr");
        let module = module_path!().split_once("::").unwrap().1;
        let name =
            format!("{module}::redirect_location_encoder_uses_one_owned_buffer_per_warmed_call");
        let mut child = Command::new(std::env::current_exe().unwrap())
            .arg("--exact")
            .arg(&name)
            .arg("--nocapture")
            .arg("--test-threads=1")
            .env(CHILD_ENV, "1")
            .stdout(Stdio::from(File::create(&stdout).unwrap()))
            .stderr(Stdio::from(File::create(&stderr).unwrap()))
            .spawn()
            .unwrap();
        let started = Instant::now();
        let status = loop {
            if let Some(status) = child.try_wait().unwrap() {
                break status;
            }
            if started.elapsed() >= Duration::from_secs(30) {
                let _ = child.kill();
                child.wait().unwrap();
                panic!("isolated redirect allocation test exceeded 30 seconds");
            }
            thread::sleep(Duration::from_millis(20));
        };
        let output = fs::read(&stdout).unwrap();
        std::io::stdout().write_all(&output).unwrap();
        std::io::stderr()
            .write_all(&fs::read(&stderr).unwrap())
            .unwrap();
        fs::remove_dir_all(directory).unwrap();
        assert!(
            std::str::from_utf8(&output)
                .unwrap()
                .contains("running 1 test"),
            "isolated redirect allocation filter did not select exactly one test"
        );
        assert!(
            status.success(),
            "isolated redirect allocation test failed: {status}"
        );
        return;
    }

    let input: Vec<u8> = (0..=u8::MAX).collect();
    let prior = previous_encoding(&input);
    let expected = (0..WARM_CALLS).fold(0xcbf2_9ce4_8422_2325, |hash, _| {
        fingerprint(hash, prior.as_bytes())
    });
    let before_cold = GLOBAL.stats();
    let cold = black_box(encode_location(black_box(&input)));
    let cold_stats = GLOBAL.stats() - before_cold;
    assert_eq!(cold, prior);
    let before_warm = GLOBAL.stats();
    let mut actual = 0xcbf2_9ce4_8422_2325;
    for _ in 0..WARM_CALLS {
        let encoded = black_box(encode_location(black_box(&input)));
        actual = fingerprint(actual, encoded.as_bytes());
    }
    let warm_stats = GLOBAL.stats() - before_warm;
    assert_eq!(actual, expected);
    println!(
        "{}",
        serde_json::json!({
            "scope": "production redirect Location encoder only; no HTTP or CPU claim",
            "input_bytes": input.len(),
            "encoded_bytes": prior.len(),
            "warm_calls": WARM_CALLS,
            "cold_allocations": cold_stats.allocations,
            "cold_reallocations": cold_stats.reallocations,
            "cold_bytes": cold_stats.bytes_allocated,
            "cold_bytes_reallocated": cold_stats.bytes_reallocated,
            "warm_allocations": warm_stats.allocations,
            "warm_reallocations": warm_stats.reallocations,
            "warm_bytes": warm_stats.bytes_allocated,
            "warm_bytes_reallocated": warm_stats.bytes_reallocated,
            "fingerprint": actual,
        })
    );
    // This deliberately fails against the original per-byte String expression.
    assert_eq!(warm_stats.allocations, WARM_CALLS);
    assert_eq!(warm_stats.reallocations, 0);
}
