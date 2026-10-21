//! Source-authored executable origins for the frozen recovery family.
//!
//! This is input to the strict audit, not an observed-trace allowlist. The
//! hosted gate admits a bounded executable/result roster and retains raw
//! argv. It claims exact argv only for the harness and launch-descriptor shape.

use crate::recovery_exec_audit::{
    audit, capture_program_seal, parse_trace_dir, write_receipt_exclusive, AliasRelation,
    CreatedProgramKind, CreatedProgramRule, ExpectedProgram, ExpectedTypedAttempt, ExplicitPolicy,
    Outcome, OwnedAliasReceiptRule, ProgramSeal, ScriptInterpreter, TypedArg,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeSet;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};

pub const HOST_PROVIDERS: &[&str] = &[
    "/usr/bin/git",
    "/usr/lib/git-core/git",
    "/usr/lib/git-core/git-upload-pack",
    "/bin/sh",
    "/bin/bash",
    "/usr/bin/bash",
    "/usr/bin/stty",
    "/usr/bin/tmux",
    "/usr/bin/od",
    "/usr/bin/sleep",
    "/bin/sleep",
    "/usr/lib/x86_64-linux-gnu/utempter/utempter",
    "/usr/bin/clear",
    "/bin/cat",
    "/usr/bin/script",
    "/usr/bin/readelf",
    "/usr/bin/head",
    "/usr/bin/tr",
    "/usr/bin/touch",
];

pub const ACTION_SCENARIOS: &[&str] = &[
    "git_branches",
    "git_log",
    "git_reset",
    "git_sync",
    "git_pull",
    "git_rebuild",
    "rebuild_all",
    "git_pull_repair",
    "git_pull_preserve",
    "git_pull_multiple",
    "git_pull_missing",
    "git_checkout",
    "git_checkout_local",
    "git_checkout_commit",
    "git_reset_repo",
    "quoted_branch",
    "stale_index",
    "fresh_index",
    "symlink_index",
    "busy_lock",
    "unknown",
    "missing_branch",
    "false_branch",
    "number_commit",
];

#[derive(Clone)]
pub struct AuthoredScript {
    pub path: PathBuf,
    pub bytes: Vec<u8>,
    pub mode: u32,
}

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

const MAX_PREFLIGHT_BYTES: u64 = 64 * 1024;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SelectedElfs {
    pub server: PathBuf,
    pub actions: PathBuf,
    pub upload: PathBuf,
    pub child: PathBuf,
    pub cwp: PathBuf,
    pub tmux: PathBuf,
    pub harness: PathBuf,
}

impl SelectedElfs {
    fn six(&self) -> [PathBuf; 6] {
        [
            self.server.clone(),
            self.actions.clone(),
            self.upload.clone(),
            self.child.clone(),
            self.cwp.clone(),
            self.tmux.clone(),
        ]
    }

    fn seven(&self) -> [PathBuf; 7] {
        let [server, actions, upload, child, cwp, tmux] = self.six();
        [
            server,
            actions,
            upload,
            child,
            cwp,
            tmux,
            self.harness.clone(),
        ]
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Preflight {
    pub native_root: PathBuf,
    pub selected: SelectedElfs,
    pub host_path: String,
    pub programs: Vec<ProgramSeal>,
}

fn regular_bytes(path: &Path, maximum: u64) -> Result<Vec<u8>> {
    let held = File::from(rustix::fs::open(
        path,
        rustix::fs::OFlags::RDONLY
            | rustix::fs::OFlags::NONBLOCK
            | rustix::fs::OFlags::NOFOLLOW
            | rustix::fs::OFlags::CLOEXEC,
        rustix::fs::Mode::empty(),
    )?);
    let before = held.metadata()?;
    let named = fs::symlink_metadata(path)?;
    if !before.is_file()
        || !named.is_file()
        || named.file_type().is_symlink()
        || before.len() > maximum
        || (before.dev(), before.ino(), before.len()) != (named.dev(), named.ino(), named.len())
    {
        return Err("invalid bounded recovery preflight receipt".into());
    }
    let mut bytes = Vec::new();
    held.take(maximum + 1).read_to_end(&mut bytes)?;
    let after = fs::symlink_metadata(path)?;
    if bytes.len() as u64 != before.len()
        || (after.dev(), after.ino(), after.len()) != (before.dev(), before.ino(), before.len())
    {
        return Err("recovery preflight receipt changed during read".into());
    }
    Ok(bytes)
}

fn normal_absolute(path: &Path) -> bool {
    path.is_absolute()
        && path.components().all(|part| {
            matches!(
                part,
                std::path::Component::RootDir | std::path::Component::Normal(_)
            )
        })
}

pub fn capture_preflight(
    output: &Path,
    native_root: &Path,
    selected: SelectedElfs,
    host_path: String,
) -> Result<String> {
    if !normal_absolute(output)
        || !normal_absolute(native_root)
        || host_path.is_empty()
        || host_path.len() > 16 * 1024
    {
        return Err("invalid recovery audit preflight paths".into());
    }
    let mut paths = BTreeSet::new();
    for path in selected.seven() {
        if !normal_absolute(&path) || !paths.insert(path) {
            return Err("selected recovery ELF paths are not unique and absolute".into());
        }
    }
    for provider in HOST_PROVIDERS {
        paths.insert(PathBuf::from(provider));
    }
    let programs = paths
        .iter()
        .map(|path| {
            capture_program_seal(path).map_err(|error| {
                format!(
                    "cannot seal pre-harness provider {}: {error}",
                    path.display()
                )
                .into()
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let preflight = Preflight {
        native_root: native_root.to_owned(),
        selected,
        host_path,
        programs,
    };
    let bytes = serde_json::to_vec_pretty(&preflight)?;
    if bytes.len() as u64 > MAX_PREFLIGHT_BYTES {
        return Err("recovery preflight receipt exceeds bound".into());
    }
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    file.write_all(&bytes)?;
    file.flush()?;
    Ok(format!("{:x}", Sha256::digest(&bytes)))
}

pub fn audit_after_harness(
    preflight_path: &Path,
    expected_preflight_sha256: &str,
    trace_dir: &Path,
    output: &Path,
    harness_exit_code: i32,
) -> Result<()> {
    if !normal_absolute(preflight_path)
        || !normal_absolute(trace_dir)
        || !normal_absolute(output)
        || expected_preflight_sha256.len() != 64
        || !expected_preflight_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit())
    {
        return Err("invalid recovery audit postflight inputs".into());
    }
    let bytes = regular_bytes(preflight_path, MAX_PREFLIGHT_BYTES)?;
    if format!("{:x}", Sha256::digest(&bytes)) != expected_preflight_sha256 {
        return Err("pre-harness executable receipt digest changed".into());
    }
    let pre: Preflight = serde_json::from_slice(&bytes)?;
    if !normal_absolute(&pre.native_root) || pre.host_path.is_empty() {
        return Err("invalid pre-harness recovery root or PATH".into());
    }
    let six = pre.selected.six();
    let mut required = pre.selected.seven().into_iter().collect::<BTreeSet<_>>();
    required.extend(HOST_PROVIDERS.iter().map(|path| PathBuf::from(*path)));
    let actual = pre
        .programs
        .iter()
        .map(|seal| PathBuf::from(std::ffi::OsStr::from_bytes(&seal.requested)))
        .collect::<BTreeSet<_>>();
    if actual != required || pre.programs.len() != required.len() {
        return Err("pre-harness executable roster changed".into());
    }
    let mut preexisting = Vec::with_capacity(pre.programs.len());
    for before in &pre.programs {
        let path = Path::new(std::ffi::OsStr::from_bytes(&before.requested));
        let after = capture_program_seal(path)?;
        if *before != after {
            return Err("pre-harness executable changed during family traffic".into());
        }
        preexisting.push((before.clone(), after));
    }
    let mut created = authored_program_rules(&pre.native_root)?;
    created.extend(copied_http_program_rules(
        &pre.native_root,
        &pre.selected.server,
        &pre.selected.child,
    )?);
    created.extend(owned_tmux_symlink_rules(
        &pre.native_root,
        &pre.selected.tmux,
    )?);
    let mut relations = Vec::with_capacity(4);
    let mut alias_receipts = Vec::with_capacity(2);
    for lane in ["checkout", "private"] {
        let action_root = pre.native_root.join(lane).join("actions");
        let bin = action_root.join("rust-owned-native-bin/native-bin");
        let candidate_alias = bin.join("recovery_actions");
        let child_alias = bin.join("openpilot-process-child");
        for (source, alias) in [
            (&pre.selected.actions, &candidate_alias),
            (&pre.selected.child, &child_alias),
        ] {
            let source = source.as_os_str().as_bytes().to_vec();
            let seal = capture_program_seal(alias)?;
            relations.push(AliasRelation {
                alias: seal.requested.clone(),
                source: source.clone(),
                same_inode: true,
            });
            created.push(CreatedProgramRule {
                final_seal: seal,
                kind: CreatedProgramKind::Alias { source },
            });
        }
        alias_receipts.push(OwnedAliasReceiptRule {
            path: action_root.join("rust-owned-native-bin/owned-native-bin-bindings.jsonl"),
            candidate_source: pre.selected.actions.as_os_str().as_bytes().to_vec(),
            candidate_alias: candidate_alias.as_os_str().as_bytes().to_vec(),
            child_source: pre.selected.child.as_os_str().as_bytes().to_vec(),
            child_alias: child_alias.as_os_str().as_bytes().to_vec(),
        });
    }
    let mut independent = preexisting
        .iter()
        .map(|(before, _)| (before.requested.clone(), before.sha256.clone()))
        .collect::<Vec<_>>();
    let pre_map = preexisting
        .iter()
        .map(|(before, _)| (before.requested.as_slice(), before))
        .collect::<std::collections::BTreeMap<_, _>>();
    for program in &created {
        let digest = match &program.kind {
            CreatedProgramKind::AuthoredScript { bytes, .. } => {
                format!("{:x}", Sha256::digest(bytes))
            }
            CreatedProgramKind::CopiedElf { source, .. }
            | CreatedProgramKind::Alias { source }
            | CreatedProgramKind::SymlinkAlias { source, .. } => pre_map
                .get(source.as_slice())
                .ok_or("created executable has no pre-harness ELF source")?
                .sha256
                .clone(),
        };
        independent.push((program.final_seal.requested.clone(), digest));
    }
    let scripts = authored_scripts(&pre.native_root)
        .into_iter()
        .map(|script| {
            let shebang = script
                .bytes
                .split(|byte| *byte == b'\n')
                .next()
                .ok_or("authored script shebang missing")?;
            Ok(ScriptInterpreter {
                script: script.path.as_os_str().as_bytes().to_vec(),
                exact_shebang: shebang.to_vec(),
                interpreter: b"/bin/sh".to_vec(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let mut allowed = provenance_program_rules(&pre.native_root, &six, &pre.host_path)?;
    allowed.push(ExpectedProgram {
        program: pre.selected.harness.as_os_str().as_bytes().to_vec(),
        outcome: Outcome::Success,
        minimum: 1,
        maximum: 1,
    });
    let same_elf_digests = six
        .iter()
        .map(|path| {
            pre_map
                .get(path.as_os_str().as_bytes())
                .map(|seal| seal.sha256.clone())
                .ok_or("selected ELF absent from pre-harness receipt")
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let harness_program = pre.selected.harness.as_os_str().as_bytes().to_vec();
    let policy = ExplicitPolicy {
        expected: vec![],
        typed_expected: vec![],
        allowed_programs: allowed,
        program_seals: preexisting,
        created_programs: created,
        expected_program_sha256: independent,
        alias_relations: relations,
        scripts,
        alias_receipts,
        admission_path: pre.native_root.join("admission.json"),
        same_elf_digests,
        historical_sources: 13,
        harness_exit_code,
        harness_argv: vec![
            harness_program.clone(),
            b"--exact".to_vec(),
            b"eight_native_recovery_controllers_same_elf_present_and_absent".to_vec(),
            b"--test-threads=1".to_vec(),
            b"--nocapture".to_vec(),
        ],
        harness_program,
    };
    let corpus = parse_trace_dir(trace_dir)?;
    let receipt = audit(corpus, &policy)?;
    write_receipt_exclusive(output, &receipt)?;
    Ok(())
}

/// The receipt path follows the controller's source-authored writer, not the
/// trace. Every returned program must have a writer with a creation boundary.
fn receipt_for(relative: &str, root: &Path) -> Result<PathBuf> {
    let parts = relative.split('/').collect::<Vec<_>>();
    let receipt = match parts.as_slice() {
        ["lifecycle", "fixture-shell"] => root.join("lifecycle/created-lifecycle-shell.jsonl"),
        ["terminal", "native", "fixture-shell"] => {
            root.join("terminal/created-terminal-shell.jsonl")
        }
        ["support", "native", "fixture-shell"] => root.join("support/created-support-shell.jsonl"),
        ["shutdown", "fixture-shell"] => root.join("shutdown/created-shutdown-shell.jsonl"),
        ["shutdown", "cloudflared"] => root.join("shutdown/created-shutdown-cloudflared.jsonl"),
        ["actions", "rust-owned-effects", name, "repo", "rust", "bin", "openpilot-runtime-install"]
            if ACTION_SCENARIOS.contains(name) =>
        {
            root.join(format!(
                "actions/rust-owned-effects/{name}/created-package-helper.jsonl"
            ))
        }
        ["actions", name, "tools", "tmux"]
            if [
                "permission_tmux",
                "nonzero_tmux",
                "whitespace_tmux",
                "replacement_tmux",
            ]
            .contains(name) =>
        {
            root.join(format!("actions/{name}/created-tmux.jsonl"))
        }
        ["actions", name, "tools", "sudo" | "reboot"]
            if ["sudo_reboot", "fallback_reboot"].contains(name) =>
        {
            root.join(format!("actions/{name}/created-reboot-tool.jsonl"))
        }
        ["actions", "reboot_shutdown", "tools", "sudo"] => {
            root.join("actions/reboot_shutdown/created-reboot-cleanup-sudo.jsonl")
        }
        ["actions", "rust-owned-effects", name, "tools", tool]
            if ["git_rebuild", "rebuild_all"].contains(name)
                && ["scons", "sudo"].contains(tool) =>
        {
            root.join(format!(
                "actions/rust-owned-effects/{name}/created-rebuild-{tool}.jsonl"
            ))
        }
        ["upload", "direct-http-actions", name, "tools", "tmux"]
            if ["success", "partial", "all-failed"].contains(name) =>
        {
            root.join(format!(
                "upload/direct-http-actions/{name}/created-upload-tmux.jsonl"
            ))
        }
        _ => return Err("unmapped authored recovery program writer".into()),
    };
    Ok(receipt)
}

/// Capture only the *final* seal of each program after the successful harness.
/// `CreatedProgramGuard` supplies the earlier creation row independently.
pub fn authored_program_rules(native_root: &Path) -> Result<Vec<CreatedProgramRule>> {
    authored_scripts(native_root)
        .into_iter()
        .map(|script| {
            let relative = script.path.strip_prefix(native_root)?;
            let parts = relative.components().collect::<Vec<_>>();
            let lane = parts.first().ok_or("missing frozen lane")?.as_os_str();
            let root = native_root.join(lane);
            let name = script
                .path
                .strip_prefix(&root)?
                .to_str()
                .ok_or("non-UTF8 owned script")?;
            Ok(CreatedProgramRule {
                final_seal: capture_program_seal(&script.path)?,
                kind: CreatedProgramKind::AuthoredScript {
                    bytes: script.bytes,
                    mode: script.mode,
                    receipt: receipt_for(name, &root)?,
                },
            })
        })
        .collect()
}

pub fn copied_http_program_rules(
    native_root: &Path,
    server_source: &Path,
    child_source: &Path,
) -> Result<Vec<CreatedProgramRule>> {
    let mut rules = Vec::with_capacity(4);
    for lane in ["checkout", "private"] {
        let output = native_root.join(lane).join("http");
        let package = output.join("minimal-host-package/bin");
        for (name, source, label) in [
            ("openpilot-carrot-recovery", server_source, "server"),
            ("openpilot-process-child", child_source, "child"),
        ] {
            rules.push(CreatedProgramRule {
                final_seal: capture_program_seal(&package.join(name))?,
                kind: CreatedProgramKind::CopiedElf {
                    source: source.as_os_str().as_bytes().to_vec(),
                    receipt: output.join(format!("created-http-{label}.jsonl")),
                },
            });
        }
    }
    Ok(rules)
}

/// The real-tmux control authors one PATH symlink per lane after preflight.
/// Its target is the selected, held native tmux ELF, never an authored script.
pub fn owned_tmux_symlink_rules(
    native_root: &Path,
    tmux_source: &Path,
) -> Result<Vec<CreatedProgramRule>> {
    ["checkout", "private"]
        .into_iter()
        .map(|lane| {
            let output = native_root.join(lane).join("actions/rust-owned-tmux");
            Ok(CreatedProgramRule {
                final_seal: capture_program_seal(&output.join("tools/tmux"))?,
                kind: CreatedProgramKind::SymlinkAlias {
                    source: tmux_source.as_os_str().as_bytes().to_vec(),
                    receipt: output.join("created-owned-tmux-symlink.jsonl"),
                },
            })
        })
        .collect()
}

/// A narrow argv-path exception for process-supervision's non-executable
/// tempfile descriptor. Count must come from the controller contract, never
/// the observed trace. Every selected child exec remains independently sealed.
pub fn process_child_descriptor_rule(program: &Path, count: usize) -> Result<ExpectedTypedAttempt> {
    if count == 0
        || program
            .file_name()
            .is_none_or(|name| name != "openpilot-process-child")
    {
        return Err("invalid process-child descriptor policy".into());
    }
    let program = program.as_os_str().as_bytes().to_vec();
    Ok(ExpectedTypedAttempt {
        program: program.clone(),
        argv: vec![TypedArg::Exact(program), TypedArg::LaunchDescriptor],
        outcome: Outcome::Success,
        count,
    })
}

/// Source-authored executable path/result roster for the narrower provenance
/// gate. `host_path` must be the explicit hosted PATH recorded before the run.
/// Search misses are admitted only for the three commands the retained shell
/// and Git fixtures actually resolve through PATH. No path is read from trace.
pub fn provenance_program_rules(
    native_root: &Path,
    six_built_elfs: &[PathBuf; 6],
    host_path: &str,
) -> Result<Vec<ExpectedProgram>> {
    let mut success = BTreeSet::new();
    for binary in six_built_elfs {
        if !binary.is_absolute() || !success.insert(binary.clone()) {
            return Err("duplicate or nonabsolute built recovery ELF".into());
        }
    }
    for name in HOST_PROVIDERS {
        success.insert(PathBuf::from(name));
    }
    for script in authored_scripts(native_root) {
        // The 0644 permission fixture is an expected EACCES, not a success.
        if script.mode & 0o111 != 0 {
            success.insert(script.path);
        }
    }
    for lane in ["checkout", "private"] {
        let root = native_root.join(lane);
        success.insert(root.join("actions/rust-owned-tmux/tools/tmux"));
        for name in ["recovery_actions", "openpilot-process-child"] {
            success.insert(
                root.join("actions/rust-owned-native-bin/native-bin")
                    .join(name),
            );
        }
        for name in ["openpilot-carrot-recovery", "openpilot-process-child"] {
            success.insert(root.join("http/minimal-host-package/bin").join(name));
        }
    }
    let mut rules = success
        .into_iter()
        .map(|path| {
            let maximum = if path == Path::new("/usr/bin/git") {
                4096 // 24 owned Git action fixtures across two roots.
            } else if path == Path::new("/bin/sh") || path == Path::new("/bin/bash") {
                512
            } else if six_built_elfs.contains(&path)
                || path
                    .ends_with("actions/rust-owned-native-bin/native-bin/openpilot-process-child")
            {
                2048
            } else if path.starts_with(native_root) {
                128 // One authored helper per case, with bounded retries.
            } else {
                256
            };
            ExpectedProgram {
                program: path.as_os_str().as_bytes().to_vec(),
                outcome: Outcome::Success,
                minimum: 0,
                maximum,
            }
        })
        .collect::<Vec<_>>();

    let mut missing = BTreeSet::new();
    for directory in host_path.split(':') {
        let directory = Path::new(directory);
        if !directory.is_absolute()
            || directory.components().any(|part| {
                !matches!(
                    part,
                    std::path::Component::RootDir | std::path::Component::Normal(_)
                )
            })
        {
            return Err("nonabsolute or non-normal hosted PATH entry".into());
        }
        for command in ["git", "bash", "sleep"] {
            let path = directory.join(command);
            if !HOST_PROVIDERS
                .iter()
                .any(|provider| Path::new(provider) == path)
            {
                missing.insert(path);
            }
        }
    }
    for lane in ["checkout", "private"] {
        let root = native_root.join(lane).join("actions");
        for (case, name) in [
            ("fallback_reboot", "sudo"),
            ("missing_reboot", "sudo"),
            ("missing_reboot", "reboot"),
            ("missing_tmux", "tmux"),
        ] {
            missing.insert(root.join(case).join("tools").join(name));
        }
        for case in ["git_rebuild", "rebuild_all"] {
            missing.insert(root.join("rust-owned-effects").join(case).join("tools/git"));
        }
        rules.push(ExpectedProgram {
            program: root
                .join("permission_tmux/tools/tmux")
                .as_os_str()
                .as_bytes()
                .to_vec(),
            outcome: Outcome::Errno("EACCES".into()),
            minimum: 1,
            maximum: 1,
        });
    }
    rules.extend(missing.into_iter().map(|path| ExpectedProgram {
        maximum: if path.starts_with(native_root) {
            8
        } else if path.file_name().is_some_and(|name| name == "git") {
            4096
        } else {
            512
        },
        program: path.as_os_str().as_bytes().to_vec(),
        outcome: Outcome::Errno("ENOENT".into()),
        minimum: 0,
    }));
    Ok(rules)
}

fn script(root: &Path, relative: &str, body: &[u8], mode: u32) -> AuthoredScript {
    AuthoredScript {
        path: root.join(relative),
        bytes: body.to_vec(),
        mode,
    }
}

/// Every persistent script written by the frozen Rust controller. A writer
/// must provide its own held-file creation/after-family receipt before this
/// list can be admitted as `CreatedProgramKind::AuthoredScript`.
pub fn authored_scripts(native_root: &Path) -> Vec<AuthoredScript> {
    let mut scripts = Vec::new();
    let basic_shell = b"#!/bin/sh\nexec /bin/bash --noprofile --norc -i\n";
    let terminal_shell = b"#!/bin/sh\nprintf \"%s\\n\" \"$@\" >> \"$CARROT_RECOVERY_SHELL_LOG\"\nexec /bin/bash --noprofile --norc -i\n";
    let package = b"#!/bin/sh\nprintf '%s\\n' \"$@\" > package-attempt\necho 'Owned matching release unavailable'\nexit 17\n";
    let reboot = b"#!/bin/sh\nprintf '%s\\n' \"$$\" > \"$CARROT_FIXTURE_REBOOT_PID\"\nprintf '%s\\n' \"$0 $*\" >> \"$CARROT_FIXTURE_REBOOT_RECORD\"\n";
    for lane in ["checkout", "private"] {
        let root = native_root.join(lane);
        scripts.push(script(&root, "lifecycle/fixture-shell", basic_shell, 0o700));
        scripts.push(script(
            &root,
            "terminal/native/fixture-shell",
            terminal_shell,
            0o700,
        ));
        scripts.push(script(
            &root,
            "support/native/fixture-shell",
            basic_shell,
            0o700,
        ));
        scripts.push(script(&root, "shutdown/fixture-shell", basic_shell, 0o700));
        scripts.push(script(&root, "shutdown/cloudflared", b"#!/bin/sh\nprintf \"%s\\n\" \"$$\" > \"$CARROT_OWNED_TUNNEL_ENTERED\"\nwhile ! test -f \"$CARROT_OWNED_TUNNEL_RELEASE\"; do sleep 0.1; done\nprintf \"https://owned-fixture.trycloudflare.com\\n\"\nexec sleep 600\n", 0o700));
        for name in ACTION_SCENARIOS {
            scripts.push(script(
                &root,
                &format!(
                    "actions/rust-owned-effects/{name}/repo/rust/bin/openpilot-runtime-install"
                ),
                package,
                0o755,
            ));
        }
        for (name, body, mode) in [
            ("permission_tmux", b"#!/bin/sh\nexit 0\n".as_slice(), 0o644),
            (
                "nonzero_tmux",
                b"#!/bin/sh\nprintf 'owned stderr\\n' >&2; printf ignored; exit 7\n".as_slice(),
                0o755,
            ),
            (
                "whitespace_tmux",
                b"#!/bin/sh\nprintf ' \\n' >&2; exit 7\n".as_slice(),
                0o755,
            ),
            (
                "replacement_tmux",
                b"#!/bin/sh\nprintf 'owned\\377\\r\\n'\n".as_slice(),
                0o755,
            ),
        ] {
            scripts.push(script(
                &root,
                &format!("actions/{name}/tools/tmux"),
                body,
                mode,
            ));
        }
        for (name, tool) in [("sudo_reboot", "sudo"), ("fallback_reboot", "reboot")] {
            scripts.push(script(
                &root,
                &format!("actions/{name}/tools/{tool}"),
                reboot,
                0o755,
            ));
        }
        scripts.push(script(&root, "actions/reboot_shutdown/tools/sudo", b"#!/bin/sh\nprintf '%s\\n' \"$$\" > \"$CARROT_FIXTURE_REBOOT_PID\"\n/bin/sleep 60 &\nprintf '%s\\n' \"$!\" > \"$CARROT_FIXTURE_REBOOT_DESCENDANT\"\nwait\n", 0o755));
        for name in ["git_rebuild", "rebuild_all"] {
            for (tool, marker) in [("scons", "scons.argv"), ("sudo", "sudo.argv")] {
                scripts.push(script(
                    &root,
                    &format!("actions/rust-owned-effects/{name}/tools/{tool}"),
                    format!("#!/bin/sh\nprintf '%s\\n' \"$*\" > {marker}\n").as_bytes(),
                    0o700,
                ));
            }
        }
        let tmux = b"#!/bin/sh\nprintf 'owned capture\\377\\r\\n'\n";
        for name in ["success", "partial", "all-failed"] {
            scripts.push(script(
                &root,
                &format!("upload/direct-http-actions/{name}/tools/tmux"),
                tmux,
                0o755,
            ));
        }
    }
    scripts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn authored_roster_is_bounded_and_has_no_duplicate_paths() {
        let scripts = authored_scripts(Path::new("/owned/native"));
        let names = scripts
            .iter()
            .map(|script| &script.path)
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(scripts.len(), names.len());
        assert_eq!(scripts.len(), 2 * (5 + 24 + 4 + 2 + 1 + 4 + 3));
        assert!(scripts
            .iter()
            .all(|script| script.bytes.starts_with(b"#!/bin/sh\n")
                && [0o644, 0o700, 0o755].contains(&script.mode)));
        let mut receipts = std::collections::BTreeSet::new();
        for script in scripts {
            let relative = script.path.strip_prefix("/owned/native").unwrap();
            let mut components = relative.components();
            let lane = components.next().unwrap().as_os_str();
            let root = Path::new("/owned/native").join(lane);
            let name = script.path.strip_prefix(&root).unwrap().to_str().unwrap();
            assert!(receipts.insert(receipt_for(name, &root).unwrap()));
        }
    }

    #[test]
    fn provenance_roster_comes_from_sources_and_hosted_path_not_trace() {
        let elfs = std::array::from_fn(|index| PathBuf::from(format!("/owned/bin/native-{index}")));
        let rules = provenance_program_rules(
            Path::new("/owned/native"),
            &elfs,
            "/snap/bin:/usr/local/bin:/usr/bin:/bin",
        )
        .unwrap();
        let contains = |path: &[u8], outcome: Outcome| {
            rules
                .iter()
                .any(|rule| rule.program == path && rule.outcome == outcome)
        };
        assert!(contains(b"/snap/bin/git", Outcome::Errno("ENOENT".into())));
        assert!(!contains(b"/usr/bin/git", Outcome::Errno("ENOENT".into())));
        assert!(contains(b"/usr/bin/git", Outcome::Success));
        assert!(contains(
            b"/owned/native/private/actions/permission_tmux/tools/tmux",
            Outcome::Errno("EACCES".into())
        ));
        for lane in ["checkout", "private"] {
            let alias = format!("/owned/native/{lane}/actions/rust-owned-tmux/tools/tmux");
            assert!(contains(alias.as_bytes(), Outcome::Success));
            assert!(!authored_scripts(Path::new("/owned/native"))
                .iter()
                .any(|script| script.path == Path::new(&alias)));
        }
        assert!(!contains(b"/usr/bin/python3", Outcome::Success));
        assert!(
            provenance_program_rules(Path::new("/owned/native"), &elfs, "relative:/usr/bin")
                .is_err()
        );
    }
}
