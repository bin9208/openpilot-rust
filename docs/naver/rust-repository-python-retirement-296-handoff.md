# 저장소 전체 Python 전환 작업 인수인계

## 2026-10-18 재개 상태 — 아래 과거 중단 기록보다 우선

작업 폴더와 #296 브랜치는 동일하다. dev-md/#338은 별도 작업으로 유지한다.
사용자의 최신 지시는 부모 Astra, 별도 워커는 모두 GPT-6.1 Sol이다.
현재 콤마는 없으며 사용자가 git pull 후 정상부팅·녹화·업로드를 직접 확인한다.
연결을 다시 요청하거나 기기 접근을 기다리지 않는다. 검증된 변경을 커밋하고
필수 PR/통합 검사를 거쳐 반영하며, 실기 결과는 사용자 확인 전까지 미확인이다.

원격 a8189aec까지 JSON 접근 할당 최적화와 UI 진단 CI 명령 수정이 반영됐다.
후속 fast timeout은5035bd54에서 알림 관리자 검사를 별도 필수 작업으로
분리했다. Params 검사기는 실제 원본47사례/오류15경계와 현재 후보를 비교하고
Python 삭제 후에도 통과했다. selfdrived 원본 비교의 끊어진 import를 복구하고
planner 입력 주기 차이도 수정했다. 후속 정확한 커밋의 원격 검사는 필수다.
검증 중인 배치 행렬 컴파일러는 일괄 커밋하지 않는다.
Forms 참조/검사기는208 추가 사례와 원본 삭제 후18장면/388프레임 일치,
원본·의존성 변경 거부 검사까지 통과했다. 두 Python 도구는 제거했다.
카메라 프레임·패킷 C++ 원본 생성기2개도 Rust로 전환했다. 원본 출력 바이트,
실제 호출부,512시나리오/72,493단계 및3,281패킷 비교를 통과했다.
후속 카메라 생성기10개와 UI 렌더2개도 호출부 전환 및 삭제 후 검증을 통과했다.
센서 상대 경로 검색 차이와 Toggle 연속 release 처리 차이를 원본 사례로 수정했다.
UI framework 오케스트레이터의 실제13단계 검사도 모두 통과했다.
X11 실제 입력·종료, UI 녹화3프레임/로그/EGL을 포함한다. 기기 검증은 아니다.
카메라 전체 수명주기 비교도32+8사례와오류·경로·환경회귀를 통과하여
CLI/공유헬퍼/이전테스트4파일을 추가 제거했다.
카메라 상태·연속 스트레스 검사도 Rust 호출 경로로 전환했다. 원본 스키마와
프로세스 오류 처리·상대 경로·errno 회귀를 보존했다. 일반 stress 검사도 별도의
엄격한 stderr 계약을 유지해 전환했다.
상태 검사기의 마지막 Python 의존이던 센서 비교·노출 quoting 헬퍼도 제거했다.
삭제 후19개 관련 테스트와 실제1,080프레임 비교를 다시 통과했다.
연속 카메라 런타임·cleanup·CI 오케스트레이터3파일도 Rust로 전환했다.
원본60개 검증 계약과25개 제어 흐름 사례를 보존했고, 실제58단계 호스트 검사가
통과했다. 삭제 후49개 테스트·Clippy와 새로 빌드한 카메라의8개 실행 사례도 통과했다.
인벤토리는119 replaced-local /3 partially-replaced /3,458 pending이다.
UI 수명주기, 인코더 호스트 및 ARM 대상 검증 도구가 추가 전환되었다.
원본·Rust ARM 비교는 AGNOS 라이브러리를 사용하는 호스트 QEMU에서
5사례·112패킷을 통과했다. UI 테스트의 ETXTBSY 경합도 재현·수정했다.
모델 컴파일러와 전체 전환 및 정확한 최종 커밋의 원격 검증은 아직 진행 중이다.
부분 전환 항목의 공용 원본 로더는 추가 파일 selfdrived_helpers_source.py에
남아 있으며 이동을 Rust 전환으로 세지 않는다. 전체 전환은 미완료다.
최신 검증 결과는 evidence 및 runtime-module-audit 문서를 따른다.
공유 primary/dev-md 캐시 대신 아래의 cargo-target-retirement-296을 사용한다.

아래 2026-10-16 중단 기록의 로컬 모델 워커 지시, 재연결 요구 및 중간 실패 상태는
역사 기록이다. 현재 워커 상태와 후속 소스/검증 기록을 먼저 확인한다.

2026년 10월 16일 03:26 KST 기준. 사용자가 사용량 한도로 부모 Sol 또는 Astra와 로컬 모델 워커로 전환하려고 요청하여 구현을 중단했다. 모든 워커가 중단을 확인했고 로컬 Cargo·캡처 작업은 종료됐다. 소스와 검증 자료는 보존되어 있다.

**전체 전환은 미완료다.** 기준 인벤토리 3,580개 중 51개 소스를 검증 후 제거했고 3,529개는 pending이다. 별도로 6개 내장 Python 명령을 네이티브로 대체했다. 현재 미커밋 모델 컴파일러에는 알려진 수치 불일치가 있으며, UI 도구에도 재검증할 변경이 있다. 이 상태를 일괄 커밋하거나 장치에 배포하면 안 된다.

## 1 시작 위치와 브랜치

실제 작업 디렉터리:

```text
/home/fma1/.codex/worktrees/native-build-tools-296/openpilot-rust
```

- 저장소: `bin9208/openpilot-rust`
- 작업 브랜치: `codex/feat-296-repository-python-retirement`
- 기반 dev: `b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`
- 이슈: https://github.com/bin9208/openpilot-rust/issues/296 및 #1
- Draft PR #343: https://github.com/bin9208/openpilot-rust/pull/343 — 대상은 **dev**.
- `/home/fma1/rust/openpilot-rust`는 별도 **dev-md / #338** 작업이다. 그 폴더에서 이 작업을 구현·스테이징·리셋하거나 dev로 통합하지 않는다.
- `bin9208/openpilot`, `ajouatom/openpilot`으로 push하지 않는다. main 병합·장치 배포는 이번 인수인계 범위에 없다.

현재 폴더를 열었던 다른 에이전트는 먼저 아래처럼 실제 worktree로 이동한다.

```bash
cd /home/fma1/.codex/worktrees/native-build-tools-296/openpilot-rust
git branch --show-current
git status --short
git log -5 --oneline
```

기존 파일을 유지한다. `git reset --hard`, `git clean`, worktree 삭제, 현재 scratch 일괄 삭제를 하지 않는다. 특히 untracked Rust 파일과 ignored scratch 안의 디코더 프로토타입도 진행 중인 구현이다.

## 2 커밋과 원격 검증 상태

인수인계 문서 작성 직전 구현 HEAD는 `2fbf969fad61e973d54af213cabed518c518947e`다. 이후 문서 보존 커밋만 추가될 수 있다.

| 커밋 | 내용 | 원격 상태 |
| --- | --- | --- |
| `a077b0d246863865d943010104b857ed9fe1a5c9` | EGL 참조·검사, 두 명령 검사, Params 검사 등 5개 Python 소스 추가 제거. 누적 51개. | push 완료, 현재 PR head |
| `464309e054fb66ef5d8145b65deb1ac082c53130` | Miri libcore 해시 수집 내장 Python을 `hash-miri-core`로 교체 | 로컬만 |
| `178230571c26e43be6ccbc7b2459104785f9855a` | 검증된 카탈로그 ID를 `model-run --catalog-generation`으로 읽음 | 로컬만 |
| `2fbf969fad61e973d54af213cabed518c518947e` | NumPy 이미지 정수 배열 비교 연산을 Rust로 구현 | 로컬만 |

`a077b0d2`의 Fast checks(PR), user docs, Integration은 통과했다. push 이벤트 Fast checks 하나는 concurrency로 cancelled되었으며 성공으로 세지 않는다.

Rust CI: https://github.com/bin9208/openpilot-rust/actions/runs/38777900468

마지막 조회에서 43개 job 성공, `rust workspace checks`와 `rust aarch64 build` 실행 중, 확인된 실패는 없었다. 집계 job까지 포함한 최종 성공은 아직 확인하지 않았다. 외부 CI는 취소하지 않았으므로 재개 시 정확한 SHA의 최종 결과를 먼저 확인한다.

```bash
gh run view 38777900468 --json headSha,status,conclusion,jobs
gh pr view 343 --repo bin9208/openpilot-rust --json headRefOid,baseRefName,isDraft
```

이전 `38a5abc6a1e3bb0d7e3ffe7729b7fc62cc92d75c`는 46개 Rust job 전체 통과:
https://github.com/bin9208/openpilot-rust/actions/runs/38759226239
이 결과는 후속 커밋의 검증을 대신하지 않는다.

## 3 완료되어 커밋된 최근 작업

- `a077b0d2`: 실제 fast routing으로 dev-tools 310개 + service-tools 16개, 선택적 EGL 6개 테스트 통과. 기본/EGL strict Clippy, fmt, Python 호출부 10개 테스트와 Ruff 통과. Params는 현재 C++ 원본을 동적으로 빌드하여 367개 키와 잠금·복사·정리 동작을 비교했다. 기존 Python 잠금 시간초과의 무한 대기만 명시적으로 소유 자식 종료·회수로 보완했다.
- `464309e0`: 원본 명령 31개 사례, 관련 테스트 47개, strict Clippy와 실제 Miri sysroot 비교 통과. 읽기 불가능한 경로 접두부와 삭제된 현재 디렉터리의 원본 동작도 검토 후 보존했다. 실제 출력 SHA는 `1d88caa42ab506cdb3c5d1d7137837d5f70b4b35cd5f5a5115e2de2dc1f7e528`.
- `17823057`: catalog 테스트 4개 + workflow isolation 32개와 strict Clippy 통과. x86_64/aarch64 원본 메타데이터 캡처를 보존했다. `Catalog::load` 검증 후 ID를 출력하며, 기존 `--check-catalog` 출력은 유지한다. 검증 뒤 파일이 변조되면 재검증에서 거부한다.
- `2fbf969f`: `image_array_math::compare`의 독립 NumPy 2.5.3 캡처 97개(성공 91, 오류 6), strict Clippy, 독립 검토 통과. broadcasting, 마지막 축의 any/sum, i64 wrapping subtraction/abs 및 `abs(MIN)==MIN`을 보존한다. 이 함수만으로 이미지 디코더나 Python 검사기가 대체된 것은 아니다.

상세 결과는 `docs/naver/rust-repository-python-retirement-296-evidence.md`, 작업 범위는 `...-plan.md`, 전체 목록은 `...-inventory.tsv`, 내장·동적 실행 경로는 `...-execution-edges.tsv`에 있다.

## 4 부모와 로컬 워커 운영 규칙

이번 사용자의 최신 선호는 **부모 Sol 또는 Astra + 로컬 모델 워커**다. 이전 지침의 모든 워커 GPT-6.1 Sol 요구는 이 재개 방식에 대해 최신 사용자 요청으로 대체한다. 실제 로컬 모델 이름·API 주소·에이전트 프로그램 설정은 아직 지정하거나 변경하지 않았다.

- 부모: 범위·브랜치·통합·검토·Cargo 소유권·Git/PR/CI·문서 관리.
- 워커 A: 모델 LLVM provider 및 IMAGE 배치/스케줄.
- 워커 B: polygon 원본 참조 도구의 정책·raw raylib ABI·오류/경로 검증.
- 워커 C: PIL→NumPy 이미지 배열 디코더. 먼저 작은 작업 단위로 로컬 워커의 검증 품질을 확인한다.
- 워커들은 지정 파일만 수정한다. 공유 Cargo.toml/lock/lib.rs, workflow, 파일 삭제, 스테이징·커밋·push는 부모가 관리한다.
- **Cargo 빌드는 한 번에 한 워커만** 실행한다. 공용 target cache를 쓰므로 실행 전 소유권을 요청하고 완료 후 넘긴다. 부모는 대형 작업을 중복 실행하지 않는다.
- 리뷰 결과와 원본 캡처가 기대값이다. 교체 구현으로 기대값을 다시 만들어 통과시키거나 기존 양성 테스트·허용 오차를 줄이지 않는다.
- fixed-input golden 검사는 독립 원본·실제 dependency drift guard·고정 입력 한계를 명시한다. generic helper를 canned response로 바꾸지 않는다.
- Python을 실행하는 Rust 래퍼, 옛 소스 다운로드 실행, 알려진 지원 형식 삭제는 전환 완료가 아니다.

### 새 부모에게 붙여 넣을 시작 지시문

```text
/home/fma1/.codex/worktrees/native-build-tools-296/openpilot-rust 에서 작업한다.
docs/naver/rust-repository-python-retirement-296-handoff.md를 먼저 읽고 AGENTS.md,
개발 workflow, plan/evidence/inventory/execution-edges를 확인하라.
사용자는 저장소 전체 개발·빌드·검증·모델·vendor 기능의 Rust 전환과 검증 후
대체 Python 제거를 요청했다. dev-md 폴더와 작업은 분리한다.
기존 미커밋/미추적 소스와 ignored scratch를 보존하고 재개한다.
부모는 Sol 또는 Astra, 워커는 사용자가 설정한 로컬 모델을 사용한다.
한 번에 하나의 Cargo 빌드만 실행하며 부모만 공유 파일/Git/CI를 통합한다.
먼저 현재 git 상태와 a077b0d2 CI 최종 결과를 확인하고, 알려진 실패/미검증
변경부터 작은 단위로 재현·수정하라. 모델 IMAGE init은 현재 잘못된 부분이
있으므로 일괄 커밋하지 않는다. 부분 검증을 전체 완료로 보고하지 않는다.
```

### 로컬 워커에게 붙여 넣을 공통 지시문

```text
지정 worktree와 지정 파일에서만 일한다. dev-md 및 다른 워커 파일은 수정하지 않는다.
착수 전에 담당 파일, 원본 근거, 현재 실패, 완료 판정을 짧게 보고한다.
Cargo를 실행하려면 부모에게 소유권을 받아라. 모든 빌드 전에 디스크 여유를 확인한다.
원본 캡처와 기존 테스트를 보존하고 검증을 약화하지 않는다.
Git stage/commit/push, 원본 Python 삭제, workflow/공유 manifest 수정은 부모에게 맡긴다.
보고에는 실제 명령·종료 결과·로그 경로·아직 검증하지 않은 변경을 구분한다.
중단 요청을 받으면 새 작업을 시작하지 말고 파일과 현재 상태를 보존한다.
```

## 5 미커밋 워커 A 모델 컴파일러

담당 파일은 `rust/crates/model-runtime/` 아래:

- `src/compiler_backend_cpu.rs`, `src/compiler_backend_cpu/{contraction,llvm,image}.rs`
- `src/compiler_onnx/{lower,render}.rs`, `src/compiler_onnx/render/{math,reduction}.rs`
- `src/bin/offline-llvm-compile.rs`, 부모가 추가한 Cargo bin 등록
- `tests/compiler_{contraction,norm,llvm}.rs`, 기존 `compiler_storage_alias.rs`의 사례명 진단 개선
- `tests/fixtures/compiler/README.md`, `original-{mulacc-f32,norm,driving-contraction21,image-muladd-barrier,image-muladd-wide-pending,image-packed,llvm-provider}.json`
- `native-rendered-a630-proof.json` 수정도 미커밋이며 최종 source-matched 재검증이 필요하다.

### 이미 얻은 제한된 결과

직접 입력 scalar norm, raw Sqrt, 일부 Mul/Add contraction을 구현하고 독립 원본 비교를 확장했다. 일반 contraction 290개, IMAGE 160개, norm 435개 기록이 있다. 기존 storage alias 양성 36개를 삭제하지 않았다. 이전에 허용되던 driving prefix 109/115의 불일치를 수정한 중간 결과와 실제 prefix21 비교도 보존되어 있다. 이 결과는 아래 후속 발견 때문에 최종 전체 통과로 간주할 수 없다. 일반 normalization과 전체 compiler parity는 미완료다.

### 현재 핵심 실패와 올바른 재개 방향

`render/reduction.rs`의 aligned Conv first-product 초기화 규칙은 일부 크기에서 signed zero가 다르다. 이 미커밋 규칙을 그대로 확정하지 않는다.

원본 LLVM IR은 +0 초기 accumulator와 `nsz arcp contract afn`을 사용한다. 실제 원인은 IMAGE 메모리 배치·stride에 따른 reduction 축 병합 및 LLVM unroll/SLP다.

- `cin_last`(iy=ix=1) weight layout `[cout/4,cin/4,outlane4,inlane4]`에서는 stride가 달라 reduction 축이 합쳐지지 않는다. inner4만 unroll하고 cin/4 loop를 남긴다.
- 다른 layout `[cout/4,cin/4,inlane4,outlane4]`에서는 축 병합 후 원본 unroll 규칙이 적용된다.
- CPU/features와 LLVM 버전이 바뀌면 zero/NaN 결과도 달라질 수 있다. 크기별 예외 목록으로 맞추지 말고 원본 배치·축 병합·루프 생성 알고리즘을 포팅한다.

### LLVM provider

`compiler_backend_cpu::llvm`와 `offline-llvm-compile`은 기존 libloading으로 별도 프로세스에서 컴파일한다. library SHA, triple, CPU, features, passes와 unroll/vector/SLP/verify 옵션을 명시한다. private output, 실패 시 소유 파일 정리, receipt/hash 검증, timeout/crash containment를 구현했다.

- LLVM20.1과 LLVM18 각각 provider 테스트 5개 통과. 각 버전의 원본 object/optimized IR 3개와 정확히 일치했다.
- strict provider/library/bin/test Clippy와 독립 read-only 검토 통과.
- 테스트 터미널 출력은 파일로 저장되지 않았다. 재개 시 다시 실행해 로그를 남긴다.
- provider는 아직 IMAGE 스케줄 생성이나 모델 admission을 완성하지 않는다.
- **가장 마지막 변경**: `image.rs`의 fail-closed `Image1x1::render` stub과 모듈 등록, `original-image-packed.json`을 추가했다. 이 추가 뒤 빌드·테스트는 하지 않았다. packed-layout TDD는 시작하지 않았다.
- packed fixture는 두 LLVM 버전의 materialized-input 72개 기록과 scheduler 관측 12개다. whole-export 검증으로 해석하지 않는다.

LLVM18 재현 정보:

```text
Ubuntu package libllvm18:amd64 = 1:18.1.3-1ubuntu1
pool/main/l/llvm-toolchain-18/libllvm18_18.1.3-1ubuntu1_amd64.deb
package bytes: 27518500
package SHA256: af6197096ce3b89aab86de59f01c1b5feb008b2ae1514a03906c0664edb8b0b1
library: /usr/lib/llvm-18/lib/libLLVM.so.1
library SHA256: 5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0
```

필수 CI 연결은 아직 안 됐다. 현재 workflow의 LLVM_PATH는 LLVM18이다. exact pinned package를 provision하고 테스트를 필수로 실행해야 한다. 라이브러리가 없을 때 조용히 skip하지 않는다. znver4 object 비교는 다른 CPU에서 실행하지 않고 object-only로 검증한다. 실제 추론은 해당 host/target에 맞는 명시적 설정을 사용한다.

자료: `.analysis/scratch/2026-10-15-native-compiler-launch/prefix-contexts/`

- `image-source-schedules.json`
- `image-optimized-context.json`, `image-optimized-context18.json`
- `original-image-packed-generic18.json`, `original-image-packed-generic20.json`
- `ImageMulAdd-wide-8-1-False-negativezero.ll` 및 C12/W1, C12/W3의 `.ll/.opt.ll/.asm/.o/.jit.bin`

다음 순서: 마지막 stub 상태 확인 → native packed layout/stride merging/LLVM emitter TDD → 동일 버전·동일 target의 원본 비교 → 기존 양성/actual21/109/115 유지 → 전체 model suite/Clippy와 새 A630 proof → 독립 검토 → CI 연결. 그 전 모델 Python을 제거하지 않는다.

## 6 미커밋 워커 B polygon 참조 도구

주요 파일:

- `rust/crates/dev-tools/src/ui_polygon_reference.rs`, `src/bin/ui_polygon_reference.rs`
- `rust/crates/dev-tools/tests/ui_polygon_reference.rs`, `tests/fixtures/ui-polygon/`
- `rust/crates/startup-ui/native/polygon_reference.{h,cc}`, `src/polygon_reference_abi.rs`
- startup-ui `build.rs`, 기존 audited `bridge.rs`와 `lib.rs`의 제한된 추가
- 부모가 dev-tools Cargo/lib에 `polygon-reference` feature와 gated bin을 등록했다.

후보 renderer/정책을 재사용하지 않는 독립 Rust 참조이며 C++는 raw raylib ABI다. 추가 blanket unsafe lint 허용은 없다.

검증 이력과 현재 상태를 구별한다.

- 초기 50개 실제 원본/네이티브 프로세스의 PNG·JSON state·exit/artifact 결과 일치. PATH=/nonexistent 확인. 당시 feature build/Clippy 통과.
- **그 50개 proof는 현재 최종 소스 proof가 아니다.** 이후 검토 수정과 사례가 추가됐다.
- 마지막 feature 테스트에서 live helper/shader/lock pin, raw ABI 음성, PYTHONWARNINGS 12개 검사는 통과했다.
- 76개 scene 검사 중 `overflow-then-bad-point`만 실패했다. NumPy는 뒤의 정수 변환이 실패해도 앞의 overflow warning을 출력한다.
- 마지막 소스 변경은 shape를 먼저 검증하고, 순서대로 숫자를 변환하며 오류가 나도 overflow flag를 전달하도록 고쳤다. **rustfmt만 했고 아직 빌드/테스트/Clippy하지 않았다.**

남은 문제:

1. 위 마지막 수정 재검증.
2. 실패 시 Gradient stdout warning의 순서: 원본 Python은 버퍼링하여 traceback 뒤에 출력한다. 네이티브 CLI는 현재 render 중 먼저 출력한다. 실패 시 진단 뒤에 buffer를 출력하도록 수정하되 가짜 Python traceback을 만들지 않는다.
3. 원본 JSON recursion limit과 NumPy 64차원 경계의 독립 캡처/guard 추가.
4. 최신 binary로 실제 76개, raw/alias/trailing-slash 등 경로 14개, warning 12개 비교를 다시 수행.

numeric whitespace/Unicode decimal/underscore, 큰 정수와 string Infinity 구별, 중첩 좌표와 unknown mode, stop None 거부, lexical Path 정규화, helper live pin 등은 마지막 테스트 빌드까지 반영되어 있었다. 오류 메시지는 native 진단이며 Python traceback 전체 바이트 동일성을 주장하지 않는다.

자료: `.analysis/scratch/2026-10-16-ui-polygon/`

- `capture.py`, `reference-original.json` — 현재 76개, 성공 46개
- `capture_traces.py`, `trace_source.py`; 로그의 `PASS30` 문구는 오래됐고 실제 fixture는 76개
- `review-green.log` — 이름과 달리 마지막 scene 하나 실패한 실행
- `actual_reference.py`, `actual-reference-proof.json` — 이전 50개 snapshot만 검증
- `path_capture.py`, `path-original.json` 14개; 최종 native path proof 미완료
- `warning-original.json`, `warning-red.json`, `raw-source-original.json`
- `checker-original.json`, `checker-blobs/`, `checker_capture.py` — 원본 전체 검사기 32개
- private Xvfb와 필요한 라이브러리는 `host/`, `debs/` 및 실행 스크립트를 확인한다. 소유 Xvfb PID1806479는 중단 시 종료했다. 재개하면 새 소유 display를 띄운다.

`rust/tools/check_ui_polygon.py`, `rust/tools/ui_qa/polygon_source.py`, runtime `shader_polygon.py`는 아직 제거하지 않았다. native checker 자체는 아직 없다. 원본 framework는 13개 gate(7 native +6 Python)이며 orchestration만 Rust로 감싸는 작업은 하지 않았다.

## 7 워커 C 이미지 디코더 프로토타입

**저장소 production decoder 파일이나 dependency edge는 아직 없다.** 구현은 ignored scratch에 있다.

```text
.analysis/scratch/2026-10-16-image-array/
  prototype/Cargo.toml, Cargo.lock, src/main.rs, src/legacy.rs
  capture.py, original.json, inputs/
  compare.py, prototype-comparison.json
  capture_headers.py, header-original.json
  jpeg_decode.c, jpeg-decode, jpeg-comparison.json
  capture_jpeg_errors.py, jpeg-errors-original.json
  libjpeg-release.json, libjpeg-turbo-3.1.4.1.tar.gz, jpeg-headers/
```

원본은 Pillow12.3.0 / NumPy2.5.3 / Python3.12.15다. 182 mode×format 시도 중 decode 가능한 93개 배열의 mode·shape·i64값을 캡처했다.

- 마지막 빌드된 Rust prototype: **88/93** 일치. 4 JPEG와 1 RGB-ICNS가 남았다.
- 독립 native libjpeg probe는 유효 JPEG 6개 모두 일치(CMYK 포함). 아직 Rust 통합 전이므로 이를 합쳐 93/93이라고 하지 않는다. 통합하면 예상 92/93이지만 미검증이다.
- 마지막 **미빌드 수정**: ICNS의 initial RGBA→loaded RGB `asarray` byte-layout 특이 동작, JPEG EOF warning을 원본처럼 실패 처리하는 수정.
- JPEG malformed/truncated 12개 원본 캡처가 있다. 이전 native probe가 원본 오류 5개를 수용했으므로 EOF 수정은 반드시 재검증한다.
- source checker는 확장자가 .png여도 TIFF/TGA/DDS/SGI/IM 등을 수용한다. palette GIF와 16-bit PNG broadcasting도 성공한다. RGBA 변환/PNG 전용 decoder로 바꾸면 기능이 달라진다.

API 방향: `ImageArray { mode, shape: Vec<usize>, values: Vec<i64> }`; 수학 비교는 이미 커밋된 `image_array_math::compare`에 위임한다.

scratch dependency: png0.17.16; gif0.14.2(std, default off); tiff0.11.3(deflate/jpeg/lzw, default off); image0.25.10(bmp/dds/tga/jpeg/pnm/webp/ico, default off); serde_json1.0.145. isolated lock 43개이며 AVIF/rayon/EXR은 없다. image/ico가 png0.18.1을 추가로 가져온다. 필요한 dependency가 검증되면 부모가 production manifest를 변경한다.

Pillow pixel threshold의 header-only 7개 캡처: 89,478,485까지 warning 없음, 89,478,486~178,956,970 warning, 그 초과 error. 0 dimension 거부. 임의로 더 작은 codec 기본 제한을 유지하지 말고 원본 경고/거부와 fallible allocation을 맞춘다. 큰 이미지를 실제 할당하지 않았다.

JPEG는 zune/image decoder 픽셀과 달라 원본과 맞는 native libjpeg가 필요하다.

- 기존 read-only native library: `/home/fma1/rust/openpilot-rust/.venv/lib/python3.12/site-packages/pillow.libs/libjpeg-31e2ca52.so.62.4.0`
- ABI62 / libjpeg-turbo3.1.4.1. library SHA는 아직 별도 기록하지 않았다.
- 공식 source archive SHA256: `ecae8008e2cc9ade2f2c1bb9d5e6d4fb73e7c433866a056bd82980741571a022`, GitHub asset digest와 대조했다.
- 같은 source headers와 생성한 ABI62/8-bit/MEM_SRCDST jconfig를 사용한 C probe가 `-Wall -Wextra -Werror`로 빌드되었다.
- setjmp는 C 경계 안에 두고 cleanup state는 heap 소유. Rust/C++ RAII 경계를 longjmp로 넘지 않는다. mode 정책은 Rust에 둔다.
- native source/dependency 작업은 사용자 전체 전환 범위 안에서 승인되었다. primary venv를 수정하지 말고 별도 cache에서 pin·license·빌드 재현성을 보존한다.

다음 순서: 미빌드 patch 재빌드 → 93개 배열 + JPEG 오류12개 재비교 → JPEG 통합 → malformed/planar TIFF/palette/SGI RLE/IM header 등 음성 검증 → source pin/license/threshold/경로·오류 정책 → production module/검사기 통합 → 독립 검토. 전체 PIL plugin 동등성과 polygon checker 제거는 아직 미완료다.

## 8 실행 환경과 빌드 명령

매번 실제 worktree에서 실행한다. env.sh가 primary root와 다른 Cargo cache를 설정하므로 아래 override가 필수다.

```bash
cd /home/fma1/.codex/worktrees/native-build-tools-296/openpilot-rust
source /home/fma1/.config/openpilot-rust/env.sh
export OPENPILOT_RUST_ROOT="$PWD"
export PYTHONDONTWRITEBYTECODE=1
export PYTHONPATH="$PWD:$PWD/opendbc_repo:$PWD/tinygrad_repo:$PWD/rust/tools"
export CARGO_TARGET_DIR=/home/fma1/rust/cache/cargo-target-retirement-296
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2
```

빌드·dependency 설치·큰 복사 전에 25GiB + 예상 증가분을 확보한다. 부족하면 새 빌드를 멈추고 35GiB 이상 복구 후 재개한다. 중단 시 약270GiB 여유였다.

```bash
/home/fma1/.codex/worktrees/runtime-retirement-296/openpilot-rust/.analysis/scratch/2026-10-14-agnos296/host-tools-430ec72b/check-disk-space --path /home/fma1/rust/cache --growth-mib 512
```

필요한 작업만, Cargo 소유권을 받은 뒤 실행한다.

```bash
LLVM_PATH=/usr/lib/llvm-18/lib/libLLVM.so.1 cargo test --manifest-path rust/Cargo.toml -p openpilot-model-runtime --test compiler_llvm --locked
cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --features polygon-reference --test ui_polygon_reference --locked
cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --test image_array_math --locked
```

원본 캡처용 Python은 `/home/fma1/rust/openpilot-rust/.venv/bin/python -B`를 read-only로 사용한다. primary venv에 설치·수정하지 않는다. 최종 native 실행 검증은 Python 없는 PATH로 한다. 장치 테스트/실주행 결과와 Linux host 결과를 혼동하지 않는다.

## 9 파일 보존과 재개 체크리스트

복구용 snapshot은 active worktree의 아래 ignored 폴더에 있다.

```text
.analysis/handoff/2026-10-16-repository-python-retirement/
  git-status.txt, git-log.txt, untracked-paths.nul
  tracked-wip.patch
  untracked-source.tar.gz
  current-ui-and-codec-scratch.tar.gz
  SHA256SUMS
```

현재 파일이 원본이며 snapshot은 실수로 잃었을 때의 복구용이다. 현재 WIP 위에 자동으로 patch/archive를 덮어쓰지 않는다. 모델의 큰 scratch 전체는 archive하지 않았으므로 `.analysis/scratch/2026-10-15-native-compiler-launch/`도 그대로 유지한다. 다른 컴퓨터로 옮길 때는 local-only 커밋·untracked 소스·필수 scratch까지 함께 이전해야 한다. GitHub clone만으로는 현재 상태를 복구할 수 없다.

재개 순서:

1. 경로·브랜치·Git 상태와 이 문서의 snapshot을 확인한다.
2. 원격 `a077b0d2` CI의 최종 상태를 확인하고 exact SHA로 기록한다.
3. 전체 uncommitted 상태가 GREEN이라고 가정하지 않는다. polygon 마지막 warning 수정과 모델 image stub을 먼저 확인한다.
4. 워커별 파일 소유권과 단일 Cargo 소유권을 설정한다.
5. 각 작은 단위의 원본 비교·음성·최신 binary proof·review가 끝나면 부모가 통합한다.
6. Python 제거는 실제 호출부 이관과 검증 후 수행한다. 인벤토리와 실행 경로를 함께 갱신한다.
7. push 전에 선택한 변경만 stage하고 dev-md 혼입을 확인한다. PR #343과 #296에 exact SHA, CI URL, 실패·남은 작업을 기록한다.
8. 저장소 전체 기능과 Python 실행 경로 제거가 끝나기 전에는 이슈를 닫거나 전체 완료라고 보고하지 않는다.

이 문서는 인수인계를 위한 개발 기록이다. 사용자 설정·사용자 가이드 변경은 없다.
