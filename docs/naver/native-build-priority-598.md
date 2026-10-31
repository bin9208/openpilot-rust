# 성능 개선·콤마 자체 빌드 우선 실행 — #598

## 자체 빌드 집중과 native ARM 실행 — 2026-10-30 사용자 지시

최우선 결과는 공개 입력을 Git에 고정한 완전한 자체 빌드 후보다. 현재 완료 순서는
AGNOS native ARM 검증 → 정상 PR/병합 후 검사 → 입력34개 실제 발행·공개 body 검증 →
최종 index 원본 bytes의 정상 Git pin → production source entry의 최초/반복/문서/작은 변경
빌드·시작용 패키지 검증 → 정확 SHA와 한/영 시험 절차 제공이다. 모델 준비 Python과
기존 C/C++ 라이브러리를 유지하며 새 기능·광범위 언어 전환·별도 cache 작업을 추가하지 않는다.

사용자는 AGNOS와 다른 ARM 빌드를 `ubuntu-24.04-arm`에서 수행하고 QEMU 비용을
제거하도록 명시했다. 기존8개 native ARM 검사는 유지하고 Genuine·factory transport·
전체 source entry·standalone ARM 빌드의 실제 실행 경로를 ARM에 맞춘다. 기존 x86 전용
PRoot 대신 같은 v5.5.0과 원래 uthash gitlink의 고정 source를 Actions에서 native C로
빌드하며, 임의 PATH 바이너리나 다른 AGNOS 이미지를 사용하지 않는다. 원래 이미지,
factory sudo body/mode, comma UID/GID1000, 실제 kernel controller와 readonly proc,
source/ABI/seed pin·성장/메모리 예산·deadline·소유 범위/cleanup 조건을 유지한다.
hosted ARM의 실제 권한·용량·빌드 시간은 새 정확 source의 Actions로 확인해야 한다.

성능 이슈의 실제 계층은 #422 → #524 → #522다. #598 본문의 관련 참조와 정식 하위
관계를 구분한다. 사용자의 자체 빌드 집중 지시에 따라 PR #494→dev-md의 추가 수정·
push/병합/발행을 보류하고 기존 실행·검토 초안·실패 증거를 보존한다. 해당 별도 트랙의
완료를 자체 빌드 병합·발행의 선행 조건으로 추가하지 않는다. #606/#644의 별도 cache
소스와 실행도 보존하며 사용자가 거부한 다른 채팅 메시지를 보내지 않는다.

이 결정 전 source `27547f08c6daf7deac99e35b0f6e1d0c8ffb0298`의 검사는 보존한다.
새 native ARM source가 실제 등록되기 전 현재 실행을 취소하지 않고, 구형 x86/QEMU의
성공을 새 ARM 경로의 성공으로 대체하지 않는다. 실제 공개 발행·Git pin·전체 source
entry·후보는 아직 미완료다. 실제 사용자 기기 수용은 계속 #528에서 별도로 기록한다.

새 ARM 실행은 기존 x86 검사의 대기열 때문에 시작이 지연되지 않도록 Rust의 전용
preserve queue를 사용한다. `queue: max`와 `cancel-in-progress: false`는 유지하고
기존 실행·다른 작업의 cache 검사를 취소하지 않는다. 기존 factory transport workflow도
현재 #599 issue branch의 변경에서 자동 실행해 실제 sudo/UID/controller 능력을 먼저
확인한다. 같은 source의 factory 검사를 별도로 중복 dispatch하지 않는다.

실제 관측에서 뒤이은 CI 정책 변경의 N/O 실행은 기존 M 실행과 같은 branch key에
묶여 job0의 pending 상태가 됐다. Rust와 Genuine은 group에 `github.sha`를 포함해
서로 다른 소스를 독립 실행한다. 같은 소스/이벤트/ref는 같은 queue를 사용하고
취소 방지·기존 모든 검사와 소유 자원 분리는 유지한다. 독립 hosted ARM VM에서
현재 검증을 시작하기 위한 CI 조정이며 runner 수·요금·compile/test 기준은 바꾸지 않는다.

## #644 CI 정리와 고정 workflow 회귀 대조 — 2026-10-30

source `6f744c55bf3a5784b398b1a1a1228371e2085afe`의
[foundation ARM job123901870304](https://github.com/bin9208/openpilot-rust/actions/runs/41006302295/job/123901870304)은
고정 snapshot의 두 회귀가 의도적으로 삭제한 `naver-upstream-sync.yml`을 읽으며
`FileNotFoundError`로 실패했다(12개 중10개 성공·2개 오류, 실제 Cargo reuse 전).
과거 fixture는 그대로 두고 sync·NAS·Wiki의 정확한 retired4개만 공통 목록으로 제외하며,
각 파일의 부재를 명시적으로 단언한다. 나머지 workflow의 원래 명령·pin·의존성·matrix·
ARM 증거·deadline 대조는 유지한다. Fast 등 삭제 정책을 되돌리지 않는다.
로컬은 AST·diff만 확인하며 수정 후 실제 회귀 성공은 Actions 검증 전이다.
기존 S/C 실행은 재시작하거나 취소하지 않는다.

후속 source `a395db720284efe171444ff7eb893279c535de83`의
[foundation ARM job123903206610](https://github.com/bin9208/openpilot-rust/actions/runs/41006731360/job/123903206610)에서
routing12개는 모두 통과했다. 다음 publication 회귀32개 중1개는 삭제된 Fast fixture
ID105 참조로 `KeyError`였다. 해당 참조만 제거하고 실제 세 PR fixture103/104/106과
원래 runner/gate 단언을 유지한다. 기존 foundation에서 수정 후 회귀 전체를 먼저
확인한 뒤 최신 PR 검증을 재개하며 실행 중인 Genuine 입력 준비는 보존한다.

수정 source `458d1516cbbb6ebb35a875df07d4a44a198c69a6`의
[foundation ARM job123904525231](https://github.com/bin9208/openpilot-rust/actions/runs/41007145972/job/123904525231)은
`2026-10-30T05:25:06Z`에 전체 성공했다. routing12개·publication32개와 source driver/
cache/취소 경계, 실제 offline Cargo 재사용 등 원래 ARM foundation 검증을 통과했다.
X64 job은 아직 대기 중이며 전체 runtime/production entry·후보 완료를 뜻하지 않는다.
이 결과 기록만 추가한 후속 source에서 [PR611 필수 검사](https://github.com/bin9208/openpilot-rust/pull/611/checks)를
재개한다. 기능 파일·입력 pin·명령·deadline은458과 동일하며 S 입력 준비와 기존 실행을 보존한다.

source `8c537abed1b21f17469f43f25e4ff71557a16f02`의 [foundation](https://github.com/bin9208/openpilot-rust/actions/runs/41016536626)은
ARM/X64 모두 routing15개 중14개 통과·1개 normalization 비교 실패였다.
preflight 제거 전에 scope를 baseline index에 복원하여 scope 위치가 한 칸 당겨졌다.
기존 단언과 전체 YAML 대조를 유지하고 preflight 제거 뒤 scope를 복원하도록 순서만 고친다.
로컬 AST·diff만 확인했으며 수정 후 실제 회귀는 Actions 검증 대기다.
같은 source의 [Genuine 조기 seed ABI 단계](https://github.com/bin9208/openpilot-rust/actions/runs/41016536609/job/123934032903)는
07:03:39~07:07:22 UTC에 실제 성공했다. 정규 전체 Genuine·입력 발행·전체 source entry 성공과 구분한다.

## 기존 #484 복구 unit fixture의 PID 공개 경합 — 2026-10-30

source `ed1061eb5f6bdc540a70c8a2fbc84071f64388b0`의
[workspace job123873260557](https://github.com/bin9208/openpilot-rust/actions/runs/40997291847/job/123873260557)은
carrot-recovery20개 성공/1개 실패/3개 ignored로 종료했다. `child_driver`가
`descendant.pid`의 존재만 확인한 뒤 빈 내용을 읽어 `ParseIntError::Empty`로 중단됐으며
실제 cleanup 단언에 도달하기 전이다. N4b0a의 해당 source도 같은 bytes다.

이는 이미 열린 [#484](https://github.com/bin9208/openpilot-rust/issues/484)의 동일 unit
fixture 사건이다. 기존 `8ae95f73330fa8a18bc78b3b6192de7485443719`의 임시 파일/rename
방식을 필요한 descendant writer 한 곳에 재사용하고, 현재 PID bytes/파서와 나머지
단언을 유지한다. leader PID는 background fork 전에 쓰기가 끝나므로 바꾸지 않는다.
runtime 코드,2초 준비/5초 전체 한도, SIGKILL·같은 session/다른 PGID·무관 child 생존·
leader/descendant 회수 판정은 그대로다. 기존 #296 작업 전체를 가져오거나 #474의
production shutdown 원인을 해결했다고 주장하지 않는다. 로컬은 서식·구문·diff만
확인하며 정확 수정 source의 실제 workspace/복구 결과는 Actions에서 확인한다.

## S237 Genuine 실패와 메모리 진단 보강 — 2026-10-30

source `2375949c19c63667ad66de0f9ee7e2c90ad8ba70`의
[Genuine run40961936699](https://github.com/bin9208/openpilot-rust/actions/runs/40961936699/job/123757390030)은
bootstrap initial 컴파일 중 host 잔여 headroom 검사로 중단됐다. CI fixture의6GiB
한도(`6442450944` bytes)에서 kernel peak는 `5021356032` bytes였고,
`limit_events=0`, observer의 raw `max/oom/oom_kill`도 모두0이었다. OOM이나
양산·기기 메모리 요구량의 측정 결과로 표현하지 않는다.

`resources.jsonl`의 실패 행은 current `1370300416`과 reserve `134217728` bytes로
잔여 요구량 `5206368256` bytes를 계산했지만, 실패 순간의 정확한 MemAvailable은
보존하지 않았다. historical minimum `5230579712` bytes는 검사 실패 전까지 갱신된
값이며 실패 순간 값이 아니다. 이번 변경은 기존 한 번의 읽기값과 required 계산의
locals를 host/ancestor 오류 reason에 남기고, CI observer가 이미 읽은 meminfo에서
`system_memory_total_bytes`만 추가한다. guard 수식·예산·controller·동시실행 정책과
정상 startup의20초 deadline은 변경하지 않는다.

로컬 확인은 Python AST·scoped Rustfmt·diff로 한정한다. 진단 보강 후 실제 값의 기록,
기존 resource CI 검사와 bootstrap 재사용 및 이후 Genuine 전체 검증은 아직 미완료다.

[전체 입력과 빌드 연결](native-complete-inputs-599.md)은18개 역할과 ordinary Git 자산을
하나의 검증된 Layout에 묶고 실제99개 빌드·source-built 봉인/ABI·재사용 검사를 실행하도록
연결한다. 구현 후 새 Actions 성공은 아직 미확인이며 정상 기기 수용은 계속 남아 있다.

[모델 불변 실행 입력](native-model-runtime-inputs-600.md)에 선택한4개 pipeline의 export,
Git source/원래 compiler provenance와 전체 output 확인, native build/Layout/seal/source
generation 시작 검사를 연결했다. 실제11개 runtime Tree 계획 조합과 새 기능 검사는 대기다.
전체 Layout/99개 빌드·실제 봉인과 정상 기기 수용이 완료된 상태는 아니다.

[실제 ONNX 생성·입력별 cache](native-model-generation-cache-600.md)에 내부 QCOM/운전자
모델의 카메라별4개 생성과 입력 동일성에 따른 생략, 원래 provenance와 이전 세대 보존,
실제 ARM Rust consumer 및 docs/작은 logger 변경 재사용 검사를 연결했다. 새 검사는 대기다.
전체 Layout/99개 native 빌드·정상 시작·녹화·업로드와 사용자 comma 3X 수용은 계속 남는다.

[모델 준비 wheel·factory compiler 안내](native-model-wheel-inputs-599.md)에 NumPy/zstandard
내용별 준비와 원래 C 라이브러리 대조, 실제 factory LLVM20.1/QCOM 경로의 Actions 검사를
추가했다. ONNX 전체 생성·생성물 cache의 새 검증과 전체 native runtime 수용은 계속 남아 있다.

모델 준비용 [ARM Python 입력과 실행 검사](native-python-inputs-599.md)를 추가했다.
공식 CPython3.12.16 archive를 내용별로 준비하고 실제 ARM interpreter/stdlib/ELF closure와
반복 전송·복사0을 Actions에서 확인한다. 새 결과와 전체 모델 환경·생성물 cache는 아직 대기다.

현재 source와 독립 Panda firmware의 입력871개·출력4개/원래 provenance를 결속하는
[Panda 준비·검사 안내](native-panda-inputs-599.md)를 추가했다. 현재 source binding,
공개 debug 서명·버전, 반복 전송/복사0을 Actions에서 검사하며 새 결과는 대기다.
전체 native 빌드/실기 flash·수용이 완료된 상태는 아니다.

사용자 결정: 2026-10-26 KST.
[현재 Git LFS 입력](native-source-lfs-599.md)을 OID/size에 묶어 준비하고 문서 변경 재사용,
한 객체만 바꾼 전송/복사와 ARM 준비기의 실제 파일 검사를 연결했다. 새 Actions 결과는 대기다.
[시작용 FFmpeg 도구·클러스터 글꼴](native-media-tools-font-599.md)을 독립 component로
추가했다. 실제 CLI lossless 왕복·Rust glyph/window 픽셀 비교와 ARM ELF 검사는 대기다.
[SDL·TurboJPEG 입력](native-display-inputs-599.md)을 추가해 원래 공용 library53개 경로의
준비 목록을 완성한다. 실제 Rust API/독립 C 비교와 ARM ELF 검사는 대기이며 전체 배치·
모델·99개 native 빌드 완료를 뜻하지 않는다.
[생성 MPC 입력 결속·재사용](native-mpc-source-bindings-599.md)을 추가했다. 원래4662개
Git 생성 입력과 output hash를 고정하고 현재 source/실제 Rust solver ABI를 별도 검증한다.
재생성·전체 Python 준비 환경/99개 native 빌드와 실기 수용은 계속 남으며 새 검사는 대기다.
기존 C/C++ [실행 library4개 component](native-runtime-companions-599.md)의 내용별 준비와
ARM API/ELF·`runtime_inputs` 연결 검사를 추가했다. 생성 MPC·전체 Layout/빌드는 별도이며
이 새 경로는 Actions 검증 대기다.
준비된 toolchain/vendor/SDK/provider9개 역할을 전체13그룹의 계획에 연결하는
[native 입력 조합기](native-input-bindings-599.md)를 추가했다. 그룹별 캐시 분리와
현재 vendor lock 검사, 실제 AGNOS plan 생성/반복 CI는 검증 대기다.
FFmpeg6.1.1의 단일 검증 prefix, 실제 Rust 인코더 release 빌드/반복 재사용 및 원본5개
codec case의 정확 packet 비교를 #599 CI에 추가했다. 구현·검사 방법은
[provider 기록](native-provider-components-599.md)에 있으며, 현재 단계는 기능 검증 대기다.
[대단원 #598](https://github.com/bin9208/openpilot-rust/issues/598)의 모든 완료 조건을
충족한 뒤 종료한다. 이 이슈의 종료를 우선 작업의 완료로 사용한다.

## 범위

성능 개선과 콤마 자체 빌드, 작은 소스 변경마다 전체 runtime을 재패키징하고
약1.1GB를 다시 내려받는 구조의 제거가 우선이다. 모델 준비는 Python을 유지한다.
기존 C/C++ 라이브러리를 재사용하며 비용이 크고 이득이 작은 언어 전환은 뒤로 미룬다.
전체 전환은 이 우선 작업 완료 후 점진적으로 진행한다.

기존 Rust 전환은 버리지 않는다. 검증된 구현을 각 트랙의 정상 PR 경로로 반영한다.
미검증 코드의 무조건 병합과 dev-md→dev/main 통합은 하지 않는다. 전체 전환
#523/#296는 남겨 두고 우선순위 변경 댓글을 추가했다. 전체 관리 #522에 이 대단원을
native 하위 이슈로 연결하고 최신 실행 범위를 기록했다.

| 실행 단위 | 이슈 | 완료할 결과 |
| --- | --- | --- |
| AGNOS 자체 빌드 | [#599](https://github.com/bin9208/openpilot-rust/issues/599) | 실제 native toolchain/provider·Python 모델 준비와 전체 runtime 소스 빌드 |
| 다운로드·빌드 재사용 | [#600](https://github.com/bin9208/openpilot-rust/issues/600) | 대형 자산 캐시·영향 대상 재빌드·검증 후 세대 전환과 복구 |
| 기존 전환·성능·전달 | [#601](https://github.com/bin9208/openpilot-rust/issues/601) | 기존 구현 반영·집중 성능 회귀·최종 전달 및 사용자 수용 |

## 첫 조사 결과

- stable `origin/dev`는 `b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`다. 이 기준에서
  `codex/feat-598-device-native-build`를 만들었다. 기존 dev-md의 미커밋 작업과
  다른 worktree는 변경하지 않았다.
- `ensure_runtime.sh`는 Git HEAD별 `runtime-<SHA>` release를 찾는다. launcher와
  installer 모두 package source_commit과 checkout identity를 검증한다.
  이 검사를 삭제하는 것은 새 source의 올바른 runtime을 만드는 대체 수단이 아니다.
- PR343의 조회 head는 `126fdbea36598566d1040be0a35a365c909be4e4`, dev 대비776개
  커밋이다. 최신 #523 실행 branch는 별도다. source/검증 범위를 감사하지 않고
  모두 merge하지 않는다. 이 커밋 수를 진척률이나 일정으로 쓰지 않는다.
- 기존 성능 PR494는 dev-md 전용이며 복사·할당 후보가 있다. 정확 소스 필수 검증과
  전달·실기 수용은 남는다. 이 변경을 dev에 가져와 원본 반영 트랙을 섞지 않는다.

## 첫 구현: 오프라인 Cargo 빌드와 영속 target cache

`openpilot-runtime-build`는 이미 준비된 native Rust1.94.0과 Cargo 의존성으로
지정 runtime target을 빌드하는 초기 도구다. Cargo의 source dependency 추적을
재사용하고 commit SHA를 target cache 경로에 넣지 않는다. 문서 변경이나 새 커밋이라는
이유만으로 compiled crate를 버리지 않으며 도구 체인 binary·target·feature·명시 외부
입력이 다르면 캐시를 분리한다. 자원 증가량 추정 변경은 compilation identity가 아니다.

- `--locked --offline`, `CARGO_BUILD_JOBS=1`로 직접 Cargo를 호출한다.
  compiler incremental과 별개로 Cargo의 crate freshness를 재사용한다.
- 명시 외부 입력을 전후 hash로 확인하고 작업별 예상 디스크 증가량으로 admission한다.
  원래 source/provider binding을 대체하는 완성된 seal/수용 증거는 아니다.
- cache owner lock으로 같은 cache의 동시 빌드를 거부한다. 소유가 다른 runtime이나
  source tree를 정리하지 않고 build/report만 만들며 설치·활성화는 수행하지 않는다.
- 마지막 성공 보고서는 Cargo executable/fresh 결과다. artifact hash/전체 source를
  봉인한 설치 manifest가 아니며 이후 빌드로 target 내용이 달라질 수 있다.
- 모델 준비, provider/toolchain 다운로드, offline Cargo cache 준비, 전체 runtime recipe,
  source-bound generation 봉인·시작·업데이트 연결은 아직 미구현이다. 자동 시작 경로는
  바꾸지 않았다. 이 도구만으로1.1GB 다운로드 제거나 자체 빌드 완료를 주장하지 않는다.

첫 CI는 독립된 실제 Cargo fixture를 사용해 문서 수정의 binary freshness, 소스 수정의
실제 재빌드/실행 출력, provider 변조·갱신, 동시 cache 소유 및 컴파일 실패를 검증한다.
fixture의 host 결과를 AGNOS 자체 빌드나 장치 성능으로 표현하지 않는다.

## 검증 및 다음 작업

- 로컬: scoped Rustfmt와 diff 확인. 기능·Cargo·빌드·Clippy는 로컬 실행하지 않는다.
- 첫 기능 검증은 `.github/workflows/runtime-source-build.yml`의 실제 Actions 결과로
  기록한다. 실행 전 상태는 **미검증**이다.
- 다음은 AGNOS native toolchain/표준 라이브러리·linker·provider receipt/cache 준비,
  각 기존 runtime의 실제 Cargo build/feature/asset roster 확정, 전체 recipe와
  기존 source-matched manifest/activation 경로의 연결이다.
- clean/no-op/변경 후 빌드의 시간·메모리·공간·전송량과 사용자 정상 부팅·녹화·업로드
  수용을 구분한다. 이 문서나 이슈 생성만으로 #598/#599/#600/#601을 종료하지 않는다.

4~8주는 초기 계획 예산이며 장치 native 빌드 실측 전 확정 일정이 아니다.

## 첫 실제 Actions 결과와 독립 리뷰

source `10d57ed8c9c37246e2caf5986925439f3d68cc05`의
[40273209126](https://github.com/bin9208/openpilot-rust/actions/runs/40273209126)은
실제 offline Cargo 회귀3개와 strict all-target Clippy·서식·locked metadata 검사를
모두 통과했다. 문서 변경 후 Cargo fresh/동일 binary hash, 코드 변경 후 실제43 출력,
provider 변조 거부/갱신 후 cache 분리, 동시 owner 거부와 컴파일 실패 보존을 확인했다.
이는 Linux x64 fixture의 성공이며 AGNOS나 전체 runtime 자체 빌드가 아니다.

[PR602](https://github.com/bin9208/openpilot-rust/pull/602)는 dev 대상 draft다.
필수 전체 검사와 기존 전환분 반영, 장치 자체 빌드/다운로드 제거는 아직 미완료다.

독립 GPT-6.1 Sol 리뷰는 시스템 linker가 바뀌어도 Cargo freshness가 이전 결과를
재사용할 수 있음을 지적했다. 후속은 실제 cc/ld/collect2 경로·hash를 cache 입력과
후검증에 연결하고 Cargo의 linker 선택을 확인한 경로로 고정한다. build 환경의
임의 linker override는 허용하지 않는다. 필요한 외부 provider/전체 toolchain receipt는
여전히 #599에서 준비해야 하며, 이 작은 도구의 자동 hash가 전체 ABI 봉인을 대신하지 않는다.

후속 CI에는 native ARM Linux host의 동일 fixture·Clippy와 작업별3GiB 예상 증가량
admission을 추가했다. x64/ARM 어느 결과도 comma 장치/AGNOS 검증으로 표현하지 않는다.
후속 소스는 새 실제 결과를 확인하기 전까지 미검증이다.

source `d7fb4a54a072f49809bcfc7e7091db7ff32f13f7`의
[branch40273655126](https://github.com/bin9208/openpilot-rust/actions/runs/40273655126)와
[PR40273660041](https://github.com/bin9208/openpilot-rust/actions/runs/40273660041)는
x64/ARM Linux 두 host에서 모두 fixture3·strict Clippy·서식/locked metadata를
통과했다. 실제 linker 입력 보완까지 이 범위로 수용하며 AGNOS 검증으로 확대하지 않는다.

후속 working tree에는 관련 없는 target의 Cargo freshness를 확인하는 실제2-package
회귀와, 컴파일 실패 후 이전 출력 binary가 실제로 계속42를 출력하는 검사를 준비했다.
push/PR에 중복된 focused matrix는 이후 branch에서 한 번 실행하고 원래 PR workspace
gate는 유지하는 조정이다. 이 후속의 새 실행 결과는 아직 없다.

## 이어가기: 기반 반영과 자산·소스 연결 — 2026-10-26 KST

PR #602 source `13d7ce113bf602b2be5b5abedb55c4debeb83a8c`의 최신 검사68개가 모두
SUCCESS인 것을 확인하고 `dev`에 병합했다. 병합 SHA는
`e445c9d19c3041bc79d8624589e2eb6ba7d061c6`이다. 별도 병합 후 집중 검사
[40299610428](https://github.com/bin9208/openpilot-rust/actions/runs/40299610428),
[fast40299610443](https://github.com/bin9208/openpilot-rust/actions/runs/40299610443),
[docs40299610391](https://github.com/bin9208/openpilot-rust/actions/runs/40299610391)은
SUCCESS다. 전체 Rust/통합 검사는 진행 중이며 아직 성공을 가정하지 않는다.

후속 branch `codex/feat-600-native-input-cache`는 이 병합 SHA에서 분리했다.
기존 다른 채팅 소유 worktree와 dirty dev-md는 수정하지 않았다.

- HTTPS 자산을 SHA-256·정확 byte 수로 확인하는 내용별 캐시를 추가했다. 같은 입력은
  hash 재검증 후 전송0, provider 하나 변경은 그 객체만 준비한다. 이전 객체는 보존한다.
- per-object lock·임시 파일·fsync 후 원자적 발행과 작업별 공간 admission을 사용한다.
  잘못된 hash/길이·변조된 hit·중단·동시 owner·symlink를 거부한다. 압축 해제나
  설치·활성화는 수행하지 않는다.
- 빌드 보고서 schema2에 실제 clean Git commit/tree·recipe·rustc/Cargo·native linker·
  실행 파일 hash/길이·freshness·소요 시간을 묶는다. 미커밋 입력과 빌드 중 source
  변경은 성공 보고서를 발행하지 않는다. compiler와 Cargo도 빌드 후 재검증한다.
- 실제 toolchain 전체 표준 라이브러리/header/ABI receipt, 생성물과 전체 source graph,
  완성된 runtime recipe·generation/activation은 여전히 후속이다. 작은 보고서가
  기존 root source/AGNOS/ABI/package 검사를 대체하지 않는다.
- 실제 Cargo 회귀5개와 자산 fixture5개를 x64/ARM Actions에 연결했다. 이 후속은
  실행 결과를 확인하기 전까지 미검증이다. 로컬은 Rustfmt·TOML/YAML 구문·diff만 확인했다.

개발자 실행 명령과 최종 사용자 기기 수용 절차는
[검증 안내](native-build-test-guide-598.md)에 정리했다. #598/#599/#600/#601의
미완료 조건은 유지하며 전체 장치 자체 빌드나1.1GB 다운로드 제거 완료로 계산하지 않는다.

## 압축 해제 입력 검증 후속 — #599

자산 캐시 후속 PR #603의 source `b42e5eaeec5a13bf83f24ccd31ab5e3364eb2d47`은
[40301461308](https://github.com/bin9208/openpilot-rust/actions/runs/40301461308)의
x64/ARM 집중 검사에서 모두 SUCCESS다. 실제 HTTPS CLI가 고정 공개 입력403바이트를
처음 수신하고 재준비에서0바이트인 것도 확인했다. 이것은 큰 실제 model 전송의
실측값이 아니며 전체 필수 PR 검사는 별도로 남아 있다.

이 source에서 `codex/feat-599-verified-input-trees`를 분리해 압축 해제된 toolchain,
provider header/library와 생성 입력의 전체 tree receipt를 추가했다. 파일 hash/크기/
권한, 내부 파일 symlink와 directory/member 목록을 빌드 전후 재검증한다. receipt는
기존 `inputs`의 pinned SHA로 묶어야 하므로 변조한 tree의 receipt만 다시 생성해도
자동으로 신뢰하지 않는다. 기존 자동 source/AGNOS/ABI 검사는 제거하지 않는다.

이 후속은 Actions 실행 전까지 미검증이다. source receipt·provider 별 검증과
전체 AGNOS recipe에 실제 입력을 연결하는 단계는 구분한다. 빈 `verified_input_trees`를
전체 도구체인/표준 라이브러리/AGNOS 시스템 검증의 증거로 쓰지 않는다.

### 후속 검증 및 이어가기 상태

입력 tree source `98e0b13ead50e21c86a4c8bf1d86a407e0ecab83`의
[40302186437](https://github.com/bin9208/openpilot-rust/actions/runs/40302186437)은
x64/ARM 모두 SUCCESS다. 각 host에서 자산5·실제 offline Cargo6·입력 tree/실제
receipt CLI3 회귀와 strict Clippy, 실제 HTTPS 최초403바이트/재사용0바이트를 확인했다.
이는 전체 AGNOS runtime 자체 빌드·세대 전환·실기 수용의 성공이 아니다.

- PR #603 head `b42e5eaeec5a13bf83f24ccd31ab5e3364eb2d47`은 필수 전체 검사를
  기다린다. 이 PR의 변경은 더 수정하지 않고 최신 source의 실패/성공을 확인한다.
  모두 통과한 뒤 정상 병합하고 별도 병합 후 검사도 확인한다.
- 입력 tree branch는 PR #603의 후속이다. 집중 검사는 수용했으며 #603 반영 후
  최신 dev 기준의 PR 및 필수 검사로 이어간다. 전체 이슈는 닫지 않는다.
- runtime 작업 checkout은 `/home/fma1/.codex/worktrees/runtime-native-598/openpilot-rust`,
  branch는 `codex/feat-599-verified-input-trees`다.
- 기존 성능 PR #494 source `72ba387abaf54f9116409402c141faa5bb1ca54a`의
  workspace log를 직접 읽었다. FFmpeg fixture 보완은 통과했고 dev-tools lib는
  52 passed/12 failed다. media4개/raylib8개가 실제 filesystem 공간에 의존했다.
  단순 빌드 전 admission 실패로 표현하지 않는다. 새 실행 이슈
  [#604](https://github.com/bin9208/openpilot-rust/issues/604)를 #601 아래에 연결했다.
- #604용 checkout은 `/home/fma1/.codex/worktrees/runtime-performance-601/openpilot-rust`,
  branch는 `codex/fix-604-media-fixture-space`다. 원래 성능 head에서 분리했고 현재
  읽기 감사만 수행했다. 기존 owner의 checkout과 원격 PR #494는 아직 변경하지 않았다.
- 성능 checkout의 AGENTS에는 2026-10-23 all-local 지침이 있지만 이후
  [#535](https://github.com/bin9208/openpilot-rust/issues/535)에 사용자가 2026-10-25
  `bin-actions`는 타관리자의 현재 사용 불가 runner이며 GitHub 기본 runner를 쓰라고
  명시한 결정이 있다. 이전 all-local 변경을 현재 검증 경로로 무조건 반영하지 않고
  최신 지시와 각 트랙을 확인한다. 타관리 runner에 새 작업·정리를 보내지 않는다.
- 위 runner 판단은 당시 기록이다. 2026-10-26 이 채팅에 다시 제공된 사용자 지침이
  이를 대체한다. 이후 모든 새 Actions job은 등록된 `bin-actions`
  (`self-hosted`, `Linux`, `X64`, `dev-md-local`)에서만 실행하며 hosted fallback은 없다.
  기존 실행 중 검사와 다른 세션의 소스·CI·cache는 보존한다. ARM은 별도 QEMU/PRoot
  userspace로 원래 pin/명령을 유지하고 실제 기기/물리 ARM 수용과 구분한다.
- 원래 정상 배포 source b555의11개 build group과 feature/env 기록은 이전 #296
  checkout의 `.analysis/scratch/2026-10-14-agnos296/build-target-groups-b55552db.sh`
  및 `target-build-b55552db.json`에 있다. 실제 native recipe를 정할 원문 증거이며,
  cross SDK 경로나 과거 build 결과를 comma native 환경으로 위장해 사용하지 않는다.

다음 구현은 실제 native toolchain/헤더/provider/Python 모델 준비와 runtime roster,
완전한 source/result 검증 후 generation 전환 및 기존 startup/update 연결이다.
기기 설치·시험은 아직 하지 않았다. 사용자 장치 종류 질문은 답변 대기이며,
구현/CI는 장치 종류 답변 없이도 진행할 수 있다. 사용자의 수용을 대신 체크하지 않는다.


## 2026-10-30 — #598 seed ABI 사전 검사와 실패 증거 분리

S `84fbed62953b61474e80f308aa1f530343d20561`의 Genuine
[41001309333/job123886092388](https://github.com/bin9208/openpilot-rust/actions/runs/41001309333/job/123886092388)는
step65에서 실제 실패했다. `bootstrap_seed.py export`의 `inspect_abi()`가 호출한
`ldd`가 exit1이었으며, 당시 내부 stdout/stderr는 보존되지 않았다. candidate index I와
payload는 생성되지 않았다. 이 S를 입력 supplier로 재사용하는 계획과 old S ARM/WS
조건부 재시도는 폐기한다. 실제 재시도는 각각0회이며 기존 Actions 자체는 보존한다.

기존 Genuine job에 `--bootstrap-seed-abi-preflight` 내부 mode를 추가한다. 고정 image,
Rust1.94.0, 인증된 Cargo vendor, Capnp/host preparer가 준비된 뒤 codec/provider의 큰
준비 전에 실행한다. 동일 canonical PRoot bind/env와 system SDK pin을 유지하고,
정규 `native_cereal_probe`와 공유하는 동일 Cargo argv로 실제 `openpilot-runtime-inputs`만
빌드한다. guest 안에서 `runpy`로 기존 seed의 `native`, `elf`, `inspect_abi`를 호출하며
early export·generation 발행은 하지 않는다. 아직 준비 전인 bind source는 고정된
job-owned 빈 root 목록만 생성하며 provider가 준비되었다고 간주하지 않는다.

사전 검사 결과는 별도 `native-bootstrap-seed-abi-preflight.json`과 같은 이름의
증거 폴더에 보존하며 `preflight_only=true`, `full_probe_success=false`,
`seed_exported=false`, `payload_created=false`로 구분한다. 성공 때만 owner/비 symlink/
inode를 재확인하여 본인이 만든 canonical work를 제거한다. 이후 무인자 전체 probe는
fresh work에서 원래 모든 검사와 late seed export를 계속 수행한다. 실패 시 work와
오류 자료는 유지하고 기존 always artifact의 수집 경로에 별도 report/증거 폴더를 추가한다.

이번 수정은 AST/YAML/diff 정적 검토만 수행한다. unit/기능/build/gate helper는 로컬에서
실행하지 않는다. 새 patch source의 실제 preflight·Genuine 전체 성공·입력 발행 검증은
후속 Actions에서 필요하며 현재 성공으로 계산하지 않는다. commit/push/CI 재실행·취소는
이 변경 담당 범위에 포함되지 않는다.

## 2026-10-30 — #598 c774 Genuine SDK probe 인자 전달 누락

`c774453b5a9c0a39f7c89288de6299e0a7cf6761`의 Genuine
[41018353145/job123939710792](https://github.com/bin9208/openpilot-rust/actions/runs/41018353145/job/123939710792)는
step66에서 `native_system_sdk_probe`의 미정의 `source_mount`로 실패했다. seed export와
prepare 2회는 실제 exit0이며, 뒤쪽 SDK 회귀 cargo test argv 작성 중 Python 오류가 났다.
함수 signature와 main 호출에 기존 canonical `source_mount`를 전달하는 두 곳만 수정한다.
기존 명령·단언·ABI·입력 pin·자원 한도·startup20은 유지한다. 함수 body AST 불변과 호출1개·인자7개 일치를 확인했다.
`uv.lock`에 고정된 Ruff0.16.7 wheel의 SHA·bytes를 검증해 정적 검사에만 사용했다. 원본은 F821 한 건을 재현했고,
수정본의 native CI·source entry·모델 준비 helper·fixture·관련 검사 파일은 F821/F823이 없다. 로컬 기능 검사는 실행하지 않았다.
원본 성공6개와3 immutable context는 보존하며, 후속 정확 source의 전체 Genuine·I/payload·필수 검사 성공은 별도로 확인한다.

## 2026-10-30 — #598 84d05 FFmpeg C++ fixture 전처리 실패

`84d05e994d9cd8f18d41c6ca75cc54bfff260c81`의 Genuine
[41043044388/job124020269257](https://github.com/bin9208/openpilot-rust/actions/runs/41043044388/job/124020269257)는
SDK 회귀4개를 실제 통과한 뒤 `native_codec_probe`의 FFmpeg C++ fixture에서 실패했다.
실제 stderr는 `libavutil/common.h:30`의 `__STDC_CONSTANT_MACROS` 미정의 `#error`이며,
링크 및 ABI 버전 비교 전 전처리 실패다. 해당 fixture의 include 앞에 매크로 한 줄만 추가한다.
공식 [FFmpeg n6.1.1 common.h](https://raw.githubusercontent.com/FFmpeg/FFmpeg/n6.1.1/libavutil/common.h)의
26–28행에서도 같은 C++ macro guard를 확인했다. `native_ffmpeg_headers_probe`와
`native_provider_probe`의 동형 inline C++에도 각각 한 줄을 추가해 총3 fixture만 보완한다.
원본 archive의 선택적 읽기는 network timeout으로 검증하지 못했으며 실제 asset URL·SHA는 변경하지 않는다.
version58/59·140 headers·hash·`avutil_version()`/ABI 비교·실행 stdout·ELF183·입력 pin·guard·startup20·원래 g++ argv와 호출 순서는 유지한다.
로컬 AST·F821/F823·diff만 확인하며 C++ 링크·기능 검사는 실행하지 않는다. 수정 후 실제 Genuine 전체·I/payload 성공은 미검증이다.

codec 검사는 `native_cereal_probe` 직후·`native_bootstrap_seed_probe` 직전으로 한 호출만 앞당긴다.
호스트 wheel/cache와 guest compiler는 이미 준비돼 있으며 codec 출력은 독립 owned 경로를 사용하고
source·factory SDK를 수정하지 않는다. seed helper를 덮어쓰지 않으며13tools의 별도 store도 유지한다.
회귀는 codec1회·cereal 바로 뒤·bootstrap 바로 앞을 명시하고, 나머지 원래 호출 순서·개수와 seed 함수
본문·공유 Cargo argv 전체 대조를 유지한다. 원래 각 검사·명령·단언은 모두1회 수행한다.
선행 컴파일에 따른 시간·디스크/cache 수치는 후속 실제 Actions 결과로 다시 판정하며 성공을 예단하지 않는다.

## 2026-10-31 — #598 Python ELF의 객체 기준 ORIGIN 해석

`1614b70eb3d12ee57ba0ce9765ec601e987283c8`의 Genuine
[41071607072/job124116883850](https://github.com/bin9208/openpilot-rust/actions/runs/41071607072/job/124116883850)는
Python health와 FFmpeg 세 fixture의 해당 compile/실행/ABI 단언을 통과한 뒤 Python ELF 탐색에서 실패했다.
`libpython3.so`의 DT_NEEDED `$ORIGIN/../lib/libpython3.12.so.1.0`을 기존 resolver가 탐색
디렉터리에 그대로 붙였다. 대상 파일은 인증된 prefix receipt에 존재한다. 진단 artifact13000705489의
52,032,053B·SHA `aa60516576323efb3d7ebea78dffd6894fff9f98eb1a79327909e7669fe729ff`가 API와 일치한다.
의존성 탐색에서 중단되어 전체 Python 심볼·버전 보고서는 생성되지 않았다. 추가 오류가 없다는 뜻은 아니다.

[동적 loader 문서](https://man7.org/linux/man-pages/man8/ld.so.8.html)의 DT_NEEDED 토큰 의미에 따라
ORIGIN을 선언한 객체의 디렉터리를 기준으로 해석한다. 서로 다른 객체의 같은 상대 문자열을 하나의
provider로 합치지 않으며, 기존 명시적 탐색 경계와 아키텍처·버전·강한 심볼 판정을 유지한다.
Python 검사를 codec 뒤·긴 bootstrap fixture 앞으로 옮기고, 필수 auditor의 원래 Cargo 빌드 한 호출도
같이 옮긴다. 이후 provider는 같은 auditor를 사용한다. 원래 Cargo argv·로그 이름과 검사 횟수,
나머지 호출의 순서·개수, seed 본문·공유 Cargo argv를 회귀에서 계속 비교한다.
로컬은 서식·구문·diff 검토만 수행하며 실제 resolver 회귀·Python 전체 ABI·Genuine/입력 공급·전체
source entry와 완전 후보 성공은 Actions에서 별도 확인한다. 기존 성공6 workflow와 실패 증거는 보존한다.

같은 고정 Python 원본29,234,064B·SHA `bb4b6392670c5d3f93876996fcaa19858a4a413a0c7720144c18b0d6f1f0db2d`의
10개 ELF를 실행 없이 읽어 receipt의 각 SHA/byte와 대조했다. `_crypt`·`_dbm`·`_tkinter`에는
각각9·31·96개의 강한 Python 미정의 심볼과 Python DT_NEEDED 부재를 확인했다. 실제 실행파일은
해당 Python export를 제공한다. 독립 root closure만 검사하면 이 실제 로딩 소유자 관계가 빠진다.
[dlopen의 main program/dependency 심볼 범위](https://man7.org/linux/man-pages/man3/dlopen.3.html)에 맞춰
3개 확장에만 `--load-owner`로 실제 Python 실행파일을 명시한다. 나머지7개 ELF는 기존 독립 검사로
유지하고 소유자 자체도 독립 검사한다. 무관 root의 export 합집합이나 Python 심볼 무시는 허용하지 않는다.
기존 health에서도 세 확장을 실제 import하고 `RTLD_NOW`·prefix·격리 상태를 확인한다. Tcl/Tk는
확장의 실제 DT_NEEDED에 있으며 별도 Tcl 소유자 예외를 추가하지 않는다. factory 의존성과 전체 ABI의
실제 수용은 Actions에서 확인해야 한다. 이 추가 원본 읽기는 실행 중인46b9e 검사의 성공/실패 판정이 아니며,
그 검사·기존 성공 결과를 취소하거나 변경하지 않는다.

## 2026-10-31 — 같은 ORIGIN 오류의 I/O 원문 확보

`bcd395ba8cb9a471807dbb1c84ca5e59ce775ee0`의 [Genuine41107570272](https://github.com/bin9208/openpilot-rust/actions/runs/41107570272)는
확장3개 import·RTLD_NOW·prefix 검사와 early codec을 통과했지만 독립7 ELF 검사에서 같은 경로 오류로 실패했다.
진단 artifact13009388723은47,422,909B·SHA `169c614723b36078bfac3c3e26d26b337aaba8d20630e7ede85b3612889a6e4e`다.
원래 I/O 오류와 확장한 경로가 버려져 정확한 OS 원인·실제 분기는 미확정이다. 반면 현재 PR611의
`8b9d2c429e6ad11701cea54b079edba313d3d0e1`은 정상 runner의 workspace에서 strict Clippy와
ELF 통합12개·unit4개를 실제 통과했다. 호스트 회귀를 AGNOS 전체 성공으로 대신하지 않는다.

현재 PR과 진행 검사는 보존하고 진단 브랜치에서 기존 seed ABI 조기 단계의 경로 읽기만 보강한다.
원래 Python cache3단계의 본문·키·인자·횟수는 유지한 채 앞으로 옮긴다. 인증된2파일을 같은 guest
경로에 투영하고 factory Python에서 [Rust1.94와 같은 `realpath(path,NULL)`](https://raw.githubusercontent.com/rust-lang/rust/1.94.0/library/std/src/sys/fs/unix.rs)을
호출해 경로·errno·stat·SHA를 남긴다. 이 결과는 projection 전용이며 전체 prefix/ABI 검증이 아니다.
후보 ELF·인터프리터·모델은 실행하지 않고 원래 seed 검사·Cargo argv·공간 admission·전체 probe를 유지한다.
Rust resolver도 원래 I/O 오류와 확장 경로를 보존한다. 원인을 추측해 경로 fallback이나 ABI 예외를 넣지 않는다.
별도 workflow·모드·검증 체계·발행 후보를 추가하지 않으며 실제 진단 결과는 Actions에서 확인한다.

## 2026-10-31 — 네이티브 ARM 검증 도구의 접근 검사 매핑

진단 source `1c62279a457909bb005ad7ad1ca45491a8fa9a81`의
[Genuine41129815036/job124317574169](https://github.com/bin9208/openpilot-rust/actions/runs/41129815036/job/124317574169)은
기존 조기 단계에서 실패했다. 두 고정 파일 모두 plain 경로의 `realpath(path,NULL)`은 성공하고
`lib/../lib/` 형식만 NULL·ENOENT(2)였다. 네 경로의 stat·열기·SHA·크기·모드와 파일별 dev/ino는
모두 일치했다. artifact13013821376은8,048,145B·SHA
`55e9baea8e080d20f1051c5619ebcf4f919b7cf4083222071dc486bb24597a42`로 API와 일치한다.
이는 두 파일의 투영 경로 증거이며 원래 seed ABI·전체 prefix·전체 Genuine 성공은 아니다.
같은 source의 [기반 검사41129773929](https://github.com/bin9208/openpilot-rust/actions/runs/41129773929)는
네이티브 ARM과 x64에서 성공했다. PR611의 기존 검사는 별도로 보존한다.

현재 PRoot pin은 `25dc6a3134891f98a79f57ce1c2c1b23ff15cad1`(v5.5.0)이다.
[고정 ARM64 syscall 테이블](https://github.com/proot-me/proot/blob/25dc6a3134891f98a79f57ce1c2c1b23ff15cad1/src/syscall/sysnums-arm64.h)에
`faccessat2`의439번 매핑이 없지만 같은 소스의 enter·seccomp에는 해당 처리 코드가 있다.
[glibc2.39 canonicalize](https://github.com/bminor/glibc/blob/release/2.39/master/stdlib/canonicalize.c)는
`..` 앞 디렉터리 검사에서 `faccessat`의 `AT_EACCESS` 경로를 사용한다.
CI 도구의 ARM64 매핑 한 항목만 보완하고 원본 archive 및 수정 전후 파일 SHA와 빌드 receipt를 검증한다.
원래 네 경로 검사가 실제 통과하는지 Actions에서 먼저 확인하며 내부 원인의 완전한 입증이나 전체 성공을
수정만으로 주장하지 않는다. 원본 이미지·libc·후보 resolver·Python·네이티브 라이브러리·ABI 경계는 유지한다.

## 2026-10-31 — 모델 준비 수치 fixture의 float64 비교

`26598ebfe4b24ddcf640bebce79aeadedcb5d638`의
[Genuine41133249968/job124329282603](https://github.com/bin9208/openpilot-rust/actions/runs/41133249968/job/124329282603)은
정규 모델 환경 fixture의 SVD 비교에서 실패했다. NumPy2.5.3의 고정 float64 대각행렬
`[[3,0],[0,2]]` 결과는 `[2.9999999999999996,2.0000000000000004]`이며,
기대값 `[3,2]`와 각1 ULP 차이인데 `assert_array_equal`이 완전동일을 요구했다.
artifact13024950834의52,149,990B·SHA
`5c26b912720c195837de8dd9b5e7aaf5012f709fcbbe11e6e8db291ad0400a07`가 API와 일치한다.

이 실행에서는 네 경로의 realpath/stat/hash와 PRoot patch receipt, 원래 seed ABI,
Python 독립7개 및 실제 owner/확장3개의 전체10 ELF·health, SDK 회귀4개,
현재 bootstrap13 tools의 initial/reused/docs/small이 실제 통과했다.
13 tools의 성공은99개 full-generation·전체 source entry·입력 index의 성공이 아니다.
역행렬 검사와 zstandard·CPU LLVM·QCOM fixture 및 그 뒤 전체 검증은 아직 미확인이다.

수정은 해당 SVD 비교 한 곳에만 명시적 float64·최대1 ULP를 사용한다.
[NumPy의 ULP 비교 API](https://numpy.org/doc/stable/reference/generated/numpy.testing.assert_array_max_ulp.html)로
한 단위보다 큰 차이는 계속 거부한다. 원래 행렬·역행렬/정수 tensor의 완전동일 검사,
모델·provider·버전·SHA·ABI·유효성·시작 deadline은 유지한다.
로컬은 구문·diff와 원문 읽기만 수행하며 수정 후 실제 수치/모델 검증은 Actions에서 확인한다.

같은 실패의 늦은 발견을 줄이기 위해 긴 `native_bootstrap_seed_probe` 한 호출을
기존 model wheels 검사 바로 뒤로 옮긴다. SDK·FFmpeg headers·provider·model wheels는
bootstrap 전용 출력물을 읽지 않음을 정적으로 대조했다. 나머지 원래 probe의 상대 순서와
호출 횟수·함수본문·Cargo argv·후속 production driver를 유지하며, 기존 순서 회귀도 seed
정확1회·새 위치와 나머지 원래 순서를 검사한다. 실제 순서 변경의 수용은 새 Actions로 확인한다.
