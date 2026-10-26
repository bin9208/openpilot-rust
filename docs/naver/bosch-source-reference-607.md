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
