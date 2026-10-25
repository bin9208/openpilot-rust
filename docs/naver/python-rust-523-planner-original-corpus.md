# #523 source5의 독립 원본 corpus 관측 수집

2026-10-26. 작업 루트는
`/home/fma1/.codex/worktrees/python-rust-523-completion/openpilot-rust`다.
이번 추가 쓰기는 기존 `tests/planner_ci_controller.rs`와 본인 Planner 문서뿐이다.
stage/commit/push/WF/GHA 실행은 하지 않았다. source5 예정 batch에서 관측을 함께 남겨
같은 불변 성공 cohort의 원본 실행을 나중에 다시 요구하지 않도록 준비한다.
현재 배정된 native14/source5/adapter13/memory5의 정의 수와 selector는 바꾸지 않았다.

## 기록하는 원본 데이터

기존 `reference`가 이미 실행하는 Python의 원본 stdout, stderr, 종료 코드와 마지막
stdout JSON을 그대로 저장한다. `original_result`는 그 JSON에서 파싱한 값이며
native scheduler/controller 출력으로 만들지 않는다. 기존 native 동일성 assertion은
그 뒤에 그대로 수행한다. 관측 파일 자체가 native 비교 PASS를 뜻하지 않는다.

원본 Python snippet 5개는 수정 전후 바이트가 동일하다. 각 case의 원본 실행 호출은
1회 그대로이고 실패 전파 case의 기존 실패 child도 그대로다. 추가 Python subprocess나
`python --version` 실행은 없다. 이전에 생성한 다음 동일 cohort proof를 재사용한다.

- `$CI_JOB_TEMP/proof/planner-oracle-python.sha256`
- `$CI_JOB_TEMP/proof/planner-oracle-python.version`

현재 `PLANNER_REFERENCE_PYTHON` 실행물의 SHA와 기존 proof의 실행물 경로/SHA가
일치해야 한다. 버전 proof는 원문을 보존한다. 실행물·proof·원본/Rust source hash가
reference 호출 전후 동일한지도 확인한다. 원본 결과가 `python` 필드를 제공하는
3개 case에서는 그 실제 `sys.executable`도 요청한 canonical 실행물과 비교한다.

`GITHUB_SHA`와 함께 원본 source2 및 현재 Rust source8의 SHA256을 기록한다.
Rust source8은 이 testfile, controller 본체, cli/plan/scheduler, bin 진입점,
qa_process, disk-space lib다. 원본 source2는 `check_plannerd_ci.py`와 `plannerd_ipc.py`다.
현재 Rust source를 expected 값으로 실행하는 방식이 아니라 provenance hash로만 결합한다.

## 파일과 크기 계약

기록 위치는 `$CI_JOB_TEMP/proof/planner-original-corpus/<test-name>.json`이다.
정확히 기존 source5 test 이름만 허용하며 새 파일을 `create_new`와 mode0600으로 쓴다.
동일 이름의 기존 관측을 덮어쓰지 않는다. credential/environment 전체 dump는 없고
입력 경로는 원래 합성 tempfile/config 경로다.

각 JSON은 newline 포함 최대1MiB, 디렉터리는 최대8MiB다. 기존 entry도 허용 이름의
regular file이며 각1MiB 이하인지 확인한다. 허용 이름5개×1MiB 제한은 병렬 실행에도
8MiB를 넘을 수 없고, 쓰기 전 실제 합계도 검사한다. 파일을 write/sync한 뒤 반환한다.

| JSON 파일 stem | 원본 cases와 expected 관측 |
| --- | --- |
| `source_scheduler_matches_native_protocol_without_reimplementing_the_oracle` | finite76개 입력의 IEEE bits, 원래 numeric stdin, 실제 Python target bits |
| `source_public_scheduler_preserves_python_ordered_comparison_at_float_boundaries` | signed-zero/NaN/무한대9개 입력 bits, 실제 Python target bits |
| `source_runtime_plan_matches_every_native_leaf_and_argument` | synthetic config, 실제 원본 12개 name/전체 argv/output 경로 |
| `source_memory_subset_matches_every_native_sanitizer_leaf_argument` | synthetic wrapper config, 실제 원본 abi/owner/fault name/전체 argv |
| `source_failure_forward_matches_native_controller_and_raw_log` | 실제 failing/later fixture source와 경로, 기존 disk-usage 주입값, 원본 첫 실패 행/exit42/argv/stdout 및 실제 원본 first.log |

schema_version은1이다. 주요 필드는 `test_name`, `github_sha`, `source_sha256`,
`python`, `cases`, `argv`, `stdin_utf8`, `original_result`, `original_stdout_utf8`,
`original_stderr_utf8`, `original_returncode`, `original_first_log_utf8`다.
`native_comparison`은 enclosing assertion의 결과를 Actions에서 함께 확인해야 한다는
설명이다. 실패 case의 raw log도 native log에서 복사하지 않고 실제 원본 출력 파일에서 읽는다.
IEEE bits는 u64 정수로 유지해야 하며 향후 reader에서 부동소수 JSON Number로 축소하면 안 된다.

새 데이터는 기존 private proof artifact 경로 아래 있으므로 artifact 경로 추가가 필요 없다.
일부 원본 관측 뒤 native 비교가 실패하거나 수집 I/O가 실패하면 해당 batch는 완료가 아니다.
동일 job에서 성공 관측을 덮어쓰기 위한 재실행도 허용하지 않는다.

## old module 소비자 읽기 전용 추적

검색은 파일명만 출력하는 `rg -l`로 수행했고 workflow 전체/single-flow 본문은 출력하지 않았다.
현재 확인한 분류는 다음과 같다. workflow 활성화 상태는 조회하지 않았다.

| 소비자 | 현재 의미와 격리 조건 |
| --- | --- |
| `.github/workflows/python-rust-523-native-arm-more.yml` | old `python -P rust/tools/check_plannerd_ci.py` direct caller가 아직 있다. 부모가 유지/퇴역 정책과 실제 실행 가능성을 확인하고 연결을 제거하거나 전환해야 한다. 임의로 비활성이라고 가정하지 않는다. |
| `rust/tools/tests/test_plannerd_ci.py` | `Check/ROOT/run_checks`를 old module에서 직접 import한다. native 대응 검사 및 이번 corpus 감사와 별도로 부모가 이 소비자 퇴역을 승인해야 한다. |
| `rust/crates/dev-tools/tests/planner_ci_controller.rs` | source5의 runpy 원본 실행과 이번 source hash 관측이 old module의 실제 소비자다. versioned corpus reader로 전환할 때 실행·현재 파일 재해시 의존도 함께 바꿔야 한다. |
| manager-qa-original의 `repo-python-before.sha256`/`repo-python-after.sha256` | old path가 남은 역사적 manifest다. 실제 호출/import가 아니며 현재 퇴역을 주장하기 위해 과거 fixture를 고치지 않는다. |
| `rust/tools/check_plannerd_native_memory.py` | 앞선 actual caller 변경으로 old import/실행을 native memory CLI로 전환한 상태다. 원본 Python wrapper 자체의 삭제를 의미하지 않는다. |

main/reusable WF는 이번 exact direct-caller 검색에 나타나지 않았다. 본인 helper도
old controller를 실행하지 않는다. Focus/원장/역사 문서나 Rust bin
`check_plannerd_ci.rs`의 넓은 basename 일치는 Python actual caller로 세지 않는다.

별도로 `plannerd_lifecycle_peer.py`는 `plannerd_ipc`의 `OUTPUTS/peers_ready/policy`를
import한다. native12-leaf의 Python IPC/wire/lifecycle 및 나머지 원본 leaf도 남아 있다.
pacing corpus가 생겨도 `plannerd_ipc.py` 전체를 고립하거나 삭제할 수 있다는 뜻은 아니다.

## 향후 source 퇴역 판단에 필요한 것

부모는 이번 batch에서 JSON5개, 동일 `GITHUB_SHA`/source hash/Python binding과
source5 실제 native 동일성 성공을 함께 감사한다. 수집물만으로 기존 모듈을 삭제하지 않는다.
그다음 합성 경로를 명시적 placeholder로 정규화한 versioned corpus를 별도 변경에서
검토하고, 원본 stdout/bit/raw-log 및 원본 source hash와의 역추적을 보존해야 한다.
native expected를 새로 계산하여 원본 기대값을 대체하는 변환은 허용하지 않는다.

원본 actual caller/import 폐쇄, source5의 data reader 전환, parent 원장/등록/absence 검증이
완료되기 전까지 old `check_plannerd_ci.py`와 독립 oracle는 그대로 필요하다.
현재 삭제 수량과 retirement credit은0이다. 구현한 것은 수집이며 reader/삭제 framework는 없다.

## SDK 문서 경계와 로컬 확인

actual caller 문서도 정정했다. main host Planner에는 whole-SDK before/after가 없으며,
host는 기존 일반 SDK/stager provenance와 절대 SDK/RUSTUP_AUTO_INSTALL=0 경로를 유지한다.
ARM만 기존 `arm530_sdk` before/verify/after 고정 검증을 가진다. host에 새 whole-SDK
설치나 검증 범위를 요구하지 않는다. 구현 helper나 shared workflow는 이 정정에서 바꾸지 않았다.

로컬에서는 추가 helper/reference/source5 함수 묶음에만 Rustfmt2021을 적용했고,
Python snippet5개는 AST 구문 및 수정 전 SHA 동일성을 확인했다. reference의
`NativeRunner.run`은 case당 기존1개, source case 호출부는5개 그대로다.
native14 본문을 포맷하지 않았고 diff whitespace를 확인했다.
original/기능/Cargo/build/Clippy/WF/GHA/stage/commit 실행은0이다.

부모 통합 후속: 이 조사 뒤 standalone `python-rust-523-native-arm-more.yml`의 Planner 두 run 본문도 동일 runtime/memory Bash helper로 전환했다. main host·canonical reusable ARM 및 standalone ARM의 actual caller 변경을 같은 batch에서 검증한다. 위 표의 old direct caller 잔존은 조사 시점의 상태이며, 이 후속 연결의 GHA 수용은 아직 대기다. 별도 host Planner workflow는 main 전체 SHA와 동일 job/global env를 묶어 무관43개 job을 반복하지 않고 실제 host12/ASan3을 검증한다. Python test 모듈과 source5의 실제 import 의존은 여전히 남아 있다.
