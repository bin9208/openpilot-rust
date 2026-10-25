# #523 Planner memory wrapper의 실제 native 호출 전환 준비

2026-10-26. 작업 루트는
`/home/fma1/.codex/worktrees/python-rust-523-completion/openpilot-rust`다.
기존 `check-plannerd-ci memory`에 `--asan-library PATH`를 추가했다. 이 옵션이 있으면
새 `planner_ci_controller/memory.rs`가 기존 Python wrapper의 orchestration을 실행한다.
실제 `tools/ci/planner_native_controller_523.sh memory`도 같은 native CLI를 호출한다.
새 bin·feature·Cargo 의존성·공유 lib.rs 등록은 필요하지 않다.

`--asan-library` 없는 memory core는 원래대로 비교 명령 세 개만 실행한다. 기존 Python
wrapper와 테스트의 독립 reference 경로이므로 유지한다. runtime에서 ASan 옵션을 주면
거부하며, native memory 실패 뒤 Python fallback이나 full-runtime receipt 생성은 없다.

## 원본과 유지한 실제 계약

먼저 `rust/tools/check_plannerd_native_memory.py`, 그 unittest5,
`planner_ci_controller`의 CLI/plan/run_checks 및 `qa_process`를 읽었다.
원본 wrapper와 unittest5, 기존 `tests/planner_ci_controller.rs`는 바이트 불변이다.
James 소유 frozen reader와 workflow/contract pin/Cargo/Focus는 수정하지 않았다.

| 경계 | native 구현에서 유지한 내용 |
| --- | --- |
| 입력 | artifact/source/binding/output의 non-strict resolve, ASan/각 trace의 strict canonical path |
| provenance | artifact의 `provenance.json`에서 기존 `sanitizers` truth 검사; generator/source/compiler provenance는 수정하지 않음 |
| wrapper | `wrappers/examples`를 새로 만들고 solver/owner/runtime trace별 `#!/bin/sh`, `exec env … "$@"`, chmod0755; Python shlex와 같은 인용 |
| sanitizer | trace에만 `LD_PRELOAD`, `ASAN_OPTIONS=detect_leaks=1:abort_on_error=1:strict_string_checks=1`, `UBSAN_OPTIONS=halt_on_error=1:print_stacktrace=1` 적용 |
| leaf | 기존 `plan::memory_checks`의 `abi → owner → fault` 및 전체 argv 사용; fault는 `--faults --frames 80` |
| 자원·기한 | helper의 wrapper admission128 MiB, abi128/owner1024/fault1024 MiB, jobs1, host2700초를 그대로 사용 |
| 실패 | 기존 `qa_process`의 owned group 회수·기한을 사용; 첫 실패에서 중단, exit42는42, signal/기타 오류는 CLI1; 이후 leaf와 PASS 없음 |
| 완료 | `comparison/commands.json`의 세 name/순서/returncode0 재확인; 실행물3개 SHA는 실행 전, manifest·ASan SHA는 성공 후 계산 |
| receipt | 기존 status/checks/binary_sha256/artifact_sha256/asan_sha256/scope 필드의 memory 한정 `output/result.json` |

scope 원문은 다음과 같다.

```text
generated solver C is instrumented; allocator leak/ownership checks enabled; Rust and pinned external solver libraries are not instrumented
```

native memory core는 `comparison/result.json`에 full-runtime PASS를 만들지 않는다.
ASan 환경을 Python 비교기에 주입하지 않고 실제 trace wrapper에만 설정한다.
기존 build의 generator wheel/source/compiler/ELF provenance 및 leaf 내부 비교 경로는
그대로 사용한다. host/ARM SDK preflight와 ARM sanitizer skip 조건도 바꾸지 않았다.

## 독립 관측과 새 검사

부모가 감사한 로컬 ZIP
`.analysis/scratch/2026-10-25-523-completion/ci/12609249847.zip`에서 source73의 기존
`planner-native-memory/result.json` 1073 B와 `comparison/commands.json` 2127 B만
읽었다. 세 명령은 abi/owner/fault, returncode0, growth134217728/1073741824/1073741824였다.
receipt는 위 한정 scope와 세 실행물·manifest·ASan SHA를 담았다.
이전 artifact 성공은 새 native orchestration의 실행 검증을 대신하지 않는다.

새 `tests/planner_memory_controller.rs`에는 일반8개/ignored1개가 있다. 일반 검사는
production `memory::run`, receipt 검증 및 실제 등록 CLI를 사용해 wrapper/hash/argv/
기한/growth, 첫 실패42, signal→1, 누락·불완전·순서 변경·실패 행 receipt 거부,
unsanitized/없는 ASan/없는 trace/이미 존재하는 wrapper/runtime ASan 오용을 확인한다.
통제 leaf fixture가 실제 ASan solver 합격을 뜻하지는 않는다.

기존 wrapper5의 별도 관측 fixture는 아직 없으므로 ignored selector
`source_memory_wrapper_five_boundaries_match_native_and_capture_original_results`를 준비했다.
Actions에서 기존 unittest의 fixture와 실제 Python wrapper main을 그대로 호출해
success/failure42/empty/incomplete/unsanitized의 actual 결과·인자·wrapper 원문을 얻는다.
expected를 native 결과에서 만들지 않는다. 성공 receipt와 wrapper 바이트, 인자/예산,
실패·거부 경계를 native와 비교하며 작은따옴표·공백·비ASCII 경로도 포함한다.

원본 관측은 native parity 검사 전에
`$CI_JOB_TEMP/proof/planner-memory-original/observations.json`에 create_new/0600으로
남긴다. 단일 새 파일·최대1 MiB이며 stdout1 MiB/stderr64 KiB·원본 관측 명령30초다.
통제한 합성 파일·경로만 사용한다. 원본 두 source와 관련 Rust/helper source SHA,
Python 실행물 SHA·기존 `planner-oracle-python.{sha256,version}` proof, `GITHUB_SHA`,
actual stdout/stderr/returncode와 결과를 결합하고 관측 전후 identity를 다시 확인한다.
새 Python version process는 만들지 않는다. 이 파일만으로 native parity 합격을 주장하지 않는다.

## 부모 등록과 필요한 Actions

new integration test는 Cargo의 기본 test discovery 대상이므로 공유 Cargo 등록은 없다.
부모가 helper SHA pin/Focus/unit selector와 증거 upload 경로를 연결한다. 실제 workflow의
memory run 본문은 계속 같은 Bash helper 호출이어서 workflow 본문 변경이 필요 없다.
기존 Python wrapper와 테스트·원본 controller는 capture/parity/실제 native memory PASS 및
부모 retirement 감사가 끝날 때까지 삭제하지 않는다. 이번 삭제·retirement credit은0이다.

아래 명령은 부모 Actions용이며 로컬에서 실행하지 않았다.

```bash
CARGO_BUILD_JOBS=1 cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --test planner_memory_controller
PLANNER_REFERENCE_PYTHON="$oracle_python" CARGO_BUILD_JOBS=1 cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --test planner_memory_controller source_memory_wrapper_five_boundaries_match_native_and_capture_original_results -- --ignored --exact --nocapture --test-threads=1
CARGO_BUILD_JOBS=1 cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --test planner_ci_controller registered_memory_controller_
CARGO_BUILD_JOBS=1 cargo clippy --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --bin check-plannerd-ci --test planner_memory_controller -- -D warnings
```

`oracle_python`과 기존 interpreter proof는 같은 job에서 승인한 값을 사용한다.
첫 명령은8 passed/1 ignored가 기대값이고, 둘째는 원본5경계를 한 selector에서 관측한다.
셋째는 바뀐 CLI에서 기존 bare memory core 두 실행 회귀만 확인한다.
변경 없는 source5/adapter/다른 기능군의 전체 재실행을 요구하지 않는다.
실제 host Planner에서는 기존 runtime 단계가 새 controller를 빌드한 다음 기존 sanitizer
build와 아래 실제 memory 호출을 수행해야 한다. unit fixture만으로 이 단계를 대체하지 않는다.

```bash
/usr/bin/bash "$CI296_SOURCE/tools/ci/planner_native_controller_523.sh" memory
```

로컬 확인은 Rustfmt2021 구문·서식, Python/내장 capture snippet AST, `bash -n`,
원본 바이트 불변과 diff에 한정한다. Cargo/build/test/Clippy/Python oracle·기능 실행0,
WF/GHA/stage/commit/push0이다. 새 native PASS와 원본 capture/parity는 아직 미완료다.

## 안정 source SHA256

```text
ca36ec936776d6ccc8e84694ecb42d860f377655c59a8db9bd44b888a31aadcc  rust/crates/dev-tools/src/planner_ci_controller.rs
b06b4fed1e59cf63c449c7dbec34b221e9cfba0ec74d8bdaa65544127d147680  rust/crates/dev-tools/src/planner_ci_controller/cli.rs
5420fdab0317a8083306075b503208282db42ee66a358bb0c42caf4a637e4881  rust/crates/dev-tools/src/planner_ci_controller/memory.rs
7dc897ad2d4ff28dfc06bc411d5ddd5e94f0a63b0ed2d02e0151e0608bdc0c1d  rust/crates/dev-tools/tests/planner_memory_controller.rs
32cfc4a9b57010c57bf3bdd112c4339ee22ea2ecd5618c39f6b1aa441de5ea63  tools/ci/planner_native_controller_523.sh
```
