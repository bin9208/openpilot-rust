#[allow(dead_code)]
#[path = "../build.rs"]
mod build_script;

use std::{collections::BTreeSet, fs, path::PathBuf};

fn fixture() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().canonicalize().unwrap();
    let source = root.join("source");
    let output = root.join("target/build/out");
    fs::create_dir(&source).unwrap();
    fs::create_dir_all(&output).unwrap();
    (temp, source, output)
}

#[test]
fn existing_regular_inputs_remain_direct_and_deduplicated() {
    let (_temp, source, output) = fixture();
    let input = source.join("input");
    fs::write(&input, b"source").unwrap();
    assert_eq!(
        build_script::watch_paths(&[input.clone(), input.clone()], &output).unwrap(),
        BTreeSet::from([input])
    );
}

#[test]
fn missing_nested_input_watches_existing_ancestor_then_created_file() {
    let (_temp, source, output) = fixture();
    let input = source.join("missing/nested/input");
    assert_eq!(
        build_script::watch_paths(std::slice::from_ref(&input), &output).unwrap(),
        BTreeSet::from([source])
    );
    fs::create_dir_all(input.parent().unwrap()).unwrap();
    fs::write(&input, b"created").unwrap();
    assert_eq!(
        build_script::watch_paths(std::slice::from_ref(&input), &output).unwrap(),
        BTreeSet::from([input])
    );
}

#[test]
fn directory_fallback_rejects_observing_cargo_output() {
    let (temp, _source, output) = fixture();
    let missing = temp.path().canonicalize().unwrap().join("missing");
    let error = build_script::watch_paths(&[missing], &output).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    assert_eq!(
        error.to_string(),
        "build watch would recursively include Cargo OUT_DIR"
    );
}

#[cfg(unix)]
#[test]
fn dangling_link_watches_link_and_target_parents_then_target_creation() {
    use std::os::unix::fs::symlink;
    let (_temp, source, output) = fixture();
    let links = source.join("links");
    let targets = source.join("targets");
    fs::create_dir(&links).unwrap();
    fs::create_dir(&targets).unwrap();
    let link = links.join("input");
    let target = targets.join("missing/input");
    symlink(&target, &link).unwrap();
    assert_eq!(
        build_script::watch_paths(std::slice::from_ref(&link), &output).unwrap(),
        BTreeSet::from([links.clone(), targets])
    );
    fs::create_dir(target.parent().unwrap()).unwrap();
    fs::write(&target, b"created").unwrap();
    assert_eq!(
        build_script::watch_paths(&[link], &output).unwrap(),
        BTreeSet::from([links, target])
    );
}

#[cfg(unix)]
#[test]
fn link_retarget_watches_link_parent_and_new_existing_target() {
    use std::os::unix::fs::symlink;
    let (_temp, source, output) = fixture();
    let links = source.join("links");
    fs::create_dir(&links).unwrap();
    let first = source.join("first");
    let second = source.join("second");
    fs::write(&first, b"first").unwrap();
    fs::write(&second, b"second").unwrap();
    let link = links.join("input");
    symlink(&first, &link).unwrap();
    assert_eq!(
        build_script::watch_paths(std::slice::from_ref(&link), &output).unwrap(),
        BTreeSet::from([links.clone(), first])
    );
    fs::remove_file(&link).unwrap();
    symlink(&second, &link).unwrap();
    assert_eq!(
        build_script::watch_paths(&[link], &output).unwrap(),
        BTreeSet::from([links, second])
    );
}

#[cfg(unix)]
#[test]
fn directory_link_keeps_its_spelling_and_missing_target_ancestor() {
    use std::os::unix::fs::symlink;
    let (_temp, source, output) = fixture();
    let links = source.join("links");
    let targets = source.join("targets");
    fs::create_dir(&links).unwrap();
    fs::create_dir(&targets).unwrap();
    let link = links.join("dir");
    symlink(&targets, &link).unwrap();
    assert_eq!(
        build_script::watch_paths(std::slice::from_ref(&link), &output).unwrap(),
        BTreeSet::from([link.clone()])
    );
    fs::remove_file(&link).unwrap();
    symlink(targets.join("missing"), &link).unwrap();
    assert_eq!(
        build_script::watch_paths(&[link.join("child")], &output).unwrap(),
        BTreeSet::from([links, targets])
    );
}

#[cfg(unix)]
#[test]
fn existing_root_directory_links_preserve_spelling_with_in_tree_cargo_output() {
    use std::os::unix::fs::symlink;
    let (_temp, source, _output) = fixture();
    let output = source.join("rust/target/debug/build/logging/out");
    fs::create_dir_all(&output).unwrap();
    let mut inputs = Vec::new();
    for name in ["msgq", "opendbc", "rednose", "teleoprtc", "tinygrad"] {
        let target = source.join(format!("{name}_repo/{name}"));
        fs::create_dir_all(&target).unwrap();
        let link = source.join(name);
        symlink(format!("{name}_repo/{name}"), &link).unwrap();
        inputs.push(link);
    }
    let watches = build_script::watch_paths(&inputs, &output).unwrap();
    assert_eq!(watches, inputs.into_iter().collect::<BTreeSet<_>>());
    assert!(!watches.contains(&source));
    assert!(watches.iter().all(|path| path.is_symlink()));
}

#[cfg(unix)]
#[test]
fn symlink_cycle_is_bounded_and_remains_watched_for_replacement() {
    use std::os::unix::fs::symlink;
    let (_temp, source, output) = fixture();
    let first = source.join("first");
    let second = source.join("second");
    symlink("second", &first).unwrap();
    symlink("first", &second).unwrap();
    assert_eq!(
        build_script::watch_paths(&[first, second], &output).unwrap(),
        BTreeSet::from([source])
    );
}

#[cfg(unix)]
#[test]
fn excessive_symlink_chain_fails_instead_of_dropping_state() {
    use std::os::unix::fs::symlink;
    let (_temp, source, output) = fixture();
    for n in 0..41 {
        symlink(format!("link{}", n + 1), source.join(format!("link{n}"))).unwrap();
    }
    fs::write(source.join("link41"), b"target").unwrap();
    let error = build_script::watch_paths(&[source.join("link0")], &output).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    assert_eq!(
        error.to_string(),
        "build watch exceeds 40 symlink resolutions"
    );
}
