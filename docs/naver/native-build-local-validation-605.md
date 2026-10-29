# 자체 빌드 트랙의 지정 runner·ARM 실행 연결 — #605

기준일: 2026-10-26 KST. 상위 #598/#599, 실행
[#605](https://github.com/bin9208/openpilot-rust/issues/605).

## 적용 근거와 범위

이 채팅에 새로 제공된 AGENTS.md는 이후 모든 Actions를 등록된 `bin-actions`로 실행하고
ARM 검사를 별도 ARM user space에서 유지하도록 명시했다. 앞서 시작한 native PR #603의
실행은 보존하고, 이 지침에 맞는 후속 검증 경로를 준비한 뒤 새 PR/병합 검사를 수행한다.

기준은 dev `e445c9d19c3041bc79d8624589e2eb6ba7d061c6`이다. 별도 branch
`codex/feat-605-local-native-validation`에서 공통 실행 경로만 수정한다. 기존 #599의
자산/tree 구현과 dev-md runtime을 이 branch에 섞지 않는다. #502의 공통 job-temp,
Rust 경로 및 고정 QEMU/PRoot ARM 실행 helper를 재사용하며 실시간 기능·모델·안전·주기·
유효성 정책과 원래 비교/빌드 명령을 변경하지 않는다.

## 보존과 변경

- 모든 workflow의 실제 `runs-on`은 등록된 runner의 labels로 고정한다. job-temp와 ARM
  transport는 실제 runner 이름·환경도 확인한다. fork PR에서 다른 저장소의 소스를
  self-hosted에서 실행하지 않도록 원본 요구 검사에 같은 저장소 조건을 적용한다.
- ARM matrix와 원래 ARM 명령·feature·입력 pin·depends-on 관계를 유지한다. 호스트
  setup-python Action을 ARM에서 사용하지 않고 고정 ARM Python을 guest에 준비한다.
  host/guest/source/hash와 `physical_arm_cpu=false`를 artifact로 남긴다.
- guest target/toolchain과 job별 도구 출력은 소유 임시 디렉터리에 둔다. host-side
  artifact 업로드는 실제 guest target 경로를 참조하고 원래 결과물도 유지한다.
  X64는 원래 `rust/target` argv를 유지하면서 job 소유 target으로 연결한다. 이미 존재하는
  target은 덮어쓰지 않고 소유 link만 정리한다. 준비된 고정 host toolchain cache는
  Rustup의 버전 관리 경로를 재사용하며 ARM toolchain은 guest에 분리한다.
- GitHub 기본 이미지용 Android/GHC/.NET 삭제 명령은 제거한다. 작업별 실제 예상
  증가량 검사는 유지하고 폐기된25/35GiB 공통 reserve를 성장량 admission으로 바꾼다.
  device installer의 별도 용량·무결성 정책은 변경하지 않는다.
- 원래 job/의존성/matrix/명령 hash를 고정 source에서 기록했다. 정책 회귀는 실제
  `git show`로 baseline을 다시 읽고, 검토한 hosted 삭제 제거·고정 reserve 제거 외의
  원래 명령 변경/누락·실행 skip·continue-on-error 추가를 거부한다.

## 검증과 제한

### PR #606의 준비 대기·APT·기존 CI 보존 보완

기준 `bfffa2d30d1e8037cc3f7eb80b5a2b5264c8070c`의 필수 Rust 실행
[40315420260](https://github.com/bin9208/openpilot-rust/actions/runs/40315420260)에는
CarrotMan ARM 준비 대기와 webcam X64 APT 실패가 확인되었다. 기존 실행은 보존한다.
후속 source에 #609의 실제 peer 준비 대기와 #610의 job 범위 native APT를 함께 반영한다.
원래 기능 deadline·입력 pin·ARM 명령과 명시적인 cross APT 인자는 그대로 유지한다.
#609의 별도 ARM 성공은 source `18e174c2788af1eb3d8de3e20826a3d84160cb43`의 근거이며,
합친 새 source의 전체 PR 검사 성공으로 대신 쓰지 않는다. #610의 집중 실행도 별도다.

후속 커밋이 기존 실행을 중단시키지 않도록 validation workflow9개의 branch별 group을
`-preserve-v1` 접미사의 새 이름 공간으로 옮기고 `queue: max`, `cancel-in-progress: false`를
적용한다. 기존의 중도 취소 group과 충돌하지 않아 전환 시점의 기존 대기도 보존한다. 재사용되는
integration 하위 tests/naver/APK와 문서 설정 검사도 포함한다. publish/sync는 변경하거나
활성화하지 않는다. concurrency가 없는 다른 workflow는 기본 실행을 유지한다.

[GitHub의 공식 concurrency 규칙](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/control-workflow-concurrency)에
따르면 취소 false만으로는 기본 pending 한 개의 교체를 막지 못한다. `queue: max`는 같은
group에 최대100개 pending run을 보존하며, 그 한도 뒤의 새 run은 취소된다. job 수가 아니라
workflow run 대기 한도다. 검사 실행 순서/runner는 이 큐와 기존 job needs에 따라 유지된다.
기존 실행을 수동 취소하거나 검사 결과를 다른 SHA의 성공으로 바꾸지 않는다.

정책 회귀는 이9개 queue/취소 설정과 기존66개 job·needs·matrix·명령 보존을 함께 확인한다.
로컬은 YAML/Python/Bash 구문·diff/읽기만 확인한다. PR 갱신 후 기존 run의 상태를 다시 읽어
보존 여부를 확인하고, 정확 새 SHA의 필수 검사와 별도 병합 후 검사를 기다린다.

로컬은 YAML/Python/Bash/Rust 구문·서식·diff/읽기 감사만 수행한다. 집중 workflow는
X64와 별도 ARM guest에서 기존 실제 Cargo 재사용 회귀, native 공간 guard 및 bootstrap,
실행 경로/원래 명령 보존 정책을 검사한다. ARM은 emulated execution이며 AGNOS나 물리
기기 효과를 검증하지 않는다. 실행 결과를 확인하기 전 성공으로 기록하지 않는다.

필수 fast/integration/docs/Rust/ARM 검사를 유지한다. 정확 source의 PR 검사와 병합 후
검사는 별도 수용이며, 이 기반 변경이 #598의 AGNOS 자체 빌드·다운로드 제거·정상 부팅·
녹화·업로드·사용자 수용을 완료한 것은 아니다. 테스트 안내와 native build 후속은
`codex/feat-599-verified-input-trees`의 `docs/naver/native-build-test-guide-598.md`에 유지한다.

성능 트랙 #604는 source `dfe09f66238dbecb74c43aa9ee177c36c4ae40a3`의
[bin-actions40304295923](https://github.com/bin9208/openpilot-rust/actions/runs/40304295923)에서
library66·정확 경로16·Clippy를 통과했고 PR #494에 후속 `02006e0f2`를 연결했다.
이 별도 dev-md 상태를 common dev source의 기능 성공으로 대신 사용하지 않는다.

## 최초 실제 실행과 에뮬레이션 시간 제한 보완

source `b865767054f3415e7d5402060c7c8be6c88fc3e1`의
[40308571039](https://github.com/bin9208/openpilot-rust/actions/runs/40308571039)는
X64 전체 leaf가 성공했다. 실제 runner 이름은 양쪽 모두 `bin-actions`다.
ARM QEMU도 원래 offline Cargo 회귀4개와 strict Clippy, 명령·경로 보존 정책4개,
disk-space library4개와 CLI5개를 통과했다. 이후 dev-tools 공간 회귀의 새 의존성을
컴파일하던 중45분 제한으로 종료됐다. API의 job conclusion은 `cancelled`이며 annotation은
`The job has exceeded the maximum execution time of 45m0s`다. ARM 전체 성공으로
기록하지 않는다. 원래 Cargo 회귀는10:53:47~11:03:19 UTC에 성공한 별도 step이다.

집중 ARM 예산을90분으로 늘리고 기존 ARM 원본 검증 job은 에뮬레이션에 맞게180분,
더 큰 Xiaoge job은240분 상한을 적용한다. matrix의 X64 상한과 원래 명령·feature·pin·
의존 관계는 유지한다. 원래 ARM 비교를 생략하거나 hosted fallback을 도입하지 않는다.
정책 회귀도 ARM 예산과 원래 X64 상한 유지 여부를 검사한다. 소유 임시 출력·ARM proof
artifact와 정리는 취소 때도 성공했으며 다른 job/cache를 삭제하거나 중단하지 않았다.

dev 대상 정상 검토 PR은 [#606](https://github.com/bin9208/openpilot-rust/pull/606)이다.
최종 source의 빠른·집중·필수 PR/병합후 결과는 별도로 확인해야 한다.

## 생성된 Cargo target 링크와 엄격한 소스 검사

source `0b844f4ce422461cb8ac5fe936cd0ade9ebe4f0d`의
[40385740213](https://github.com/bin9208/openpilot-rust/actions/runs/40385740213)에서
X64 job `121793394426`은 assets5개와 실제 Cargo 재사용9개를 통과한 뒤 catalog3개 중
2개가 `runtime source differs from its committed source or exact LFS asset identities`로
실패했다. ARM job `121793394834`는 이 기록 시점에 계속 실행 중이며 중단하지 않는다.

공통 helper는 원래 `rust/target` 인자를 유지하면서 job 소유 출력으로 심볼릭 링크를
만든다. 기존 `rust/.gitignore`의 디렉터리 전용 `/target/` 규칙은 이 링크를 무시하지
않는다. 정확 생성 경로 규칙 `/target`으로 고치고 helper에서 실제 Git ignore 판정을
확인한다. 소스 동일성 판정이나 LFS 예외는 완화하지 않는다.

새 실제 Git 회귀는 이전 규칙에서 같은 미추적 링크를 재현하고, 수정 규칙에서는
생성 출력만 깨끗해지는지 확인한다. 다른 링크·추적 소스 변경은 계속 상태에 나타난다.
실제 helper의 준비·정리 스크립트도 소유 fixture에서 실행해 출력만 제거하고 소스 변경을
보존하는지 검사한다. 기능 검사는 Actions에서 실행하며 새 SHA의 결과가 나올 때까지
통과로 기록하지 않는다. 기존 실패 SHA와 실행 기록은 보존한다.

같은 실행의 ARM job `121793394834`는 이후 library6개 통과·객체 import1개 실패로
종료했다. 오류는 크기·권한·SHA를 구분하지 않았으므로 target ignore 문제와 같은
원인으로 처리하지 않는다. native #599의 `3982002f129a31136f4e5f55f3057a07e612f8df`가
기대/실제 속성 진단을 추가했다.

[고정 PRoot5.5.0의 fake-id 구현](https://github.com/proot-me/proot/blob/v5.5.0/src/extension/fake_id0/fake_id0.c)은
가짜 root의 경로 변환에서 읽기 전용 파일에 임시 쓰기 권한을 주고 원래 경로에서 복원한다.
readonly 파일의 rename/unlink 뒤 원래 경로가 사라지는 경우 객체 권한이 달라질 수 있다는
소스 감사 결과에 따라, foundation 빌드/검증은 같은 guest에서 일반 사용자로 실행한다.
패키지 설치 bootstrap만 기존 가짜 root를 유지한다. 다른 job의 기본 identity는 보존한다.
원래 ARM 명령·입력 pin·QEMU/PRoot·runner는 유지하고 guest UID/identity를 proof에 더한다.
실제 readonly444/555 rename, 열린 fd의 mode, hardlink의 한 이름 삭제 후 나머지 객체의
mode·본문 보존을 양쪽 실행 환경에서 필수 검사한다. 객체 검증 허용값은 바꾸지 않는다.
이 원인 후보와 transport 수정의 기능적 확인은 새 정확 source의 Actions 결과 대기다.


## 2026-10-29 — #606 공통 CI의 세 runner 분산

사용자 지시에 따라 #606의 기존 head `ec418df39d3b5820f2751978e3898f0a4d858cbc`에서
공통 CI만 별도 포트했다. heavy X64와 기존 ARM QEMU job은 공통
`[self-hosted, Linux, X64]` pool에서 `bin-actions`, `bin-actions2`, `bin-actions3`를
사용한다. fast/docs/두 필수 집계 gate와 Wiki 문서 검사는 hosted `ubuntu-24.04`로
분담한다. 원래 matrix, 의존 관계, 필수 check 이름, 원래 검사 명령과 조건·pin은
유지한다. 집계 step의 runner admission만 공통 허용 정책에 맞춘다.

읽기 검사의 concurrency는 `parallel-v2`로 분리하고 `queue:max`,
`cancel-in-progress:false`를 유지해 기존 실제 실행을 중단하지 않는다. inherited sync/publish 네 workflow
(`carrot-route-vault-publish.yaml`, `naver-upstream-sync.yml`, `sync.yml`,
`wiki-settings-publish.yaml`)는 runner/guard/concurrency까지 `ec418` 원문 전체를
유지하며 기존4label 예외를 정확 검사한다. 자동 발행을 활성화하지 않는다.
job-temp의 소유자/inode 범위와 기존 source/target view, ARM QEMU proof를 유지하고
실제 runner 이름·환경과 job owner·target 경로를 연결한다. fixture checkout에는 실제
job BASH_ENV가 읽는 native APT helper와 정책 파일도 포함한다.

이 포트에는 #599 공급자·native_cache·중앙 publisher·실제 입력 발행 코드 또는
새 native ARM 실행 분기를 가져오지 않는다. 부모 #611에 이미 있는 native ARM 확장은
부모 통합 때 별도로 유지한다. 로컬은 AST/YAML/Bash 구문과 diff 읽기만 수행했고
기능 검사·build·push·CI dispatch/cancel은 수행하지 않았다. 새 정확 SHA의 Actions
결과는 아직 없다.


## 2026-10-29 — #606 a5c 실제 실패 두 건 수정

source `a5c5267554c1c1c5317d029c1f5fcc0400b9cc4c`의 fast
[40881014451](https://github.com/bin9208/openpilot-rust/actions/runs/40881014451)와 foundation
[40881005066](https://github.com/bin9208/openpilot-rust/actions/runs/40881005066) 로그를 읽었다.
Cargo target fixture가 실제 composite step을 subprocess로 호출하면서
`GITHUB_ACTION_PATH`를 주지 않아 `/../../../tools/ci/runner_policy.py`를 찾지 못했다.
fixture env에 실제 `ROOT/.github/actions/job-temp`를 명시한다. production helper의
경로·소유권·정리 검사는 바꾸지 않는다. 부모 main fixture에는 이미 이 값이 있다.

fast 로그의 `carrot-peer-readiness.yml:106: new blank line at EOF`는 이번 HEAD 대비가
아닌 검사 기준 `e445c9d19c3041bc79d8624589e2eb6ba7d061c6`부터의 diff에서 보인다.
해당 EOF 빈 줄 하나만 제거하고 이 전체 기준 diff를 다시 읽어 확인한다. 로컬은
AST/YAML/diff만 확인하며 기능 재검사·build·push·CI 조작은 하지 않는다.


## 2026-10-29 — 사용자 지시로 고정 Cargo/테스트 병렬 제한 제거

공통 workflow와 carrot-man job template의 `CARGO_BUILD_JOBS=2`, 문서 검사의 inline
대입, disk-space bootstrap과 sanitizer/Miri helper의 `-j2`·환경 덮어쓰기를 제거한다.
별도 숫자 fallback을 만들지 않고 Cargo와 Rust test harness의 CPU 기본 병렬 수를
사용한다. 호출 환경의 명시적 `CARGO_BUILD_JOBS`/`RUST_TEST_THREADS` override는 그대로
상속한다. 이번 #606의 project-owned `.cargo/config`에는 별도 고정 jobs 설정이 없고,
`RUST_TEST_THREADS=1/2` 대입도 없었다. vendor 설정은 변경하지 않는다.

planner Miri, radar ARM/Miri, OpenCV/JPEG sanitizer의 `--test-threads=1`도 제거한다.
읽기 근거: planner scheduler의 Atomic 관찰자는 해당 단일 테스트만 쓰며,
radar numerical mock은 호출자가 넘긴 버퍼를 쓰고 scheduler mock은 지역 인자를 확인한다.
radar Params fixture는 PID/나노초로 고유 경로를 만든다. OpenCV native 테스트는
`OnceLock`의 동일 초기화 값2를 사용하고 0은 초기화 전에 거부된다. JPEG 테스트의
입출력 버퍼는 각 테스트 소유다. 이 범위에서 순서 의존을 이유로 남긴 직렬 flag는 없다.
실제 병렬 실행의 성공 또는 속도 개선은 아직 검증하지 않았다.

원래 command hash fixture는 보존하고, 정책 검사는 사용자 허가된 병렬 제한 제거만
명시적으로 정규화해 원문 비교를 계속한다. source/동작/검사 feature와 validity 정책,
자원별 성장 admission·memory 검증은 변경하지 않는다. 과거 실행 증거 문자열인
`write_mici_offroad_alerts_receipt.py`의 `invocation ... -j2`는 실행 명령이 아닌 이전
검증 기록이므로 재작성하지 않는다. 로컬은 AST/YAML/Bash 구문과 기준 diff만 확인하고
기능 실행·build·push·CI 조작은 하지 않는다.


host 사용자 Cargo config에 `jobs=2`가 있는 경우 env 대입 삭제만으로는 제한이 남는다.
따라서 공통 `rust-path` action이 실제 affinity CPU 수(`os.sched_getaffinity(0)`)를 우선,
없으면 `os.cpu_count()`를 사용해 두 환경 변수의 기본값을 `GITHUB_ENV`와 현재 step의
export로 설정한다. 명시적 환경 override가 있으면 그대로 보존한다. CPU 수를 알 수
없으면 실패하고 고정1/2 fallback은 넣지 않는다. available/affinity/logical CPU 수와
적용값·override 여부를 로그에 남긴다. host의 Cargo config 파일은 수정하지 않는다.

원래 rust-path가 skip되는 ARM QEMU job에는 `parallelism-only` 호출을 추가해 CPU 정책만
적용한다. 원래 toolchain bootstrap과 step 조건·QEMU pin은 유지한다. 모든 직접 Cargo
CI job이 공통 action을 거치는지도 정적 정책 검사에 포함했다. cap 제거 후 빈 env
mapping이 남지 않도록 해당 빈 header도 제거한다. frozen fixture의 원래 hash는 그대로
유지하고, 현재 명령이 정확한 cap 제거 결과와 일치하는 때만 원래 명령을 가상 복원해
hash/조건을 비교한다. 다른 argv 차이는 허용하지 않는다.


## 2026-10-29 — #606 Android SDK 준비 복원

사용자가 확인한 a5c Android job `123472213274`의 SDK location 실패에 따라
`jetlink.yml`의 `android-core`를 부모 main과 같은 `ubuntu-24.04`로 분담한다.
Python3.12, 원래 Java17과 기존 job 소유 `android-sdk` 경로의 admission, 고정
`setup-android` commit 및 cmdline-tools15859902/platform35/build-tools35.0.0 준비를
연결한다. 원래 Gradle core/runtime test·lint·APK build와 APK 검사 및 artifact는
그대로 보존한다. 정책 검사에는 이 Android job만 hosted 예외를 추가한다.

로컬은 AST/YAML/Bash 구문과 원래 기능 명령·조건의 읽기 대조만 수행했다. SDK 설치나
Android 기능 검사를 실행하지 않았고 원격 push/CI 조작도 하지 않았다. build release의
bin-actions3 sudo 권한 문제는 코드 우회나 보안 설정 변경으로 처리하지 않는다.


## 2026-10-29 — 2dd 실제 빈 env 및 ARM CPU 준비 실패 수정

source `2dd0b6cf722a728e6fc880d42b5d1d36e630dfa2`의 fast
[40884430656](https://github.com/bin9208/openpilot-rust/actions/runs/40884430656) 로그에서
빈 `rust.yml: jobs.model-memory.env`와 carrot-peer의 ARM CPU 준비 호출 누락을
확인했다. 빈 env는 그 다음 키가 defaults여서 이전 문자열 정리가 놓쳤다. 모든 공통
workflow/action/template YAML을 재귀 node/type 읽기로 감사해 실제 빈 key를 제거한다.
테스트에서 scalar env를 빈 dict로 바꿔 우회하지 않고 mapping schema 검사를 추가한다.

carrot-peer의 원래 조건부 도구 준비와 QEMU pin은 유지하고 ARM CPU 정책 전용 준비
호출만 보완한다. 공통 정책 검사는 최초 Cargo 앞에 실제 준비 호출이 있는지와 전체
준비 또는 CPU 전용 mode를 확인하되 특정 if 문자열 모양은 강제하지 않는다. 기존
source/matrix/gate와 기능 명령은 유지한다. 로컬은 AST/YAML/Bash 구문·재귀 schema·
기준 diff만 확인하며 기능·build·원격 CI는 실행하지 않는다.


## 실제 unsupported PPA 404: job 소유 공식 Ubuntu APT view

[#635](https://github.com/bin9208/openpilot-rust/issues/635)의 실제
[telemetry job123482820046](https://github.com/bin9208/openpilot-rust/actions/jobs/123482820046)에서
bin-actions2의 기존 appimagelauncher-team/stable Ubuntu noble PPA Release404로
apt update가 exit100인 실패를 사용자와 부모가 확인했다. 공유 /etc/apt 설정은
삭제·수정하지 않고 기본 CI APT만 job 소유 source view를 사용하도록 한다.

native-apt-shell은 root-owned 공식 Ubuntu archive/security/ports source만 읽어
기존 deb822 ubuntu.sources와 legacy sources.list 형식을 그대로 보존한 private
view를 만든다. HTTPS URI와 Signed-By/architecture/suite/component 필드는 원문을
유지하고 signature 우회 source는 거부한다. PPA source는 포함하지 않는다.
source hash와 실제 runner/job/source metadata를 job 임시 경로에 남긴다.

명시 architecture 인자는 원문 그대로 전달하면서 기본 source view를 적용한다.
APT_CONFIG 또는 명시 config/source 설정이 있으면 해당 호출 전체를 원래 방식으로
통과시킨다. 실제 host의 /usr/bin/dpkg architecture와 /usr/bin/uname 결과 일치를
확인한다. PATH의 fixture uname은 호출하지 않는다. header는 qemu-user 모드만
제외하므로 host X64→ARM bootstrap APT도 보호하고, 명시 qemu-user인 guest 내부는
원래 동작을 유지한다. job-temp는 기존 owner/cleanup 절차를 유지하며 이 wrapper를
BASH_ENV에 연결한다. 기존 API/환경/명령/패키지 pin과 공유 설정은 변경하지 않는다.

로컬에서 Python AST·YAML·Bash -n·diff만 확인했다. 현재 parser/재귀 회귀는 작성만
되어 있고 실행하지 않았으며, 로컬 apt·기능·빌드·원격 CI 조작은 하지 않았다.
실제 PPA404 해소와 기존 fake-uname 회귀 성공은 부모의 Actions 검증이 남아 있다.

source view뿐 아니라 APT lists와 binary/source package cache도 이 job의 경로를
사용한다. 임시 source 선택에 따른 APT list cleanup이 공유 PPA metadata를 지우지
않게 한다. 기존 명시 source/config/architecture 인자와 서명 검증은 유지한다.


## 2026-10-29 — hosted Ubuntu의 제공된 APT mirror 보존

`b1c604074` integration의 hosted wire-schema job123495106545는 공식 archive URI
직접 지정이 없는 이미지에서 source filter가 비어 실패했다. self-hosted의 실제
PPA404를 해결하는 private official source/list view를 `self-hosted` 환경에만
적용한다. hosted는 제공된 mirror 설정을 유지하고 기존 native architecture 범위만
선택한다. URI allowlist·서명 기준을 넓히지 않으며 명시 config/architecture와 원래
검사 command를 보존한다. AST/Bash/diff만 확인하고 기능 결과는 Actions에서 확인한다.


## 2026-10-30 — #606 네 hosted runtime 검사와 일반 host admission

bin-actions2/3이 같은 CPU16 PC임을 사용자에게 확인했다. 고정2/1 제한을 다시 넣지
않고 support·telemetry·web upload timeout·joystick의 독립 X64 네 job을 hosted
ubuntu24.04에 분담한다. local-arm의 일반 실행은 이미 hosted를 허용하는 공통
runner policy를 사용하게 한다. 실제 self-hosted 이름 세 개·Linux/X64/ARM64
허용 범위·job owner/source/target view·기존 QEMU·root 전용 입력 도구 정책은
유지한다. 원래 job 의존/argv/기준/최종 gate는 바꾸지 않으며 AST/YAML/Bash/diff만
로컬에서 확인한다. 기존 공유 source/cache·호스트 설정은 변경하지 않는다.

## 2026-10-30 — ARM 게스트의 helper용 system Python 준비

source `49d6728701b3cad94d50ce72e3cbbbc8e6511985`의
[foundation ARM](https://github.com/bin9208/openpilot-rust/actions/runs/40897448743/job/123530324068)은
local routing 검사 뒤 target-view 실제 helper 실행에서 `/usr/bin/python3` 부재로
실패했다. 고정 oracle venv는 있지만 job-temp가 사용하는 독립 system interpreter는
bootstrap 패키지 목록에 없었다. 게스트에 `python3`와 원래 binary 진단용 `file`을
준비하고 빈 loader 환경에서 isolated system Python의 ARM identity/ctypes를 확인한다.
oracle Python3.12.14·PRoot/QEMU/rootfs pin·원래 target-view 충돌/정리 단언은 유지한다.
main #611에는 이미 같은 준비가 있으므로 main source나 그 CI는 재시작하지 않는다.
로컬은 Bash 문법과 diff만 확인하고 실제 ARM 회귀는 수정 SHA의 Actions에서 확인한다.

## 2026-10-30 — #606에 검증된 native ARM backend와 분산 경로 선택 이식

원격 PR #606의 head가 `099474a5d5578809a9c1ecbc2e64a6376b0da8bd`임을 먼저
확인했다. main `c7bd7b6a514e122a6cd6ea8bba957f3ae8709297`의 native ARM 분기와
분산 경로만 선택 이식하며 dev `abea63ad` 병합과 #599 입력 발행·cache 기능은 가져오지
않는다. main 전체 파일이나 Rust 기능 코드는 복사하지 않는다.

`rust.yml`의 webcam·planner·CarrotMan·WebRTC·Xiaoge·RadarCAN·radard·Carrot Navi
ARM 8종, foundation과 peer 각 1종을 `ubuntu-24.04-arm`으로 배치한다. matrix X64는
기존 `[self-hosted, Linux, X64, openpilot-rust]` pool을 유지하고 CarrotMan CI template도
같은 경로로 맞춘다. common에는 foundation/peer 표시 이름을 소비하는 publisher나
receipt 경계가 없음을 확인하고 `ARM QEMU`/`emulated ARM` 표시만 실제 native로 고친다.
job ID·전체 원래 명령/feature·skip·needs·matrix 개수·timeout·고정 원본 SHA·최종 gate는
보존한다.

host와 target이 같은 때만 native 실행하고 job 소유 Cargo target view를 확인한다.
실제 source SHA·runner 이름/환경·host/target·UID·job owner·target 경로와 native/비기기
증거를 남긴다. native ARM에 Python3.12.14와 Rust 전체 준비를 원래 shell 준비 전에
추가한다. 명령 실행은 setup-python의 `pythonLocation`과 loader 경로를 사용한다.
기존 ARM CPU 전용 준비와 affinity/CPU 기반 기본값·명시 override 정책은 그대로 둔다.
X64→ARM 요청은 기존 QEMU/PRoot/rootfs/Python/Rustup pin·owner 증거·argv를 유지한다.
`099474a5`의 Cargo/FFmpeg 정확한 runner-temp prefix remap과 cleanup 경계도 보존한다.

target-view fixture는 고정 X64 대신 실제 실행 host architecture를 사용하게 하고,
정책 검사에는 허용한 10개 job 경로와 template의 native Python/Rust 준비만 추가한다.
로컬에서는 AST·YAML과 Bash `-n` 402개 command block 및 wrapper 구문, 기준 source
대비 diff만 확인했다. 원래 step 전체와 조건·matrix·timeout·workflow trigger의 동일성,
QEMU prepare/argv/pin/owner·정확 prefix/cleanup·CPU 정책·frozen fixture의 byte 동일성을
읽기 대조했다. 기능 검사·빌드·push·CI dispatch/취소·원격 글은 실행하지 않았으며 새
source의 Actions 성공은 미확인이다.
기존 queued/running 작업은 그대로 보존하고 대체 여부는 부모가 별도로 판단한다.

## 2026-10-30 — A294 fast의 두 routing 단언 실패 보정

`a2943e2c2fffda1c7acf8a0595864409dbeffbb1`의
[fast run40934083328/job123659886644](https://github.com/bin9208/openpilot-rust/actions/runs/40934083328/job/123659886644)은
50개 검사 중 48개가 성공하고 routing 단언 두 개가 실패했다. PR synthetic merge에는
dev의 `messaging-allocation-431.yml`이 포함되므로 common의 self-hosted 기본 단언과
충돌했다. main c7과 같은 한 파일 예외를 적용해 전체 YAML을 고정 `abea63ad` Git blob과
비교하고 comparison 1개 job·원래 matrix selector·두 공식 hosted label을 모두 확인한
후에만 일반 단언을 건너뛴다. 고정 blob은 이미 로컬 Git에 있으며 dev 병합은 하지 않는다.
workflow event/if/fork/steps/permissions는 변경하지 않는다.

RadarCAN 단언은 이미 변경된 `radar-arm`의 `ubuntu-24.04-arm` 경로에 맞춘다. fresh
source/runtime/ARM/memory 필수 명령과 목록은 그대로 보존한다. 두 검사 파일과 이 기록만
수정하고 로컬 AST·diff 읽기만 확인했다. 기능 검사·빌드·push·CI 조작은 하지 않았다.
