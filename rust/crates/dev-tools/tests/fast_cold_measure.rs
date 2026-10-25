//! #594 실제 Fast 함수 + fake GNU du 경계. 작성 후 실행은 GHA에서만 수행한다.
#![cfg(target_os = "linux")]

use openpilot_dev_tools::qa_process::{Captured, CommandSpec, NativeRunner, Runner};
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};

const HELPER: &str = include_str!("../../../../tools/ci/fast_native_cold_560.sh");
const FAKE_DU: &str = r#"#!/usr/bin/bash
set -euo pipefail
n=0
[[ ! -f $DU_COUNT ]] || read -r n < "$DU_COUNT"
n=$((n+1));printf '%s\n' "$n" > "$DU_COUNT"
printf '%s\t' "$@" >> "$DU_CALLS";printf '\n' >> "$DU_CALLS"
apparent=false
while [[ $1 != -- ]];do [[ $1 != --apparent-size ]] || apparent=true;shift;done
shift;roots=("$@")
emit() { local p;for p in "${roots[@]}";do printf '%s\t%s\n' "$1" "$p";done; }
enoent() { printf "du: cannot access '%s': No such file or directory\n" "$DU_MISSING" >&2; }
case $DU_CASE in
  delete-race|race|race2|race3)
    until=1;[[ $DU_CASE != race2 ]] || until=2;[[ $DU_CASE != race3 ]] || until=3
    if ((n<=until));then
      [[ $DU_CASE != delete-race ]] || rm -- "$DU_MISSING"
      emit 3;enoent;exit 1
    fi;;
  aggregate)
    # One GNU du invocation deduplicates hardlinks across both arguments.
    # Splitting this into two invocations would report 600+600 instead of 600+0.
    if ((${#roots[@]}==2));then printf '600\t%s\n0\t%s\n' "${roots[0]}" "${roots[1]}";else emit 600;fi
    exit;;
  cap|partial-cap) emit 1025;[[ $DU_CASE != partial-cap ]] || { enoent;exit 1; };exit;;
  aggregate-cap) printf '700\t%s\n400\t%s\n' "${roots[0]}" "${roots[1]}";enoent;exit 1;;
  huge) printf '999999999999999999999999999\t%s\n' "${roots[0]}";exit;;
  wrong-root) printf '7\t/wrong\n';exit;;
  missing-output) exit;;
  extra-output) emit 7;printf '8\t/extra\n';exit;;
  negative) printf -- '-1\t%s\n' "${roots[0]}";exit;;
  leading-zero) printf '07\t%s\n' "${roots[0]}";exit;;
  nonnumeric) printf 'x\t%s\n' "${roots[0]}";exit;;
  unterminated-output) printf '7\t%s' "${roots[0]}";exit;;
  extra-unterminated) emit 7;printf garbage;exit;;
  nul-output) printf '7\000\t%s\n' "${roots[0]}";exit;;
  permission) emit 7;printf "du: cannot read directory '%s': Permission denied\n" "$DU_MISSING" >&2;exit 1;;
  unknown-stderr) emit 7;printf 'du: Input/output error\n' >&2;exit 1;;
  mixed-stderr) emit 7;enoent;printf 'du: Input/output error\n' >&2;exit 1;;
  success-stderr) emit 7;enoent;exit;;
  missing-stderr) emit 7;exit 1;;
  other-status) emit 7;enoent;exit 2;;
  unterminated-stderr) emit 7;printf "du: cannot access '%s': No such file or directory" "$DU_MISSING" >&2;exit 1;;
  nul-stderr) emit 7;printf "du: cannot access '%s': No such file or directory\000\n" "$DU_MISSING" >&2;exit 1;;
  swap-job|swap-owner|swap-provider)
    case $DU_CASE in swap-job) victim=$F_JOB;;swap-owner) victim=$F_OWNER;;swap-provider) victim=$F_JOB/cold-strace-560;;esac
    mv -- "$victim" "$victim.replaced";mkdir -m 700 -- "$victim"
    mkdir -p -- "$F_JOB/proof/measure"
    emit 7;exit;;
  clean) ;;
  *) exit 93;;
esac
if [[ $apparent == true ]];then emit 9;else emit 7;fi
"#;

fn function(name: &str) -> &'static str {
    let marker = format!("{name}() {{\n");
    assert_eq!(HELPER.matches(&marker).count(), 1);
    let start = HELPER.find(&marker).unwrap();
    let end = start + HELPER[start..].find("\n}\n").unwrap() + 3;
    &HELPER[start..end]
}

struct Fixture {
    _temp: tempfile::TempDir,
    base: PathBuf,
    job: PathBuf,
    owner: PathBuf,
    out: PathBuf,
    script: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        let base = temp.path().canonicalize().unwrap();
        let job = base.join("job");
        let owner = base.join("owner");
        let out = job.join("proof/measure");
        for path in [
            job.clone(),
            owner.clone(),
            out.clone(),
            job.join("cold-strace-560/build/tmp"),
            job.join("proof/cold-strace-560"),
            job.join("fast-cold-560/tmp"),
            owner.join("source/rust/target"),
            base.join("bin"),
        ] {
            fs::create_dir_all(&path).unwrap();
            fs::set_permissions(path, fs::Permissions::from_mode(0o700)).unwrap();
        }
        fs::set_permissions(
            job.join("cold-strace-560"),
            fs::Permissions::from_mode(0o700),
        )
        .unwrap();
        let fake = base.join("bin/du");
        fs::write(&fake, FAKE_DU).unwrap();
        fs::set_permissions(&fake, fs::Permissions::from_mode(0o700)).unwrap();
        let script = base.join("measure.sh");
        let functions = ["owned_dir", "admission_failure", "measure"]
            .map(function)
            .join("\n");
        fs::write(
            &script,
            format!(
                "set -euo pipefail\numask 077\n\
                 job=$F_JOB;owner=$F_OWNER;source=$owner/source;root=$job/cold-strace-560;build=$job/fast-cold-560\n\
                 out=$job/proof/measure;phase=fixture\n\
                 measure_job_identity=$(stat -c '%d:%i' \"$job\")\n\
                 DIAG_SOURCE_IDENTITY=$(stat -c '%d:%i' \"$owner\")\n\
                 {functions}\n\
                 case $DU_SCOPE in job) roots=(\"$job\");;owner) roots=(\"$owner\");;provider) roots=(\"$root\" \"$job/proof/cold-strace-560\");;esac\n\
                 measure 1024 \"$DU_KIND\" \"${{roots[@]}}\"\n"
            ),
        )
        .unwrap();
        Self {
            _temp: temp,
            base,
            job,
            owner,
            out,
            script,
        }
    }

    fn missing(&self) -> PathBuf {
        self.job.join("cold-strace-560/build/tmp/cc-removed")
    }

    fn run(&self, case: &str, scope: &str, kind: &str, missing: &Path) -> Captured {
        let mut spec = CommandSpec::new(
            vec![
                "/usr/bin/bash".into(),
                "--noprofile".into(),
                "--norc".into(),
                self.script.to_str().unwrap().into(),
            ],
            Duration::from_secs(5),
        );
        spec.env_remove = ["BASH_ENV", "ENV", "LD_PRELOAD", "LD_LIBRARY_PATH"]
            .map(std::ffi::OsString::from)
            .to_vec();
        for (key, value) in [
            ("LC_ALL", "C".into()),
            ("PATH", format!("{}/bin:/usr/bin:/bin", self.base.display())),
            ("F_JOB", self.job.to_str().unwrap().into()),
            ("F_OWNER", self.owner.to_str().unwrap().into()),
            ("DU_COUNT", self.base.join("count").to_str().unwrap().into()),
            ("DU_CALLS", self.base.join("calls").to_str().unwrap().into()),
            ("DU_CASE", case.into()),
            ("DU_SCOPE", scope.into()),
            ("DU_KIND", kind.into()),
            ("DU_MISSING", missing.to_str().unwrap().into()),
        ] {
            spec.env.insert(key.into(), value);
        }
        NativeRunner.run(&spec).unwrap()
    }

    fn attempts(&self) -> usize {
        fs::read_to_string(self.base.join("count"))
            .unwrap()
            .trim()
            .parse()
            .unwrap()
    }

    fn reject(&self, case: &str, scope: &str, missing: &Path, attempts: usize, reason: &str) {
        let result = self.run(case, scope, "", missing);
        assert_ne!(result.code, 0, "{case}: {result:?}");
        assert!(result.stdout.is_empty(), "{case}: {result:?}");
        assert_eq!(self.attempts(), attempts, "{case}");
        let record = fs::read_to_string(self.out.join("first-admission-failure.tsv")).unwrap();
        assert!(record.contains(reason), "{case}: {record}");
    }
}

#[test]
fn complete_single_and_aggregate_samples_preserve_gnu_du_arguments() {
    for (case, scope, kind, expected) in [
        ("clean", "owner", "", "7\n"),
        ("clean", "job", "--apparent-size", "9\n"),
        ("aggregate", "provider", "", "600\n"),
        ("aggregate", "provider", "--apparent-size", "600\n"),
    ] {
        let f = Fixture::new();
        let result = f.run(case, scope, kind, &f.missing());
        assert_eq!(result.code, 0, "{result:?}");
        assert_eq!(result.stdout, expected.as_bytes());
        assert_eq!(f.attempts(), 1);
        let calls = fs::read_to_string(f.base.join("calls")).unwrap();
        assert_eq!(calls.lines().count(), 1);
        assert_eq!(calls.contains("--apparent-size\t"), !kind.is_empty());
        if scope == "provider" {
            assert!(calls.ends_with(&format!(
                "--\t{}\t{}\t\n",
                f.job.join("cold-strace-560").display(),
                f.job.join("proof/cold-strace-560").display()
            )));
        }
        assert!(!f.out.join("first-admission-failure.tsv").exists());
    }
}

#[test]
fn actually_removed_mutable_files_retry_to_a_complete_clean_sample() {
    for (scope, relative) in [
        ("job", "job/cold-strace-560/build/tmp/cc-removed"),
        ("owner", "owner/source/rust/target/removed.o"),
        ("job", "job/fast-cold-560/tmp/cc-removed"),
        ("provider", "job/cold-strace-560/build/tmp/cc-removed"),
    ] {
        let f = Fixture::new();
        let missing = f.base.join(relative);
        fs::write(&missing, b"removed during the fake du sample").unwrap();
        let result = f.run("delete-race", scope, "--apparent-size", &missing);
        assert_eq!(result.code, 0, "{result:?}");
        assert_eq!(
            result.stdout,
            if scope == "provider" { "18\n" } else { "9\n" }.as_bytes()
        );
        assert_eq!(f.attempts(), 2);
        let raw = fs::read_to_string(f.out.join("du-failed-samples.log")).unwrap();
        assert!(raw.contains("attempt=1 exit=1"));
        assert!(raw.contains("No such file or directory"));
        let measured = match scope {
            "owner" => f.owner.clone(),
            "provider" => f.job.join("cold-strace-560"),
            _ => f.job.clone(),
        };
        assert!(raw.contains(&format!("3\t{}\n", measured.display())));
    }
}

#[test]
fn only_a_complete_third_sample_can_succeed_and_three_enoents_fail() {
    let f = Fixture::new();
    let result = f.run("race2", "job", "", &f.missing());
    assert_eq!(result.code, 0, "{result:?}");
    assert_eq!(result.stdout, b"7\n");
    assert_eq!(f.attempts(), 3);
    let f = Fixture::new();
    f.reject(
        "race3",
        "job",
        &f.missing(),
        3,
        "du-three-attempts-exhausted",
    );
    let raw = fs::read_to_string(f.out.join("du-failed-samples.log")).unwrap();
    assert_eq!(raw.matches("---stdout---").count(), 3);
    assert_eq!(raw.matches("No such file or directory").count(), 3);
    assert!(raw.len() <= 65536);
    assert!(
        fs::metadata(f.out.join("first-admission-du.stderr"))
            .unwrap()
            .len()
            > 0
    );
    let f = Fixture::new();
    fs::write(f.out.join("du-failed-samples.log"), vec![b'x'; 65536]).unwrap();
    f.reject("race", "job", &f.missing(), 1, "du-evidence-bound");
    assert_eq!(
        fs::metadata(f.out.join("du-failed-samples.log"))
            .unwrap()
            .len(),
        65536
    );
}

#[test]
fn complete_or_partial_cap_excess_never_retries_including_aggregate() {
    for (case, scope) in [
        ("cap", "job"),
        ("partial-cap", "job"),
        ("aggregate-cap", "provider"),
        ("huge", "job"),
    ] {
        let f = Fixture::new();
        f.reject(case, scope, &f.missing(), 1, "measured-cap");
    }
}

#[test]
fn malformed_stdout_never_becomes_an_accepted_partial_sample() {
    for case in [
        "wrong-root",
        "missing-output",
        "extra-output",
        "negative",
        "leading-zero",
        "nonnumeric",
        "unterminated-output",
        "extra-unterminated",
        "nul-output",
    ] {
        let f = Fixture::new();
        f.reject(case, "job", &f.missing(), 1, "du-output-shape");
    }
}

#[test]
fn permission_unknown_status_and_immutable_scope_errors_fail_closed() {
    for (case, reason) in [
        ("permission", "du-error-shape"),
        ("unknown-stderr", "du-error-shape"),
        ("mixed-stderr", "du-error-shape"),
        ("success-stderr", "du-status"),
        ("missing-stderr", "du-status"),
        ("other-status", "du-status"),
        ("unterminated-stderr", "du-error-shape"),
        ("nul-stderr", "du-error-shape"),
    ] {
        let f = Fixture::new();
        f.reject(case, "job", &f.missing(), 1, reason);
    }
    for (scope, relative) in [
        ("job", "job/proof/immutable"),
        ("owner", "owner/source/immutable"),
        ("owner", "job/cold-strace-560/build/tmp/cc-removed"),
    ] {
        let f = Fixture::new();
        f.reject(
            "race",
            scope,
            &f.base.join(relative),
            1,
            "du-absent-mutable-path",
        );
    }
}

#[test]
fn existing_symlink_and_noncanonical_mutable_paths_are_not_retried() {
    for kind in ["existing", "symlink", "parent-symlink", "dotdot"] {
        let f = Fixture::new();
        let missing = match kind {
            "existing" => {
                let p = f.missing();
                fs::write(&p, b"still present").unwrap();
                p
            }
            "symlink" => {
                let p = f.missing();
                symlink("no-target", &p).unwrap();
                p
            }
            "parent-symlink" => {
                let p = f.job.join("cold-strace-560/build/link");
                symlink(&f.base, &p).unwrap();
                p.join("absent")
            }
            "dotdot" => f.job.join("cold-strace-560/build/tmp/../removed"),
            _ => unreachable!(),
        };
        f.reject("race", "job", &missing, 1, "du-absent-mutable-path");
    }
}

#[test]
fn replacing_job_owner_or_a_measured_provider_root_is_rejected() {
    for (case, scope, reason) in [
        ("swap-job", "job", "owner-job-identity"),
        ("swap-owner", "job", "owner-job-identity"),
        ("swap-provider", "provider", "root-identity-changed"),
    ] {
        let f = Fixture::new();
        f.reject(case, scope, &f.missing(), 1, reason);
    }
}
