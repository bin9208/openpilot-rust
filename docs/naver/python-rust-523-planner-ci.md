# #523 Planner CI의 prepared SDK 구조 기대값 수정

2026-10-25. [#523](https://github.com/bin9208/openpilot-rust/issues/523)의
PR301f 인계 범위. 작업 루트는
`/home/fma1/.codex/worktrees/python-rust-523-completion/openpilot-rust`이며,
읽은 기준 HEAD는 `f9879504a544a9d7e475206841b218b6d8d81cad`다.
공유 worktree의 다른 WIP는 보존한다. commit/push/Actions/이슈/Focus 및
공유 Cargo 등록은 부모가 담당한다.

## 회귀 원인과 수정 범위

사용자 인계에 따르면 job `121194258229`와 native ARM job `121194258939`에서
`test_planner_host_arm_and_memory_results_are_required`가 실패했다.
24개 중 나머지 23개는 성공했다는 인계이며, 이번 작업에서 원격 결과를 새로
조회하거나 테스트를 재실행하지 않았다.

소스로 확인한 원인은 `assert_planner_native_contract`가 CarrotMan x64의
의존성 단계에 `rustup toolchain install 1.94.0 --profile minimal`이
정확히 1회 있다고 가정한 것이다. 현재 x64는 이미 prepared SDK를 사용한다.

- `carrot530_sdk`가 private 경로의 fresh SDK를 준비하고,
  `OPENPILOT_CI_PREPARED_RUST_ROOT`,
  `OPENPILOT_CI_PREPARED_RUST_MANIFEST`, `RUSTUP_AUTO_INSTALL=0`,
  `RUSTUP_TOOLCHAIN=1.94.0`을 전달한다.
- 의존성 단계는 `CI_PREPARED_RUST_ROOT`와 `CI_PREPARED_RUST_MANIFEST`를
  각각 공개된 prepared 입력과 비교하고 manifest를 strict SHA 검증한다.
- `carrot530_sdk_final`은 시도된 작업 뒤 `always()` 조건으로 SDK를 재검증한다.
  `prepare-readonly-ci-rust`의 `after`는 tool pin, 전체 파일 SHA와 roster를
  `before` 결과와 비교한다.

`rust/tools/tests/test_plannerd_ci.py`만 수정했다. 기존
`ci_policy::validate_carrot_x64_sdk530_steps`와 같은 before/after 본문 SHA pin,
name/id/조건/shell/source 경로/host-control 환경을 요구한다. 이 pin은
fresh 경로·소유권·성장 admission·manifest export·자동 설치 금지·proxy 확인을
포함한 현재 정식 본문에 대한 것이다. private Python 준비 → SDK 준비 → 모든
기능 단계 → SDK 사후 검증 → source seal 순서도 확인한다.

x64 의존성의 root/manifest/SHA 검증 블록이 정확히 1회이면서 마지막에 있는지
확인한 뒤 비교용 복사본에서만 ARM SDK `verify` 한 줄로 치환한다. CarrotMan
x64/ARM step에는 `rustup toolchain install`과 `rustup target add`가 없어야 한다.
그 밖의 명령·조건·순서를 포함한 전체 functional step 동등 비교와 개수는 유지한다.
ARM SDK before/after의 고정 helper pin, 20/10분, trusted/always 조건 및
8개 role의 gate, Planner 기능·메모리 요구와 다른 23개 검사 본문은 유지한다.
workflow/helper/공유 policy 및 main 코드는 변경하지 않는다.

## 검증 및 남은 작업

수정한 Python 파일의 `ast.parse` 구문 검사와 `git diff --check`는 통과했고,
변경 diff를 검토했다. 로컬 검증은 이 구문·diff 범위로 제한했다.
pytest/Cargo/build/test/Clippy/원본/모델/프로젝트 CLI는 실행하지 않는다.
기능 결과는 미검증이며, 부모가 통합한 정확한 SHA의 기존 host/ARM Actions에서
동일한 24개 검사와 후속 필수 gate 결과를 확인해야 한다.

## 24개 검사의 Rust 이전 계획 — 구현 패치 0개

기존 `rust/crates/dev-tools/src/ci_policy.rs`의 `parse`,
`validate_native_arm_reuse`, `validate_rust_gate`와
`tests/ci_workflow_policy.rs`의 mutation/shell 검사 기반을 재사용한다.
새 crate나 YAML parser, 별도의 CI-policy CLI는 만들지 않는다.

| 기존 검사 수 | 이전 범위와 보존할 증거 |
| --- | --- |
| 1 | host/ARM/memory 양성 계약. 현재 Rust policy에 없는 전체 functional step 동등 비교, Planner 명령/순서/성장량, transport 순서 및 SDK 투영을 보강한다. PyYAML bool/int와 Rust parser의 문자열 scalar 차이를 명시한다. |
| 19 | 현재 parametrize의 `(scope, path, value)` 각각을 Rust mutation 사례로 일대일 이전한다. `both`는 양쪽을 함께 변조하고, baseline은 수용하면서 각 변조는 거부해야 한다. 기존 Rust 음성과 중복되는 사례도 대응표를 남긴다. |
| 1 | Miri 실패 은폐 회귀. 실제 workflow step을 읽고 소유된 임시 PATH의 실패 child가 42로 종료하도록 하여 로그 capture 뒤 종료 코드와 호출 횟수를 확인한다. 실제 Cargo/Miri 실행은 이 회귀 fixture가 하지 않는다. |
| 2 | IPC 20Hz pacing·지연 후 catch-up 금지. 실제 `plannerd_ipc.py::next_model_target`를 호출하던 검사의 의미를 유지해야 한다. 실제 sender가 공유해서 사용하는 Rust 스케줄러 소유 구현으로 이전할 때 같은 수치 사례를 연결한다. 테스트 전용으로 수식을 복제하는 것은 이전 완료로 인정하지 않는다. |
| 1 | `run_checks`의 첫 실패 기록·후속 명령 중단. native Planner orchestration 소유 구현에 기존 `qa_process` 실행 기반을 연결하는 범위에서 이전한다. 첫 child 42, `commands.json` 기록, 다음 sentinel 부재를 모두 검사하고 테스트 전용 가짜 orchestration으로 대체하지 않는다. |

먼저 20개 구조/변조 검사와 Miri 실패 회귀를 기존 Rust CI-policy target으로
옮긴다. 나머지 3개는 실제 sender 스케줄러 및 orchestration 구현의 소유권
전환이 선행 조건이다. 이 조건이 해결되기 전에는 이 파일 전체의 Rust 이전이나
Python 제거 완료를 주장하지 않는다. `run_checks`의 기존 자원 admission 문제를
이번 구조 회귀 수정에 섞지 않는다.

후속 패치에서 부모가 Cargo/모듈 등록과 workflow 호출 교체를 통합한다.
Actions에서 기존 24개와 Rust 대응 검사를 같은 SHA로 비교하고, 19개 변조의
실제 거부 및 실패 전파를 확인한 뒤 Python testfile/caller 제거를 진행한다.
Planner 원본 비교·실제 IPC/lifecycle·ARM 기능·메모리 gate는 계속 필수다.

## 2026-10-26 구조/변조 20개 Rust 소스패치

위의 구현 0개 상태 이후 부모의 bounded 지시에 따라 다음 소스를 추가했다.
이번 단계의 쓰기는 이 두 Rust 파일과 이 문서의 append로 한정했다.
기존 Python 24개 본문과 앞선 SDK 회귀 수정, 공유 WIP는 보존했다.

- `rust/crates/dev-tools/tests/planner_ci_policy.rs`: 양성 1개와 독립 변조 19개.
- `rust/crates/dev-tools/tests/planner_ci_policy/contract.rs`: 원래
  `assert_planner_native_contract`의 직접 이전과 작은 loader/step helper.

기존 `ci_policy::parse`를 사용하여 YAML alias와 문자열 scalar를 읽는다.
timeout·bool 기대값과 변조값은 같은 parser의 문자열 표현으로 옮겼다.
functional step의 전체 dict·개수·조건·순서 비교, Radard matrix 치환,
final-two candidate의 기준 선택, SDK 검증 블록 투영, native ARM 8개 role,
Planner 기능 명령과 성장량, host/ARM/memory gate와 transport 경계를 유지한다.
새 parser/crate/CLI/범용 mutation framework는 만들지 않았다.

양성 검사는 기존 `ci_policy::run_shell`로 실제 gate의 정상 결과 및
`failure/cancelled/skipped/빈값/false/unknown` 결과 거부를 검사한다.
기존 `qa_process::{CommandSpec, NativeRunner}`로 각 대상 step의 `bash -n`도
수행하도록 소스를 옮겼다. 이는 Actions에서 실행할 테스트 본문이며,
이번 로컬 세션에서 shell gate나 테스트를 실행했다는 의미가 아니다.

각 변조는 다음 순서로 처리한다. baseline 계약 성공 → catch 밖에서 변조 적용 →
동일 계약의 panic 거부 확인. 따라서 baseline/loader/변조 위치 오류를 음성
성공으로 삼지 않으며, 원본의 `pytest.raises(AssertionError)`에 대응하는
실패 전파를 유지한다. `both` 2개는 양쪽 workflow를 함께 바꾼다.

| 원본 parametrize 순서 | 독립 Rust selector |
| --- | --- |
| 1 | `planner_rejects_radar_proxy_noop` |
| 2 | `planner_rejects_navi_proxy_missing_needs` |
| 3 | `planner_rejects_xiaoge_timeout_change` |
| 4 | `planner_rejects_radar_lifecycle_skip` |
| 5 | `planner_rejects_radard_result_cross_binding` |
| 6 | `planner_rejects_native_caller_skip` |
| 7 | `planner_rejects_unknown_native_workflow` |
| 8 | `planner_rejects_fast_missing_native_result` |
| 9 | `planner_rejects_native_planner_skip` |
| 10 | `planner_rejects_native_planner_x64_host` |
| 11 | `planner_rejects_empty_native_matrix` |
| 12 | `planner_rejects_unknown_native_caller` |
| 13 | `planner_rejects_unpinned_python` |
| 14 | `planner_rejects_dependency_stager_skip` |
| 15 | `planner_rejects_incomplete_native_aggregate` |
| 16 | `planner_rejects_success_only_native_aggregate` |
| 17 | `planner_rejects_optional_native_gate` |
| 18 | `planner_rejects_both_dependency_noops` |
| 19 | `planner_rejects_both_comparison_noops` |

### SDK before/after pin의 부모 갱신 위치

부모가 x64 SDK before에 실제 조기 실패 행을 표시하는 ERR trap을 추가하고
주석 2줄을 제거했다는 인계를 받았다. initial/기능/예산 guard는 불변이라는
인계이며, 실제 fresh preflight 원인은 부모가 Actions에서 관측할 예정이다.
소스 readback 시 `ci_policy.rs`의 before pin은
`3ac35298662ca18b12658fead672e2b4deacbb2f72e1ec68699794e167f2502f`다.

부모가 함께 관리해야 하는 기존 하드코딩 위치는 다음과 같다.

- `rust/crates/dev-tools/src/ci_policy.rs`의
  `validate_carrot_x64_sdk530_steps`: before/after digest tuple.
  before는 위 새 값으로 갱신된 것을 읽었다.
- `rust/tools/tests/test_plannerd_ci.py:168`: before는 아직
  `ae5fc6a75f44aa66152666f03584f9153304204d8877c584c2366d6e026ef142`였으므로
  부모가 새 값으로 갱신해야 한다. 이번 쓰기 범위에는 Python이 없어 직접 바꾸지 않았다.
- 같은 Python 파일 `:172` 및 공유 policy의 after tuple: 현재 값
  `4d7305f25cbb9f9de394305c1aa4514c222aa2d6f3d02b623d0ad201bac161c7`은
  before 변경의 갱신 대상이 아니다. after 본문이 달라질 때 두 위치를 갱신한다.

새 `contract.rs`는 CarrotMan SDK before/after hash literal을 추가하지 않고
`validate_private_job_layout(reference)`를 호출한다. 정식 SDK 본문·조건·metadata
pin은 공유 policy가 검증하며, 이 테스트는 추가로 Python 준비·모든 기능 단계·
source seal 사이의 순서와 manifest 블록·재설치 부재를 직접 확인한다.
그러므로 새 파일에 부모가 동기화해야 할 중복 x64 body pin은 없다.

### 부모 등록 요청 및 검증 상태

부모가 공유 Cargo/feature/Focus/GHA 연결을 담당한다. Actions용 전체 selector는
다음과 같다. 이 명령은 로컬에서 실행하지 않았다.

```sh
cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --test planner_ci_policy
```

양성 한 개를 정확히 지정할 때는 같은 target에
`planner_host_arm_and_memory_results_are_required -- --exact`를 붙인다.
변조 19개는 위 표의 이름을 각각 `-- --exact`와 함께 지정하거나
`planner_rejects_` prefix로 묶는다. 전체 target은 20개를 제공한다.

설치된 Rust 1.94.0의 절대 `bin/rustfmt`를 `--edition 2021`로 새 두 파일에만
실행하여 통과했다. rustup proxy/설치나 Cargo/컴파일/테스트/Clippy/원본/모델/
프로젝트 CLI는 실행하지 않았으며 **로컬 테스트 실행은 0회**다.
컴파일·20개 양성/음성 및 실제 gate shell 결과는 모두 Actions 미검증 상태다.
실행 회귀 3개와 Miri 1개는 아직 이전하지 않았고, 네 개의 구현·검증 완료 전에는
기존 Python 24개/caller를 retire하지 않는다. 이번 단계는 #523 전체 완료가 아니다.

## 2026-10-26 Miri 실패 전파를 21번째 소스 검사로 추가

같은 bounded 쓰기 범위에서 `planner_ci_policy.rs`에
`planner_miri_failure_cannot_be_hidden_by_log_capture`를 추가했다.
`rust.yml`의 `planner-memory`에서 `MIRIFLAGS=`를 포함한 실제 step 본문을 읽고
그 문자열을 수정하지 않은 채 원본과 같은 `bash -e -c` argv로 실행한다.
테스트가 별도로 `pipefail`을 강제하지 않아 원본 Bash의 pipeline 실패 전파가
깨지면 42 기대값이나 호출 횟수 조건에서 실패해야 한다.

원본과 같은 `python` 성공 fixture와 `cargo` 실패 fixture를 임시 PATH에 둔다.
가짜 Cargo 본문은 `MIRI_FAILURE_CALLS`에 `failed`를 append하고 42로 종료한다.
외부 Bash 종료 코드가 정확히 42인지, calls 파일이 정확히 `failed` 한 행인지
검사한다. 전체 target은 이제 구조/변조 20개와 이 검사까지 **21개**다.

순수 unit의 GITHUB_WORKSPACE는 임시 workspace로 격리하고,
`rust/tools/check-disk-space` 경계만 성공하는 shell fixture를 둔다.
실제 disk bootstrap이 가짜 Cargo를 먼저 호출하여 Miri 실패로 오인하는 경우와
실제 SDK/build 의존을 피하기 위한 것이다. 검사 대상 Bash나 Miri pipeline은
바꾸지 않는다. 10초 상한과 프로세스 정리는 기존 `qa_process`가 담당한다.
이 검사는 Miri 기능 실행이나 메모리 검증 합격의 증거가 아니다.

### Cargo 및 Actions 연결

소스 확인상 `autobins = false`만 있으며 `autotests = false`는 없다.
`tests/planner_ci_policy.rs`는 Cargo 자동 test 탐색 대상이므로 새 `[[test]]`
등록이 필요 없다. `ci_policy`, `qa_process`는 feature 조건 없이 공개되고,
`tempfile`, `serde_json`도 기존 비선택 의존성이므로 **새 feature나 의존성도
필요 없다**. 공유 Cargo 파일은 수정하지 않았다.

부모가 Focus nativeunit 및 새 전용 GHA에 다음 target을 연결한다.
사용자의 최신 로컬 runner 조건 80GB/4core/4GB RAM과 순수 unit의
`CARGO_BUILD_JOBS=1` 배정 지시를 기록한다. runner/workflow 변경은 부모 전담이다.
아래는 Actions용 selector이며 로컬에서는 실행하지 않았다.

```sh
CARGO_BUILD_JOBS=1 cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --test planner_ci_policy
CARGO_BUILD_JOBS=1 cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --test planner_ci_policy planner_miri_failure_cannot_be_hidden_by_log_capture -- --exact
```

수정한 Rust 파일에만 설치된 Rust 1.94.0의 절대 `bin/rustfmt --edition 2021`을
실행하여 통과했다. 이번 단계도 컴파일/테스트/프로젝트 CLI 로컬 실행은 0회이며,
21개 검사와 Miri 실패 전파의 실제 결과는 Actions에서 확인해야 한다.
나머지 3개 실행 회귀는 Python controller에 의존하므로 미완료다.
기존 Python 24개와 caller는 유지하고, 세 실행 회귀 이전 및 전체 검증 전에는
all24 retirement나 #523 전체 완료를 선언하지 않는다.
