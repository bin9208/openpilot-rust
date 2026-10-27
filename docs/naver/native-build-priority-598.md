# 성능 개선·콤마 자체 빌드 우선 실행 — #598

사용자 결정: 2026-10-26 KST.
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
