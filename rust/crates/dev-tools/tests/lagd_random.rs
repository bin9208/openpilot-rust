#![cfg(feature = "lagd-tools-native")]
//! Pinned NumPy primitives, independently captured before native generation.
use openpilot_dev_tools::lagd_random::Generator;
use openpilot_log_format::{JsonValue, JsonView};
use sha2::{Digest, Sha256};
use std::io::Read;

fn rows(value: &JsonValue) -> Vec<JsonValue> {
    let JsonView::Array(rows) = value.view() else {
        panic!("original array required")
    };
    rows
}
#[test]
fn original_seedsequence_pcg64_and_38912_distribution_words_are_bit_exact() {
    let packed = include_bytes!("fixtures/lagd-tools/random-original.json.gz");
    assert_eq!(
        format!("{:x}", Sha256::digest(packed)),
        "9be49c9a813b8468765be61ad2cd72fcd6601b0589326c9dbcd6c863d2b7157e"
    );
    let mut raw = String::new();
    flate2::read::GzDecoder::new(packed.as_slice())
        .read_to_string(&mut raw)
        .unwrap();
    assert_eq!(
        format!("{:x}", Sha256::digest(raw.as_bytes())),
        "857601180c092f314d31b60298565aa8469003b53d5566cdec927fb8b5180c1f"
    );
    let corpus = JsonValue::parse(&raw).unwrap();
    let cases = rows(&corpus.get("cases").unwrap());
    assert_eq!(cases.len(), 16);
    let mut total = 0;
    for row in cases {
        let seed = row
            .get("seed")
            .unwrap()
            .to_utf8()
            .unwrap()
            .parse::<u64>()
            .unwrap();
        let mut generator = Generator::new(seed);
        let state = |key: &str| {
            let value = row.get(key).unwrap().get("state").unwrap();
            (
                value.get("state").unwrap().to_json().unwrap(),
                value.get("inc").unwrap().to_json().unwrap(),
            )
        };
        let (initial, increment): (u128, u128) = generator.state();
        assert_eq!(
            (initial.to_string(), increment.to_string()),
            state("before")
        );
        let kind = row.get("kind").unwrap().to_utf8().unwrap();
        for (index, expected) in rows(&row.get("values").unwrap()).iter().enumerate() {
            let expected = expected.to_utf8().unwrap();
            let bits = match kind.as_str() {
                "raw" => generator.raw(),
                "random" => generator.random().to_bits(),
                "uniform" => generator.uniform(0.15, 1.).to_bits(),
                "normal" => generator.normal().to_bits(),
                _ => unreachable!(),
            };
            let actual = if kind == "raw" {
                format!("{bits:016x}")
            } else {
                bits.to_le_bytes()
                    .iter()
                    .map(|byte| format!("{byte:02x}"))
                    .collect::<String>()
            };
            assert_eq!(actual, expected, "seed={seed} kind={kind} sample={index}");
            total += 1;
        }
        let (final_state, increment): (u128, u128) = generator.state();
        assert_eq!(
            (final_state.to_string(), increment.to_string()),
            state("after")
        );
    }
    assert_eq!(total, 38912);
}

#[test]
fn pinned_upstream_algorithm_tables_and_license_provenance_are_retained() {
    let raw = include_bytes!("fixtures/lagd-tools/random-source-provenance.json");
    assert_eq!(
        format!("{:x}", Sha256::digest(raw)),
        "42863c1e8fb8c4c4829dd3541175472bc61d31d7efc46ee4c9ad81f3b0b72103"
    );
    let manifest: serde_json::Value = serde_json::from_slice(raw).unwrap();
    assert_eq!(manifest["numpy_version"], "2.5.3");
    assert_eq!(
        manifest["numpy_commit"],
        "dd88c0c19b54ad9ed3533224221285bf0873249a"
    );
    assert_eq!(
        format!(
            "{:x}",
            Sha256::digest(include_bytes!("../src/lagd_random/tables.rs"))
        ),
        manifest["tables_sha256"].as_str().unwrap()
    );
    for (name, raw) in [
        (
            "LICENSE-MIT",
            include_bytes!("../src/lagd_random/LICENSE-MIT").as_slice(),
        ),
        (
            "LICENSE-distributions",
            include_bytes!("../src/lagd_random/LICENSE-distributions").as_slice(),
        ),
    ] {
        assert_eq!(
            format!("{:x}", Sha256::digest(raw)),
            manifest["license_sha256"][name].as_str().unwrap()
        );
    }
}
