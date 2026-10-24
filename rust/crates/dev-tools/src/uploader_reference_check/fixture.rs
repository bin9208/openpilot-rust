//! Frozen observations include filesystem-dependent ties. Recreate those input
//! relations without sorting away cross-priority input or changing expectations.
use super::{make_files, relative, require, Case, Result};
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, path::Path};

fn class(key: &str) -> Result<(String, u16)> {
    let path = relative(key)?;
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or("invalid fixture filename")?;
    let priority = match name {
        "qlog" | "qlog.zst" => 0,
        "qcamera.ts" => 1,
        _ => 1000,
    };
    Ok((
        path.parent()
            .ok_or("fixture directory absent")?
            .to_string_lossy()
            .into_owned(),
        priority,
    ))
}
fn entries(path: &Path) -> Result<Vec<String>> {
    fs::read_dir(path)?
        .map(|entry| {
            Ok(entry?
                .file_name()
                .into_string()
                .map_err(|_| "nonUTF8 controlled fixture entry")?)
        })
        .collect()
}
pub(super) fn make(case: &Case) -> Result<(tempfile::TempDir, Value, bool)> {
    let temp = tempfile::Builder::new()
        .prefix("uploader-fixture-")
        .tempdir_in("/dev/shm")?;
    // Linux TMPFS_MAGIC. An htree directory's per-filesystem hash order cannot
    // replay another runner's captured order by changing insertion order.
    require(
        rustix::fs::statfs(temp.path())?.f_type == 0x0102_1994,
        "uploader fixture requires Linux tmpfs",
    )?;
    let probe = temp.path().join("order-probe");
    fs::create_dir(&probe)?;
    let names = ["middle", "last", "first"];
    for name in names {
        fs::write(probe.join(name), [])?;
    }
    let observed = entries(&probe)?;
    let forward = observed.iter().map(String::as_str).eq(names);
    let reverse = observed
        .iter()
        .map(String::as_str)
        .eq(names.into_iter().rev());
    require(
        forward || reverse,
        "tmpfs does not expose a controlled insertion order",
    )?;
    fs::remove_dir_all(probe)?;

    let mut ties: BTreeMap<(String, u16), Vec<String>> = BTreeMap::new();
    for row in case.expected["files"]
        .as_array()
        .ok_or("captured listing absent")?
    {
        let row = row
            .as_array()
            .filter(|r| r.len() == 2)
            .ok_or("captured listing row invalid")?;
        let key = row[1].as_str().ok_or("captured listing key invalid")?;
        require(
            relative(key)?.file_name().and_then(|s| s.to_str()) == row[0].as_str(),
            "captured listing basename differs",
        )?;
        require(
            case.files
                .iter()
                .any(|(name, _, uploaded)| name == key && !uploaded),
            "captured candidate not in active original input",
        )?;
        let names = ties.entry(class(key)?).or_default();
        require(
            !names.iter().any(|name| name == key),
            "duplicated captured listing key",
        )?;
        names.push(key.into());
    }
    let mut files = case.files.clone();
    // Only permute the active entries within the SAME original priority class.
    // Every other class, uploaded entry and excluded file keeps its input slot.
    // A controller that ignored source priority still fails the exact outputs.
    for (group, ordered) in &ties {
        let slots = case
            .files
            .iter()
            .enumerate()
            .filter_map(|(i, (name, _, _))| ordered.contains(name).then_some(i))
            .collect::<Vec<_>>();
        require(
            slots.len() == ordered.len(),
            "captured tie constraint has missing inputs",
        )?;
        for (slot, name) in slots.into_iter().zip(ordered) {
            require(
                class(&case.files[slot].0)? == *group,
                "fixture ordering crossed source priorities",
            )?;
            files[slot] = case
                .files
                .iter()
                .find(|(key, _, _)| key == name)
                .ok_or("tie input absent")?
                .clone();
        }
    }
    if reverse {
        files.reverse();
    }
    make_files(temp.path(), case, &files)?;
    let mut actual = BTreeMap::new();
    for (name, _, _) in &case.files {
        let directory = relative(name)?.parent().ok_or("fixture parent absent")?;
        if !actual.contains_key(directory) {
            actual.insert(
                directory.to_path_buf(),
                entries(&temp.path().join(directory))?,
            );
        }
    }
    let mut unsorted = false;
    for (directory, names) in &actual {
        for (group, expected) in &ties {
            if directory == Path::new(&group.0) {
                let selected = names
                    .iter()
                    .map(|name| directory.join(name).to_string_lossy().into_owned())
                    .filter(|key| expected.contains(key))
                    .collect::<Vec<_>>();
                require(
                    &selected == expected,
                    "tmpfs fixture failed captured tie-order input contract",
                )?;
            }
        }
        let active = names
            .iter()
            .map(|name| directory.join(name).to_string_lossy().into_owned())
            .filter(|key| ties.values().any(|v| v.contains(key)))
            .map(|key| class(&key).map(|(_, priority)| priority))
            .collect::<Result<Vec<_>>>()?;
        unsorted |= active.windows(2).any(|pair| pair[0] > pair[1]);
    }
    Ok((
        temp,
        json!({"provider":"Linux tmpfs", "insertion_forward":forward, "probe_order":observed, "actual_directory_entries":actual, "source_tie_constraints":ties.iter().map(|((directory, priority), names)|json!({"directory":directory,"priority":priority,"captured_order":names})).collect::<Vec<_>>(), "cross_priority_input_not_sorted":unsorted}),
        unsorted,
    ))
}
