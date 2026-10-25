# #523 Planner memory wrapper의 실제 native 호출 전환 준비

2026-10-26. 작업 루트는
`/home/fma1/.codex/worktrees/python-rust-523-completion/openpilot-rust`다.
기존 `check-plannerd-ci memory`에 `--asan-library PATH`를 추가했다. 이 옵션이 있으면
새 `planner_ci_controller/memory.rs`가 기존 Python wrapper의 orchestration을 실행한다.
실제 `tools/ci/planner_native_controller_523.sh memory`도 같은 native CLI를 호출한다.
새 bin·feature·Cargo 의존성·공유 lib.rs 등록은 필요하지 않다.

`--asan-library` 없는 memory core는 원래대로 비교 명령 세 개만 실행한다. 기존 native
core 실행 회귀를 위해 유지한다. runtime에서 ASan 옵션을 주면
거부하며, native memory 실패 뒤 Python fallback이나 full-runtime receipt 생성은 없다.

## 원본과 유지한 실제 계약

먼저 `rust/tools/check_plannerd_native_memory.py`, 그 unittest5,
`planner_ci_controller`의 CLI/plan/run_checks 및 `qa_process`를 읽었다.
원본 wrapper와 unittest5의 삭제 전 바이트는 source425와 일치시켰다. 부모의 artifact 감사
완료·명시적 삭제 승인 후 아래 두 파일을 삭제했다. 기존 `tests/planner_ci_controller.rs`는
계속 바이트 불변이다.

- `rust/tools/check_plannerd_native_memory.py`
- `rust/tools/tests/test_plannerd_native_memory_controller.py`

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

## source425의 독립 관측·실제 native 성공

부모가 [run40227415620/job121271640924](https://github.com/bin9208/openpilot-rust/actions/runs/40227415620/job/121271640924)의
native8/original1과 기존 old14/source5/adapter13/wrapper5/frozen5·strict Clippy 성공을 확인했다.
artifact12612286621은 1,605,293 B, SHA256
`41abeb085169ae55372ab2cb665175dbf1ac4a7f320915f1f7682e53b83aa6c1`,
56 unique/nosymlink/CRC였다. 그 ZIP의 `planner-memory-original/observations.json`
24,396 B를 바이트 그대로 복사했다. 원문 SHA는
`f2f8c972e65638a1eac1d3eb5ede9114502a48547a403e5f163a2291cd8a0780`다.
원문에 기록된 source13 SHA도 source425 Git blob과 모두 일치시켰다.

부모가 [actual host run40227447987](https://github.com/bin9208/openpilot-rust/actions/runs/40227447987)의
native runtime12+native ASan3 전부0/PASS와 아래 artifact 감사를 완료하고 삭제를 승인했다.
이 감사 결과는 부모 전달이며, 이 세션에서 55 MiB ZIP 전체 검사를 반복하지 않았다.

| artifact | bytes | SHA256 | 부모 감사 |
| --- | --- | --- | --- |
| functional12612670103 | 55,186,524 | `96b275577f6d9eec3fe106d65b258e00cbfcd3be8421d0659122e73a6f007252` | 34,282 safe/CRC |
| private12612575557 | 296,340 | `73a1c564eec4b5f05b7894b505fcd90a84e5d0bc640c17b6eb7ec395ba62e845` | 29 CRC/source-index 전후 동일/runtime+memory exit0 |

이 성공은 source425의 삭제 전 native orchestration 근거다. 이번 source-absence guard와
frozen reader·삭제 이후 actual ASan의 성공으로 대신하지 않는다.

## frozen parity5와 정확한 두 경로의 재등장 거부

기존 일반8개 테스트의 본문은 그대로 보존했다. 실제 Python을 실행하던 ignored capture1은
원문 데이터 전용 frozen 비교5개로 바꿨다. 전·후 guard negative control2개를 더해
`planner_memory_controller`는 **일반15개/ignored0개**다. 실제 wrapper/ASan/hash/receipt/42,
signal→1, 기한·growth와 malformed/missing record 거부 경계를 유지한다.

새 `tests/fixtures/planner-memory-original/observations.json`과 `PROVENANCE.md`에 원본5개
경계·실행/Python/source13 provenance와 허용한 치환을 고정했다. reader는 전체 원문 SHA,
schema/kind/source425/Python proof/outer returncode0/stderr 및 raw stdout JSON의 일치를
확인한다. 기존 원본 module·테스트·Python 실행물은 열거나 import/실행하지 않는다.

허용 치환은 각 `calls`의 exact case root와 `/` component 하위, canonical Python exact
file, success receipt의 binary SHA key3개, wrapper의 이미 quote된 case root 접두사2곳뿐이다.
status/checks/hash 값/scope·옵션·escape·순서·2700초·예산은 그대로다. expected receipt나
wrapper를 native 함수에서 만들지 않는다. incomplete의 `abi0/owner0` 두 입력 행은 기존
source425 unittest fixture의 고정 입력이며, 원본 rejection 결과는 frozen 관측에서 읽는다.

`memory::run`은 출력·leaf 이전에 다음 **두 경로만** `symlink_metadata`로 검사한다.
일반 파일·live/dangling symlink·디렉터리 또는 기타 조회 오류를 거부하며 NotFound만 허용한다.
기존 source를 허용하는 새 CLI flag나 fallback은 없다.

- `rust/tools/check_plannerd_native_memory.py`
- `rust/tools/tests/test_plannerd_native_memory_controller.py`

negative control은 private fixture root에서 정상 memory 성공 후 각 경로를 재등장시켜,
없는 ASan을 검사하기보다 먼저 거부하고 출력·leaf를 만들지 않는지 확인한다. 인접한
`check_plannerd_ci.py`와 `plannerd_ipc.py`의 존재는 허용한다. 실제 repo 원본은 복원하지 않는다.
receipt 형식/필드/scope를 바꾸거나 별도 policy field를 섞지 않았다.

`run_checks`의 Result를 `checked`에 보존한 뒤 성공·실패 모두에서 같은 두 경로를 다시
검사하고, 그 다음 기존 `checked?`와 receipt 쓰기를 수행한다. 재등장이 없으면 기존 실패
코드를 유지한다. 재등장하면 postguard 오류로 CLI1을 반환하고 이미 쓴 commands 행은
변경하지 않는다. receipt writer는 source425와 바이트 동일하다.

추가 selector `memory_post_guard_preserves_real_leaf_records_and_rejects_restored_sources`는
실제 등록 CLI의 첫 shell leaf가 각 퇴역 경로를 file/live symlink로 복원하고0 또는42로
끝나는8개 조합을 확인한다. 실제 commands.json의42와 후속 미실행 또는 성공3개 행을
보존하면서 postguard 거부·최종 PASS 부재를 요구한다. Python을 실행하지 않는다.
기존 native8/frozen5/before-guard1의 본문은 바이트 그대로 보존했다.

## 부모 등록과 삭제 후 필요한 Actions

새 Cargo/lib/CLI/helper/WF 변경은 없다. 부모는 `planner_memory_controller`의 기대 count를
15/0으로 바꾸고 기존 original-memory ignored 호출을 제거한다. 정확한 새 selector는
`frozen_memory_`의5개와
`memory_absence_guard_rejects_exact_two_reappearances_before_output_or_leaf`와
`memory_post_guard_preserves_real_leaf_records_and_rejects_restored_sources`의2개다.
기존 원본 controller/source5/IPC와 James의 별도 frozen5에는 손대지 않는다.

읽기 전용 caller 추적에서 부모가 정리할 두 workflow를 확인했다.

- `python-rust-523-native-units.yml`의 bounded native unit 단계: 삭제된 wrapper unittest
  discover 호출과 `source_memory_wrapper_five_boundaries_match_native_and_capture_original_results -- --ignored` 호출.
- `python-rust-523.yml`의 focused admission: 삭제된 unittest 파일의 SHA 검사와 unchanged cohort case 목록.

manager-qa fixture의 역사 Python roster에 남은 경로는 실행 caller가 아니므로 보존했다.
Focus/pin/원장/inventory·삭제2개 credit 및 실제 삭제 후 검증은 부모 소유다.
이 패치가 삭제한 source2개와 아직 승인되지 않은 retirement credit을 구분한다.

부모 Actions 명령(로컬 실행 금지):

```bash
CARGO_BUILD_JOBS=1 cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --test planner_memory_controller
CARGO_BUILD_JOBS=1 cargo clippy --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --bin check-plannerd-ci --test planner_memory_controller -- -D warnings
```

전체 target은 native8+frozen5+absence2의15 passed/0 ignored가 기대값이다. 이 target은
`PLANNER_REFERENCE_PYTHON`이나 interpreter proof·원본 import가 필요 없다.
부모가 같은 삭제 후 source에서 실제 host native ASan3와 source-absence qualification을
확인해야 retirement credit을 반영한다. 기존 actual 호출은 다음과 같다.

```bash
/usr/bin/bash "$CI296_SOURCE/tools/ci/planner_native_controller_523.sh" memory
```

로컬은 Rustfmt2021 구문·서식, JSON/역사 blob/바이트·경로 검토와 diff만 수행했다.
Cargo/build/test/Clippy/Python oracle·기능 실행0, WF/GHA/stage/commit/push0이다.

## source425 native 구현의 역사 SHA256

아래는 원본 관측 cohort의 값이다. 현재 memory guard/reader의 최종 SHA는 별도로 인계한다.

```text
ca36ec936776d6ccc8e84694ecb42d860f377655c59a8db9bd44b888a31aadcc  rust/crates/dev-tools/src/planner_ci_controller.rs
b06b4fed1e59cf63c449c7dbec34b221e9cfba0ec74d8bdaa65544127d147680  rust/crates/dev-tools/src/planner_ci_controller/cli.rs
5420fdab0317a8083306075b503208282db42ee66a358bb0c42caf4a637e4881  rust/crates/dev-tools/src/planner_ci_controller/memory.rs
7dc897ad2d4ff28dfc06bc411d5ddd5e94f0a63b0ed2d02e0151e0608bdc0c1d  rust/crates/dev-tools/tests/planner_memory_controller.rs
32cfc4a9b57010c57bf3bdd112c4339ee22ea2ecd5618c39f6b1aa441de5ea63  tools/ci/planner_native_controller_523.sh
```

## 이번 퇴역 준비 patch의 최종 SHA256

```text
ed44b213ce67176f3a3193a5ad2f15b80983e8d0ee4fea6c2981adbfb9a4c958  rust/crates/dev-tools/src/planner_ci_controller/memory.rs
4728bd30f246969e0b7b96d0f1b2b465eb95b86cf41d25f6732557c8f53715ee  rust/crates/dev-tools/tests/planner_memory_controller.rs
f2f8c972e65638a1eac1d3eb5ede9114502a48547a403e5f163a2291cd8a0780  rust/crates/dev-tools/tests/fixtures/planner-memory-original/observations.json
ea47bd7f44fb618a04fd904a1e460e47ee6dd10fb26468da6af4ef355c9b3edb  rust/crates/dev-tools/tests/fixtures/planner-memory-original/PROVENANCE.md
```

문서 자체의 SHA는 최종 인계 메시지에서 전달한다. 로컬 source 검토에서 native8의 본문과
전후 guard와 Result 보존 호출부 외 memory 구현이 source425와 바이트 동일함을 확인했다. 퇴역 두 경로는
실제로 부재하며 fixture는 원래 ZIP member와 완전히 같다. 이 확인은 기능 실행이 아니다.
