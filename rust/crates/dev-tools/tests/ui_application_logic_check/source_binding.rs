//! #557 source admission 회귀. 원문 Python/실제 candidate는 실행하지 않는다.
use super::*;
use std::ops::Range;

const CURRENT: &[u8] = include_bytes!("../../../../../openpilot/system/ui/lib/application.py");
const SOURCE_ERROR: &str = "GuiApplication source changed; independently recapture original corpus";
const FONT_BLOCKS: [(&[u8], usize, &[u8]); 3] = [
    (b"import datetime\n", 55, b"import datetime\n"),
    (
        b"SOURCE_FONT_DIR = ASSETS_DIR.joinpath(\"fonts\")\n",
        3342,
        b"FONT_DIR = ASSETS_DIR.joinpath(\"fonts\")\n",
    ),
    (b"    if \"OPENPILOT_FONT_ROOT\" in os.environ:\n", 114, b""),
];

fn current_source() -> &'static [u8] {
    // 시험 입력의 출처 pin이다. production admission은 이 full SHA를 사용하지 않는다.
    assert_eq!(
        format!("{:x}", Sha256::digest(CURRENT)),
        "c659ce3574276546da11a7bd25e373321b62226411b1eaadda37401fe3b2110d"
    );
    CURRENT
}

fn ranges() -> [Range<usize>; 3] {
    FONT_BLOCKS.map(|(anchor, length, _)| {
        let mut positions = current_source()
            .windows(anchor.len())
            .enumerate()
            .filter_map(|(index, value)| (value == anchor).then_some(index));
        let start = positions.next().unwrap();
        assert!(positions.next().is_none());
        start..start + length
    })
}

fn root_with_source(bytes: &[u8]) -> tempfile::TempDir {
    let directory = tempfile::tempdir().unwrap();
    let path = directory
        .path()
        .join("openpilot/system/ui/lib/application.py");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, bytes).unwrap();
    directory
}

struct RefusedRunner {
    calls: usize,
}
impl Runner for RefusedRunner {
    fn run(&mut self, _: &CommandSpec) -> Result<Captured> {
        self.calls += 1;
        Err("source admission incorrectly reached candidate".into())
    }
}

fn rejected(bytes: &[u8], label: &str) {
    for kind in [Kind::Fps, Kind::Stack] {
        let directory = root_with_source(bytes);
        let output = directory.path().join("output");
        let mut runner = RefusedRunner { calls: 0 };
        let error = check(
            directory.path(),
            Path::new("controlled-native"),
            &output,
            kind,
            &mut runner,
        )
        .unwrap_err();
        assert_eq!(error.to_string(), SOURCE_ERROR, "{label}/{kind:?}");
        assert_eq!(runner.calls, 0, "{label}/{kind:?}");
        assert!(output.is_dir(), "{label}/{kind:?}");
        assert_eq!(fs::read_dir(output).unwrap().count(), 0, "{label}/{kind:?}");
    }
}

#[test]
fn current_font_source_reaches_frozen_fps_and_stack_runners() {
    let fixtures =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ui-application-logic");
    let rows: Value = serde_json::from_str(include_str!(
        "../fixtures/ui-application-logic/failures.json"
    ))
    .unwrap();
    for (kind, name) in [(Kind::Fps, "fps"), (Kind::Stack, "stack")] {
        let directory = root_with_source(current_source());
        let output = directory.path().join("output");
        let row = rows
            .as_array()
            .unwrap()
            .iter()
            .find(|row| row["kind"] == name && row["case"] == "success")
            .unwrap();
        let mut runner = Peer {
            fixtures: &fixtures,
            row,
            calls: 0,
        };
        let printed = check(
            directory.path(),
            Path::new("controlled-native"),
            &output,
            kind,
            &mut runner,
        )
        .unwrap();
        assert_eq!(runner.calls, 1);
        assert_eq!(printed, row["stdout"].as_str().unwrap());
        let mut names = fs::read_dir(output)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect::<Vec<_>>();
        names.sort();
        let mut expected = row["files"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        expected.sort();
        assert_eq!(names, expected);
    }
}

#[test]
fn font_block_changes_missing_or_duplicates_fail_before_runner() {
    for (index, range) in ranges().into_iter().enumerate() {
        let mut changed = current_source().to_vec();
        changed[range.end - 1] = b'X';
        rejected(&changed, &format!("font block {index} byte mutation"));

        let mut missing = current_source().to_vec();
        missing.splice(range.clone(), FONT_BLOCKS[index].2.iter().copied());
        rejected(&missing, &format!("font block {index} partial legacy form"));

        let mut duplicate = current_source().to_vec();
        duplicate.splice(
            range.end..range.end,
            current_source()[range].iter().copied(),
        );
        rejected(&duplicate, &format!("font block {index} duplicate"));
    }
}

#[test]
fn font_block_reordering_fails_before_runner() {
    let [first, second, _] = ranges();
    let source = current_source();
    let mut changed = source[..first.start].to_vec();
    changed.extend_from_slice(&source[second.clone()]);
    changed.extend_from_slice(&source[first.end..second.start]);
    changed.extend_from_slice(&source[first.clone()]);
    changed.extend_from_slice(&source[second.end..]);
    assert_eq!(changed.len(), source.len());
    rejected(&changed, "known blocks in the wrong order");
}

#[test]
fn fps_and_stack_changes_fail_before_runner() {
    for (before, after) in [
        ("FPS_DROP_THRESHOLD = 0.9", "FPS_DROP_THRESHOLD = 0.8"),
        ("FPS dropped below", "FPS dropped Below"),
        (
            "if len(self._nav_stack) < 2:",
            "if len(self._nav_stack) < 3:",
        ),
        (
            "self._nav_stack.pop(idx_to_pop)",
            "self._nav_stack.pop(idx_to_Pop)",
        ),
    ] {
        let source = std::str::from_utf8(current_source()).unwrap();
        assert_eq!(source.matches(before).count(), 1);
        assert_eq!(before.len(), after.len());
        let changed = source.replacen(before, after, 1);
        rejected(changed.as_bytes(), before);
    }
}
