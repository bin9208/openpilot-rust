# #607 원본 수치와 실행 환경 분리

상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598),
성능 [#601](https://github.com/bin9208/openpilot-rust/issues/601)의 회귀 차단 원인 조사다.
runtime의 레이더 수식·허용 오차·고정 fixture는 변경하지 않는다.

`aec405d6db871a762a939cfe0ccbb82f935dfc50`의
[Actions 40330013444](https://github.com/bin9208/openpilot-rust/actions/runs/40330013444)에서
x86_64는 hash로 고정한 실제 원본 Python과 Rust의 2,544회 결과/debug/전체 filter state를
같은 환경에서 부동소수점 bit까지 비교해 일치했다. Clippy도 통과했다.
같은 실행의 원본 Python도 과거 고정 fixture와 8개 필드가 달랐다. 첫 차이는
`state/190/states/2/lead_filter/response_weight`의
`0.0016548305925441127` / `0.0016548305925441129`다.
이것은 현 환경에서의 원본 동등성 근거이며, 과거 fixture 차이의 원인을 확정하거나
필수 전체 workspace 회귀를 통과한 결과는 아니다. 과거 비교 step은 진단용으로 별도 보존한다.

ARM job `121596623745`는 guest에 `/usr/bin/time`이 없어서 비교 명령 실행 전에 exit127로
실패했다. ARM 수치 비교 결과로 간주하지 않는다. focused workflow의 명시적 의존성에
`time`을 추가하고 공통 same-repository 실행 조건을 유지한다. 재검증 SHA와 결과는
실행 후 이슈에 기록한다. ARM은 등록된 bin-actions의 QEMU/PRoot user space이며
물리 ARM CPU·AGNOS kernel·comma 3X 검증이 아니다.

## 양쪽 환경의 실제 결과와 일반 회귀 연결 — 2026-10-27

source `fb07c98294269e0144d56aeeea02f0cdfa85980e`의
[Actions40358677927](https://github.com/bin9208/openpilot-rust/actions/runs/40358677927)은
x64 job121697923570과 ARM job121697923802 모두 성공했다. 같은 환경의 hash 고정 원본과
Rust의2,544회 result/debug/전체 state bit 비교 및 strict Clippy를 양쪽에서 통과했다.
ARM artifact12717802440은 원본 대 과거 fixture 차이0과 historical Rust 비교 성공을 기록했다.
x64 artifact12709949965는 원본도 과거 fixture와8행/8필드가 다르고 historical Rust 비교도
실패했음을 보존했다. x64 첫 필드는 response_weight의 `3f5b1cdcd1a01bdb` / `3f5b1cdcd1a01bda`다.

ARM은 x64 host/QEMU-PRoot aarch64 guest/Python3.12.14/glibc2.39다. 물리 ARM CPU,
실제 AGNOS kernel 또는 차량 성능 개선의 증거가 아니다. compile 포함 단일 명령 wall time은
x64 2:30.08, ARM 46:21.89이며 single-child maximum RSS는789,096/950,140KiB다.
aggregate peak나 차량 측정값으로 사용하지 않는다. 과거 bit의 환경 의존성은 확인했지만
libm만을 원인으로 확정하지 않는다.

후속 변경은 일반 `cargo test`에 같은 환경 원본 검사를 필수로 유지한다. 전체 필드 비교와
2,544 update count 검사는 공통 support로 이동하고, 과거 고정 fixture 검사는 별도
`bosch_historical_fixture` 예제로 이동한다. ignored test나
허용 오차, field 생략, fixture/원본 변경을 추가하지 않는다. focused CI도 별도 test 이름 필터
없이 실제 test target 전체를 실행하며, historical 예제를 독립 로그로 실행한다.

```sh
cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-radarcan --test bosch_acceleration -- --nocapture
cargo run --manifest-path rust/Cargo.toml --locked -p openpilot-radarcan --example bosch_historical_fixture
```

위 명령은 Actions에서 실행한다. 두 번째는 해당 x64 환경에서 과거 fixture 차이를
진단하는 실패를 그대로 반환한다. 이 분리는 새 source의 focused·일반 전체 workspace·
필수 PR/병합 후 검증 전이며 #607/#601/#598 완료가 아니다. 기존 PR494의 dev-md 트랙을
유지하며 dev/main으로 runtime을 통합하지 않는다.

2026-10-28의 source `0cfefca4d2e3389672314da7122dfe41e87b81f9`는 focused
[40469418812](https://github.com/bin9208/openpilot-rust/actions/runs/40469418812)의
x64/ARM 원본 filter 검사를 모두 통과했다. 이후 필수 workspace
[job122669024975](https://github.com/bin9208/openpilot-rust/actions/runs/40469686475/job/122669024975)는
tracking의 `state/426/last_decisions/4/residual_bearing_rad`에서
`0.006532432521419149` / `0.0065324325214191625`의 bit 차이로 실패했다.
다른 성공을 이 실패의 통과 근거로 사용하지 않는다.

같은 hash 고정 원본에서 기존 raw tracking 정의15개와 원래 NumPy solver를 읽는
검사로 기존768 transition을 연결한다. 각 입력의 전체 before state를 복원하고
모든 결과·전체 after state·원래 atomic error를 비교한다. 고정 fixture는 바꾸지 않고
별도 historical 예제로 계속 실행한다. runtime 수식, epsilon/ULP 허용, 필드 생략은
추가하지 않는다. 원본 대 과거 fixture 차이와 Python/NumPy/libc/libm 환경은 별도
evidence에 남긴다. 수정 source의 실제 Actions 결과는 아직 확인 전이다.
