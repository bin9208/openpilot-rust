//! #559 source admission 회귀. 원본/graphics/candidate 프로그램은 실행하지 않는다.
use openpilot_dev_tools::ui_application_lifecycle_reference::validate_source;
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
    let original: Value = serde_json::from_str(include_str!(
        "../fixtures/ui-application-lifecycle/provenance.json"
    ))
    .unwrap();
    let closure: Value = serde_json::from_str(include_str!(
        "../fixtures/ui-application-lifecycle/source-closure.json"
    ))
    .unwrap();
    let mut paths = original["sources"]
        .as_object()
        .unwrap()
        .keys()
        .chain(closure["sources"].as_object().unwrap().keys())
        .cloned()
        .collect::<BTreeSet<_>>();
    paths.insert("uv.lock".into());
    for relative in paths {
        let from = repository.join(&relative);
        if !from.exists()
            && matches!(
                relative.as_str(),
                "rust/tools/check_ui_application.py" | "rust/tools/ui_qa/application_source.py"
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
        format!("application original source changed: {APPLICATION}"),
        "{label}"
    );
    fs::write(root.join(APPLICATION), CURRENT).unwrap();
    validate_source(root).unwrap();
}

#[test]
fn current_font_source_passes_both_frozen_pin_boundaries() {
    // 어느 한 루프의 연결만 빠져도 두 번째 검증까지 성공할 수 없다.
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
fn projection_does_not_admit_other_paths_fonts_or_params_providers() {
    let source = source_copy();
    let root = source.path();
    validate_source(root).unwrap();
    // 동일한 유효 application bytes도 다른 경로/expected pin에는 적용할 수 없다.
    for (name, message) in [
        (
            "openpilot/system/ui/widgets/inputbox.py",
            "application original source changed",
        ),
        (
            "openpilot/selfdrive/assets/fonts/Inter-Medium.ttf",
            "application dependency changed",
        ),
    ] {
        let path = root.join(name);
        let saved = fs::read(&path).unwrap();
        fs::write(&path, CURRENT).unwrap();
        assert_eq!(
            validate_source(root).unwrap_err().to_string(),
            format!("{message}: {name}")
        );
        fs::write(path, saved).unwrap();
        validate_source(root).unwrap();
    }
    let fallback = root.join("openpilot/selfdrive/assets/fonts/Inter-Medium.fnt");
    fs::write(&fallback, b"new fallback").unwrap();
    assert_eq!(
        validate_source(root).unwrap_err().to_string(),
        "application source font fallback layout changed"
    );
    fs::remove_file(fallback).unwrap();
    validate_source(root).unwrap();

    let provider = root.join("openpilot/common/params_pyx.py");
    fs::write(&provider, b"new Params provider").unwrap();
    assert_eq!(
        validate_source(root).unwrap_err().to_string(),
        "application Params import provider changed"
    );
    fs::remove_file(provider).unwrap();
    validate_source(root).unwrap();
}
