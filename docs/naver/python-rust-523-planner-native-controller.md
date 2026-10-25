# #523 Planner native controller와 실제 pacing endpoint 소스 인계

2026-10-26. 작업 루트는
`/home/fma1/.codex/worktrees/python-rust-523-completion/openpilot-rust`다.
소스 인계 직전 읽은 HEAD는 `126fdbea36598566d1040be0a35a365c909be4e4`다.
부모가 batch commit 중인 Planner CI native contract 21개·release 3개 및
기존 공유 파일은 수정하지 않았다. 이번 쓰기는 새 Rust 소스·새 테스트·이 문서뿐이다.
commit/push/공유 mod·Cargo·Focus·workflow 등록과 기존 caller 전환은 부모 전담이다.

## 실제 실행 경로 확인

기존 Rust에서는 `qa_process`의 실제 child/로그/timeout/프로세스 그룹 정리와
`openpilot-disk-space::inspect`의 작업별 성장 admission을 재사용할 수 있었다.
Planner 전용 비교 controller나 `next_model_target` 대체 경로는 찾지 못했다.
`openpilot-plannerd`의 차량 런타임은 이 QA 송신 스케줄러의 대체 대상이 아니다.

- `rust/tools/check_plannerd_ci.py::main`은 `runtime_checks`의 12개 leaf를
  `run_checks`로 순서대로 실행하고, 전부 성공한 뒤 실행물 SHA/source commit과
  PASS receipt를 기록한다. host/ARM workflow의 실제 비교 단계가 이 main을 호출한다.
- `rust/tools/check_plannerd_native_memory.py:14,38,39`도 같은 `Inputs`,
  `runtime_checks`, `run_checks`를 import하여 `abi/owner/fault`만 실행한다.
  이 import는 현재 남아 있으며, memory wrapper 전체의 변환은 이번 완료 범위가 아니다.
- `rust/tools/plannerd_ipc.py:123`은 각 프레임을 실제 publish하기 전에
  `next_model_target`을 호출한다. 이전 송신+50ms를 지키고, 모델 처리가 50ms 이상이면
  이전 처리 완료+50ms까지 회복하며 catch-up burst를 만들지 않는다.
- `plannerd_lifecycle_peer.py`는 `plannerd_ipc`의 `OUTPUTS/peers_ready/policy`도
  import한다. pacing 이전만으로 IPC 모듈 전체를 지울 수 없다.

## 추가한 실제 native 경로

| 새 파일 | 소유 동작 |
| --- | --- |
| `rust/crates/dev-tools/src/planner_ci_controller.rs` | `Check`, 실제 `run_checks`, 종료 코드가 있는 `CheckFailure`, 명령별 로그·`commands.json`, 첫 실패 즉시 반환 |
| `rust/crates/dev-tools/src/planner_ci_controller/plan.rs` | 원래 12개 leaf의 순서·전체 argv·output option, 실행물/binding/solver SHA 및 최종 receipt |
| `rust/crates/dev-tools/src/planner_ci_controller/scheduler.rs` | 실제 pacing 함수와 지속 stdio endpoint, endpoint 호출 증거 검증 |
| `rust/crates/dev-tools/src/planner_ci_controller/cli.rs` | native 비교 main과 `schedule` endpoint 진입점 |
| `rust/crates/dev-tools/src/bin/check_plannerd_ci.rs` | 등록할 `check-plannerd-ci` 실행 파일. 실패 child의 양수 종료 코드를 외부로 전달 |
| `rust/crates/dev-tools/tests/planner_ci_controller.rs` | 실제 controller/endpoint를 호출하는 native 8개 및 Actions 원본 비교 3개 |

이 경로는 테스트 전용 수식을 복제한 것이 아니다. 공개 scheduler API, 실제 stdio
endpoint, endpoint 호출 TSV 검증이 같은 함수를 사용한다. 공개 `run_checks`와
실제 CLI 모두 기존 `NativeRunner`로 child를 실행하고 종료 상태·로그를 기록한다.
원본 비교 Python leaf는 현재 명시적인 `--python` 실행물로 계속 호출한다.
IPC wire/message 생성·원본 planner·lifecycle 전체를 Rust로 바꿨다는 뜻은 아니다.

명령 실패 시 원본처럼 해당 행의 name/argv/returncode/free_before와 merged raw log를
먼저 남기고 다음 leaf 전에 반환한다. CLI는 42를 받은 경우 42를 전달한다.
spawn/timeout/admission 실패도 PASS가 되지 않으며, 기존 출력 디렉터리는 재사용하지 않는다.
최종 PASS/source/실행물 SHA receipt는 모든 실제 비교와 scheduler 증거가 성공한 뒤에만 쓴다.

고정 25GiB+512MiB/35GiB 복귀 정책은 제거된 사용자 지시에 맞춰 이식하지 않았다.
12개 명령 각각에 이름이 붙은 예상 증가량을 필수 입력으로 받아 실행 직전 실제 파일시스템
가용량과 비교한다. 누락/중복/다른 명령명·overflow·0초 timeout을 거부한다.
테스트 fixture의 growth=0은 출력이 작은 합성 child의 입력이며, 실제 12-leaf admission
예산으로 승인된 값이 아니다. 부모가 실제 작업 증가량을 정해 공급해야 한다.

## 부모가 연결할 정확한 등록

`rust/crates/dev-tools/src/lib.rs`에 다음 공개 모듈 한 줄이 필요하다.

```rust
pub mod planner_ci_controller;
```

`autobins = false`이므로 공유 `rust/crates/dev-tools/Cargo.toml`에는 다음 bin 등록이
필요하다. 기존 serde/serde_json/sha2/tempfile/disk-space/qa_process를 사용하므로
새 의존성이나 feature는 필요 없다.

```toml
[[bin]]
name = "check-plannerd-ci"
path = "src/bin/check_plannerd_ci.rs"
```

새 test target은 자동 탐색되는 `planner_ci_controller`다. `CARGO_BIN_EXE_check-plannerd-ci`
를 검사하므로 위 bin 등록이 선행되어야 한다. 공용 mod/Cargo는 직접 수정하지 않았다.

Focus의 source pins·실행 목록과 새 unit GHA에 이 target을 연결하는 일은 부모가 맡는다.
그 전에 기존 `planner_ci_policy` 21개 파일/pin과 release 3개 batch를 건드리지 않는다.

## 실제 IPC caller 연결 계약 — 아직 부모 작업

새 controller의 `ipc` leaf는 기존 argv를 그대로 유지하면서 다음 환경을 넘긴다.

- `PLANNER_NATIVE_SCHEDULER`: 현재 실행 중인 native CLI의 절대 경로.
- `PLANNER_NATIVE_SCHEDULER_EVIDENCE`: 새 비교 출력 아래 `ipc-scheduler.tsv`.

부모는 `plannerd_ipc.py::run`의 실제 송신 루프에서 지속 프로세스
`[PLANNER_NATIVE_SCHEDULER, "schedule"]`를 하나 열고 `:123`의 호출을 연결해야 한다.
요청마다 Python float의 round-trip 문자열 세 개를
`repr(nominal) + '\t' + repr(previous_sent) + '\t' + repr(previous_model_done) + '\n'`
형태로 stdin에 쓰고 flush한다. stdout 한 행을 float로 읽어 기존 `target`에 사용한다.
정상 경우 stdin을 닫아 EOF로 종료시키고 wait하며, 예외에도 terminate/kill·wait한다.
별도의 detached process나 프레임마다 새 프로세스를 만들지 않는다.
기존 모델 입력·메시지·프레임 수·wait/publish 순서·결과 비교는 유지한다.
endpoint 오류나 EOF를 원래 Python 수식으로 fallback하여 숨기지 않고 실패로 전파한다.

stdio endpoint는 TSV 파일에 요청 세 시각과 반환 target을 기록하고 flush한 뒤 응답한다.
기존 기본 40프레임 × source/rust × 2epoch = 정확히 160행을 controller가 요구하고,
각 행의 정책 결과도 IEEE float bits로 다시 검증한다. 따라서 기존 IPC caller가
이 환경을 무시하거나 endpoint 사용이 빠지면 성공으로 표시되지 않는다.
**현재 기존 caller를 수정하지 않았으므로 실제 12-leaf 실행의 scheduler gate는
부모 연결 전까지 통과할 수 없다.** native 실제 경로와 연결 미완료를 구분한다.

host `rust.yml`, reusable `rust-native-arm.yml`의 실제
`Compare exact policies, main loops, real IPC and lifecycle` 단계는 부모가 새 CLI로
전환한다. 소스 검색에 남은 `python-rust-523-native-arm-more.yml`의 옛 caller도
유지/폐기 상태를 부모가 확인한다. memory wrapper는 별도로 전환해야 한다.
기존 Python/Rust CI contract의 옛 명령 기대값도 같은 통합에서 갱신해야 한다.

## 검사 의미와 원본 비교

native 8개는 pacing 2개, 첫 실패 기록/중단 1개, 등록된 CLI의 실제 exit42 전파,
성장 admission 거부, scheduler 미사용 거부, 등록된 endpoint의 실제 요청/증거,
12개 명령과 admission 입력 계약을 검사한다. pure unit에는 Python 실행이 없다.

별도 `#[ignore]` 원본 비교 3개를 작성했다. 이들은 Actions의 명시 실행이 필수다.

1. `source_scheduler_matches_native_protocol_without_reimplementing_the_oracle`:
   실제 `plannerd_ipc.py`에서 원본 함수를 AST로 추출해 실행한다. 경계·overrun을 포함한
   72개 입력에 대해 native endpoint 결과와 IEEE bits를 비교한다. 원본 수식을 재작성하지 않는다.
2. `source_runtime_plan_matches_every_native_leaf_and_argument`:
   실제 `check_plannerd_ci.py::runtime_checks` 결과와 12개 name/전체 argv/output option을 비교한다.
3. `source_failure_forward_matches_native_controller_and_raw_log`:
   실제 원본 `run_checks`와 native controller가 동일 Python fixture child를 실행하도록 하여
   exit42, 첫 행·명령, merged raw log, 다음 sentinel 부재를 비교한다. 원본의 폐기된 고정
   disk floor는 이 실패 전파 비교에서만 `disk_usage` 관측값을 주입해 독립시킨다.
   자원 정책 동등성을 주장하지 않으며 native admission은 별도 음성으로 검사한다.

이 비교들은 아직 실행하지 않았다. 원본 파일을 그대로 읽으므로 향후 삭제 전에는
부모가 원본 보존/역변환 증거 경로와 source pin을 먼저 확정해야 한다.

## Actions에 필요한 명령

부모의 80GB/4core/4GB RAM 전용 runner에서는 `CARGO_BUILD_JOBS=1`을 유지한다.
위 mod/bin 등록 뒤 pure unit 8개와 원본 비교 3개를 분리한다.

```sh
CARGO_BUILD_JOBS=1 cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --test planner_ci_controller
PLANNER_REFERENCE_PYTHON="$(command -v python)" CARGO_BUILD_JOBS=1 cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --test planner_ci_controller source_ -- --ignored --test-threads=1
```

기존 Planner build/ABI/source 준비 단계 뒤의 실제 비교 호출은 다음 형태다.
각 growth와 timeout은 부모가 실제 작업 증가량·허용 시간으로 정하고 비어 있으면 거부한다.
이 문서는 자원 수치를 임의로 승인하지 않는다.

```sh
"$CI296_SOURCE/rust/target/debug/check-plannerd-ci" \
  --root "$CI296_SOURCE" --python "$(command -v python)" \
  --bin-dir "$CI296_SOURCE/rust/target/debug" \
  --artifact "$RUNNER_TEMP/planner-artifact" --source-native "$RUNNER_TEMP/planner-source" \
  --binding "$PLANNER_PARAMS_BINDING" --output "$RUNNER_TEMP/planner-comparison" \
  --timeout-seconds "${PLANNER_CHECK_TIMEOUT_SECONDS:?}" \
  --growth-mib "abi=${PLANNER_ABI_GROWTH_MIB:?}" \
  --growth-mib "policy=${PLANNER_POLICY_GROWTH_MIB:?}" \
  --growth-mib "radar=${PLANNER_RADAR_GROWTH_MIB:?}" \
  --growth-mib "gap=${PLANNER_GAP_GROWTH_MIB:?}" \
  --growth-mib "owner=${PLANNER_OWNER_GROWTH_MIB:?}" \
  --growth-mib "main=${PLANNER_MAIN_GROWTH_MIB:?}" \
  --growth-mib "fault=${PLANNER_FAULT_GROWTH_MIB:?}" \
  --growth-mib "volkswagen=${PLANNER_VOLKSWAGEN_GROWTH_MIB:?}" \
  --growth-mib "stock-longitudinal=${PLANNER_STOCK_LONGITUDINAL_GROWTH_MIB:?}" \
  --growth-mib "radar-off=${PLANNER_RADAR_OFF_GROWTH_MIB:?}" \
  --growth-mib "ipc=${PLANNER_IPC_GROWTH_MIB:?}" \
  --growth-mib "lifecycle=${PLANNER_LIFECYCLE_GROWTH_MIB:?}"
```

같은 source의 host/native ARM에서 실제 12개 비교·scheduler160행·final receipt를 확인해야 한다.
원래 기능·메모리·ARM gate를 줄이거나 source leaf를 성공 stub으로 바꾸지 않는다.

## 이번 로컬 검증과 미완료

새 Rust 파일만 설치된 Rust1.94.0의 절대 `rustfmt --edition 2021`로 서식/구문을
검사해 통과했다. 새 테스트에 포함한 원본 비교 Python snippet 3개는 `ast.parse`만
수행하여 구문 통과했다. Cargo/컴파일/테스트/Clippy/원본/모델/프로젝트 CLI 로컬
실행은 **0회**다. caller 전환, 컴파일과 native8/source3, 실제 host/ARM 비교는 미검증이다.
남은 회귀 3개에 대한 실제 native 구현 소스는 제공했지만, 원본 비교와 실제 caller
연결 검증 전에는 그 세 개나 all24 retirement를 완료로 처리하지 않는다.
이번 Python 삭제 수량은 **0개**이며 #523 전체 완료나 실기 수용을 주장하지 않는다.
