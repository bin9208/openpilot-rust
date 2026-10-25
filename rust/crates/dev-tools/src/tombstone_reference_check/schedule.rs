//! Mechanical schedule from check_tombstoned_reference.py; no expected responses.
use super::{
    json::{array, object, scalar},
    Pair, Random,
};
use crate::seal::Result;
use openpilot_log_format::JsonValue as J;
use std::{
    ffi::OsString,
    fs,
    os::unix::{
        ffi::OsStringExt,
        fs::{symlink, PermissionsExt},
    },
    path::Path,
    time::Instant,
};
pub(super) const STAMP: &str = "2020-01-02--03-04-05";
pub(super) const TRACE: &str =
    "header\n#0 0xdead system (p=0x1) at sys.c\n#1 0xbeef model (a=1) at selfdrive/modeld.cc:7\n";
pub(super) const BODY:&str="ExecutablePath: /data/openpilot/selfdrive/modeld/modeld\nSignal: 11\nProcMaps:\n omitted\nProcStatus:\n kept\nCoreDump: omitted\n";
fn call(p: &mut Pair<'_>, op: &str, fields: &[(&str, J)], files: bool) -> Result<J> {
    let mut values = vec![("op", J::text(op))];
    values.extend_from_slice(fields);
    p.call(&object(&values)?, files)
}
fn write(p: &Pair<'_>, relative: &str, data: impl AsRef<[u8]>, mode: Option<u32>) -> Result<()> {
    p.write(Path::new(relative), data.as_ref(), mode)
}
fn remove(p: &Pair<'_>, relative: &str) -> Result<()> {
    for path in p.paths(Path::new(relative)) {
        match fs::remove_file(path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
fn mode(p: &Pair<'_>, relative: &str, mode: u32) -> Result<()> {
    for path in p.paths(Path::new(relative)) {
        fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    }
    Ok(())
}
fn report(p: &mut Pair<'_>, path: &str, stamp: &str) -> Result<()> {
    call(
        p,
        "report",
        &[
            ("path", J::text(path)),
            ("root", J::text("$FIXTURE/logs")),
            ("stamp", J::text(stamp)),
        ],
        true,
    )?;
    Ok(())
}
fn init(p: &mut Pair<'_>, project: &str) -> Result<()> {
    call(p, "init", &[("project", J::text(project))], false)?;
    Ok(())
}
fn gates(p: &mut Pair<'_>) -> Result<()> {
    let cases = [
        ("{}", false, Some("fixture")),
        (
            r#"{"openpilot":{"git_origin":"https://github.com/fork/openpilot"}}"#,
            false,
            Some("fixture"),
        ),
        (
            r#"{"openpilot":{"git_origin":"github.com/comgit@maai/openpilot"}}"#,
            false,
            Some("fixture"),
        ),
        (
            r#"{"openpilot":{"git_origin":"https://github.com/commaai/openpilot"}}"#,
            false,
            None,
        ),
        (
            r#"{"openpilot":{"git_origin":"https://github.com/commaai/openpilot"}}"#,
            false,
            Some("UnregisteredDevice"),
        ),
        (
            r#"{"openpilot":{"git_origin":"https://github.com/commaai/openpilot"}}"#,
            true,
            Some("fixture"),
        ),
        (
            r#"{"openpilot":{"git_origin":"https://github.com/commaai/openpilot"}}"#,
            false,
            Some("fixture"),
        ),
        (
            r#"{"channel":"devel-staging","openpilot":{"git_origin":"git@github.com:commaai/openpilot.git","git_commit":42}}"#,
            false,
            Some("fixture"),
        ),
        (
            r#"{"channel":["nightly"],"openpilot":{"git_origin":"github.com/commaai/openpilot","git_commit":[true,null]}}"#,
            false,
            Some("fixture"),
        ),
        (
            r#"{"openpilot":{"git_origin":null}}"#,
            false,
            Some("fixture"),
        ),
    ];
    for (build, pc, dongle) in cases {
        write(p, "base/build.json", build, None)?;
        remove(p, "params/fixture/DongleId")?;
        if let Some(dongle) = dongle {
            write(p, "params/fixture/DongleId", dongle, None)?;
        }
        call(
            p,
            "configure",
            &[
                ("base", J::text("$FIXTURE/base")),
                ("params", J::text("$FIXTURE/params")),
                ("pc", scalar(if pc { "true" } else { "false" })),
                ("device", J::text("tici")),
            ],
            false,
        )?;
        for project in ["selfdrive", "selfdrive_native"] {
            init(p, project)?;
        }
    }
    write(
        p,
        "base/build.json",
        r#"{"channel":"nightly","openpilot":{"git_origin":"github.com/commaai/openpilot","git_commit":"abcdefghijk"}}"#,
        None,
    )?;
    write(p, "params/fixture/DongleId", b"\xff", None)?;
    init(p, "selfdrive_native")?;
    write(p, "params/fixture/DongleId", "fixture", None)?;
    init(p, "selfdrive_native")?;
    let exception =
        scalar(r#"{"kind":"FixtureIo","message":"fixture failure","causes":[],"backtrace":null}"#);
    for sent in ["0", "1", "junk"] {
        write(p, "params/fixture/CarrotExceptionSent", sent, None)?;
        remove(p, "params/fixture/CarrotException")?;
        call(p, "capture", &[("exception", exception.clone())], true)?;
    }
    write(p, "params/fixture/CarrotExceptionSent", "0", None)?;
    remove(p, "params/fixture/CarrotException")?;
    mode(p, "params", 0o500)?;
    let result = call(p, "capture", &[("exception", exception.clone())], true);
    mode(p, "params", 0o700)?;
    result?;
    write(p, "params/fixture/CarrotExceptionSent", "1", Some(0o0))?;
    let result = call(p, "capture", &[("exception", exception.clone())], true);
    mode(p, "params/fixture/CarrotExceptionSent", 0o600)?;
    result?;
    for operation in ["capture_exception", "flush"] {
        call(p, "fail", &[("operation", J::text(operation))], false)?;
        call(
            p,
            "capture",
            &[
                ("exception", exception.clone()),
                ("log_exception", scalar("false")),
            ],
            true,
        )?;
    }
    call(p, "fail", &[("operation", scalar("null"))], false)?;
    let tombstone = [
        ("filename", J::text("fixture.crash")),
        ("message", J::text("message")),
        ("contents", J::text("contents")),
    ];
    call(p, "tombstone", &tombstone, false)?;
    call(p, "fail", &[("operation", J::text("set_extra"))], false)?;
    call(p, "tombstone", &tombstone, false)?;
    call(p, "fail", &[("operation", J::text("init"))], false)?;
    init(p, "selfdrive_native")?;
    call(p, "fail", &[("operation", scalar("null"))], false)?;
    remove(p, "base/openpilot/common/version.h")?;
    init(p, "selfdrive_native")?;
    write(
        p,
        "base/openpilot/common/version.h",
        "#define VERSION \"fixture-1.2\"\n",
        None,
    )?;
    Ok(())
}
fn files(p: &mut Pair<'_>) -> Result<()> {
    init(p, "selfdrive_native")?;
    for text in [
        "#single",
        "header\n",
        TRACE,
        "header\n#0 0xa fun (one(two)) tail\n",
        "first\nwithoutnumber\n",
        "a\n#0 0XFF fun(a)(b) at elsewhere\n",
    ] {
        write(p, "trace", text, None)?;
        write(p, "apport/test.crash", BODY, Some(0o640))?;
        report(p, "$FIXTURE/apport/test.crash", STAMP)?;
    }
    write(p, "trace", TRACE, None)?;
    for (index, signum) in [
        "11",
        "+11",
        "1_1",
        "١١",
        "34",
        "64",
        "35",
        "0",
        "-1",
        "9999999999999999999999",
        "broken",
    ]
    .iter()
    .enumerate()
    {
        write(
            p,
            "apport/signal.crash",
            format!("ExecutablePath: /data/openpilot/a{index}\nSignal: {signum}\nCoreDump: data\n"),
            Some(0o640),
        )?;
        report(p, "$FIXTURE/apport/signal.crash", STAMP)?;
    }
    let mut spaces = b"CoreDump:\n".to_vec();
    spaces.extend(vec![b' '; 8192]);
    spaces.push(255);
    for (index,contents) in [b"bad\xff\nCoreDump:\n".to_vec(),b"CoreDump:\n\xff".to_vec(),spaces,b"ExecutablePath: /data/openpilot/x\r\nSignal: 6\rProcMaps:\rhide\rProcStatus:\rshow\rCoreDump:\r".to_vec()].iter().enumerate(){
        write(p,&format!("apport/utf8-{index}.crash"),contents,Some(0o640))?;report(p,&format!("$FIXTURE/apport/utf8-{index}.crash"),STAMP)?;
    }
    for kind in ["exit", "invalid", "invalid-exit", "normal"] {
        write(p, "mode", kind, None)?;
        write(p, "apport/command.crash", BODY, Some(0o640))?;
        report(p, "$FIXTURE/apport/command.crash", STAMP)?;
    }
    write(p, "apport/permission.crash", BODY, Some(0o640))?;
    mode(p, "apport", 0o500)?;
    let result = report(p, "$FIXTURE/apport/permission.crash", STAMP);
    mode(p, "apport", 0o700)?;
    result?;
    write(p, "apport/copy-error.crash", BODY, Some(0o640))?;
    mode(p, "logs/crash", 0o500)?;
    let result = report(
        p,
        "$FIXTURE/apport/copy-error.crash",
        "2020-01-03--03-04-05",
    );
    mode(p, "logs/crash", 0o700)?;
    result?;
    for path in p.paths(Path::new(
        "logs/crash/2020-01-04--03-04-05_abcdefgh_selfdrive_modeld_modeld",
    )) {
        fs::create_dir(path)?;
    }
    write(p, "apport/nested.crash", BODY, Some(0o640))?;
    report(p, "$FIXTURE/apport/nested.crash", "2020-01-04--03-04-05")?;
    let same = "logs/crash/2020-01-05--03-04-05_abcdefgh_selfdrive_modeld_modeld";
    write(p, same, BODY, Some(0o640))?;
    report(p, &format!("$FIXTURE/{same}"), "2020-01-05--03-04-05")?;
    for size in [100_000_001, 100_000_000] {
        for path in p.paths(Path::new("apport/large.crash")) {
            let mut file = fs::File::create(&path)?;
            use std::io::Write;
            file.write_all(b"CoreDump:\n")?;
            file.set_len(size)?;
            fs::set_permissions(path, fs::Permissions::from_mode(0o640))?;
        }
        report(p, "$FIXTURE/apport/large.crash", STAMP)?;
    }
    Ok(())
}
fn scan(p: &mut Pair<'_>) -> Result<()> {
    let shared = p.root().join("shared");
    fs::create_dir(&shared)?;
    for (name, mode) in [
        ("tombstone-dir", None),
        ("tombstone-wrong-mode", Some(0o600)),
        ("valid.crash", Some(0o640)),
        ("wrong.crash", Some(0o600)),
        ("special.crash", Some(0o4640)),
        (".hidden.crash", Some(0o640)),
        ("ordinary", Some(0o640)),
    ] {
        let path = shared.join(name);
        if let Some(mode) = mode {
            fs::write(&path, b"fixture")?;
            fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
        } else {
            fs::create_dir(path)?;
        }
    }
    symlink("valid.crash", shared.join("linked.crash"))?;
    let request = [("path", crate::python_fs::json_value(shared.as_os_str()))];
    call(p, "scan", &request, false)?;
    symlink("absent", shared.join("tombstone-broken"))?;
    call(p, "scan", &request, false)?;
    fs::remove_file(shared.join("tombstone-broken"))?;
    for index in 0..1005 {
        fs::File::create(shared.join(format!("tombstone-{index:04}")))?;
    }
    call(p, "scan", &request, false)?;
    for (relative, contents) in [
        ("apport/ordinary", "x"),
        ("apport/.hidden", "hidden"),
        ("apport/keep/child", "child"),
    ] {
        write(p, relative, contents, None)?;
    }
    call(p, "clear", &[("path", J::text("$FIXTURE/apport"))], true)?;
    call(
        p,
        "scan",
        &[(
            "path",
            crate::python_fs::json_value(shared.join("absent").as_os_str()),
        )],
        false,
    )?;
    Ok(())
}
fn loops(p: &mut Pair<'_>, reporting: bool) -> Result<()> {
    if !reporting {
        write(p, "base/build.json", "{}", None)?;
    }
    write(p, "apport/old.crash", BODY, Some(0o640))?;
    write(p, "apport/.old.crash", BODY, Some(0o640))?;
    call(
        p,
        "start",
        &[
            ("apport", J::text("$FIXTURE/apport")),
            ("root", J::text("$FIXTURE/logs")),
            ("stamp", J::text(STAMP)),
        ],
        true,
    )?;
    write(p, "apport/new.crash", BODY, Some(0o640))?;
    write(p, "apport/tombstone-unknown", "unknown", Some(0o600))?;
    write(p, "apport/invalid.crash", b"bad\xff", Some(0o640))?;
    call(p, "cycle", &[], true)?;
    call(p, "cycle", &[], true)?;
    let mut name = b"apport/raw-\xff-".to_vec();
    name.extend_from_slice("한".as_bytes());
    name.extend_from_slice(b"-\xe2\x82.crash");
    p.write(
        Path::new(&OsString::from_vec(name)),
        BODY.replace("modeld", "raw").as_bytes(),
        Some(0o640),
    )?;
    p.write(
        Path::new(&OsString::from_vec(b"apport/tombstone-\x80-raw".to_vec())),
        b"unknown",
        Some(0o640),
    )?;
    p.write(
        Path::new(&OsString::from_vec(b"apport/invalid-\xfe.crash".to_vec())),
        b"bad\xff",
        Some(0o640),
    )?;
    call(p, "cycle", &[], true)?;
    call(p, "cycle", &[], true)?;
    write(
        p,
        "apport/newer.crash",
        BODY.replace("modeld", "newer"),
        Some(0o640),
    )?;
    call(p, "cycle", &[], true)?;
    Ok(())
}
fn text(p: &mut Pair<'_>) -> Result<()> {
    for text in ["a/b-한글_Ⅻ²١e\u{301}\u{345}\u{200b}", "", "\t.\n"] {
        call(p, "safe", &[("text", J::text(text))], false)?;
    }
    let mut rng = Random::new(76);
    for _ in 0..100 {
        let points = (0..20)
            .map(|_| {
                let point = rng.below(0x110000) as u32;
                if (0xd800..=0xdfff).contains(&point) {
                    point + 0x800
                } else {
                    point
                }
            })
            .collect();
        call(
            p,
            "safe",
            &[(
                "text",
                J::codepoints(points).ok_or("invalid random Unicode")?,
            )],
            false,
        )?;
    }
    for commit in [
        J::text("abcdefghijk"),
        J::text(""),
        scalar("null"),
        scalar("false"),
        scalar("0"),
        scalar("[]"),
        array(&[scalar("1"), scalar("true"), J::text("한글")])?,
        scalar("4"),
        scalar("true"),
        scalar("{}"),
    ] {
        for path in [
            "selfdrive/modeld".to_owned(),
            "한".repeat(100),
            "a-b/.x".to_owned(),
        ] {
            call(
                p,
                "filename",
                &[
                    ("stamp", J::text(STAMP)),
                    ("commit", J::text(&commit.to_json()?)),
                    ("path", J::text(&path)),
                ],
                false,
            )?;
        }
    }
    Ok(())
}
fn subprocess(p: &mut Pair<'_>) -> Result<()> {
    write(p, "apport/subprocess.crash", BODY, None)?;
    call(
        p,
        "retrace",
        &[
            ("path", J::text("$FIXTURE/apport/subprocess.crash")),
            ("shell", J::text("$FIXTURE/missing-bash")),
        ],
        false,
    )?;
    for (kind, ms) in [("timeout", 100), ("flood", 100), ("timeout", 30000)] {
        write(p, "mode", kind, None)?;
        let before = Instant::now();
        call(
            p,
            "retrace",
            &[
                ("path", J::text("$FIXTURE/apport/subprocess.crash")),
                ("timeout_ms", J::parse(&ms.to_string())?),
            ],
            false,
        )?;
        let elapsed = before.elapsed().as_secs_f64();
        if elapsed < f64::from(ms) / 1000.0 - 0.1 {
            return Err("retrace returned before original timeout lower bound".into());
        }
        p.stop_helpers()?;
        let timing = object(&[
            ("seconds", J::parse(&elapsed.to_string())?),
            ("configured_ms", J::parse(&ms.to_string())?),
        ])?;
        fs::write(
            p.output.join(format!("{kind}-{ms}-time.json")),
            timing.to_json()? + "\n",
        )?;
    }
    Ok(())
}
pub(super) fn run(name: &str, p: &mut Pair<'_>) -> Result<()> {
    match name {
        "gates" => gates(p),
        "files" => files(p),
        "scan" => scan(p),
        "reporting-loop" => loops(p, true),
        "disabled-loop" => loops(p, false),
        "text" => text(p),
        "subprocess" => subprocess(p),
        _ => Err("unknown tombstone schedule".into()),
    }
}
