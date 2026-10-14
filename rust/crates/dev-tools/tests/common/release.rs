#![allow(dead_code)]
use openpilot_dev_tools::{
    package::package,
    seal::{digest, BOOT_BINARIES},
};
use serde_json::{json, Value};
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
    process::Command,
};
pub fn elf(machine: u16, kind: u32) -> Vec<u8> {
    let count = if kind == 0 { 1 } else { 2 };
    let mut b = vec![0; 64 + count * 56 + 16];
    b[..7].copy_from_slice(b"\x7fELF\x02\x01\x01");
    b[16..18].copy_from_slice(&2u16.to_le_bytes());
    b[18..20].copy_from_slice(&machine.to_le_bytes());
    b[32..40].copy_from_slice(&64u64.to_le_bytes());
    b[52..54].copy_from_slice(&64u16.to_le_bytes());
    b[54..56].copy_from_slice(&56u16.to_le_bytes());
    b[56..58].copy_from_slice(&(count as u16).to_le_bytes());
    for i in 0..count {
        let p = 64 + i * 56;
        b[p..p + 4].copy_from_slice(&(if i == 0 { 1 } else { kind }).to_le_bytes());
        b[p + 8..p + 16].copy_from_slice(&((64 + count * 56) as u64).to_le_bytes());
        b[p + 32..p + 40].copy_from_slice(&16u64.to_le_bytes());
    }
    if kind == 2 {
        let n = b.len();
        b[n - 16..n - 8].copy_from_slice(&1u64.to_le_bytes());
    }
    b
}
pub fn git(root: &Path, args: &[&str]) -> String {
    let o = Command::new("git")
        .args(args)
        .current_dir(root)
        .output()
        .unwrap();
    assert!(o.status.success(), "{}", String::from_utf8_lossy(&o.stderr));
    String::from_utf8(o.stdout).unwrap().trim().into()
}
pub struct Fixture {
    pub _temp: tempfile::TempDir,
    pub root: PathBuf,
    pub installer: PathBuf,
    pub receipt: PathBuf,
    pub output: PathBuf,
}
impl Fixture {
    pub fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let root = temp.path().join("source");
        fs::create_dir(&root).unwrap();
        git(&root, &["init", "-q"]);
        git(&root, &["config", "user.name", "Owned fixture"]);
        git(&root, &["config", "user.email", "fixture@example.invalid"]);
        git(
            &root,
            &[
                "remote",
                "add",
                "origin",
                "https://github.com/bin9208/openpilot-rust.git",
            ],
        );
        fs::write(
            root.join(".gitignore"),
            "rust/bin/\nrust/runtime-manifest.json\n",
        )
        .unwrap();
        fs::write(root.join("source.txt"), "owned release input\n").unwrap();
        git(&root, &["add", "."]);
        git(&root, &["commit", "-qm", "owned release fixture"]);
        let dir = root.join("rust/bin");
        fs::create_dir_all(&dir).unwrap();
        for name in BOOT_BINARIES {
            let p = dir.join(name);
            fs::write(&p, elf(183, 0)).unwrap();
            fs::set_permissions(p, fs::Permissions::from_mode(0o755)).unwrap();
        }
        fs::create_dir(dir.join("usbgpu-assets")).unwrap();
        fs::write(
            dir.join("usbgpu-assets/kernel"),
            b"owned parser fixture data",
        )
        .unwrap();
        symlink("usbgpu-assets/kernel", dir.join("kernel-alias")).unwrap();
        let mut paths: Vec<_> = BOOT_BINARIES
            .iter()
            .map(|n| format!("rust/bin/{n}"))
            .collect();
        paths.extend([
            "rust/bin/usbgpu-assets/kernel".into(),
            "rust/bin/kernel-alias".into(),
        ]);
        paths.sort();
        let rows:Vec<_>=paths.into_iter().map(|n|json!({"path":n,"sha256":digest(&root.join(&n)).unwrap(),"executable":BOOT_BINARIES.contains(&Path::new(&n).file_name().unwrap().to_str().unwrap())})).collect();
        let commit = git(&root, &["rev-parse", "HEAD"]);
        fs::write(root.join("rust/runtime-manifest.json"),serde_json::to_vec(&json!({"schema":1,"source_commit":commit,"agnos_version":"19.8-carrot-bt1","files":rows})).unwrap()).unwrap();
        let installer = temp.path().join("installer");
        fs::write(&installer, elf(183, 0)).unwrap();
        fs::set_permissions(&installer, fs::Permissions::from_mode(0o755)).unwrap();
        let receipt = temp.path().join("receipt.json");
        fs::write(&receipt,serde_json::to_vec(&json!({"schema":1,"source_commit":commit,"architecture":"aarch64","target":"aarch64-unknown-linux-musl","installer_sha256":digest(&installer).unwrap(),"build_command":"owned ELF parser fixture; not a production executable"})).unwrap()).unwrap();
        let output = temp.path().join("release");
        Self {
            _temp: temp,
            root,
            installer,
            receipt,
            output,
        }
    }
    pub fn run(&self) -> openpilot_dev_tools::seal::Result<Value> {
        package(&self.root, &self.installer, &self.receipt, &self.output)
    }
    pub fn edit(&self, path: &Path, change: impl FnOnce(&mut Value)) {
        let mut v: Value = serde_json::from_slice(&fs::read(path).unwrap()).unwrap();
        change(&mut v);
        fs::write(path, serde_json::to_vec(&v).unwrap()).unwrap();
    }
    pub fn manifest(&self, change: impl FnOnce(&mut Value)) {
        self.edit(&self.root.join("rust/runtime-manifest.json"), change)
    }
    pub fn refresh_archive(&self, path: &Path) {
        self.edit(&self.output.join("runtime-release.json"), |v| {
            v["archive"]["sha256"] = digest(path).unwrap().into();
            v["archive"]["bytes"] = fs::metadata(path).unwrap().len().into();
        });
        fs::write(
            path.with_file_name(format!(
                "{}.sha256",
                path.file_name().unwrap().to_str().unwrap()
            )),
            format!(
                "{}  {}\n",
                digest(path).unwrap(),
                path.file_name().unwrap().to_str().unwrap()
            ),
        )
        .unwrap();
    }
}
