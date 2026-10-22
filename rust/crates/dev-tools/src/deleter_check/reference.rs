//! Replay source-authenticated requests against one persistent native process.
use super::{
    corpus::Corpus,
    owned::{Directory, Permission},
    process::Process,
    require, save, Result,
};
use serde_json::{json, Value};
use std::{
    ffi::OsString,
    fs::{self, File},
    io::Write,
    os::unix::{
        ffi::{OsStrExt, OsStringExt},
        fs::symlink,
    },
    path::{Component, Path},
    process::Command,
};
pub fn unhex(text: &str) -> Result<Vec<u8>> {
    require(
        text.len().is_multiple_of(2) && text.len() <= 4096,
        "deleter hex bound",
    )?;
    text.as_bytes()
        .chunks_exact(2)
        .map(|p| Ok(u8::from_str_radix(std::str::from_utf8(p)?, 16)?))
        .collect()
}
fn hex(path: &Path) -> String {
    path.as_os_str()
        .as_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}
fn name(value: &Value) -> Result<OsString> {
    let bytes = unhex(value.as_str().ok_or("deleter name is not hex")?)?;
    require(!bytes.contains(&0), "deleter fixture NUL name")?;
    let name = OsString::from_vec(bytes);
    let p = Path::new(&name);
    require(
        p.components().count() == 1 && p.components().all(|p| matches!(p, Component::Normal(_))),
        "deleter fixture escaped private root",
    )?;
    Ok(name)
}
pub fn snapshot(root: &Path) -> Result<Value> {
    fn walk(root: &Path, path: &Path, rows: &mut Vec<Value>) -> Result<()> {
        // os.walk's original default ignores unreadable-directory errors.
        let entries = match fs::read_dir(path) {
            Ok(entries) => entries,
            Err(_) => return Ok(()),
        };
        for entry in entries {
            let entry = entry?;
            let path = entry.path();
            let ty = fs::symlink_metadata(&path)?.file_type();
            rows.push(json!([hex(path.strip_prefix(root)?), ty.is_symlink()]));
            if ty.is_dir() && !ty.is_symlink() {
                walk(root, &path, rows)?;
            }
        }
        Ok(())
    }
    let mut rows = Vec::new();
    walk(root, root, &mut rows)?;
    rows.sort_by(|a, b| a[0].as_str().cmp(&b[0].as_str()));
    Ok(json!(rows))
}
pub fn run(corpus: &Corpus, trace: &Path, output: &Path) -> Result<Value> {
    let mut total = 0;
    let mut reports = Vec::new();
    for case in &corpus.reference {
        let work = Directory::temporary()?;
        let root = work.path.join("root");
        let outside = work.path.join("outside");
        fs::create_dir(&root)?;
        fs::create_dir(&outside)?;
        fs::write(outside.join("untouched"), b"keep")?;
        for directory in &case.directories_hex {
            let path = root.join(name(&json!(directory))?);
            fs::create_dir(&path)?;
            fs::write(path.join("rlog.zst"), b"synthetic")?;
        }
        let path = output.join(&case.name);
        fs::create_dir(&path)?;
        let mut process = Process::spawn(Command::new(trace).arg(&root), &path)?;
        let mut permissions = Vec::new();
        let mut input = File::create(path.join("input.jsonl"))?;
        let mut cursor = 0;
        let mut row_index = 0;
        let mut rows = Vec::new();
        for step in &case.steps {
            let op = step
                .first()
                .and_then(Value::as_str)
                .ok_or("deleter step absent")?;
            if !["cycle", "preserved"].contains(&op) {
                let path = root.join(name(step.get(1).ok_or("deleter mutation name absent")?)?);
                match op {
                    "attr" => {
                        let value = unhex(
                            step.get(2)
                                .and_then(Value::as_str)
                                .ok_or("preserve value absent")?,
                        )?;
                        rustix::fs::setxattr(
                            &path,
                            "user.preserve",
                            &value,
                            rustix::fs::XattrFlags::empty(),
                        )?;
                    }
                    "remove_attr" => rustix::fs::removexattr(&path, "user.preserve")?,
                    "lock" => {
                        File::create(path.join("held.lock"))?;
                    }
                    "symlink" => symlink(&outside, &path)?,
                    "deny" => permissions.push(Permission::deny(&path)?),
                    _ => return Err("unknown original deleter mutation".into()),
                }
                continue;
            }
            let command = if op == "cycle" {
                json!({"op":op,"bytes":step.get(1).and_then(Value::as_u64).ok_or("cycle byte count absent")?.to_string(),"percent":step.get(2).and_then(Value::as_str).ok_or("cycle percent absent")?})
            } else {
                json!({"op":op})
            };
            let expected = case
                .rows
                .get(row_index)
                .ok_or("unexpected deleter request")?;
            require(
                command == expected.input,
                "original deleter request order differs",
            )?;
            let mut wire = serde_json::to_vec(&command)?;
            wire.push(b'\n');
            input.write_all(&wire)?;
            input.flush()?;
            process.send(&wire)?;
            let response = process.line(&mut cursor)?;
            let actual: Value = serde_json::from_slice(&response)?;
            let current = snapshot(&root)?;
            rows.push(json!({"input":command,"original":expected.original,"expected":expected.expected,"actual":actual,"original_snapshot":expected.original_snapshot,"expected_snapshot":expected.expected_snapshot,"actual_snapshot":current,"intentional_correction":expected.intentional_correction}));
            save(&path.join("trace.json"), &json!(rows))?;
            require(
                actual == expected.expected,
                "native deleter output differs from immutable original/corrected observation",
            )?;
            require(
                current == expected.expected_snapshot,
                "native deleter filesystem state differs from original/corrected observation",
            )?;
            require(
                fs::read(outside.join("untouched"))? == b"keep",
                "deleter followed outside symlink",
            )?;
            row_index += 1;
        }
        require(
            row_index == case.rows.len(),
            "original deleter request omitted",
        )?;
        let code = process.finish()?;
        require(
            code == 0 && cursor == process.stdout.len(),
            "deleter trace exit or extra response differs",
        )?;
        drop(permissions);
        total += row_index;
        reports.push(json!({"name":case.name,"comparisons":row_index,"passed":true,"both_original_and_corrected_retained":true}));
    }
    require(
        total == 78 && reports.len() == 28,
        "native deleter complete observed roster differs",
    )?;
    Ok(
        json!({"passed":true,"reference_scenarios":reports.len(),"reference_comparisons":total,"original_and_corrected_rows_retained":true,"scenarios":reports}),
    )
}
