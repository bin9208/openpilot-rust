# 준비된 native 입력의 전체 그룹 연결 — #599 (Actions 검증 대기)

상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598),
환경 [#599](https://github.com/bin9208/openpilot-rust/issues/599),
캐시·세대 [#600](https://github.com/bin9208/openpilot-rust/issues/600).
이 변경은 [컴파일 입력 계약](native-compiler-input-contract-599.md)을 실제 준비된
입력과 연결한다. 전체 runtime99개/13그룹의 목록·feature는 그대로다. 컴파일·모델 준비·
세대 봉인·기기 설치를 수행하는 단계가 아니다.

## 입력과 신뢰 경계

기존 `openpilot-runtime-plan`에 `--native-inputs`와 `--budgets`를 추가했다.
준비기에서 독립적으로 확인한 receipt SHA를 전달한다. 현재 파일을 새로 snapshot해
스스로 신뢰하거나 다른 SHA의 프로젝트 실행 파일을 현재 runtime으로 표시하지 않는다.

`native-inputs.json`은 `schema:1`, `inputs`, `runtime_inputs`를 가진다. `inputs`의
키는 아래 역할이며 각 값은 `root`, `receipt`, `sha256`이다. 이 이름은 일반 CLI의
Tree 경로와 그 Tree receipt SHA를 명시적으로 연결한 것이다. 기존 준비 보고서의
`root`, `tree_receipt`, `tree_sha256`에서 각각 옮긴다.

| 역할 | 기존 검증 입력 | 배정 |
| --- | --- | --- |
| `rust` | 고정 Rust1.94 ARM prefix 전체 Tree | 모든 그룹 |
| `cargo` | 현재 Cargo.lock의 전체 vendored source Tree | 모든 그룹 |
| `system-sdk` | 고정 AGNOS 이미지의 typed SDK receipt, 실제 root `/` | 모든 그룹 |
| `capnp` | 기존 고정 capnp prefix | catalog의 capnp 소비 그룹 |
| `ffmpeg` | FFmpeg6.1.1 header와 기존 shared provider의 합성 prefix |00,01,03,07,08,10 |
| `libyuv` | 기존1922.0 wheel의 static library prefix |00,01 |
| `raylib` | 기존6.0 SDK header/static library prefix |00,01,02 |
| `opencv` | 기존4.13 header와 shared provider의 합성 prefix |03,04 |
| `eigen` | 기존3.4 header prefix |04,06 |

역할 목록은 실제 source catalog에서 도출한 목록과 정확히 같아야 한다. root/receipt는
절대 canonical 경로이고 SHA는 소문자64자리다. `/`는 typed system SDK만 사용할 수 있다.
receipt는 최대16MiB의 같은 byte를 hash·parse하고 역할별 필요한 파일 배치도 확인한다.
실제 전체 Tree는 기존 `Catalog::compose`가 set별 한 번 비교하며, 뒤의 실제 driver도
빌드 전후에 비교한다. 실제 metadata Cargo 역시 `rust` 입력의 `bin/cargo`여야 한다.
이 검사는 전달한 pin의 출처를 새로 인증하지 않는다. 고정 준비기/자산 검증이 선행된다.

system SDK는 모든 그룹에 같은 `/`와 독립 SHA를 묶고 보존한 source의
`rust/native-system-sdk.json` 전체 profile과도 비교한다. subset 또는 보통 Tree가
typed SDK를 대체할 수 없다. 원래 generation build/seal/boot의 검사를 그대로 유지한다.

## 그룹 환경과 캐시

source catalog의 suffix로 환경 경로를 계산하고 기존 factory GCC/G++/AR/RANLIB와
HOST 변수를 명시한다. 필요한 capnp/FFmpeg/OpenCV의 loader/rpath-link 경로만 해당
그룹에 연결한다. Cargo vendor 환경은 전체 vendor Tree와 같은 input set에 둔다.

입력 목록 JSON 전체의 SHA를 모든 compiler recipe에 넣지 않는다. 예를 들어 raylib
pin 변경은00/01/02에만 영향을 주며07/08/10의 compiler 입력은 그대로다. 모델·실행
companion은 `runtime_inputs`에만 연결한다. 이 변경의 CI plan에는 아직 이 자산들이
없으므로 `compiler_inputs_bound=true`가 전체 runtime 준비 완료를 뜻하지 않는다.

기존 수동 `--bindings` 조합에도 수정이 있다. set 자체를 검사할 때 그 set의 환경을
사용하도록 해 조합 시점에도 현재 Cargo.lock provenance를 검증한다. 다른 set의 Cargo
환경을 상속해 무관한 Tree를 vendor로 검사하는 일도 막는다. 실제 빌드의 기존 검사는 유지한다.

## 작업별 예상 증가량과 실행

`budgets.json`은 `schema:1`, 정확13그룹의 `group_growth_bytes`,
`expected_source_growth_bytes`, `remaining_object_growth_bytes`, `estimate_basis`다.
각 증가량은0보다 커야 하며 근거는 비어 있을 수 없다. 고정 free-space floor나 측정값으로
해석하지 않는다. 실제 build admission과 RAM/swap/디스크 관측은 기존 coordinator가 담당한다.

아래는 **Actions/준비된 환경용 개발 절차**다. 로컬 기능 실행 금지는 유지한다.

```sh
openpilot-runtime-plan --root "$SOURCE_ROOT" \
  --cargo "$PINNED_RUST_ROOT/bin/cargo" \
  --native-inputs "$PREPARED_PINS_JSON" --budgets "$OPERATION_ESTIMATES_JSON" \
  --output "$NEW_PLAN_DIRECTORY"
```

출력 부모는 미리 존재해야 한다. source와 compiler/runtime 입력 Tree 및 system SDK의
compiler namespace 밖의 새 출력만
허용한다. `bindings.json`, `build-plan.json`, `catalog-audit.json`을 fsync·원자적
NOREPLACE로 함께 발행한다. audit에 각 JSON SHA와 예상량 근거를 남긴다.
source/catalog/Cargo/config/receipt가 검증 중 바뀌면 실패한다. 기존 수동 모드는
원래 두 파일 출력과 engineering plan 계약을 유지한다.

## 검사 방법과 미완료 범위

- foundation Actions는 실제 crate 테스트·strict Clippy와 새6개 회귀를 실행한다.
  provider별 영향 그룹, model 무관성, 잘못된 pin/경로/역할/예산/CLI, source/input 내부
  출력 거부, 현재 vendor lock 불일치 및 set 환경 격리를 확인한다.
- genuine AGNOS job은 이전 단계의 실제 ARM 준비 보고서9개 역할을 그대로 받아
  source-built planner로13그룹을 조합한다. 반복 plan/bindings/audit byte 일치, 각 compiler
  환경의 전체 Tree/pin, 잘못된 receipt SHA·누락 provider·0 예산의 발행 거부를 확인한다.
  artifact는 `native-project-bindings-*`, `native-project-input-pins.json`,
  `native-project-growth-estimates.json`, commands log와 최종 evidence JSON이다.
- 이 CI의 예상량은 package당512MiB+binary당64MiB, source2GiB,
  immutable binary당64MiB의 **계획 검사용 보수적 추정**이다. 실제 전체 빌드 증가량
  실측이 아니며 전체 빌드 전에 보정해야 한다. 이 조합기의 실제 추가 쓰기는 JSON 몇 개다.
- host는 x64 `bin-actions`, guest는 QEMU/PRoot의 고정 AGNOS ARM user space다.
  물리 ARM CPU·AGNOS kernel 부팅·comma3X의 속도/메모리/기능 수용 결과로 표현하지 않는다.

`compiled=false`, `activated=false`, 전체 runtime/실행 자산 준비 미완료를 명시한다.
실행 companion·모델 준비 환경·Layout과 전체99 native build, 정상 update/start/record/upload,
실패/전원 복구, 정확 SHA PR/병합 후 검사와 comma3X 사용자 수용은 계속 남아 있다.
현재 기본 시작 경로의 SHA별 legacy runtime release 의존도 아직 바꾸지 않았다.

## 현재 native 브랜치의 검사 대기 보존

공통 #605의 대기 보존 변경은 별도 PR606에 있었고 이 native 브랜치에는 아직 없었다.
후속 문서/filter push 뒤 source22fcc92a1의 fast run40433843945가 기존
`cancel-in-progress:true` 정책으로 자동 취소된 것을 확인했다. agent가 수동 취소한 것은
아니지만 기존 대기를 보존해야 하는 요구를 충족하지 못한 상태였다.

현재 브랜치의 concurrency가 있는 validation workflow10개에도 같은 정책을 적용한다.
기존7개 fast/integration/Rust/tests/naver/APK/wiki 검사와 native3개 resource/source-reuse/
genuine이다. 새 `-preserve-v1` group, `queue:max`, `cancel-in-progress:false`를 사용해
전환 시점의 기존 group 대기와 이후 대기를 함께 보존한다. 기본 대기 한 개 교체를 피하기
위한 정책이며 [공식 규칙](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency)의
group당100개 pending 한도가 있다. 동시성 설정이 없는 foundation은 기존 동작을 유지한다.
publish/sync는 바꾸거나 활성화하지 않는다. 기존 source·job/needs/matrix·명령·runner/pin/
timeout은 그대로이며 새 정책 회귀는 기존 Actions 검사에 포함한다.
