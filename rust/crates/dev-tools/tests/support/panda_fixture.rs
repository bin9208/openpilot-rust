// Independently build an owned stage fixture; no private signing key is copied.
fn copy_public(source: &std::path::Path, target: &std::path::Path) {
    fs::create_dir_all(target).unwrap();
    for entry in fs::read_dir(source).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_str().unwrap();
        if ["obj", "tests", "__pycache__", ".git", "debug"].contains(&name)
            || matches!(
                path.extension().and_then(|x| x.to_str()),
                Some("o" | "dblite")
            )
        {
            continue;
        }
        if path.is_dir() {
            copy_public(&path, &target.join(name));
        } else {
            fs::copy(&path, target.join(name)).unwrap();
        }
    }
}
fn input_hashes(root: &std::path::Path) -> std::collections::BTreeMap<String, String> {
    use sha2::{Digest, Sha256};
    fn walk(
        root: &std::path::Path,
        directory: &std::path::Path,
        map: &mut std::collections::BTreeMap<String, String>,
    ) {
        for entry in fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            let name = path.file_name().unwrap().to_str().unwrap();
            if ["obj", "tests", "__pycache__", ".git"].contains(&name)
                || matches!(
                    path.extension().and_then(|x| x.to_str()),
                    Some("o" | "dblite")
                )
            {
                continue;
            }
            if path.is_dir() {
                walk(root, &path, map);
            } else {
                map.insert(
                    path.strip_prefix(root).unwrap().to_str().unwrap().into(),
                    format!("{:x}", Sha256::digest(fs::read(&path).unwrap())),
                );
            }
        }
    }
    let mut map = std::collections::BTreeMap::new();
    for directory in ["panda", "opendbc/safety"] {
        walk(root, &root.join(directory), &mut map);
    }
    map
}
pub fn stage_fixture(temporary: &std::path::Path) -> (PathBuf, PathBuf) {
    use sha2::{Digest, Sha256};
    let root = temporary.join("root");
    let output = temporary.join("build");
    for directory in ["panda", "opendbc/safety"] {
        copy_public(&repository().join(directory), &root.join(directory));
    }
    fs::write(
        root.join("panda/certs/debug"),
        b"synthetic non-secret source identity",
    )
    .unwrap();
    for directory in ["panda", "opendbc/safety"] {
        copy_public(
            &root.join(directory),
            &output.join("source").join(directory),
        );
    }
    let obj = output.join("source/panda/board/obj");
    fs::create_dir(&obj).unwrap();
    let mut artifacts = Vec::new();
    for name in [
        "panda.bin.signed",
        "panda_h7.bin.signed",
        "panda_tesla_wake.bin.signed",
        "panda_h7_tesla_wake.bin.signed",
        "bootstub.panda.bin",
        "bootstub.panda_h7.bin",
    ] {
        let bytes = if name.ends_with(".signed") {
            SIGNED.to_vec()
        } else {
            [0x2000_0000u32.to_le_bytes(), 0x0800_0001u32.to_le_bytes()].concat()
        };
        fs::write(obj.join(name), &bytes).unwrap();
        if name.ends_with(".signed") {
            fs::write(obj.join(name.trim_end_matches(".signed")), UNSIGNED).unwrap();
        }
        artifacts.push(serde_json::json!({"name":name,"size":bytes.len(),"sha256":format!("{:x}",Sha256::digest(&bytes))}));
    }
    let receipt = serde_json::json!({"source_inputs":input_hashes(&root),"snapshot_inputs":input_hashes(&output.join("source")),"source_unchanged":true,"artifacts":artifacts,"firmware_version":firmware_identity(&root).unwrap()});
    fs::write(
        output.join("receipt.json"),
        serde_json::to_vec_pretty(&receipt).unwrap(),
    )
    .unwrap();
    (root, output)
}
