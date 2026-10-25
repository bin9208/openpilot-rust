# #523 Planner scheduler 순서 비교 수정과 실제 IPC 연결

2026-10-26. 작업 루트는
`/home/fma1/.codex/worktrees/python-rust-523-completion/openpilot-rust`다.
[앞선 native controller 인계](python-rust-523-planner-native-controller.md)의
공개 helper 부동소수 의미와 미연결 IPC caller에 대한 부모 리뷰를 반영했다.
기존 Planner CI native contract21·release3, compiler/logging WIP 및 공유 mod/Cargo/workflow는 수정하지 않았다.

## 실제 변경 파일

- `rust/crates/dev-tools/src/planner_ci_controller/scheduler.rs`
- `rust/crates/dev-tools/tests/planner_ci_controller.rs`
- `rust/tools/plannerd_ipc.py`
- `rust/tools/tests/test_plannerd_native_scheduler.py` — 새 Actions 검사 source.
- 이 한국어 기록 — 새 문서.

## Python max의 순서 계약

`f64::max`는 Python `max(a, b)`의 첫 피연산자 유지 계약을 보장하지 않는다.
native 공개 helper는 target을 첫 피연산자로 초기화하고, 각 candidate가
`candidate > target`일 때만 교체한다. 두 max 위치 모두 같은 순서 비교로 바꿨다.
따라서 동률의 `-0.0/+0.0`과 unordered NaN에서도 원본의 선택을 유지한다.
공개 helper에서 NaN이나 음수 시각을 새로 거부하지 않는다.

실제 stdio endpoint의 입력은 앞서 명시한 finite timestamp wire domain을 유지한다.
이 경계는 signed zero·음수 finite는 허용하고 NaN/무한대는 거부한다.
Python adapter도 native mode에서 같은 finite wire domain을 요구한다.
환경변수가 없는 독립 oracle 경로에는 이 wire 제한을 적용하지 않는다.

Rust 검사에는 다음을 추가했다.

- 두 max 위치에서 첫 signed zero를 보존하는 pure helper/endpoint 검사.
- NaN payload의 첫 피연산자 유지, NaN candidate·완료 시각 및 무한대 helper 검사.
- stdio의 nonfinite 거부 검사.
- 기존 72개 원본 finite 비교에 signed-zero 4개를 더해 76개 IEEE bits 비교.
- 별도의 원본 public helper 비교. 입력/출력을 u64 bits로 운반하여 JSON의 nonfinite
  숫자 제한이나 payload 정규화를 피하고 실제 원본 함수와 9개 경계 입력을 비교한다.

## 독립 원본 함수 보존

`plannerd_ipc.py::next_model_target` 함수는 변경하지 않았다. def부터 마지막 return의
개행까지 바이트 SHA256은 수정 전후 모두 다음과 같다.

`a04e889c725715395baae751a5b3526c0c686b0075d67ab95b95fb94bd987be1`

이 함수는 기존 AST oracle와 새 원본 비교에 계속 사용한다. 새 adapter 검사에도
동일한 함수 바이트 pin을 두었다. 전체 Python 파일 SHA는 실제 caller 추가로 바뀌며,
이번 source readback 값은
`c2099127901a2883925305dc3bf0c563b338494cd5bd829cb7bd3e916ffdd031`이다.
부모가 Focus/ledger에 이 파일을 pin했다면 이 변경을 함께 반영해야 한다.

## run loop의 실제 연결

`ModelScheduler`를 기존 run의 daemon stdout/stderr context에 추가하고, 프레임별
`target = next_model_target(...)` 호출을 `scheduler.target(...)`으로 연결했다.

- `PLANNER_NATIVE_SCHEDULER`가 없으면 독립 원본 함수 경로를 그대로 사용한다.
- 변수가 존재하면 `[binary, "schedule"]`을 run당 한 번 spawn한다. 빈 설정이나
  실행 실패도 오류이며 원본 함수로 fallback하지 않는다.
- 프레임당 한 요청을 round-trip float 문자열 세 개의 TSV로 전송하고 한 target을 읽는다.
  pipe는 unbuffered/nonblocking이며 2초 요청 상한 안에서 readiness·부분 I/O를 처리한다.
- EOF, timeout, 비정상 종료, malformed/nonfinite/과다 응답은 즉시 실패로 전파한다.
  실패를 latch하여 재시도나 정상 context 종료로 숨길 수 없다.
- `PLANNER_NATIVE_SCHEDULER_EVIDENCE`는 controller에서 물려받은 값을 그대로 native
  process에 전달한다. 기본 40프레임 × source/rust × 2epoch = 160행 조건과 맞는다.

정상 종료에서는 stdin EOF를 보내고 wait한다. 예외에서는 TERM을 먼저 보낸다.
3초의 단일 회수 deadline 안에서 첫 wait 최대1초 → TERM/wait 최대1초 → KILL/남은 wait로
회수한다. 정상 경로에서 강제 종료가 필요하거나 종료 코드가 0이 아니면 실패다.
원래 run의 daemon 대기·publication 2초·startup 15초·daemon 종료 3초 값은 바꾸지 않았다.
회수 상한 내 wait가 끝나지 않으면 오류를 숨기지 않는다.

endpoint는 상위 controller의 기존 프로세스 그룹을 상속한다. detached process나
프레임마다 spawn하는 구성이 아니다. stderr는 각 case의 `scheduler.stderr.log`에 남는다.
기존 wire 입력, frames, publish/wait 순서, daemon argv, timing/records/receipt 필드와
출력 비교는 유지했다.

## 새 adapter source 검사

`test_plannerd_native_scheduler.py`는 실제 파일의 `ModelScheduler`와 독립 원본 함수를
AST로 로드한다. C++ IPC/모델을 import하지 않는다. fixture도 실제 subprocess이므로
EOF/exit/pipe/신호/회수를 가짜 Popen 반환값으로 대체하지 않는다.

13개 unittest 정의는 oracle 바이트 pin, 실제 run의 context/프레임별 호출 연결,
환경 미설정 시 signed-zero/NaN 원본 경로, native 40회/1spawn, 빈 설정·spawn 실패,
EOF/exit42/malformed/nonfinite/긴 응답/중복 응답, timeout, 잡힌 EOF의 실패 latch,
비정상 close, TERM 무시 child의 KILL·wait, 부모 예외 보존 및 실제 Rust160회를 다룬다.
마지막 검사는 각 target을 원본과 bits로 비교하고 native TSV 160행도 검증한다.
실제 Rust binary 검사는 환경변수가 없으면 skip이므로, 수용용 Actions에서는 아래
환경을 반드시 설정하고 그 검사가 skip되지 않았는지 확인해야 한다.

## 부모 Actions 연결과 남은 검증

부모가 적용한 `pub mod planner_ci_controller;` 및 `check-plannerd-ci` bin 등록을
현재 소스에서 확인했다. 추가 shared 등록이나 의존성/feature는 필요 없다.
이번 단계는 공용 mod/Cargo/workflow를 직접 수정하지 않았다.

현재 Rust target은 native11 + 명시 실행 원본4개다. 80GB/4core/4GB RAM의
전용 Actions에서 `CARGO_BUILD_JOBS=1`을 유지한다. 아래 명령은 로컬에서 실행하지 않았다.

```sh
CARGO_BUILD_JOBS=1 cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --test planner_ci_controller
PLANNER_REFERENCE_PYTHON="$(command -v python)" CARGO_BUILD_JOBS=1 cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --test planner_ci_controller source_ -- --ignored --test-threads=1
PLANNER_NATIVE_SCHEDULER_TEST_BIN="$CI296_SOURCE/rust/target/debug/check-plannerd-ci" python -m unittest discover -s rust/tools/tests -p test_plannerd_native_scheduler.py -v
```

마지막 adapter 검사는 2초 timeout과 bounded TERM/KILL 회수를 실제로 시험하므로 해당
Actions step에는 여러 회수 사례를 실행할 시간이 필요하다. 실제 host/native ARM의
12-leaf 비교와 native scheduler160행/final receipt는 앞선 controller 인계의 기존
caller 전환·성장량 입력 및 기능 gate와 함께 확인한다. 이전 pure unit 성공만으로
새 caller가 실제 장치/IPC에서 동작한다고 선언하지 않는다.

## 로컬에서 확인한 범위

Rustfmt와 Python AST 구문 검사만 실행했다. 원본 peer fixture 및 Rust에 포함한
원본 비교 snippet도 AST 구문 검사했다. source 검사에서 현재 run의 scheduler
context/target dispatch만 되돌리면 HEAD의 run AST와 동일하며, main을 포함한 나머지
기존 함수 AST도 모두 동일함을 확인했다. 원본 함수 바이트 SHA와 diff whitespace도 확인했다.

Cargo/컴파일/테스트/원본/모델/프로젝트 CLI의 로컬 기능 실행은 **0회**다.
helper 및 실제 caller 연결 소스는 구현했지만 signed-zero/NaN 원본 비교, adapter
13개 및 실제 IPC160 receipt의 기능 결과는 **Actions 미검증**이다.
호환 oracle·Python IPC/controller 경로는 남아 있고 Python 삭제 수량은 **0개**다.
이 작업을 all24 retirement나 #523 전체 완료로 처리하지 않는다.
