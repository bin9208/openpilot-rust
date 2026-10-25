# #523 Planner native scheduler client와 원래 C++ IPC peer 구현 준비

2026-10-26. 최초 분석 HEAD는 `7a82b4e21283cf540f57c88689f58b2e8aca9f07`이다.
작업 루트는 `/home/fma1/.codex/worktrees/python-rust-523-completion/openpilot-rust`다.
사용자의 후속 지시에 따라 Python peer worker/relay 제안은 철회하고 **새로 만들지 않았다**.
현재 실제 Python adapter symbol은 `plannerd_ipc.py::ModelScheduler`다.

선행 단계에서 새 scheduler_client/새 planner_ipc_native 디렉터리/새 client tests/새 C++ peer와
이 문서를 작성했다. 이후 허용된 원본 입력 capture 변경은 문서 끝에 따로 기록한다.
shared CLI/plan/Cargo/mod 등록·WF·pin·기존 Python 삭제는 하지 않았다.
부모의 memory15/PathBuf cleanup와 Hypatia CompilerBuffers 파일은 수정하지 않았다.

## 구현한 실제 source 경로와 현재 한계

`scheduler_client.rs`는 같은 `check-plannerd-ci schedule`의 persistent child를 소유한다.
Unset direct helper와 empty 오류, one spawn, 부분 nonblocking write/read를 공유2초 안에 처리,
256바이트 응답 budget, sticky failure, 3초 EOF/wait→TERM/wait→KILL/reap와 명시적 finish를
구현했다. 부모 오류가 있고 회수가 성공하면 원래 오류를 유지한다. Drop만으로 close 성공을
판정하지 않는다. endpoint는 상위 QA process group을 상속한다.

새 `planner_ipc_native::run(Options)`는 이 client를 **실제 frame loop에서 소비하는 callable
source**다. native C++ publisher/subscriber와 source/Rust daemon을 시작하고 source/rust×2epoch,
40frames/run을 수행한다. 기존 bin의 새 IPC subcommand에 부모가 연결해야 한다.
아직 기본 CLI/plan에는 연결되지 않았으며 실제 native IPC 성공을 주장하지 않는다.

중요한 blocker는 독립 raw 송신 입력 corpus다. `Corpus::open`은 원본 capture의 manifest와
호출자가 고정한 SHA가 없으면 출력/peer build/daemon 실행 전에 거부한다. 입력을 Rust에서
생성하거나 다른 owner/main 시나리오로 바꾸는 fallback은 없다. 원래 Python daemon/fixture/
oracle와 기존 adapter13은 유지한다. 이번 source 삭제·retirement credit은0이다.

## 원래 C++ transport 보존

`driver_daemon_peer.cc`, `driving_daemon_peer.cc`, `msgq/native/peer.cc`를 읽고 동일한
`Context/PubSocket/SubSocket` 직접 링크 방식을 사용했다. 새 `planner_ipc_peer.cc`는 아래
**원래 구현**을 사용하며 Python binding을 통해 transport를 호출하지 않는다.

- `msgq_repo/msgq/ipc.cc`, `event.cc`, `impl_msgq.cc`, `impl_fake.cc`, `msgq.cc`.
- 원래 IPC/header와 service queue size; conflate=false/check_endpoint=true.
- 10개 input publisher는 daemon 전에, 3개 output subscriber는 reader readiness 뒤에 연다.
- raw 파일 publish, 원래 `all_readers_updated`, nonblocking drain을 제공한다.
- file 경로는 hex로 교환해 공백/작은따옴표/UTF-8을 shell 해석 없이 전달한다.

peer를 Rust msgq 구현으로 대체하지 않았다. `build_peer`는 기존 isolated C++ peer build와
같은 C++17/pthread/source 직접 링크 및 services_header를 사용한다. native peer binary,
원래 C++ source/header, generated services.h, compiler와 원래 binding builder SHA는
`native-peer-provenance.json`에 분리한다. 새 peer SHA를 기존 ipc_pyx SHA로 표기하지 않는다.

## 실제 호출부·binding 연결표

| 소비자/단계 | 현행 유지·새 연결 |
| --- | --- |
| main host/reusable ARM/standalone ARM Planner | 기존 Bash runtime helper→native controller→native12. 부모가 `ipc` leaf만 새 subcommand로 전환 |
| 전체 native12 | `abi/policy/radar/gap/owner/main/fault/volkswagen/stock-longitudinal/radar-off/ipc/lifecycle` 순서 보존; 다른11개는 변경 없음 |
| 새 IPC driver | 새 C++ peer + persistent scheduler client + 원래 source Python daemon/실제 Rust daemon; Python worker/relay 없음 |
| source daemon | 기존 `python -P rust/tools/plannerd_daemon_source.py --binding … --source-native …`와 venv/PYTHONPATH 유지 |
| native daemon | 기존 `openpilot-plannerd --solver …` argv 유지 |
| 실제 scheduler | `Options.scheduler`가 current_exe와 같은 실행물인지 요구. 실제 schedule child만 4회/160요청; driver가 TSV를 합성하지 않음 |
| evidence | 부모 run_checks가 준 `PLANNER_NATIVE_SCHEDULER_EVIDENCE` 경로를 Options.evidence로 연결. 새 file 요구, 실제 server append/flush, driver와 상위 run_checks가160/bit 검증 |
| 기존 Params binding | `Options.binding`을 원래 source daemon에 그대로 전달; 기존 native identity receipt에 실제 SHA 유지 |
| 기존 ipc_pyx binding | `original_ipc_binding(dir)`가 isolated `ipc/msgq`의 유일한 regular `ipc_pyx.*.so`를 native 파일 조회로 찾음. 새 Python import/query process 없음 |
| module 선택 | 그 package 부모가 실제 PYTHONPATH 첫 entry인지 요구. 원래 source daemon의 isolated import 경로와 identity 입력을 연결 |
| 비교 identity | 기존 네 개: Rust daemon/Params binding/original ipc_pyx module/artifact manifest SHA를 현재 실제 파일에서 기록·전후 재확인 |
| 별도 native peer proof | 위 original binding SHA와 새 C++ binary/source/header/compiler SHA를 분리해 기록 |
| lifecycle | 기존 별도 Python lifecycle peer/daemon 비교 유지; scheduler client 소비자로 오인하지 않음 |
| memory3 | abi/owner/fault만 사용; 이번 CPP/IPC 구현과 memory15를 섞지 않음 |

기존 build_plannerd_acados/build_plannerd_source/build_params_python/build_msgq_python 단계는
부모 caller가 계속 유지해야 한다. 새 driver가 reference ipc_pyx를 직접 load하지는 않지만,
reference source의 선택 경로·기존 binding SHA 계약을 없애지 않는다. 실제 C++ transport의
대체 실행물은 별도 proof와 actual source/native 비교로 검증해야 한다.

## 포트한 orchestration과 수용 조건

`frame_loop.rs::drive40`을 생산 driver와 client 회귀가 같이 사용한다. 프레임마다 target1회,
model send→driverAssistance wait→target+25ms→liveTracks send/wait의 순서를 유지한다.
nominal/previous_sent/previous_model_done은 기존 순서 비교 정책으로 전달한다.

기존15초 readiness, 2초 publication, startup150ms+live-only120ms 검사, 일부 live frame의5ms
wait, 종료 전30ms drain, daemon SIGTERM/-15와 원래 wait3초→실패 시 KILL/wait3초를 구현했다.
scheduler client 자체의 close는 이와 별도로 총3초다. source/rust의 epoch별
Params prefix를 유지하고 CarParams.pending→rename도 raw bytes로 실행한다.
4개 run에서 valid publication3종/modelV2+liveTracks trigger/fast overlay가 모두 있어야 한다.

full Cereal schema의 dynamic projection은 기존 driving_daemon_reference 패턴을 따른다.
`processingDelay/solverExecutionTime/plannerExecutionTime/fastRadarExecutionTime`만 제외하고
float는 원래 difference처럼 bits로 비교한다. source-0의 원래 Python receipt와 raw 출력3종을
비교하는 별도 관측 회귀2개를 준비했다. 이 회귀는 source-only로 아직 실행하지 않았다.
새 비교의 실패 path/length/key/float hex 필드는 보존하지만 원래 dict 순서와 모든 비정상
publication의 diagnostic 동등성까지 관측했다고 주장하지 않는다.

C++ peer command ACK와 Rust decoder는 새 지연 경계다. 기존 기한을 늘려 숨기지 않으며,
actual timing·출력 비교·회수를 다음 Actions에서 확인해야 한다. 전체 output admission은
기존 IPC256 MiB를 유지한다. 새 C++ build의 증가량은 별도 `peer_build_growth_bytes` 양수
입력이 필요하고 실제 filesystem에 재검사한다. 180초 build cap도 전체 operation 기한 안이다.
호스트2700초/ARM5400초와 jobs1을 부모가 그대로 공급해야 한다.

## 원본13개별 semantic closure

아래 이름은 `test_plannerd_native_scheduler.py`의 실제13개 정의다. 기존 Rust의 수학
회귀만으로 닫힌 항목과 새 client/caller가 필요한 항목을 구분한다.

| 원본 test | Rust 이식에서 닫아야 할 의미 |
| --- | --- |
| `test_independent_oracle_bytes_are_unchanged` | 현재 독립 함수의 exact bytes SHA 유지; frozen 결과의 역사 provenance와 구분. 생산 Python oracle의 삭제 근거가 아님 |
| `test_actual_run_loop_uses_one_scheduler_context_and_one_request_per_frame` | 실제 native IPC driver에서 run당1 session/frame당1 target. source AST 모양을 Rust로 복사하지 말고 실제4/160 소비 기록 확인 |
| `test_absent_environment_preserves_oracle_signed_zero_and_nan_without_spawning` | None와 empty 구분; direct 정책에서 signed zero/NaN bits 유지, child/stderr 생성0. wire finite 제한을 direct helper로 확장하지 않음 |
| `test_persistent_process_serves_forty_requests_and_is_reaped_on_eof` | 단일 child40회, EOF 정상 exit0, stdin/stdout/stderr 회수와 retry 없는 종료 |
| `test_empty_setting_is_an_error_instead_of_oracle_mode` | 설정 존재+빈 값은 fatal/nospawn; unset으로 취급하지 않음 |
| `test_spawn_failure_does_not_fall_back` | spawn/pipe 설정 실패도 fail-closed; 열린 stderr/성공한 child가 있으면 회수 |
| `test_eof_exit_and_invalid_response_fail_closed_and_reap` | EOF/exit42/invalid/nan/300bytes/한 write의2행 모두 실패 latch와 reap; server 성공 검사로 대체 불가 |
| `test_response_timeout_fails_closed_and_reaps` | write+read 전체가 같은2초 deadline, 부분 I/O/WouldBlock 재시도도 같은 budget |
| `test_caught_eof_does_not_allow_retry_or_successful_context_exit` | 호출자가 첫 오류를 잡아도 다음 target 즉시 거부, 정상 finish로 바꿔 성공시킬 수 없음 |
| `test_abnormal_close_status_is_not_silently_accepted` | target 성공 후 EOF close42도 finish 실패; 종료 코드 증거 유지 |
| `test_normal_close_escalates_term_kill_wait_and_reports_failure` | 정상 close에서 EOF/wait→TERM/wait→KILL/remaining wait를 단일3초 안에 수행; 강제 회수가 필요하면 실패 |
| `test_exception_cleanup_reaps_stubborn_child_and_preserves_parent_error` | 외부 작업 오류 중 child를 회수하고 기존 부모 오류를 유지. Rust Drop만으로 오류 반환을 구현하지 않음 |
| `test_real_native_binary_receives_all_160_calls_with_matching_targets` | 실제 유지 bin의4회 spawn/40×4 request·oracle와 u64 bits 일치·160 evidence. 수용 CI에서는 skip 불허 |

현재 원본의 세부 정책도 보존해야 한다. native target 실패는 이후 호출을 latch한다.
응답은 ASCII float 한 행이고 최대256바이트 읽기 budget이다. 같은 read buffer에서 newline
뒤 extra bytes가 있으면 거부한다. 나중에 별도 write로 도착한 unsolicited 행까지 탐지한다고
현재 원본보다 강하게 주장하지 않는다. `BufReader::read_line`으로 extra를 숨겨 다음 요청에
넘기는 구현은 원본의 multiple-response 거부와 다르다.

Rust `parse::<f64>()`를 Python `float(ASCII line)`과 같은 parser라고 가정하지 않는다.
공백/underscore 등 허용 문법의 차이는 현재13개의 단일 `invalid` fixture만으로 닫히지 않는다.
이식 전에 실제 응답 grammar를 동일하게 처리할지, 공식 endpoint의 canonical float wire로
명시할지 결정·검증해야 한다. wire를 조용히 좁혀 full parity라고 기록하지 않는다.

close는 stdin을 먼저 닫는다. 정상 close의 첫 wait 최대1초, TERM 뒤 wait 최대1초,
KILL 뒤 남은 wait를 포함해 총3초다. 부모 예외/failed latch가 있으면 TERM을 먼저 보낸다.
회수가 성공한 부모 오류 경로와 회수 자체도 실패하는 경로의 오류 우선순위는 구분한다.
현재 예외 회수 검사는 KILL/reap 성공 시 부모 ValueError 보존을 확인한다. 모든 cleanup
실패까지 같은 우선순위라고 일반화하지 않는다. Rust client에는 명시적인 finish/close Result가
필요하고 Drop은 보조 회수만 담당해야 한다.

## 독립 corpus 조사와 capture 필요표

부모가 감사한 source425 functional12612670103 ZIP(55,186,524 B,
SHA `96b275577f6d9eec3fe106d65b258e00cbfcd3be8421d0659122e73a6f007252`)에서
IPC member1241개를 조사했다. 원본 source-0 receipt와 3개 valid 출력 packet, 원래 CarParams만
선택해 새 디렉터리 `fixtures/source425/`로 바이트 그대로 복사했다. 선택 member의 CRC는
ZIP read에서 확인했고 전체 55 MiB 감사는 반복하지 않았다.

| 자료 | 관측 상태/재사용 | 다음 필요한 capture |
| --- | --- | --- |
| CarParams | 280 B; 네 source/rust epoch의 bytes가 동일. SHA `74de9e672b5d93d21bfeeb57ec6f45677bec4dce85afeffcee027b11c15763df` 원문 보존 | 전체 input manifest에 원래 source425 provenance로 결합 가능 |
| Params 파일 | 종료 후 각 case의 파일은 artifact에 존재 | daemon 전 초기 Params snapshot과 key별 raw bytes/SHA를 별도 봉인. 종료 후 값을 초기값으로 추측하지 않음 |
| initial liveTracks | source script는 500000000/empty point를 실제 송신하나 raw 입력 파일 없음 | 원래 C++ send에 실제 전달한 serialized bytes 원문 |
| 40×9 model/비-model 입력 | source script가 messages(45+index,1+index×0.05) 뒤 실험모드/gas/정지/lead 변화를 적용; raw 송신 입력은 미보존 | mutation 적용 후 실제 publish한9개 packet의 순서·bytes/SHA, 40frames |
| 40×liveTracks | 출력 record의 longitudinalPlan은 입력 liveTracks가 아님 | 실제 liveTracks bytes/SHA40개; invalid index19 및 lead 값 포함 |
| source 출력 decoder | 원본 receipt452536 B/SHA `a5ac1d44574a05c1d94488c5a0a872375f97b17e85b360b487e307fd167c1b4d`와 raw3종을 보존 | 새 decoder 관측 회귀를 feature CI에서 실행 |
| Python public parser | 기존13 log는 성공하지만 확장 parser 수용/거부의 raw 결과 corpus는 없음 | 실제 ModelScheduler에서 ASCII 응답별 결과bits/오류/latch/close status 관측 |

model 입력은360개, liveTracks40개, initial live1개인 **401개 event**와 CarParams/Params가
필요하다. native 출력·예상치나 다른 owner/main input으로 대체하지 않았다.
`PROVENANCE.json`은 원본 ZIP member 이름/bytes/SHA와 source425를 기록한다.
capture 때는 serialization을 한 번만 수행하고 같은 bytes를 기록·전송하거나 원래 binding의
send 직전 bytes를 관측해야 한다. native 예상값이나 별도 재직렬화 결과를 실제 송신 bytes로
기록하지 않는다. capture hook와 원래 generator source SHA도 구분해 봉인한다.

`Corpus::open`의 manifest는 schema_version1/kind=`independent-original-planner-ipc-inputs`,
source_commit/source_sha256/capture_provenance, params map, car_params, initial_live, frames40이다.
각 raw file은 `{path,bytes,sha256}`, 각 frame은 `{index,packets:[{service,raw}],live}`다.
원래 packet 순서8개 non-model 뒤 modelV2를 요구한다. raw bytes는 file/hash/path를 재확인하고
총64 MiB/각16 MiB를 넘으면 거부한다. symbolic link/경로 탈출과 native 입력 생성은 허용하지 않는다.

capture source roster는 최소 plannerd_ipc.py/owner_fixtures.py/params_keys.h/log·custom·car schema/
services.py/modeld constants를 포함한다. 현재 파일 SHA를 대조하고 run 뒤에도 확인한다.
provenance에는 Python 실행물 SHA/version, NumPy version, origin run/artifact ID가 필요하다.
manifest 전체 SHA는 부모가 원본 관측에서 고정해 전달한다. 현재는 이 manifest를 만들지 않았다.

public parse capture는 공식 server canonical 응답과 분리한다. 예시 `+0.0`, 앞뒤 공백,
underscore, CR, nonfinite, 한 write의 복수 행에서 실제 원본 결과/예외를 받아야 한다.
새 `parse_canonical_response`는 공식 server Debug round-trip finite 문자열만 받는다.
그 밖의 Python float grammar를 native로 추측 구현하거나 동등하다고 기록하지 않는다.
따라서 원래 public adapter의 full parser closure와 원본13 삭제는 아직 승인 근거가 없다.

## 새 테스트와 부모의 정확한 등록

`tests/planner_scheduler_client.rs`는 일반13/ignored0이며 기존 source 파일을 `#[path]`로
직접 읽어 production client/frame_loop를 검사한다. 원래 Python class를 import/실행하지 않는다.
실제 binary160은 기존 independent frozen finite76에서 signed-zero4+36개를 골라4회 반복한다.
expected u64는 원래 JSON 값이다. 원래 Python13의160 입력 순서 자체를 새 관측했다고
주장하지 않으며, 그 검사와 같은 session/IEEE/evidence 의미의 별도 native 회귀다.

부모 등록(이번에는 미수정):

1. 기존 planner_ci_controller 하위에 `pub mod scheduler_client;` 추가.
2. `planner-ipc-native = ["dep:capnp", "dep:openpilot-cereal", "serde_json/float_roundtrip"]` feature와 해당 feature 아래
   `pub mod planner_ipc_native;` 등록. 새 bin은 없다.
3. 추가 integration target `planner_ipc_observations`의 path를
   `src/planner_ci_controller/planner_ipc_native/observed_tests.rs`로 등록하고
   required-features를 `planner-ipc-native`로 설정. 원래 Python receipt/raw decoder와
   missing corpus fail-closed의2개다. 기존 test target과 lib.rs/다른 agent 파일은 건드리지 않음.
   `float_roundtrip`은 Python receipt의 decimal을 원래 f64 bits로 읽기 위한 필수 항목이다.
4. 기존 CLI에 `ipc` subcommand를 연결해 `planner_ipc_native::run(Options)`를 호출.
   기존 root/python/bin-dir/artifact/source-native/binding/output/timeout/growth를 그대로 매핑한다.
   scheduler는 current_exe, evidence는 상위 `PLANNER_NATIVE_SCHEDULER_EVIDENCE`다.
   compiler와 독립 corpus path/manifest SHA/peer-build growth, original IPC module path를 추가로
   공급해야 한다. module은 `$CI_JOB_TEMP/planner-original/ipc/msgq`에서 native 조회로 찾는다.
5. plan의 `ipc` argv만 기존 binary의 이 subcommand로 바꾸고 scheduler_calls=160과 growth256 MiB,
   기존 명령 순서·상위 로그·첫 실패 회수를 유지. frozen runtime12의 옛 Python argv 원문은
   덮어쓰지 말고 의도된 delegation 변경만 별도 계약으로 검증.
6. 실제 helper build에는 기존 ci-receipt-metadata-native feature를 보존하면서 planner-ipc-native를
   함께 활성화. 원래 source/Params/msgq build와 host/ARM SDK/예산을 유지.

부모 Actions의 독립 검증 명령(로컬 실행 금지):

```bash
CARGO_BUILD_JOBS=1 cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --test planner_scheduler_client
CARGO_BUILD_JOBS=1 cargo clippy --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --bin check-plannerd-ci --test planner_scheduler_client -- -D warnings
CARGO_BUILD_JOBS=1 cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --features planner-ipc-native --test planner_ipc_observations
CARGO_BUILD_JOBS=1 cargo clippy --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --features planner-ipc-native --bin check-plannerd-ci --test planner_ipc_observations -- -D warnings
```

그 다음 필요한 capture cohort에서만 독립 입력을 수집하고 actual native12 IPC/host+ARM을
검증한다. source425의 성공 cohort를 변경 없이 다시 돌리지 않는다. input corpus가 없으므로
현 단계에서는 full actual IPC 실행을 요청할 수 없다. parser 확장 capture도 미완료다.

## 작업·검증 경계

source425 adapter13(real Rust160)의 baseline은 부모 확인
run40227415620/job121271640924이고, 로컬 artifact12612286621의
`planner-scheduler-adapter.log`2442 B에서도13개 전부ok/6.346s/skip 부재를 읽었다.
이전 native 성공을 새 client/C++ peer/decoder의 빌드·기능 성공으로 대신하지 않는다.

이번 로컬 검증은 새 Rust source의 Rustfmt2021 구문/서식, JSON·선택 ZIP bytes/경로와 diff뿐이다.
Cargo/build/Clippy/test/Python oracle/C++ compile·기능 실행0, WF/GHA/stage/commit/push0이다.
기존 Python 원본/adapter13과 memory15/CompilerBuffers/부모 공유 파일은 수정하지 않았다.

기존 독립 source SHA는 다음과 같다.

| source | SHA256 |
| --- | --- |
| `rust/tools/tests/test_plannerd_native_scheduler.py` (221줄/13개) | `b6ab371f339ce746e3a12a58dc7fdbc798a8bdda4d9be7906e914b96aa6b4564` |
| `rust/tools/plannerd_ipc.py` | `c2099127901a2883925305dc3bf0c563b338494cd5bd829cb7bd3e916ffdd031` |
| `plannerd_ipc.py::next_model_target` | `a04e889c725715395baae751a5b3526c0c686b0075d67ab95b95fb94bd987be1` |
| `rust/crates/dev-tools/src/planner_ci_controller/scheduler.rs` | `a35ad7f6c4eab2348ef3ceae318e8bd37980e251e84e59d5d17480e7b9d55f0a` |

## 새 source SHA256

아래는 source-only 준비본의 고정값이다. 빌드/기능 PASS를 뜻하지 않는다.

```text
7035ba9a249fe3067a4bf09c32d2930b8453f9ba1645c49b3e43d4962fd06421  rust/crates/dev-tools/src/planner_ci_controller/scheduler_client.rs
f8f411b4908b2df72978f254a5eacb31436cdab311a524fac88d0d59c0f0a74c  rust/crates/dev-tools/src/planner_ci_controller/planner_ipc_native/corpus.rs
4f1583be3a7118fc53bd929a93ae985f5884050938a10f36006832b5332e4dbd  rust/crates/dev-tools/src/planner_ci_controller/planner_ipc_native/decode.rs
0cfde82d09829f10a07c721ab2fc1bfbc3d19c3bbf1ed84ceae3510de560d63f  rust/crates/dev-tools/src/planner_ci_controller/planner_ipc_native/frame_loop.rs
18ecd9aa2597b2bf9f5902d0b321952206ed43968c77f2e8d65b5f1bafdea653  rust/crates/dev-tools/src/planner_ci_controller/planner_ipc_native/mod.rs
19f66b6cd0557ffb78184b0fdf4a12c3566e7ab46e27d59272794e3f28a3f147  rust/crates/dev-tools/src/planner_ci_controller/planner_ipc_native/observed_tests.rs
b2aa4fec6a6ff7b04f9606997c76ddd7a324fbb4cc840c15047a37e3107ef438  rust/crates/dev-tools/src/planner_ci_controller/planner_ipc_native/peer.rs
c41972855ac0dcdc3cd0078b4c53faeefeb4080b35c6614e2b67e0b5229ae97c  rust/crates/dev-tools/tests/planner_scheduler_client.rs
bf626c2d2d157a5c824d7eb180cc3e0d79b55a79088d036f51d00f5c7e8257c5  rust/tools/planner_ipc_peer.cc
```

## 후속: 실제 4×40 송신 입력을 선택적으로 수집하는 source 패치

사용자가 추가 소유를 허용한 `rust/tools/plannerd_ipc.py`,
`rust/tools/tests/test_plannerd_native_scheduler.py`, 이 문서만 이번에 변경했다.
위 native client/driver/C++ source는 고정했고 공유 등록·wire 전환·삭제는 계속 보류한다.
수집 코드는 준비했지만 실제 raw corpus는 아직 없고 retirement credit은0이다.

실제 `openpilot/cereal/messaging/__init__.py::PubMaster.send`를 먼저 읽었다.
이 함수는 bytes가 아닌 입력에만 `to_bytes()`를 호출한 뒤 같은 `self.sock[s].send`로 보낸다.
capture가 꺼졌을 때 기존 네 publisher 호출부는 원래 message 객체를 그대로 전달한다.
capture가 켜지면 각 message를 **한 번만** 직렬화하고 그 bytes를 create_new 파일에 기록한 뒤
**같은 bytes 객체**를 같은 PubMaster에 전달한다. CarParams는 원래 `cp.to_bytes()` 한 번으로
쓴 실제 `CarParams.pending` 파일을 rename 전에 읽는다. 새 expected나 재직렬화본을 만들지 않는다.

설정은 두 경로 환경변수다. 각각 unset이면 수집하지 않으며 빈 값/상대 경로/다른 절대 경로는
거부한다. 현재 `.github/actions/job-temp`가 만든 `/tmp/openpilot-ci.XXXXXX`와 `owner`의
run/attempt/job, canonical path, 소유 uid와 job/proof의0700을 검사한다.
하위 디렉터리는 mkdir exist_ok=False/0700, 파일은 O_EXCL|O_NOFOLLOW/0600이다.
기존 디렉터리·파일에 덮어쓰거나 symbolic link를 따라가는 경로는 없다.

```text
PLANNER_IPC_CAPTURE_DIR=$CI_JOB_TEMP/proof/planner-ipc-original-inputs
PLANNER_SCHEDULER_PARSE_CAPTURE_DIR=$CI_JOB_TEMP/proof/planner-scheduler-original-parse
```

IPC capture의 구성과 순서는 다음과 같다.

| 경로/관측 | 실제 내용/개수 |
| --- | --- |
| `source-0/`, `source-1/`, `rust-0/`, `rust-1/` | 기존 실제 네 run 각각의 독립 디렉터리. raw header나 Params의 동일성을 가정하지 않음 |
| 각 run의 `params/<key>` | 실제 daemon Popen 전 파일의 raw snapshot. 기록 후 원래 Params를 수정하지 않음 |
| 각 run의 `CarParams.pending.bin` | 실제 pending 파일 bytes1개. 합계4개 |
| 각 run의 `events/0000-liveTracks.bin` | 원래 initial liveTracks1개 |
| 나머지 `events/<seq>-<service>.bin` | 원래 순서8개 non-model→modelV2→liveTracks를40회. 합계400개/run |
| 각 run의 `manifest.json` | schema_version1/kind=`independent-original-planner-ipc-inputs`; params/car_params/initial_live/frames40 및 raw 파일별 path/bytes/SHA |
| root `index.json` | schema_version1/kind=`independent-original-planner-ipc-run-index`; 네 manifest path/bytes/SHA와 events1604/frames160/CarParams4 |

run manifest는401개 실제 send 성공과40개 완전한 frame 뒤에만 쓴다. 최종 index의 COMPLETE는
기존 output receipt를 쓴 뒤 **원래 비교·valid/trigger/overlay assertions가 모두 성공한 후에만**
생성한다. 기존 receipt bytes 형식은 변경하지 않았고 index가 그 파일의 SHA를 결합한다.
source roster9개의 현재 SHA를 수집 전, 각 run 완료, 최종 완료에 재확인한다.
roster는 위8개 generator/schema source에 실제 PubMaster 구현을 추가한 것이다.
GITHUB_SHA, 실제 Python executable/canonical path/SHA/sys.version, 이미 로드된 NumPy version,
run/attempt/job/host architecture/daemon argv/epoch/prefix를 기록한다. 새 버전 조회 child는 없다.
기존 output receipt의 binary/binding/ipc_pyx/solver manifest identities도 계속 보존한다.

| 상한 | bytes/개수 |
| --- | --- |
| event packet / CarParams.pending | 각각1 MiB |
| Params 파일 / 파일 수 / run당 snapshot 합계 | 64 KiB / 2048개 / 4 MiB |
| run manifest / 최종 index | 각각1 MiB |
| run당 전체 / 네 run+index 전체 | 64 MiB / 256 MiB; raw와 JSON을 함께 계산 |
| parser-stage observations JSON | 64 KiB; 고정35개 line, 각255 bytes 이하 |

read/hash는 bounded regular file을 O_NOFOLLOW로 열고 FD/path identity와 size/mtime/ctime의
읽기 전후 동일성을 검사한다. packet/합계 상한 초과는 send 전에 거부하고 부분 수집에서
COMPLETE를 만들지 않는다. capture 파일 I/O의 시간은 실제 sent/model_done 등 관측에 포함된다.
원래 sleep/target 식/2초 publication/15초 readiness/종료 기한과 output 관측은 바꾸지 않았다.
daemon/scheduler/model/original process 수와 원래4×40 실행 횟수도 바꾸지 않았다.

adapter에는 새 collector test **1개**만 추가해 전체14개가 된다. 이름은
`CaptureCollectionTests.test_capture_preserves_single_serialization_four_runs_bounds_and_original_parser_stage`다.
기존13개 `NativeSchedulerTests` 클래스 전체, `ModelScheduler`, `next_model_target`은 source425와
바이트가 같다. 새 test의 가짜 packet/Params 자료는 임시 unit fixture이며 실제 IPC raw corpus로
업로드하지 않는다. 같은 bytes 전달/401×4/초기 Params 보존/개별·전체 상한/권한/중복 생성/
symlink 거부를 검사하도록 작성했다. 테스트 실행은 아직 하지 않았다.

이 한 test의 기존 interpreter 안에서 실제 `ModelScheduler.target`의 정확한
`float(line.decode('ascii'))`+finite check+return AST만 추출해 고정35개 line을 관측한다.
underscore/공백/지수/signed zero/invalid/nonASCII/overflow/underflow 등을 입력하고 결과 f64
big-endian bits 또는 실제 exception type/message를 남긴다. source/class/test/AST/Python SHA와
version/GITHUB_SHA도 결합한다. line은 실제 partition 결과와 같은 bytearray다.
이는 **parse stage만**의 독립 원본 관측이며 전체 client의 framing/latch/close 수용 검사가 아니다.
child 추가0이며 기존13개가 수행하는 child/real-native160 호출 횟수도 그대로다.

## 부모 Actions 연결과 남은 수용 경계

1. 최종 host/ARM의 기존 runtime helper 호출 step에 `PLANNER_IPC_CAPTURE_DIR`를 위 정확한
   경로로 공급한다. 기존 timeout/jobs1/SDK/source/binding 경로는 그대로 둔다.
   helper의 기존 IPC256 MiB 예상 증가량에 capture 최대256 MiB를 더해
   `PLANNER_IPC_GROWTH_MIB=512`를 공급하고 실제 job filesystem admission을 유지한다.
   capture 디렉터리를 먼저 만들면 create_new가 실패하므로 job/proof까지만 준비한다.
2. 기존 native-units planner adapter의 같은 unittest 명령에
   `PLANNER_SCHEDULER_PARSE_CAPTURE_DIR`만 추가한다. 기존
   `PLANNER_NATIVE_SCHEDULER_TEST_BIN`/실제160의 non-skip 검사는 유지하고
   `.github/workflows/python-rust-523-native-units.yml`의 `^Ran 13 tests in `을14로 갱신한다.
   새 별도 collector 프로세스/모델/worker를 추가하지 않는다.
3. `.github/workflows/python-rust-523.yml`의 현재 두 Python source SHA pin을 아래 새 값으로
   갱신해야 한다. `planner_controller_frozen.rs::SOURCE_PINS`와 기존 corpus의 PROVENANCE는
   역사 source73/425 원문 provenance이므로 현재 capture source SHA로 덮어쓰지 않는다.
   부모 소유 WF/pin/Focus/수집물 업로드는 이번에 수정하지 않았다.
4. 두 proof 디렉터리를 기존 artifact에 포함하고 raw bytes/SHA/401×4/Params와 실제 최종
   출력 성공을 함께 감사한다. 최종 index가 없거나 incomplete면 수집 성공으로 계산하지 않는다.
5. 업로드 전에는 artifact ID를 알 수 없어 manifest에 `origin_artifact_id`를 추측해 넣지 않았다.
   현재 미등록 `Corpus::open`은 그 필드를 요구하므로 실제 업로드 후 별도 provenance와
   고정 manifest SHA를 결합하도록 reader를 후속 조정해야 한다. 또한 현재 준비 driver의
   단일 corpus 입력은 실제 네 run manifest 중 해당 mode/epoch를 선택하도록 조정해야 한다.
   원문을 합치거나 header가 같다고 추정하는 방식으로 이 차이를 숨기지 않는다.
   raw401 수용 전 native C++/scheduler caller wire 전환과 Python 삭제는 계속 보류한다.

현재 source-only SHA256:

```text
a4aec1991a6751af60f5616698e75e0861b0a1f8b1c478e5382889681598c8dc  rust/tools/plannerd_ipc.py
bc9e2beb046308aab5c017950e90fabc627bf11a175bdda0799341be8349f32c  rust/tools/tests/test_plannerd_native_scheduler.py
```

로컬은 두 파일 AST, 원래13개 class/math/ModelScheduler/policy/readiness의 바이트 비교,
capture 분기만 제거한 default run/main의 AST 동일성과 diff만 확인했다.
실제 수집·unittest·Python oracle·Cargo·C++ compile·기능 실행0, stage/commit/push/GHA0이다.
native client/driver와 이번 collector의 실제 수용 결과는 각각 필요한 Actions 이후에 기록한다.

## 미등록 native oracle 추출 경계 수정

부모 리뷰에서 `independent_python_oracle_bytes_remain_pinned_without_execution`의 끝 검색이
4칸 공백의 `return target`을 찾는 오류를 확인했다. 원래 함수의 return은2칸이며, 종전 검색은
뒤 ModelScheduler의 더 깊은 return 안의 부분 문자열까지 잡을 수 있었다.
끝 marker를 줄 시작까지 고정한 `\n  return target\n`으로 수정했다.
source-only bytes 추출이 독립 AST의 실제 함수 범위와 같은지 확인했고 기존 oracle SHA
`a04e889c725715395baae751a5b3526c0c686b0075d67ab95b95fb94bd987be1`은 그대로다.
Rustfmt2021 --check만 실행했고 native 테스트/Cargo 실행과 공유 등록은 하지 않았다.
capture 두 Python 파일의 SHA와 adapter14 구성은 변경하지 않았다.

## 부모 capture Actions 연결

기존 host caller와 native ARM planner caller의 runtime step에만 위 정확한 IPC capture 경로와
`PLANNER_IPC_GROWTH_MIB=512`를 추가했다. 같은 job filesystem에512MiB 증가 admission을
먼저 확인하고 원래 helper/runtime12를 그대로 호출한다. ARM workflow의 WebRTC job 객체는
그대로다. private proof의 기존 upload가 raw/manifest/index를 포함한다.
기존 planner 단위 job의 같은 interpreter/unittest 명령에 parser capture 경로만 추가하고
기존 실제160-call 비skip assertion과14개 count를 함께 요구한다. 새 worker는 없다.
새 capture의 actual host/ARM/adapter14 결과는 아직 미수용이며 기준/추가 ledger credit0이다.
현재 native IPC/client/C++ candidate16개는 local WIP이고 공유 module/CLI/Cargo에 등록하지
않았다. 이 문서의 해당 candidate SHA와 설계는 배포된 native 실행 증거가 아니다.

## 실제 aa79 capture 및 ARM standalone 관측 경계

sourceaa79의 host runtime12/ASan3과 ARM runtime12, adapter14/실제 native160·원래 frozen/memory/
strict Clippy가 통과했다. parser35의 독립 원문은 native-units artifact12615269988에9160B/
SHA `afe5ea47497bf9ea1af07231400055f348c3c8e555020c1f747c97eb51168acd`로 보존됐다.
이는 기존 Python adapter와 input capture의 수용이며 새 native IPC의 실행·퇴역 수용은 아니다.

source51fd Fast는 기존 CI source-trust와 cap 계약은 통과했지만 standalone ARM의 추가 capture
step과 canonical reusable ARM job을 통째로 같다고 요구하던 기존 isolation 검사에서 실패했다.
검사는 정확한 capture env2개·512MiB admission+원래 helper 호출을 먼저 확인한 뒤 그 관측 추가만
정규화하고 나머지 전체 job을 계속 비교하도록 보완한다. main/reusable ARM의 원래 동작은
그대로이며 임의 env/command 차이를 허용하지 않는다. 실제 후속 정책 검증은 대기다.
