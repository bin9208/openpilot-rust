# #523 changed-files: 정확한 두 원본 삭제 candidate와 기존 proof 재사용

2026-10-24 KST. GPT-6.1 Sol. #523/#296에서 추적하며 별도 이슈를 만들지 않는다.
**candidate 구현이며 새 5-test/Clippy/부재 검증은 UNRUN이다. inventory credit은0이다.**
로컬에서는 소스·Git/API·inert archive·rustfmt parsing·diff만 확인했다.
원본/candidate 실행, build/test, archived script/ELF 실행, 새 Actions 실행, commit/push는 하지 않았다.

## 정확한 변경 범위

삭제한 maintained 파일은 다음 둘뿐이다. 본문을 frozen source로 복사하지 않았고 삭제 원본 읽기/import/fallback도 추가하지 않았다.

| 삭제 경로 | 삭제 전 bytes | 삭제 전 전체 SHA256 |
| --- | ---: | --- |
| `tools/ci/check_changed_files.py` | 995 | `6bcb14f3a6d5d4e885c655e22fed23a1ecf2199b048cd3da95ef59b2795fad87` |
| `tools/ci/tests/test_changed_files.py` | 1410 | `cd9a211d076d6939b7649849212ea21c52e7e0897fa6d251e82b0b2805e08242` |

기존 `tests/changed_files_caller.rs`의4개 test 본문/assert/정책 명령 문자열은 유지하고 각 첫 줄에 RAII guard만 넣었다.
새 sibling `tests/changed_files_caller/absence.rs`에 guard와 negative test1개를 두어 같은 Cargo target이 총5개를 선택한다.
Cargo/workflow/inventory와 native parser/data는 수정하지 않았다. 전체 source tar·SDK/provider seal과 context 준비는 부모 workflow 책임이다.

## 기존4개의 actual CI와 새5개의 수용 구분

기존4개는 source `a134b32c5d50d346994d671c0882b147175021dc`의
[run40089943270/job120860396006](https://github.com/bin9208/openpilot-rust/actions/runs/40089943270/job/120860396006)에서 실제 PASS했다.
raw log1424줄에 actual test ELF 실행,1426줄 `running 4 tests`,1427–1430줄 각각 `ok`,1432줄
`4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out`가 있다.
job 전체는 이후 별도 `ci_workflow_policy` 6개 실패로 FAIL이므로 전체 Fast green으로 기록하지 않는다.

[artifact12559545652](https://github.com/bin9208/openpilot-rust/actions/runs/40089943270/artifacts/12559545652)은419,610B,
SHA256 `88755f3b00729fb0647bd151ffcf7afdf7138c2bd5da61fa2a63bf888f80782f`다.
전체 archive SHA, a134 SOURCE_COMMIT, source/index/prepared-SDK before/after 일치를 inert 확인했다.
당시 caller source는7,438B/SHA256 `085487b0867f0012ef4f2edc000b6f43cc7d506d4f0a1189e1324eb2fbccb172`였다.
같은 run의 Python/Android step은 실제 SUCCESS여도 `parsed 0 changed Python files`이므로 syntax 동작 coverage로 세지 않는다.
이전4개의 PASS는 이번 guard/삭제/negative를 포함한5개의 PASS가 아니다.

## 필수 context: 부모가 CI에서만 준비

필수 env는 다음3개다. 누락·잘못된 pin·source/root/프로그램 경로 불일치는 test 시작에서 실패한다.

```text
EXPECTED_SOURCE=<현재 Actions source의 lowercase 40자리 Git SHA>
CHANGED_FILES_CALLER_CONTEXT=<절대 경로의 현재 context JSON>
CHANGED_FILES_CALLER_CONTEXT_SHA256=<그 JSON 원문 bytes의 lowercase SHA256>
```

context schema는 unknown field를 거부한다. 아래는 **형식 설명**이며 실제 SHA/경로를 꾸민 fixture가 아니다.

```json
{
  "schema": "openpilot.changed-files-caller-context.v1",
  "source_commit": "<EXPECTED_SOURCE>",
  "source_root": "<현재 CARGO_MANIFEST_DIR의 repository root 절대 경로>",
  "proof_dir": "<source 밖에 있는 owned proof directory 절대 경로>",
  "source_sha256": {
    "<아래 exact source roster의 repository-relative 경로>": "<현재 source 원문 SHA256>"
  },
  "cli": {
    "path": "<현재 CARGO_BIN_EXE_check-changed-files의 절대 경로>",
    "sha256": "<현재 실제 CLI ELF 전체 SHA256>"
  },
  "test_elf": {
    "path": "<현재 실제 changed_files_caller test ELF의 절대 경로>",
    "sha256": "<현재 실제 test ELF 전체 SHA256>"
  }
}
```

부모는 Actions의 실제 Cargo compiler-artifact에서 CLI/test executable을 정확히 선택하고 그 bytes를 pin해야 한다.
source가 다른 과거 ELF, target 경로 추측, artifact 안의 executable 실행은 허용하지 않는다.
guard는 `cli.path`를 compile-time Cargo 경로와, `test_elf.path`를 `current_exe()`와 대조한다.
source_root는 compile-time manifest root와 같아야 한다. source commit과 full source tar의 실제 일치는 부모가 검증한다.
proof_dir는 미리 만든 현재 euid 소유 directory로 group/other write를 금지한다(예:0700). source 내부 proof는 거부한다.
같은 proof directory에서 재실행하지 않는다. receipt는 `O_EXCL`로 만들어 stale 파일 덮어쓰기를 거부한다.

`source_sha256`의 exact roster는 다음12개와 `src/changed_files_check/` 아래의 **모든 실제 regular file**이다.
directory는 별도 FD/identity/roster로 검증한다. symlink·FIFO·device·socket 또는 추가/누락된 context key는 거부한다.

```text
rust/Cargo.toml
rust/Cargo.lock
rust/rust-toolchain.toml
rust/crates/dev-tools/Cargo.toml
rust/crates/dev-tools/src/lib.rs
rust/crates/dev-tools/src/qa_process.rs
rust/crates/dev-tools/src/ci_policy.rs
rust/crates/dev-tools/src/bin/check_changed_files.rs
rust/crates/dev-tools/src/changed_files_check.rs
rust/crates/dev-tools/tests/changed_files_caller.rs
rust/crates/dev-tools/tests/changed_files_caller/absence.rs
.github/workflows/fast-checks.yml
```

여기의 Cargo/Lock/workflow SHA는 파일 전체 bytes다. 옛 admission의 Cargo semantic hash를 현재 whole-file hash로 위장하지 않는다.
기존 Fast caller 명령은 그대로이며 현재 Fast에는 이 새 context export가 없다. 부모의 CI 준비 전에는5개가 fail-closed한다.
이 작업 범위에서 Fast나 focused workflow를 수정하지 않았다.

기존 source-only context builder는 `tools/ci`/`rust/tools` 및 관련 native entry source 검색에서 찾지 못했다.
`src/bin/list_cargo_tests.rs`/`src/ci_artifact_list.rs`의 기존 `list-cargo-tests`는 `profile.test`가 truthy인 executable을 나열한다.
target/package의 정확한 단일 선택·CLI 구분·source context 생성은 하지 않으므로 이번 builder의 대체가 아니다.
기존 `.github/workflows/rust.yml`의 `Compile and seal actual bridge catalog test after grouped runtime builds` step에는
actual compiler-artifact에서 target.name/kind/profile.test/executable을 검사하고 단일 경로를 요구하는 jq 패턴이 있다.
현재 bin-actions의 jq 설치 여부를 이 source 조사로 확인한 것은 아니다. 새 helper/API/크레이트를 이 범위에서 만들지 않았다.

## guard와 receipt 경계

- 각 test의 첫 guard가 두 retired path를 root FD 기준 `statat(SYMLINK_NOFOLLOW)`로 확인한다. regular file/directory/broken link 모두 부재가 아니다. 부모 경로의 symlink/비directory/permission 오류도 부재로 바꾸지 않는다. 실제로 빠진 부모 directory는 부재다.
- 읽는 경로의 모든 component를 `openat`의 `NOFOLLOW|NONBLOCK`으로 열고 directory component에는 `DIRECTORY`를 요구한다. regular file임을 fstat로 확인한 뒤 bounded SHA read를 한다. context/source/CLI/test FD를 test가 끝날 때까지 유지한다.
- context는64KiB, source file은각8MiB/합계64MiB, ELF는각512MiB가 상한이다. source roster는최대256entries/depth16이다. SHA는64KiB buffer로 streaming하여 ELF 크기만큼 RAM을 할당하지 않는다. data directory FD도 유지하고 재귀 roster·directory identity를 전후 비교한다.
- identity는 device/inode/mode/uid/gid/length/mtime+ns/ctime+ns를 보존한다(atime 제외). 종료 때 열린 FD를 다시 hash하고 fresh NOFOLLOW path의 identity도 비교해 rename/교체/동일 길이 overwrite를 거부한다. 부모 source tar/SDK seal을 대체하거나 지속 감시를 주장하지 않는다.
- 정상 Drop에서 body 이후 재검증을 통과한 경우에만 `<test-name>.json`과 `CHANGED_FILES_CALLER_ABSENCE <JSON>`을 쓴다. load가 완성되지 않은 guard와 unwind 중 Drop은 PASS receipt를 쓰지 않는다. receipt write/sync 실패도 test 실패다.

receipt schema는 `openpilot.changed-files-caller-absence.v1`이며 test/source/context pin, 정확한 retired_paths2개,
source/data roster/held files 전후 일치, 각 held file의 전체 SHA와 identity를 기록한다.
`fresh_original_executions:0`, `historical_admission:false`를 유지한다. receipt만으로 historical data admission이나 #523/F05 전체 완료를 주장하지 않는다.

CI는 다음5개 actual test의 PASS/실행 수와5개 정상 receipt를 함께 요구해야 한다. ignored/filtered/선택0 또는 test 실패를 receipt만으로 수용하지 않는다.

```text
maintained_caller_and_native_initial_push_regression_are_mandatory
initial_push_checks_earlier_commits
changed_current_worktree_bytes_are_checked_after_head_selection
whitespace_precedes_syntax_rejection
absence::absence_controls_reject_restored_paths_and_held_mutation
```

다섯 번째 test는 private 임시 root에서 두 경로 각각 file/directory/broken link 복원을 거부하는지,
같은 길이의 held-file 변조와 동일 bytes의 inode 교체, data roster 추가와 실제 directory symlink를 거부하는지 확인한다.
정상 입력이 먼저 허용됨을 확인해 무조건 Err인 구현이 통과하지 않게 한다.
별도 nested `panic-control`은 의도한 unwind payload까지 확인하고 PASS receipt가 생기지 않아야 한다. 여섯 번째 test나 성공 receipt가 아니다.
이 control의 의도적 caught panic을 기존4개 body의 panic 허용으로 해석하면 안 된다.

## 기존14 proof: immutable manifest 접근과 데이터 추출

독립 감사 상세는 ignored
[changed-files-acceptance-audit.md](/home/fma1/.codex/worktrees/python-rust-523/openpilot-rust/.analysis/scratch/2026-10-24-python-rust-523/changed-files-acceptance-audit.md)에 있다.
72 raw/12 Git/Unicode와 codec/lifecycle의 actual run/job/source/ZIP/receipt pin, retained native test11개의 동일 bytes,
caller4 actual 결과를 확인했다. 기존 input/기대값을 새 candidate 출력에서 만들거나 원본을 다시 실행할 필요는 현재 근거상 없다.

재사용의 정본은 현재 workflow가 아니라 다음 exact Git object다.

```text
commit: ab9fef829e8131c6d960ebf46e63c4ab538dbf23
path: .github/workflows/changed-files-caller-296-once.yml
git blob: b3721c5c963907a0acd6fe3170c8bf5baf4a66b6
raw bytes: 82705
raw SHA256: 6cd5f05d274890fd8cdbf9e38adb2bfd87828ce6cfbd4ac8e48858ea139615d2
```

[고정 recipe raw](https://raw.githubusercontent.com/bin9208/openpilot-rust/ab9fef829e8131c6d960ebf46e63c4ab538dbf23/.github/workflows/changed-files-caller-296-once.yml),
[고정 recipe source](https://github.com/bin9208/openpilot-rust/blob/ab9fef829e8131c6d960ebf46e63c4ab538dbf23/.github/workflows/changed-files-caller-296-once.yml).
로컬 접근은 `git show ab9fef829e8131c6d960ebf46e63c4ab538dbf23:.github/workflows/changed-files-caller-296-once.yml`다.
Git object가 없으면 `gh api repos/bin9208/openpilot-rust/git/blobs/b3721c5c963907a0acd6fe3170c8bf5baf4a66b6`의 base64 content를 **데이터로만** 읽고 raw size/SHA/Git blob identity를 검증한다.
raw recipe는 JSON으로 파싱할 수 있다. workflow `run` 문자열을 shell로 실행하지 않는다.

이번 scope는 scratch 파일 추가를 포함하지 않으며 현재 작업/main checkout의 scratch 검색에서 decoded 두 파일은 찾지 못했다.
부모가 필요할 때 아래 위치에 **검증된 데이터만** materialize하면 된다. 기존 같은 이름의 파일을 발견하면 먼저 full SHA를 확인한다.

```text
.analysis/scratch/2026-10-24-python-rust-523/changed-files-reuse/accepted-gates.json
.analysis/scratch/2026-10-24-python-rust-523/changed-files-reuse/caller_gate.py
```

| decoded member | 추출할 step 이름 | bytes | 전체 SHA256 |
| --- | --- | ---: | --- |
| `accepted-gates.json` | `Stage sealed chunk accepted-gates.json 0`, `1`, `2`의 base64 문자열을 순서대로 먼저 연결 | 58790 | `52e8d27ba4edb8c23fbd73ea17c13a1f8e307214c86ba6b17d97713849571955` |
| `caller_gate.py` | `Stage sealed caller_gate.py` 한 step | 22602 | `a18b1bc9f3aa64cf638e535e87077dbd93221adc92ad9ad79dfab35ac21227bf` |

각 선택 step의 `run`에서 exact `printf '%s' '<base64>'` literal만 추출한다. chunk 개수/이름/순서를 확인한 뒤
`base64.b64decode(..., validate=True)`와 decoded 기대 길이+1로 제한한 gzip decompression을 적용한다. EOF/unused tail/기대 길이/full SHA를 확인한다.
`caller_gate.py`는 **소스 참고용**이다. import/exec/subprocess 실행하지 않는다. 현재 source에는 그대로 실행해 통과할 수 없는 strict pin이 있다.

`accepted-gates.json`의 `gates.codec.proofs`13개와 `gates.lifecycle.proofs`1개를 읽어 필요한 원본 data member/receipt만 선택해 재사용한다.
각 record에는 `run_id`, `artifact_id`, `source_sha`, `workflow_sha`, `archive_bytes`, `archive_sha256`, `archive_members`, `receipt_member`, `receipt_sha256`가 있다.
scope는 initial-subset/focused-registry/japanese/idna/payloads/hz/korean/gb/big5/jp2/provider-completion/cp949-original/cp949-native/lifecycle다.
부모 workflow는 다운로드 전에 API에서 소속 run·exact workflow SHA·job 결과·크기/digest를 확인하고, 다운로드 길이/full ZIP SHA,
중복·escape·link 없는 필요한 member, receipt SHA/passed/source와 source.txt binding을 검증해야 한다. archive 안의 code를 실행하지 않는다.
lifecycle은 ZIP 내부 `evidence.tar.gz`의 GNU tar layout이므로 flat ZIP으로 추정하면 안 된다.

가장 작은 독립 원본72/12/Unicode capture는
[run39798239318/artifact12434037946](https://github.com/bin9208/openpilot-rust/actions/runs/39798239318/artifacts/12434037946),
19,720B/SHA256 `123620c0e4d08b9ed368867f6d2399989e167cbc66d548ffd30efecb0efce234`다.
source `12af083b2e31bbb73cb8f73a9822e1c92d3ab8ea`, workflow `5928aaf24b644abeb5373b1b689913c3e7e19d66`다.
syntax/git/unicode member pins는 retained `changed_files_check.rs`에 고정돼 있다. 원본 Python의 version text를 full historical provider identity로 확대하지 않는다.

## 재사용 시 보존할 한계와 남은 actual CI

옛 `reviewed_native_inputs_sha256`59개 중 현재 실제 파일58개는56개 동일/lock·lib2개 변경이고 Cargo semantic1개도 변경됐다.
changed-files 구현·data/license/provenance와 retained test11개의 bytes는 동일하지만 옛 strict manifest 자체를 현재 값으로 덮어쓰면 안 된다.
current overlay에서 Cargo/source/실제 ELF/provider 관계를 별도로 연결한다. 이 candidate는 그 overlay나14-proof admission을 새로 성공시킨 것이 아니다.
기존 dedicated caller recipe의 run39884478095는 metadata mapped DSO 검증에서,39892674217는 checkout 전 준비에서 실패했으므로 그 recipe의 `accepted:true`만으로 actual 성공을 만들지 않는다.

provider-completion/CP949 original의 `historical_extension_identity_established:false`는 그대로다.
새 provider 관측과 과거540개 canonical byte 일치는 과거 `_codecs_cn/_tw/_hk/_kr`의 library identity를 소급 증명하지 않는다.
GNU CP949의8개 provider path/SHA도 당시 CI의 실제 값이며 다음 job의 provider가 같다고 추정하거나 target을 강제하지 않는다.
과거 accepted codec/lifecycle 전체의 반복 실행·새 원본 재캡처 대신 current source/ELF/provider 연결과 정확한 두 경로 부재5-test의 actual 결과가 다음 한정 경계다.

아직5-test actual 결과, 새 strict Clippy, fresh context/ELF/provider·full source tar·SDK 전후 seal이 없다.
로컬 rustfmt/diff는 컴파일·동작 수용이 아니다. 부모가 exact-source CI를 수용한 후에만 정확한2파일 inventory credit을 판단한다.

## 로컬 확인 기록

Rust1.94.0의 rustfmt `--check`는 두 Rust 파일에 대해 성공했다. tracked diff와 두 신규 파일의 whitespace diff 오류는 없다.
source 비교에서 module 등록과 네 guard 선언만 제거하면 caller 파일 전체 bytes가 a134 원문과 같았다.
`#[test]` source 선언은4+1=5이며 실행 수를 관측한 것이 아니다. 삭제2경로는 현재 filesystem에서 `lexists:false`다.
현재 source roster는고정12+data52 regular files, 합계2,089,345B로 source 상한64MiB 이내다.
CLI/test ELF의 실제 크기와 SHA는 로컬 build 없이 확정하지 않았으며 부모 Actions가 현재 compiler-artifact로 기록해야 한다.


부모의 dedicated workflow는 data admission 동안 봉인한 같은 GNU provider8개를 fresh caller/Clippy
뒤에 다시 확인한다. source SDK/reader 프로그램 pin, 각 provider의 path/FD identity와 전체SHA가
before 및 after-caller에서 같아야 하며, 누락이나 변경은 별도 최종 단계 실패로 job 수용을 막는다.
원본 codec matrix를 다시 실행하거나 과거 extension identity를 소급 확정하지 않는다.
