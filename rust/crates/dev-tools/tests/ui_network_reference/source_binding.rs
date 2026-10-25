//! #585 Network public source admission 회귀. 원본/graphics/CLI는 실행하지 않는다.
use openpilot_dev_tools::ui_network_reference::validate_source;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fs, ops::Range, path::Path};

const APPLICATION: &str = "openpilot/system/ui/lib/application.py";
const CURRENT: &[u8] = include_bytes!("../../../../../openpilot/system/ui/lib/application.py");
const FONT_BLOCKS: [(&[u8], usize); 3] = [
    (b"import datetime\n", 55),
    (b"SOURCE_FONT_DIR = ASSETS_DIR.joinpath(\"fonts\")\n", 3342),
    (b"    if \"OPENPILOT_FONT_ROOT\" in os.environ:\n", 114),
];

fn source_copy() -> tempfile::TempDir {
    let repository = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../..");
    let directory = tempfile::tempdir().unwrap();
    let original: Value =
        serde_json::from_str(include_str!("../fixtures/ui-network/provenance.json")).unwrap();
    assert_eq!(original["sources"].as_object().unwrap().len(), 57);
    assert_eq!(
        original["sources"][APPLICATION],
        "d17a3d0e8cb67c2bd37e4f48d61dc79e5ee7c7af984f0d9dc434301121a85b00"
    );
    let mut paths = original["sources"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    paths.insert("uv.lock".into());
    for relative in paths {
        let from = repository.join(&relative);
        if !from.exists()
            && matches!(
                relative.as_str(),
                "rust/tools/check_ui_network.py" | "rust/tools/ui_network_source.py"
            )
        {
            continue;
        }
        let to = directory.path().join(relative);
        fs::create_dir_all(to.parent().unwrap()).unwrap();
        fs::copy(from, to).unwrap();
    }
    assert_eq!(
        fs::read(directory.path().join(APPLICATION)).unwrap(),
        CURRENT
    );
    directory
}

fn font_ranges() -> [Range<usize>; 3] {
    assert_eq!(
        format!("{:x}", Sha256::digest(CURRENT)),
        "c659ce3574276546da11a7bd25e373321b62226411b1eaadda37401fe3b2110d",
        "회귀 입력 출처 pin이며 production의 current fullSHA 허용은 아니다"
    );
    FONT_BLOCKS.map(|(anchor, length)| {
        let mut positions = CURRENT
            .windows(anchor.len())
            .enumerate()
            .filter_map(|(index, bytes)| (bytes == anchor).then_some(index));
        let start = positions.next().unwrap();
        assert!(positions.next().is_none());
        start..start + length
    })
}

fn rejected_application(root: &Path, bytes: &[u8], label: &str) {
    fs::write(root.join(APPLICATION), bytes).unwrap();
    assert_eq!(
        validate_source(root).unwrap_err().to_string(),
        format!("{APPLICATION} changed; independently recapture network"),
        "{label}"
    );
    fs::write(root.join(APPLICATION), CURRENT).unwrap();
    validate_source(root).unwrap();
}

#[test]
fn current_font_source_passes_frozen_network_pin_boundary() {
    let source = source_copy();
    validate_source(source.path()).unwrap();
}

#[test]
fn font_blocks_reject_mutation_missing_duplicate_and_reordering() {
    let source = source_copy();
    validate_source(source.path()).unwrap();
    let ranges = font_ranges();
    for (index, range) in ranges.iter().enumerate() {
        let mut changed = CURRENT.to_vec();
        changed[range.end - 1] = b'X';
        rejected_application(source.path(), &changed, &format!("block {index} changed"));

        // 길이를 유지한 제거이므로 current 길이 검사만으로는 거부할 수 없다.
        let mut missing = CURRENT.to_vec();
        missing[range.clone()].fill(b' ');
        rejected_application(source.path(), &missing, &format!("block {index} missing"));

        let mut duplicate = CURRENT.to_vec();
        duplicate.splice(range.end..range.end, CURRENT[range.clone()].iter().copied());
        rejected_application(
            source.path(),
            &duplicate,
            &format!("block {index} duplicated"),
        );
    }
    let first = &ranges[0];
    let second = &ranges[1];
    let mut reordered = CURRENT[..first.start].to_vec();
    reordered.extend_from_slice(&CURRENT[second.clone()]);
    reordered.extend_from_slice(&CURRENT[first.end..second.start]);
    reordered.extend_from_slice(&CURRENT[first.clone()]);
    reordered.extend_from_slice(&CURRENT[second.end..]);
    assert_eq!(reordered.len(), CURRENT.len());
    rejected_application(source.path(), &reordered, "font blocks reordered");
}

#[test]
fn non_font_application_drift_is_rejected() {
    let source = source_copy();
    validate_source(source.path()).unwrap();
    let text = std::str::from_utf8(CURRENT).unwrap();
    for (before, after) in [
        ("FPS_DROP_THRESHOLD = 0.9", "FPS_DROP_THRESHOLD = 0.8"),
        (
            "if len(self._nav_stack) < 2:",
            "if len(self._nav_stack) < 3:",
        ),
        ("self.close_ffmpeg()", "self.close_ffmpeG()"),
    ] {
        assert!(
            text.contains(before),
            "mutation must address source: {before}"
        );
        let changed = text.replacen(before, after, 1);
        assert_eq!(changed.len(), CURRENT.len());
        rejected_application(source.path(), changed.as_bytes(), before);
    }
}

#[test]
fn projection_does_not_admit_other_paths_pins_fonts_or_dependencies() {
    let source = source_copy();
    let root = source.path();
    validate_source(root).unwrap();
    let pins: Value =
        serde_json::from_str(include_str!("../fixtures/ui-network/provenance.json")).unwrap();
    // 같은 application bytes라도 다른 path/expected pin의 full SHA 검사는 계속 거부한다.
    for name in [
        "openpilot/system/ui/widgets/inputbox.py",
        "openpilot/system/ui/widgets/network.py",
        "openpilot/selfdrive/assets/fonts/Inter-Medium.ttf",
    ] {
        assert_ne!(pins["sources"][name], pins["sources"][APPLICATION]);
        let path = root.join(name);
        let saved = fs::read(&path).unwrap();
        fs::write(&path, CURRENT).unwrap();
        assert_eq!(
            validate_source(root).unwrap_err().to_string(),
            format!("{name} changed; independently recapture network")
        );
        fs::write(path, saved).unwrap();
        validate_source(root).unwrap();
    }
    let fallback = root.join("openpilot/selfdrive/assets/fonts/Inter-Medium.fnt");
    fs::write(&fallback, b"new fallback").unwrap();
    assert_eq!(
        validate_source(root).unwrap_err().to_string(),
        "network font namespace changed; independently recapture"
    );
    fs::remove_file(fallback).unwrap();
    validate_source(root).unwrap();

    let lock = root.join("uv.lock");
    let saved = fs::read_to_string(&lock).unwrap();
    let changed = saved.replacen("\nname = \"numpy\"\n", "\nname = \"numpy-changed\"\n", 1);
    assert_ne!(changed, saved, "dependency mutation must address source");
    fs::write(&lock, changed).unwrap();
    assert_eq!(
        validate_source(root).unwrap_err().to_string(),
        "network dependency lock changed; independently recapture"
    );
    fs::write(lock, saved).unwrap();
    validate_source(root).unwrap();
}
