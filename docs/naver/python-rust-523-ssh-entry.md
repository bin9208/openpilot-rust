# #523 SSH 실제 native entry 후보

2026-10-25. 기존 SSH frozen39(plan26/parser13)는 수정하지 않고 실제 entry의 작은 추가 경계를 구현한다. 이 문서는 구현 전에 계획을 먼저 작성했다. 소유는 아래 세 파일뿐이며 Cargo/workflow 등록은 부모가 담당한다. 현재 기능·컴파일·Clippy·원본 실행·Actions 실행은 **UNRUN**이다. `tools/scripts/ssh.py`의 존재/부재와 무관한 native test이며 원본 삭제/retirement credit은0이다. phase1 원본 존재/SHA 검사는 부모 workflow의 별도 admission이다.

## 파일과 구현 순서

1. `rust/crates/dev-tools/tests/developer_ssh_entry.rs`: 실제 Cargo CLI/현재 test ELF/recorder source·ELF의 FD와 SHA를 유지한다. private HOME/nonce/loopback HTTP 및 owned process 경계를 만든 다음 아래 authored assertion을 검사한다. parser/marker/held-file 거부 통제를 먼저 작성하고 실제 cohort를 연결한다.
2. `rust/crates/dev-tools/tests/fixtures/developer-ssh-entry/exec_recorder.rs`: 부모가 실제 SDK rustc로 새로 빌드하는 std-only 기록 전용 ELF다. argv 원문 bytes·nonce·PID/PPID/PGID/session/startticks·실제 실행경로/CWD만 bounded binary record에 쓴다. SSH/ProxyCommand 해석, 네트워크, 자손 생성은 하지 않는다.
3. 이 기록에 exact selector/환경/검증 범위와 source freeze SHA를 인계한다. 기존 production/frozen/39 criteria와 원문 raw는 수정·재capture하지 않는다.

## 테스트 계획

`actual_native_ssh_entry_boundaries`는 명시 실행하는 ignored selector다. 별도 test worker 프로세스 안에서 다음 7회를 실행한다.

| 사례 | 기대 |
|---|---|
| alias 성공 | prefix가 붙은 private auth.json의 inert JWT로 loopback GET1, 정확한 ssh argv/connecting stdout, recorder1 |
| alias 중복 | GET1/정확한 후보 순서·stderr/exit1, recorder0 |
| debug·host·port·공백 key | GET1, 정수 정규화 및 실제 argv/기존 literal debug 출력, recorder1 |
| HTTP401 | GET1/기존 Unauthorized 문구/exit1, recorder0 |
| help | auth.json을 writer 없는 private FIFO로 둬도 exit0, HTTP0/recorder0 |
| 잘못된 port | 같은 FIFO에서 exit2/invalid-int 진단, HTTP0/recorder0 |
| 인자 누락 | 같은 FIFO에서 exit2/required-positional 진단, HTTP0/recorder0 |

FIFO 사례는 token read를 성공으로 위조하지 않는다. blocking token read가 들어가면 부모 deadline 내 정상 종료할 수 없어 실패한다. 원래 auth semantics를 바꾸는 seam은 추가하지 않는다. 실제 `/TICI`가 있으면 이 host-only 검사는 fail하며 실제 device 경로를 건드리지 않는다.

`actual_public_op_ssh_entry`는 별도 ignored selector다. 기존 `/bin/bash tools/op.sh --no-verify ssh ...`→`rust/tools/run-developer-tool`를 그대로 실행한다. `--no-verify`는 기존 공개 옵션이며 LFS/venv 설치 검사를 대체 성공으로 세지 않는다. SDK가 이미 준비됐음을 같은 whitelist 환경의 `rustup run 1.94.0 rustc -vV`로 먼저 확인한다. public helper의 실제 Cargo build/3072MiB guard를 우회하거나 fake Git/rustup/cargo로 대체하지 않는다. 부모가 동일 환경으로 미리 만든 public host ELF를 before/after hold하며 교체는 거부한다. Cargo/전후 source·SDK/actual exec trace·성장 admission은 부모가 별도 연결한다.

두 기본 negative selector는 fixture decoder의 nonce/truncation/trailing bytes·marker 추가/alias 및 held source의 rewrite/replace/hardlink 변경을 거부하도록 작성했다. 이 control도 아직 실행하지 않았다. author-synthetic control을 actual CLI 수용으로 세지 않는다.

## child/파일 경계

- 외부 supervisor는 기존 NativeRunner의 owned PGID/timeout/capture를 재사용한다. actual cohort는 mutex로 직렬화한다. 바깥 test owner는 기존 자식이 없음을 먼저 확인한 뒤 한시적 subreaper가 되고, worker도 별도 subreaper가 된다. 내부 command는 worker PGID를 상속한다. worker timeout 시에도 바깥 owner가 채택 자식을 ECHILD까지 확인한 뒤 기존 설정을 복원한다. PID 회수는 각 owner의 `/proc/self/task/*/children`에 한정하며 다른 CI job의 프로세스를 대상으로 삼지 않는다. 외부 runner가 supervisor까지 강제 종료한 경우의 회수·provider 후속 관측은 부모 workflow 경계이고, 관찰 없이 전부 회수됐다고 기록하지 않는다.
- CLI의 NativeTransport는 `http://127.0.0.1:ephemeral/v1/me/devices`만 호출한다. 응답에는 redirect가 없다. recorder 하나만 private PATH의 `ssh`에 복사한다. actual ssh/사용자 계정/디바이스 접속은0이다. proxy 환경과 실제 HOME/credential은 자식에게 상속하지 않는다.
- 새 private case마다 HOME/.commaPREFIX/auth.json 또는 FIFO, key sentinel, 정확 marker roster와 create_new stdout/stderr/proof를 둔다. marker에는 nonce·원문 argv·실제 process 정보가 있어 다른 case/partial/추가 출력을 수용하지 않는다. raw byte 비교를 mask/normalization으로 대체하지 않는다.
- readonly NOFOLLOW|NONBLOCK FD, size/owner/type, FD/name identity·전체 SHA와 EOF를 전후 확인한다. Cargo가 만든 ELF의 기존 hardlink 수는 보존하되 변경은 거부한다. private recorder 사본/marker는 single-link를 요구한다. 현재 source 전체/SDK/provider DSO closure를 일부 hold로 대체했다고 주장하지 않는다.

## 부모 등록/준비 계약

```toml
[[test]]
name = "developer_ssh_entry"
required-features = ["developer-ssh-native"]
```

부모가 compiled test 실행 전에 아래 actual 입력을 준비한다.

- `GITHUB_ACTIONS=true`, `GITHUB_REPOSITORY=bin9208/openpilot-rust`, `RUNNER_ENVIRONMENT=github-hosted`, `RUNNER_OS=Linux`, `RUNNER_ARCH=X64`, `GITHUB_SHA`: 실제 runner 값이다.
- `CI296_SOURCE`: 실제 compile-bound repository root. `CI_JOB_TEMP`: canonical/owned0700 private job. `DEVELOPER_SSH_ENTRY_PROOF=$CI_JOB_TEMP/proof/developer-ssh-entry`: 부모가 만든 owned0700 root이며 각 native/public child는 처음에는 없어야 한다.
- `DEVELOPER_SSH_ENTRY_RECORDER`: 부모가 현재 fixture source를 실제 SDK rustc로 빌드한, job/build 아래 canonical regular ELF. `DEVELOPER_SSH_ENTRY_RECORDER_SHA256`: 그 실제 출력의 전체 SHA256. artifact ELF/과거 ELF/일반 ssh로 대체하지 않는다.
- public 전용: `CI_PREPARED_RUST_ROOT=$CI_JOB_TEMP/rustup-home/toolchains/1.94.0-x86_64-unknown-linux-gnu`, `CI_PREPARED_RUSTUP_ROOT=$CI_JOB_TEMP/rustup-home`, `CI_PREPARED_RUSTUP_BIN=$CI_JOB_TEMP/cargo-home/bin/rustup`, `CARGO_HOME=$CI_JOB_TEMP/build/cargo-home`, `CARGO_TARGET_DIR=$CI_JOB_TEMP/build/target`다. public ELF는 `$CARGO_TARGET_DIR/x86_64-unknown-linux-gnu/debug/developer-ssh`를 미리 같은 helper build 환경으로 준비한다. child에는 `SHELL=/bin/bash`, `RUSTUP_AUTO_INSTALL=0`, absolute `RUSTC`/`RUSTDOC`, `CARGO_NET_OFFLINE=true`, `GIT_OPTIONAL_LOCKS=0`를 명시한다. 임의 SDK/PATH/Git/FD override 입구는 만들지 않는다.

recorder 컴파일과 Cargo build/test는 GitHub Actions에서만 수행한다. 기본 compiler 예시는 actual admitted absolute rustc로 다음을 실행하는 것이다.

```sh
"$CI_PREPARED_RUST_ROOT/bin/rustc" --edition=2021 -C debuginfo=0 -C opt-level=1 \
  "$CI296_SOURCE/rust/crates/dev-tools/tests/fixtures/developer-ssh-entry/exec_recorder.rs" \
  -o "$CI_JOB_TEMP/build/developer-ssh-entry-recorder"
```

## 동결 구현과 Actions selector

테스트 함수는 **기본2 + ignored2 = 총4개**다. 로컬 실행 결과가 아닌 소스상 개수다. 부모가 위 등록과 actual compiler/provider/source admission을 연결한 뒤 Actions에서만 실행한다.

```sh
"$CI_PREPARED_RUST_ROOT/bin/cargo" test --manifest-path "$CI296_SOURCE/rust/Cargo.toml" \
  -p openpilot-dev-tools --features developer-ssh-native --test developer_ssh_entry \
  --locked -- --nocapture --test-threads=1
"$CI_PREPARED_RUST_ROOT/bin/cargo" test --manifest-path "$CI296_SOURCE/rust/Cargo.toml" \
  -p openpilot-dev-tools --features developer-ssh-native --test developer_ssh_entry \
  --locked actual_native_ssh_entry_boundaries -- --ignored --exact --nocapture --test-threads=1
"$CI_PREPARED_RUST_ROOT/bin/cargo" test --manifest-path "$CI296_SOURCE/rust/Cargo.toml" \
  -p openpilot-dev-tools --features developer-ssh-native --test developer_ssh_entry \
  --locked actual_public_op_ssh_entry -- --ignored --exact --nocapture --test-threads=1
```

첫 명령은 기본2 PASS/ignored2, 뒤 명령은 각각 선택1/ignored0을 확인해야 한다. 직접 compiled test ELF를 실행하는 동일 exact selector도 가능하다. 기본 selector는 다음 두 개다.

- `entry_held_files_reject_rewrite_replacement_and_new_alias`
- `entry_recorder_rejects_bad_nonce_truncation_trailing_and_marker_roster`

부모가 public prebuild를 준비할 때 unchanged helper의 `CARGO_INCREMENTAL=0`, `CARGO_BUILD_JOBS=2`, `CARGO_PROFILE_DEV_DEBUG=0`, explicit native host triple 및 제거되는 compiler flag 환경까지 맞춰야 한다. test build 후 public ELF를 준비하고 실제 case 시작 전에 hold한다. public helper가 그 ELF를 다시 빌드해 identity/hash를 바꾸면 실패하며 새 ELF를 재수용하지 않는다. 따라서 최초 실제 Actions는 helper와 prebuild의 fingerprint 호환성도 확인해야 한다. 준비 환경의 private HOME/PATH는 proof의 `public/alias/home` 및 `public/bin`으로 결정되며 원래 사용자 HOME을 이용하지 않는다.

## 출력·상한 및 수용 한계

- proof는 `native/`와 `public/` 두 absent 하위 root를 각각 create_new 방식으로 사용한다. `held.before.json`/`held.after.json`은 초기14개(owner1/source9/current test·CLI2/recorder 원본·사본2) 동일 FD·SHA 검증 뒤 기록한다. 검증 실패 시 after를 과거 값으로 PASS 처리하지 않고 실패 객체로 쓴다. `per-use-held.json`은 auth/key 및 public SDK4/rustup/public ELF/bash 등 사용 전 admission과 최종 검증 상태를 별도 기록한다. SDK 전체/DSO 전체 closure를 이 부분 목록으로 대체하지 않는다.
- case별 stdout/stderr/status, HTTP 원문 hex와 headers, raw `exec.bin`, authored criteria 검증 뒤 `case.json`을 보존한다. `summary.json`의 complete는 case·자손 회수·held 최종 검증의 conjunction이다. `supervisor.json`은 바깥 owner가 관측한 worker 상태와 adopted child 회수 결과를 별도 보존한다. proof를 삭제하지 않는다.
- NativeRunner outer는 native180초/public720초, 각 actual CLI는15초/public helper600초, SDK probe30초다. 자손 회수는 owner별10초, 자식256/task128 상한이며 시간 내 ECHILD를 확인하지 못하면 실패한다. outer stdout/stderr는 각각128KiB capture cap, 내부 CLI raw log는 native64KiB/public4MiB를 약2ms 간격으로 확인한다. 후자는 sampling과 종료 후 bounded read이며 filesystem hard quota가 아니다. HTTP request header는16KiB/64개, header 전체3초, response 최대4회다. recorder는64KiB이며 단일 raw argv field는8KiB/argc32 상한이다.
- native7은 HTTP4/recorder2, public1은 HTTP1/recorder1을 각각 요구한다. 실행 provider는 무동작 recorder여서 실제 SSH/ProxyCommand 해석·원격 계정·디바이스 접속은 없다. parser FIFO3는 auth content read가 발생하면 deadline 안에 정상 종료할 수 없다는 통제이며 syscall 전체의 file-open0 주장이 아니다.
- 새 fixture는 Rust source뿐이다. Python 본문/encoded source/과거 expected를 새로 생성하지 않는다. 기존 frozen39·HTTP loopback·private Params27은 별도 증거다. 현재 source만 작성했으며 **compile/test/Clippy/actual entry 모두 UNRUN**, 삭제/retirement credit0이다. 로컬 확인은 Rust2021 rustfmt syntax/format과 새 파일 diff 검토까지만 수행한다. 최종 파일 SHA는 인계 메시지에 기록한다.

## 2026-10-25 Actions 연결 후보 후속

사용자 승인으로 `tools/ci/developer_ssh_entry_523.sh`를 추가하고 명시 Cargo test1, Focus affected SSH27 다음 준비·실행 step1, 기존 strict Clippy의 `--test developer_ssh_entry`를 연결했다. 위 초기 기록의 Cargo/workflow 미등록 상태는 이 소스 후보에서 해소한다. **컴파일·selector4·Clippy·실제 provider/자원 검증은 여전히 UNRUN**이다. 동결 test/recorder, 기존 39/27/scalar/context와 production/helper 본문은 수정하지 않는다. 부모는 reuse allowlist/literal pins/MANIFEST Cargo·outside digest 및 ledger 등록을 별도로 마무리한다.

- 준비는 actual hosted Linux X64/repository/current HEAD/private owner와 부모의 private SDK 환경/4-tool pin을 검사한다. 같은 SDK를 재설치하지 않는다. 현재 job의 registry/git 캐시를 metadata roster+전체 regular-file SHA로 전후 봉인하고 별도 `$CI_JOB_TEMP/build/cargo-home`으로 복사한다. 현재 registry의 symlink/특수 파일, 모호한 Cargo artifact, 잘못된 SDK/경로를 우회하지 않는다. 새 Python/크레이트/일반 환경 override는 없다. Focus에 이미 준비되는 Bash/jq/strace/readelf/GNU 도구를 사용한다.
- public prebuild와 test no-run은 절대 SDK, `developer-ssh-native`, explicit `x86_64-unknown-linux-gnu`, 새 Cargo home/target, env-i의 compiler 관련 동일 설정을 사용한다. public case가 나중에 생성할 HOME/PATH 문자열만 먼저 지정하고 `public/` 자체는 만들지 않는다. registry는 private copy에서 offline으로 사용한다. Cargo JSONL은 전체 eager parse/성공 EOF1 및 정확 package/manifest/name/kind/src/profile/executable unique로 선택한다. recorder는 동결 source를 현재 rustc로 새로 빌드한다. public helper의 실제 Cargo build는 생략하지 않으며 fingerprint 호환과 immutable ELF 유지 여부는 첫 Actions가 판정한다.
- 새 작업의 상한은 **target6GiB/Cargo home2GiB/build 전체8GiB/proof512MiB**다. 복사·빌드 전 실제 각 filesystem의 수요를 합산하여 free를 admission한다. 종료 증거512MiB와 inflight 여유1GiB를 growth와 별도로 둔다. 새 Cargo 상태 파일을 위해 source cache 사본은2016MiB 이하로 제한하여32MiB를 남긴다. 실행 중 약2초 간격의 du/df 감시 및 종료 후 검사를 하고 unknown/resource 실패는 sticky failure다. child 단일 파일 fsize512MiB를 사용한다. 이는 sampling/postcheck이며 여러 PID trace의 동시 write를 제한하는 hard disk quota나 측정 peak 보장이 아니다. 기존 Focus4GiB 추정으로 새 작업의 fit을 주장하지 않는다.
- 준비 명령은 registry copy180초/public build900초/test build900초/recorder build120초다. selector2 regular은 각30초, 실제 fixture 자체의 native180/public720초 및 내부15/600/30초는 그대로다. driver outer는 회수·receipt용30초를 더한210/750초이며 Focus의 총 driver wrapper는2700초+강제 종료30초다. 명령별 최대치를 모두 소진할 권리를 총 wrapper가 보장하지 않으며 총구간 초과도 실패다. 기존 job60분은 확장하지 않는다.
- 각 phase는 새로운 PGID handshake/current PPID·startticks·PGID·SID 검증으로 소유한다. 자원 실패나 신호에서 현재 소유성이 확인되는 live group에만 KILL하며 direct leader를 wait한다. 정상 wait 뒤 과거 PGID에 신호하지 않는다. group 부재를 별도 확인한다. runtime strace는 `--kill-on-exit`이며 tracer 앞 env-i가 인증 환경도 제거한다. worker 자체의 subreaper/ECHILD 검사는 별도 frozen fixture receipt다. driver의 group 검사만으로 모든 adopted child 회수를 주장하지 않는다.
- SDK/도구/현재 ELF 및 recursive NEEDED DSO를 readonly FD+name identity/SHA로 전후 묶는다. runtime raw exec/mmap trace를 보존하고 성공 exec 및 파일 기반 실행 mapping을 held provider와 대조한다. private recorder 사본의 실행 전 hold는 fixture가 수행하며 driver는 동일 SHA를 후속 대조한다. 일반 syscall full decoder/closed trace/전체 build dependency closure 수용은 아니다. 부모의 whole source/index 및 whole SDK always seal은 그대로 필수다.
- 기본 negative2와 ignored actual2를 exact selector로 실행하고 선택1/실패0/ignored0을 확인한다. actual은 nested worker 때문에 1-pass summary가 두 번이며 이 구조와 정확 completion marker를 함께 확인한다. summary source/recorder/정확 native7·public1 case/HTTP4·1/exec2·1, 각 case.json, supervisor 성공·회수, held 초기14의 byte 동일 및 per-use verified를 모두 요구한 뒤에만 `driver/outcome.json`의 candidate를 허용한다.
- 각 JSON receipt는 parse 전에 regular/single-link/owner 및8MiB 상한을 확인하여 hold한다. Cargo JSONL은64MiB 상한, PID별 raw trace는128MiB 사후 상한이다. parser FIFO3는 통제 입력으로 계속 남으며 `driver/case-path-types.tsv`에 type/path를 보존한다. upload가 FIFO 원문을 지원하지 않는다면 그 warning을 regular 증거 파일 누락과 구분해야 한다. 아직 실제 upload 결과를 예측 성공으로 기록하지 않는다.
- 실패에서도 source registry/held 전후 검증과 outcome을 독립 수행하고 최초 command failure를 보존한다. proof는 지우지 않는다. 부모의 항상 실행되는 source/SDK final → artifact upload → 성공 seal에 묶인 소유 cleanup 순서는 변경하지 않는다. 현재 `tools/scripts/ssh.py`는 초기 존재+고정 SHA만 확인하며 실행/삭제/absence/retirement credit은0이다. 실제 원격 SSH, TLS/ProxyCommand, 디바이스 수용도 아니다.

계획/독립 소스 검토는 ignored scratch `developer-ssh-entry-implementation-plan.md`, `developer-ssh-entry-independent-review.md`에 보존한다. 최초 exact-current Actions의 주요 미확인은 fresh build/Clippy, public warm fingerprint, registry copy2GiB와 전체 작업 fit, raw provider 관측, 실패 회수/봉인이다. 후보 source freeze 뒤 독립 검토 전까지 수용으로 기록하지 않는다.

## fd 실제 첫 실행과 초기 held 목록 중복 수정 계획

[40153172247/job121048793843](https://github.com/bin9208/openpilot-rust/actions/runs/40153172247/job/121048793843), source `fd3f3d02293ff4ce71b40733fa7c46c9ae9e7d1c`에서 기본 음성2·native7·public1은 성공했다. public 실제 helper도 기존 ELF를 교체하지 않았고 두 supervisor의 회수와 최종 held 검증은 성공했다. 그러나 driver line385가 public 초기 held 목록14개와 종료 목록15개의 불일치를 거부해 전체 검사는 실패했다. artifact12583960339는41,122,410B/SHA256 `a21f7b61eb8026e8d9fa73164e3a45b462a51b9a1f0d0dbc4d4b91b478da1ef9`, 전수332CRC를 확인했다.

종료 목록의15번째 항목은 새로운 경로가 아니라 public 실행 직전에 다시 hold한 동일 `developer-ssh` ELF다. 초기14개의 모든 행은 byte 동일하며, 중복된2행도 identity·SHA를 포함해 같다. fixture가 초기 경로 set으로 전체 누적 held 목록을 필터링해, 같은 경로의 후속 hold도 초기 snapshot에 포함한 것이 원인이다.

최소 수정은 초기 held 개수를 저장하고 종료 검증 성공 뒤 누적 목록의 초기 prefix만 기록하는 것이다. 목록은 append만 하며 전체 누적 FD 검증과 per-use 보고는 그대로 유지한다. driver의 정확14개·unique14·전후 byte 일치, 동결 case/기대값/marker·회수·자원 기준은 바꾸지 않는다. 다음 Actions에서 기존 두 음성 및 실제 두 selector, driver의 provider trace와 Clippy까지 다시 확인한다. 이 수정 후보의 기능은 아직 UNRUN이며 원본 실행/삭제·credit은0이다.

초기 snapshot 중복의 개별 결함은 [#564](https://github.com/bin9208/openpilot-rust/issues/564)에서 추적한다. 수정 source `c00c7a11fbee357cbfb39bee45125d73596bf56c`의 [Focus40154418302](https://github.com/bin9208/openpilot-rust/actions/runs/40154418302)는 실행 중이며 새 성공은 아직 수용하지 않았다.


## c00 실제 수용과 #564 개별 종료

[Focus40154418302/job121052614023](https://github.com/bin9208/openpilot-rust/actions/runs/40154418302/job/121052614023), source `c00c7a11fbee357cbfb39bee45125d73596bf56c`가 최종 성공했다. artifact12584392161=41,123,890B/SHA256 `ee697e2a5b4a80acd1f661235e98eba4d9eadfb3c4efc0d28ca17518985b5fe7`,336CRC/해제97,856,834B를 전수 감사했다. 기본음성2·native7·public1과 두 mode의 초기14/unique14/전후raw동일, supervisor worker0/all_owned_children_reaped=true를 확인했다. driver116경로의 종료232행도 전부같고 명시 불변21쌍이 같았다.

driver outcome은 entry4-checked/command0/final0/entry4_candidate=true다. 실제 공개helper warm ELF/exec·mmap 점검, strictClippy, source/index/wholeSDK 마지막봉인·upload/소유cleanup이 모두 성공해 #564를 개별 종료했다. 실제 원격SSH/ProxyCommand·기기·closed trace·전체builddependencyclosure·원본삭제/부재/credit은0이다. 위 초기 UNRUN 문단은 작성 시점의 기록이며 이 실행의 정확 범위만 새로 수용한다. 원본1 삭제는 별도 absent phase와 source-sensitive 재검증 뒤 판정한다.

## #567 Focus593의 mutable target 집계 경합과 최소 재관측 후보

source `593186e6a66b036c8817f0184723f839867d6856`의
[Focus40161286994](https://github.com/bin9208/openpilot-rust/actions/runs/40161286994)은
public-build 단계에서 실패했다. 부모가 감사한 artifact12587125165는38,684,242B/SHA256
`a481a8524f658bd1e271c59ab74671c0207c6072a7f3fb9358e1fc14a3189181`,143CRC/해제89,013,620B다.
`du`가 private target의 `.tmpaWru3X.temp-archive`와 dev-tools의 `-cgu.01.rcgu.o`를
순회하다 ENOENT를 출력했고, watcher가 이를 admission 실패로 받아 소유 PGID를 종료했다.
public-build의 command137/watcher1, build-finished0이며 actual entry4는 도달하지 않았다.
마지막 정상 target836,358,144B, 종료 뒤893,120,512B였고 cap 초과·Rust compiler 오류 근거는 없다.
기존 기록에는 실패한 du의 원래 exit code와 target/whole-build scan 구분이 없어 이를 추정 확정하지 않는다.
개별 관측·집계 경합은 [#567](https://github.com/bin9208/openpilot-rust/issues/567)에서 추적한다.
held99개와 registry/source/index/SDK 전후는 같지만 sticky resource-failure가 남아 전체 실패를 유지한다.

후보는 기존 helper 안에서 `du -sx --block-size=1`을 그대로 사용하여 allocated/xdev/hardlink
집계 의미를 보존한다. target6GiB/Cargo home2GiB/build8GiB/proof512MiB와 filesystem별 reserve,
2초 sampling, build900초·driver2700초 및 fixture/trace/회수·closure·cleanup 기준은 바꾸지 않는다.
target와 whole-build scan에서만 code1이고 **모든** 오류가 정확한 target 하위 ENOENT,
안전한 상대경로이며 현재 파일·symlink가 모두 없을 때 총3회까지 재관측한다.
clean code0/빈 stderr/정확한 단일 LF 전체 결과가 있어야 값을 채택한다.
실패 scan의 부분 합이라도 cap 초과는 overflow 없는 비교 뒤 즉시 sticky 실패다.
permission/외부 경로/혼합 오류/현재 존재·dangling link/기형 raw/다른 종료/3회 소진은 거부한다.
registry/proof/absence journal 오류는 재관측으로 통과시키지 않는다.

각 실제 scan은 소유한 실제 directory/no-symlink root와 dev/ino/mode/uid/gid를 전후 대조하며 mutable
directory의 mtime/nlink를 고정하지 않는다. 고유 private directory에 각 attempt의 stdout/stderr,
phase·admission·root·identity·cap·du exit·판정 metadata와 첫 오류를 남기고 덮어쓰지 않는다.
raw stdout/stderr는 du subprocess에만64KiB file limit을 적용하고 경계 도달도 거부한다.
Cargo/ELF output에 이 작은 상한을 전파하지 않는다. per-file Linux Bash 한도이며
filesystem hard quota나 측정 peak 보장으로 표현하지 않는다.

새 공개 인자/환경 fallback 없이 기존 helper가 provider hold 뒤, compile 전에 authored preflight20개를
실행한다. classifier17개는 clean/target와 whole-build ENOENT/partial-cap/outside/permission/
existing·dangling/traversal/다른 status/overflow/identity/malformed/NUL2/혼합/출력경계를 검사한다.
나머지3개는 private subshell의 함수 mock으로 동일 retry loop를 사용하여
두 ENOENT 뒤3번째 clean,3회 소진,첫 부분 합 초과의 정확한 reason·attempt 수·sticky 상태를 검사한다.
mock은 그 합성 subshell 밖에 존재하지 않으며 실제 provider 선택이나 PATH를 바꾸지 않는다.
`driver/usage-controls.tsv`20행과 `usage-controls.pass`의 synthetic_only marker를 남긴다.
추가 coreutils `ln`도 현재 ELF/DSO hold 대상이며 actual native7/public1·원문 expected는 그대로다.

로컬은 Bash 문법/YAML·AST·source/diff만 확인했다. preflight20·fresh build·원래 entry4와
현재 source/SDK 최종 봉인·upload/cleanup은 **Actions UNRUN**이다.
기존 Focus SSH step에서 실행하므로 새 step/command roster 항목은 없고 helper literal SHA만
`0554a00a43a7a7cccb28ba70456df4dac2244ca1f66fdb65f6edc0abf7a24287`로 갱신했다.
부모의 최종 원장 blob pin/다음 exact-source Actions 감사는 별도이며 이 후보를 과거 성공으로 소급하지 않는다.


## #567 a483 실제 용량 관측 수정 검증

[a483 Focus40166180665/job121087586740](https://github.com/bin9208/openpilot-rust/actions/runs/40166180665/job/121087586740)은 성공했습니다. 정확 source `a4838fc4e7d09499d1dbc50045ec049af995ffbc`, artifact12588363290/41547875B/SHA256 `fdb430be5bf03d6169db5378192b6ae24d23bdfcc98123ac1176bbbeaac42a99`의1697CRC를 부모가 감사했습니다.

- 용량 관측20통제: clean·정확 target ENOENT, partial overcap·permission/outside/현재존재/링크/잘못된 경로·NUL·혼합 오류 거부,3회 후 clean/소진 및 cap 즉시 중단이 통과했습니다.
- 실제 build/entry의9명령이 exit/wait0이고 entry4·native7/public1·독립 회수, SSH27/Unicode7/frozen10과 strict Clippy가 성공했습니다.
- 원본 부재6통제/119관측·최종 closure, source/index/SDK/고정 입력 및 held124경로의 전후 불변을 확인했습니다. 원래 cap/reserve/주기/timeout을 유지합니다.

이 한정 결함의 기능 검증은 성공했습니다. 이슈 본문의 PR source 봉인 조건은 현재 PR343의 해당 검증이 끝난 뒤 확인하므로 아직 열린 상태로 둡니다. 전체 Python 부재/실제 SSH 접속/전체 #523 완료나 추가 retirement credit는 주장하지 않습니다.


## #567의 PR source seal 수용

2026-10-25. source a483 Focus40166180665/job121087586740의 전체 SUCCESS와 usage20/Clippy·
entry4/public1/absence6·최종 봉인 수용에 더해, PR343 workspace40166518729/job121088724945의
source seal SUCCESS를 확인했다. 실제 PR merge `e4f7d4a868f3aaee36a1b485af29923f2e24cee6`와
head `a4838fc4e7d09499d1dbc50045ec049af995ffbc`의 Git tree는 모두
`5de13f49c18e5a551b65199ce791663a21140b71`다. private artifact12589331308은3,597,053B/
SHA256 `dbd08c7a653cbc93c3fb138a3f199c276a173c01e03b7648e494c7ad4b44c8f3`/35CRC이며
source/index 두 전후 쌍 bytes가 같다. 현재 SSH helper도 수용 source와 byte 동일하다.

PR job 자체는 다른 SDK whole-job 기대식 오류로 FAIL이고 cleanup은 skip이었다. 전체 PR/정리
성공으로 올려 쓰지 않는다. #567의 명시 조건인 정확 source helper 회귀·strict Clippy·전체 Focus·
PR source seal이 충족돼 개별 종료한다. wholePythonabsence false/추가 credit0 및 전체 이슈 미완료는 유지한다.
