# #523 F01/F02 순수 Rust 집중 Actions

작성: 2026-10-26. 추적: [#523](https://github.com/bin9208/openpilot-rust/issues/523).
작성 기준 HEAD: `3b819ccc5eb788b319d6b150a3288b6635c0e16f`.
작업 루트: `/home/fma1/.codex/worktrees/python-rust-523-completion/openpilot-rust`.
상태: workflow 작성, 기능 GHA **UNRUN**. commit/push/dispatch/기존 CI 취소·재실행 0회.

## 실행 범위와 runner

사용자 최신 지시로 새 `python-rust-523-native-units.yml`만 작성했다. 기존 Focus,
CPU 수치·모델 실행 workflow, 공유 helper, Rust 코드 및 다른 작업자의 WIP는
수정하지 않았다. CPU 수치·모델 실행은 현재 hosted 경로를 유지한다.

사용자가 전달한 API inventory는 runner ID 22 `bin-actions`, online/idle,
`self-hosted/Linux/X64/dev-md-local`, i9 할당 4 core·RAM 4 GB·디스크 80 GB,
최근 free 38.79 GB다. 전역 Rust 1.94.0 정식 root는 321 entries로 보고되었다.
이는 전달받은 시점의 관측이며 실행 시 가용량·성공·속도 증거가 아니다.
workflow는 label routing 후 첫 step
`Deny fork source and require registered local runner`에서 실제 `RUNNER_NAME`,
`RUNNER_ENVIRONMENT=self-hosted`, Linux/X64 및 `uname`을 확인한다.
후속 모든 step은 `SOURCE_ALLOWED`와 첫 step의 `TRUSTED=true`를 요구한다.
이벤트는 지정 브랜치 push 또는 workflow_dispatch만 허용한다.

`codex/feat-523-python-rust`의 관련 crate·Cargo·원본 경계 소스·SDK helper·
job-temp·새 workflow 경로 push와 수동 dispatch에만 연결했다.
concurrency는 이 workflow/ref 전용, `cancel-in-progress: false`, `queue: max`다.
다른 CI를 취소하거나 재실행하는 API·동작은 없다.

## 소유권·소스·SDK 보존

Focus의 다음 구조를 재사용했다.

- `job-temp@b2fc62472ce8183114d59dbeec4d1406df44e4c2`로 공통 소유 temp를 만들고,
  실제 source 파일시스템에서 checkout 전 2 GiB 증가량을 admission한다.
  proof/runner/메모리/디스크 관측과 preflight 종료코드는 이 admission 전에
  준비하여 초기 공간 부족도 artifact로 남긴다.
- `.diag296-source-<run>-<attempt>.XXXXXX/source`를 `DIAG_SOURCE`로 할당한다.
  Actions의 workspace와 Bash의 private checkout을 구분하며 alias basename은
  사용하지 않는다. checkout은 exact `github.sha`, `persist-credentials: false`,
  `set-safe-directory: false`, `clean: false`, LFS/submodule 비활성이다.
- HEAD/clean diff/경로 목록·stage index/정규화 tar의 tracked source hash를 전후
  확인하고, 집중 소스·Cargo profile/lockfile의 SHA256도 artifact에 남긴다.
- `rust/tools/prepare-readonly-ci-rust before/after`를 그대로 사용한다.
  self-hosted 경로의 `/home/actions/.rustup/toolchains/1.94.0-x86_64-unknown-linux-gnu`
  소유권·공식 ELF pin·전체 파일 hash와 메타데이터 roster 봉인을 재사용한다.
  SDK를 chmod/수리/설치하지 않는다. `RUSTUP_HOME`/`CARGO_HOME`은 job 전용이고,
  helper가 지정한 절대 `RUSTC`/`RUSTDOC` 및 SDK의 절대 Cargo를 사용한다.
  Clippy도 같은 SDK 경로에서 찾는다. rustup/apt/pip 설치는 0회다.
- artifact ID·digest 확인 후 source 경로 형태·canonical 경로·uid·device/inode·
  marker·run owner 및 성공한 source/SDK 봉인을 확인해야 source를 회수한다.
  공통 temp도 별도의 동일 소유권·inode 확인 후 기존 job-temp cleanup으로 회수한다.
  allocation 전에 실패하면 artifact 업로드 뒤 소유 temp만 회수할 수 있다.
  allocation 바인딩이 불완전하거나 봉인·업로드가 실패하면 추측해서 삭제하지 않고
  소유 source/temp를 남긴다. 기존 다른 checkout·전역 cache는 삭제하지 않는다.

## 작업별 자원 가정

순수 unit 작업의 예상 메모리는 **2 GiB**다. 실행 전 `/proc/meminfo`의 실제
`MemAvailable`과 `/proc/self/cgroup`·mountinfo로 찾은 현재 cgroup 및 가시적인
상위 cgroup의 유한 memory limit에서 usage를 뺀 잔여량 중 최소값을 적용한다.
cgroup v2/v1 memory를 읽고, 무제한은 제한으로 계산하지 않으며 경로·읽기 실패는
불확실한 성공 대신 admission 실패로 기록한다. 가시적 limit이 없으면
MemAvailable을 사용하고 실제 읽은 membership·limit 목록을 증거에 남긴다.

| 작업 | 예상 디스크 증가량/할당 상한 |
| --- | ---: |
| checkout 전 source | 2 GiB |
| 같은 private target의 unit + 영향 target Clippy | 4 GiB |
| private Cargo registry/cache | 1 GiB |
| proof 전체 | 0.5 GiB |
| checkout 후 source 잔여 증가 여유 | 2 GiB |

checkout 후 동일 파일시스템이면 남은 요구량은 최대 **7.5 GiB**다.
디바이스별로 합산하며 다른 파일시스템에 놓인 target/registry/proof는 각각
실제 가용량과 대조한다. 이미 할당된 target/registry/proof 바이트를 빼서
같은 budget을 재사용하고, Clippy용 target 예산을 중복 부여하지 않는다.
작업별 상한 초과도 실패다. 25/35 GiB 같은 고정 중단 floor는 없다.
각 unit·원본 경계 비교·Clippy 실행 전에 자원 admission을 다시 읽는다.
동시 실행에 따른 여유 감소를 성공으로 간주하지 않는다.

컴파일은 `CARGO_BUILD_JOBS=1`, `CARGO_INCREMENTAL=0`, dev/test debug=0이며,
테스트 harness도 `RUST_TEST_THREADS=1`로 순차 실행한다.
공유 workspace profile은 release thin LTO/codegen-units=1, dev의
`num-bigint-dig opt-level=3`만 있으며 변경하지 않았다. 이번 dev/test 실행은
명시된 환경으로 debug 정보를 끄고 하나의 target을 공유한다.
로그별 64 MiB 파일 제한·명령별 8분·검사 step 25분·job 40분을 둔다.
이 값들은 실행 전 운영 가정이며 실측 peak나 성공 보장이 아니다.

## 실행할 계약

모든 Cargo 명령에 `--locked --no-default-features`를 사용한다.

```text
openpilot-control-policy: cargo test --test drive --test math
openpilot-model-runtime: cargo test --test compiler_launch --test compiler_schedule
openpilot-model-runtime: cargo test --test compiler_schedule original_python_pipeline_boundaries_match_native -- --ignored --exact --nocapture
openpilot-control-policy: cargo clippy --lib --test drive --test math -- -D warnings
openpilot-model-runtime: cargo clippy --lib --test compiler_launch --test compiler_schedule -- -D warnings
```

crate별 feature 비활성화로 control-policy의 numerics, model-runtime의
native-skip-miri/cached-program-isa를 켜지 않는다. 영향 test/lib만 Clippy에
전달한다. SDK 설치·모델·동적 numerics provider·전체 crate 테스트는 실행하지 않는다.
ignored 원본 경계 비교는 Python 표준 라이브러리로 실제 소스 AST 경계만 실행한다.
그 테스트에 있는 **73개** 입력 검증을 유지하고, 정확히 1개 test가 성공했는지
로그도 확인해 이름 변경으로 0 tests가 성공 처리되는 일을 거절한다.

일반 검사 실패는 종료코드·로그를 남기고 가능한 다른 독립 검사로 진행하되,
자원 admission 실패 뒤에는 추가 컴파일을 시작하지 않는다. 이미 성공한 같은
계약을 반복하지 않는다. 실패 여부와 무관하게 성공한 초기 baseline에 대한
source/SDK 최종 봉인을 별도 step에서 시도하고 proof를 업로드한다.

## 작성 시 공유 소스 snapshot

| 파일 | SHA256 |
| --- | --- |
| `rust/Cargo.toml` | `7372b8240f7998d6a78f6ef95b574cf64523b913282240d918ef5697a7d8507d` |
| `rust/Cargo.lock` | `a82648159767a21268e61ed4f2aba64940b7458733333ece0fab653017929ea0` |
| `control-policy/Cargo.toml` | `376b2a22d5d7e19b5e8d746f3e82c95bd839522baeb4f0f9e8241e8a0fe45fe6` |
| `control-policy/tests/drive.rs` | `7550f9299bf323106eb1fd6352c9ccf257d38aeca7b1ec2cf15dbf81dcf1bf66` |
| `control-policy/tests/math.rs` | `b5dc1c1b589a043f5850800e74c852187a1ab3031c8db5466f74524bb9b04e35` |
| `model-runtime/Cargo.toml` | `b7fae464db25af5b1c13b64ce0e1556d3c4e6f88553641c927ff6f9d8f294980` |
| `model-runtime/src/compiler_launch.rs` | `f34b69a31029155b67b844340fb8190dc89fc31de304efed713d36bab63404b1` |
| `model-runtime/src/compiler_schedule.rs` | `dee3457caed0be633c8973c4e33abbd88056d1c8e8fcfb638fd6dfb35f39f8fd` |
| `model-runtime/tests/compiler_launch.rs` | `4c3a0a443c6984be3721bb7c0924f499a97066633e37fd6f019973f3a4d79b04` |
| `model-runtime/tests/compiler_schedule.rs` | `15688511f6d947a65ce96be621c393d9c7a7f10006fe32b4723c72ed18db6ef1` |
| `rust/tools/prepare-readonly-ci-rust` | `384cdcae5b30bb6d4281f39d88a54cf9a2701f9c2cbbc1f61b7feef5505fc5ef` |

crate 명으로 시작하는 경로는 `rust/crates/` 아래다. 이 표는 읽은 현재 소스의
기록이며 부모가 관리하는 Focus allow/pins를 대신하지 않는다. runtime proof에는
실제 실행 SHA의 tracked source와 집중 파일 전체 hash가 별도로 남는다.

## 검증과 부모 인계

로컬 YAML parse와 11개 `run` 블록의 `bash -n`이 통과했다.
helper 실행·Cargo/build/test/Clippy·원본 비교·모델 실행은 하지 않았다.
기능 GHA는 **UNRUN**이며 runner 가용량·빌드 peak·test/Clippy 성공·
source/SDK 보존·artifact 및 실제 회수는 아직 검증되지 않았다.

부모 담당: Focus allow/pins, 원장·이슈 갱신, commit/push, exact-SHA GHA 실행과
결과 판정. 기존 진행 중 CI의 취소·재실행은 이 전달의 전제나 실행 항목이 아니다.

## 부모 통합 — 2026-10-26

기존 CI 정책의 정확 source trust binding을 첫 단계 env에 연결하고 모든 후속 단계는 같은 trusted expression 안에서 기존 실행/정리 조건을 유지한다. 순수 단위·Clippy·원본73개 검증은 새 로컬 job에 배정했으며 다음 hosted Focus에서는 중복 명령을 제거했다. 이미 실행 중인 기존 snapshot은 취소하지 않는다. 새 workflow의 실제 admission helper는 추가 실행 원장에 pending으로 등록했고, Python 전체 제거 완료로 계산하지 않는다.

로컬은 YAML parse와 Bash syntax만 확인했다. 실제 RAM2GiB 여유/target4GiB·registry1GiB·proof0.5GiB·source2GiB의 적합성, global SDK 전후 무변경, 단위·Clippy·원본 비교·회수는 exact-SHA Actions 결과를 확인해야 한다.
