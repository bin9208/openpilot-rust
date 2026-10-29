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


## 2026-10-29 — 세 등록 runner와 hosted 병렬 실행 정책

사용자의 최신 지시가 앞선 `bin-actions` 전용 정책을 변경했다. 공통
`tools/ci/runner_policy.py`는 `bin-actions`, `bin-actions2`, `bin-actions3`를
등록된 self-hosted 실행자로 허용한다. `bin-actions3`는 사용자 제공 실제 API 확인상
online/free이며 기본 `self-hosted`, `Linux`, `X64` label만 있다. 따라서 job 증거에는
이 공통 세 label을 요구하고 `dev-md-local` 또는 `support-630`을 추가 필수로 요구하지
않는다. 이름이 다른 self-hosted 실행자는 계속 거부한다.

일반 helper는 Linux GitHub-hosted X64/ARM64도 허용하며, 실제 kernel/genuine 경로의
`self_hosted_only`는 위 세 등록 runner로 제한한다. heavy 검사의 자원·분담과 실제
workflow 라우팅은 부모 작업에서 관리한다. 새 runner의 CPU/RAM 우수성이나 기능 검사
성공을 이 허용 목록 변경만으로 주장하지 않는다. 기존 실행·다른 세션·공유 cache는
보존하며 기존 argv/pin·검사 임계값·cleanup 범위는 바꾸지 않는다.

`bin-actions3` 환경 허용, 기본 label만 있는 job 증거 허용, job 전용 cache 소유권의
양성 회귀 검사를 보강했다. 로컬은 Python AST와 diff만 확인했고 회귀 검사·빌드·CI
실행은 수행하지 않았다. #606의 공통 CI 포트는 별도 작업으로 남긴다.


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


## 2026-10-29 — 공통 label과 CPU 병렬 제한 해제

사용자 지시로 저장소 runner ID22 `bin-actions`, ID23 `bin-actions2`, ID24
`bin-actions3` 모두에 `openpilot-rust` custom label을 추가하고 실제 API 응답을 확인했다.
기존 `dev-md-local`·`support-630` label은 유지했다. active heavy X64 검사는
`[self-hosted, Linux, X64, openpilot-rust]`, ARM은 `ubuntu-24.04-arm`, 빠른 검사·집계는
`ubuntu-24.04`를 선택한다. 세 self-hosted와 hosted 네 작업 이상을 함께 실행할 수
있도록 독립 job과 기존 최종 gate를 유지한다. runner 한 개는 job 하나를 처리한다.
실제 계정 quota는 API에서 확인되지 않아 공개된 표준 한도와 실제 실행을 구분한다.

추가 지시에 따라 workflow의 `CARGO_BUILD_JOBS=2`, genuine guest와 production native
Cargo driver의 기본 `CARGO_BUILD_JOBS=1`, 호출 helper의 `-j2`를 제거한다. Cargo는
사용 가능한 CPU 수를 기본 병렬도로 사용한다. recipe에 명시한 환경은 계속 적용한다.
이미 실행 중인 프로세스의 환경은 변경되지 않으며 이 수정 source의 새 작업부터
적용된다. 로컬은 AST/YAML/Bash 구문·rustfmt·전체 기준 diff만 확인하며 기능·빌드
성공과 실제 동시 실행 수는 정확 source의 Actions 결과로 별도 확인한다.


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

공통 Rust action은 실제 CPU affinity의 CPU 수를 `CARGO_BUILD_JOBS`와
`RUST_TEST_THREADS`의 기본값으로 내보내며 선택 값을 로그로 남긴다. 이 방식은 runner
사용자 Cargo config의 `jobs=2`도 덮어쓰고 명시 env override는 존중한다. 순서 의존
근거가 없던 GenSDK·RadarCAN ARM·planner/radar Miri·Xiaoge native의 blanket
`--test-threads=1`도 제거한다. 각 검사 mode·필터·입력·판정 기준은 유지한다.

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
