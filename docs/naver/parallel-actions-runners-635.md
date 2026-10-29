# GitHub 호스팅과 세 등록 runner의 병렬 검증 — #635/#598

2026-10-29 사용자는 이 세션에서 GitHub Actions 호스팅과 `bin-actions`를 병렬로
사용하고 `bin-actions2`도 사용하라고 명시했다. 이전 단일 `bin-actions` 전용 지침을
이 요청이 대체한다. 기능·빌드 검사는 계속 Actions에서만 수행한다.

API에서 실제 등록된 세 Linux/X64 runner를 확인했다. `bin-actions`의 추가 label은
`dev-md-local`, `bin-actions2`의 추가 label은 `support-630`이다. 새 `bin-actions3`는
기본 `self-hosted/Linux/X64` label만 있고 확인 당시 online/idle이었다. 나머지 둘은
online/busy였다. 이후 사용자는 성능이 좋은 자체 runner를 우선 사용하라고 명시했다. 다른 세션과 진행 중인 job은 중단하지 않는다. 새 `parallel-v2` 동시성 그룹은
기존 pool의 전체 workflow 종료를 기다리지 않도록 분리하고 대기 보존과
`cancel-in-progress=false`를 유지한다. 발행의 전역 동시성 그룹은 그대로 보존한다.

## 실행 분담

- 무거운 X64 workspace/원본·회귀·memory·build 검사는 공통
  `self-hosted, Linux, X64` pool의 세 승인 runner 중 빈 runner가 병렬 처리한다.
  빠른 검사·문서·가벼운 통합 보조와 최종 집계는 `ubuntu-24.04`에 분담한다.
- ARM matrix와 ARM 단독 기능 검사는 `ubuntu-24.04-arm`에서 원래 명령을 native로
  실행한다. 실제 host/target arch와 실행 방식을 기록하고 comma/AGNOS 실기 결과로
  확대하지 않는다. 원래 QEMU fallback은 X64에서 ARM을 요청할 때만 유지한다.
- integration의 `build_release`도 공통 자체 runner pool을 사용한다. `support-630`에
  고정해 #630 전용 검사를 독점하거나 중단하지 않는다.
- 전체 AGNOS compiler, source foundation, 실제 kernel resource, native APT/source
  reuse와 입력 발행은 공통 `self-hosted, Linux, X64` label로 세 등록 runner가 맡는다.
  실제 runner 이름은 아래 공통 정책에서 세 이름으로 제한한다.
- 관련 없는 상속 sync/publish workflow의 event·repository 조건과 routing은 유지한다.
  이 변경으로 원래 upstream 동기화나 배포를 활성화하지 않는다.

[GitHub 공식 표](https://docs.github.com/en/actions/reference/runners/github-hosted-runners#standard-github-hosted-runners-for-public-repositories)에서
공개 저장소의 고정 Ubuntu24.04 X64/ARM standard label을 확인했다. 유료 larger runner나
새 runner 등록은 추가하지 않는다. 실제 공간·메모리 admission은 유지한다.

## 원래 검사와 독립 증거 보존

main 트랙은 이미 독립적인 runtime job을 같은 단일 runner에 줄 세우고 있었다.
job/matrix/needs와 원래 검사 명령·조건·feature·input pin·허용 기준을 그대로 두고
실행 위치를 분산한다. dev-md는 별도 branch에서 단순 직렬 선행 의존성만 분리하고
최종 gate의 전체 필수 결과·실제 data/artifact 의존성은 유지한다. dev-md를 dev/main에
통합하지 않는다. source 변경 뒤 이전 source의 성공을 새 성공으로 대신하지 않는다.

`runner_policy.py`는 hosted Linux X64/ARM64와 실제 등록된 세 self-hosted 이름만
허용한다. 캐시와 job 전용 출력은 hosted를 지원하지만 kernel/genuine/payload 및
발행 실행은 세 self-hosted로 제한한다. job 전용 Cargo target·APT view·owner 확인과
소유한 출력만 정리하는 정책을 유지한다.

ARM hosted에서는 원래 guest가 제공하던 oracle Python3.12.14와 Rust shim을 명시적으로
준비한다. 실행 shell은 실제 interpreter/tool 경로를 사용하고 X64의 기존 argv와
QEMU pin·Python·rootfs pin은 변경하지 않는다.

PR 검증 context는 원래 source/head/base와 실제 merge bytes를 계속 묶는다. publisher는
불변 artifact의 runner 이름/환경을 독립 API의 성공 job 이름·runner 이름·labels와
대조한다. 단순히 artifact에 `github-hosted`를 적거나 허용되지 않은 self-hosted 이름을
붙여 통과시킬 수 없다. digest/expiry·필수 검사·전체34개 입력의 SHA/byte/프로필과
공개 body 검증은 유지한다.

## 검증 상태

현재는 구현·읽기 대조 단계다. 로컬은 Python AST·YAML·Bash 문법·서식·diff만
확인한다. helper/publisher의 hosted·bin-actions2 양성 및 runner/label/환경 위장 음성
회귀를 Actions에 연결한다. 새 정확 source의 실제 hosted X64/ARM 및 세 self-hosted
실행, 원래 필수 PR·병합 후 결과를 확인해야 수용한다. 단순 routing 수정이나 CI 등록을
#598의 자체 빌드·전체 후보 완료로 처리하지 않는다.


## 2026-10-29 — self/peer가 동일 kernel start time을 읽도록 수정

진단 source16e4bdd3a의 Actions40891941908/job123510809794에서 두 case 모두
`native request owner identity changed`로 child 초기화 전에 거부됐다. 단순 timeout이나
prctl 미지원은 아니었다. QEMU8.2.2의 [open_self_stat](https://github.com/qemu/qemu/blob/v8.2.2/linux-user/syscall.c#L7632)은
자기 PID/stat에 TaskState start_boottime을 쓰며, [init_task_state](https://github.com/qemu/qemu/blob/v8.2.2/linux-user/main.c#L191)는
에뮬레이터 초기화 때 CLOCK_BOOTTIME으로 값을 만든다. 다른 PID는 실제 kernel stat을
읽으므로 같은 요청자의 self/peer 관측 값이 달라질 수 있다.

Python seed와 Rust supervision이 `/proc/PID/task/PID/stat`의 실제 kernel leader
record를 일관되게 읽도록 바꾼다. PID:starttime 형식·정확 비교·Z/X 거부·owner chain·
prctl·자식/독립 process 정리·4MiB capture·원래10/5초 단언은 유지한다. tolerance나
QEMU 예외 허용을 추가하지 않는다. 별도 child가 읽는 kernel identity와 parent seed
identity의 equality 및 잘못된 시작 시각 거부 회귀를 추가한다. AST/rustfmt/diff만
로컬에서 확인하고 실제 동작은 원래 PRoot ARM 진단과 X64/ARM Actions에서 확인한다.

## 2026-10-30 — 세대·객체 작업 종료 시 상속된 flock 해제

source `e6e01a6c60809294bbbc173ca4bbb4b0709499b0`의
[foundation X64](https://github.com/bin9208/openpilot-rust/actions/runs/40895764298/job/123524296923)에서
`pending.rs:315`의 세대 lock 재획득과, 전체 남은 검사 중 `generation.rs:1199`의
두 번째 `objects::import()`가 각각 `WouldBlock`(errno11)로 실패했다. 첫 실행과
남은 전체 실행은 모두33개 중32개 성공·1개 실패였고 서로 다른 case가 실패했다.
최신 `e6ec2166`과 이 두 경로의 코드가 같음을 diff로 확인했다. 별도 tempdir이며
정상 작업 종료 뒤 File close에만 의존한다는 공통점이 있다.

[Linux flock 설명](https://man7.org/linux/man-pages/man2/flock.2.html)에 따르면
fork/dup로 공유한 open file description은 마지막 descriptor close 또는 명시 Unlock까지
잠금을 유지한다. 해당 실패 순간의 fork 상속은 추적하지 않았으므로 원인 추정과
실패 로그를 구분한다. 이 수명을 결정적으로 재현할 수 있도록 live `try_clone()`을
남긴 상태에서 작업 성공/오류 뒤 재획득하고, 작업 중 별도 open의 충돌 거부를 검사한다.

확인된 두 production 경로만 `FileLock` guard로 묶어 보호 작업의 모든 정상/오류
반환에서 명시 Unlock한다. 원래 비차단 충돌 거부, 파일 종류/경로 검증, fsync,
객체 검증과 기존 generation/보존 단언은 유지한다. retry, sleep, test thread 감소는
추가하지 않는다. 로컬은 rustfmt와 diff만 확인하고 기능 결과는 정확 수정 SHA의
원래 foundation X64/ARM 및 runtime 검사에서 확인한다.

## 2026-10-30 — 실제 대기열에 따른 일반 X64 추가 분산

`9b8c6337b`에서 GitHub의 native ARM8개·일반 X64 네 job이 끝난 뒤에도 Rust workflow의
약30개 검사가 자체 runner를 기다렸다. bin-actions2/3은 같은16 CPU PC이므로 세
agent가 물리 PC 세 대를 뜻하지 않는다. 최초 네 X64 job만의 분산으로는 뒤의 긴
대기열이 줄지 않는 실제 상태를 확인했다.

encoder/camera/Panda/selfdrive/radar/navd/Carrot Navi/startup/services/sensor-audio/GNSS/
control/estimation/UI connectivity/platform/hardware/logger/Athena의 독립 X6418개도
standard `ubuntu-24.04`로 보낸다. workspace·aarch64 전체 빌드·model/memory·Card·UI·
큰 원본 oracle·AGNOS/foundation/kernel/publication은 공통 self-hosted pool에 유지한다.
유료 runner나 계정 quota 변경은 없다. 이미 확인한 hosted 동시 실행 범위와 계정의
기존 queue 정책을 사용하며 실제 동시 수를 설정 변경만으로 확정하지 않는다.

각 job의 parsed YAML에서 `runs-on` 외 모든 필드가 같음을 대조한다. 원래 명령·needs·
matrix·feature·pin·skip/continue-on-error·최종 gate는 유지하고 기존 route 검사 허용
목록에 정확18개만 추가한다. 실행 중인9b8 Genuine은 보존하여 준비·컴파일 결과를
끝까지 확인한다. 그 성공을 새 source의 성공으로 대신하지 않고 새 source의 정확한
producer/필수 검사를 별도로 확인한다. 로컬은 AST/YAML·diff 읽기만 수행한다.

## 2026-10-30 — 동시 병합된 지원 서비스 #631 보존

추가 routing push 직후 `dev`에 `3fd7fd051b0af237fb8f75a4260e48707e3ae7c8`
(#631/#630)이 들어와 PR mergeability가 dirty가 되었고 PR 검사가 등록되지 않았다.
이 dev를 정상 merge하고 지원 서비스9개 변경 및 workspace의 modem helper lifetime
feature·실제 process-child 준비를 모두 보존한다. conflict는 실행 architecture 환경과
안전식만 기존 공통 helper의 요구대로 유지하고 고정 Cargo2를 복구하지 않는다.

고정 e445 command fixture/hash는 그대로 둔다. 새 dev의 workspace 두 전체 script만
원래 e445 script와 정확한 쌍으로 대조해 기준 변경을 인정하고, 두 새 script가 현재
workspace에 전체 그대로 남는지도 별도로 검사한다. feature flag를 일반적으로
삭제하거나 새 수명 검사를 생략하는 normalization은 없다. 기능 결과는 새 merge SHA의
정상 PR 검사로 확인한다. 기존 AGNOS 실행은 계속 보존한다.

## 2026-10-30 — 파생된 Cargo·FFmpeg 경로와 작업 temp 일치

정확 source e6f3/5ac6의 logger ASan 빌드는 성공했지만 env의
`CARGO_TARGET_DIR=${{ runner.temp }}/logger-asan-target`는 원래 runner 경로이고
뒤의 `$RUNNER_TEMP`는 owned 경로라 검사 파일을 찾지 못했다. 실제 실패는
main40911190766/job123581795243, common40911705322/job123583893642다.
공유 수정099474a5d의 두 helper/test 파일만 적용한다. 기존 RUNNER_TEMP 조건 안에서
CARGO_TARGET_DIR와 FFMPEG_DIR의 정확 old prefix만 owned prefix로 옮긴다. 확인된
FFMPEG_DIR aarch64 SDK와 세 Cargo target 선언도 같은 경로 규칙을 사용한다.
외부/guest/형제 prefix 및 명시 runner override는 유지하며 실제 helper fixture로
10개 경로 case를 확인한다. 원래 workflow argv·ASan/누수 옵션·기준은 바꾸지 않는다.
로컬은 AST/YAML/Bash 문법·diff만 확인하고 실제 sanitizer는 수정 SHA Actions에서 확인한다.

## 2026-10-30 — foundation ARM만 공식 native ARM runner로 이동

기준은 `0645cf61aff4104c3d6e87ac8f4e2902116e9032`다. 사용자가 제공한9b8
foundation 결과에서 같은 Cargo test·Clippy 구간이 QEMU ARM은16:14:45부터16:50:53까지
약36분, X64는 약4.5분 걸렸고 ARM 전체는70분을 넘어서도 추가 feature 검사가 진행됐다.
원래 검사를 줄이지 않고 foundation의 ARM matrix만 공식 `ubuntu-24.04-arm`으로
옮긴다. 앞선 self-hosted 유지 기록 중 foundation ARM의 실행 위치만 이 결정으로 바뀐다.
X64는 기존 `self-hosted/Linux/X64/openpilot-rust` pool을 유지한다.

job 이름은 `Actual offline Cargo reuse (ARM native)`로 구분한다. 기존 `local-arm`의
native 경로와 job-temp의 소유 target view, 고정 Python3.12.14 준비를 사용한다.
원래 명령·features·matrix 수량·90분 ARM 제한·disk/memory admission·비특권 UID·
readonly publication·owner-death/cancellation·artifact·cleanup·최종 gate는 바꾸지 않는다.
별도 real-kernel resource 검사, genuine AGNOS/QEMU, full-entry와 완료된 PRoot owner
진단도 보존한다. 과거 QEMU 성공을 새 source의 native ARM 성공으로 바꿔 쓰지 않는다.

publisher의 `SELF_HOSTED_GATES`에는 foundation을 그대로 남긴다. 해당 workflow의
새 ARM required name에만 실제 Actions jobs API의 `ubuntu-24.04-arm` label,
`self-hosted` label 부재 및 유효한 runner 이름을 요구한다. 자체 runner의 등록 이름을
hosted ARM 이름으로 위장해 쓰는 경우도 거부한다. X64·genuine·kernel·publisher
자체 실행의 self-hosted 경계와 source/PR/base/merge object/artifact/digest/expiry
검증, 필수 job 수와 모든 conclusion 검사는 보존한다. 공통 runner policy·validation
context·execution helper는 변경하지 않는다.

추가 리뷰에서 사용자가 제공한 실제 API 관측은 hosted ARM job123555870373의
`runner_group_id=0`, `runner_group_name='GitHub Actions'`, `runner_id=1000030832`,
`runner_name='GitHub Actions 1000030832'`, `labels=['ubuntu-24.04-arm']`다.
self-hosted job123555478513은 group1/`Default`/runner23이다. 새 ARM 예외에만
group ID가 bool이 아닌 정확 int0, group 이름이 정확 `GitHub Actions`, runner ID가
bool이 아닌 양의 int라는 조건을 더한다. default label을 제거한 자체 runner가
`ubuntu-24.04-arm` label만 사용해도 공식 group이 아니면 거부한다. fixture에는
관측된 ARM row를 넣되 실제 runner ID를 이 값 하나로 고정하지 않는다. group/runner
필드 누락, 다른 group, bool·문자열 ID 및 알 수 없는 자체 runner의 group1 위장을
거부하는 회귀를 추가한다. X64·genuine·kernel의 기존 API 정책은 바꾸지 않는다.

기존 routing 단언에 foundation의 정확한 matrix 선택자만 추가한다. publication
fixture도 ARM만 실제 hosted ARM job으로 구성하고 이름으로 row를 선택한다.
공식 hosted ARM 허용, hosted X64·self-hosted ARM/label·자체 runner 이름·빈 identity
거부, ARM 누락과 옛 QEMU 이름의 대체 거부 회귀를 추가한다. 나머지 자체 runner
경계와 기존 exact-source PR proof 회귀는 유지한다.

로컬에서 Python3개 AST, YAML, 원래 script38개의 `bash -n`, diff를 확인했다.
전체 YAML 자료는 job 이름과 `runs-on` 외 모든 필드가 같다. publisher의 다른 함수·
class는 AST가 같고 gate 함수는 두 정책 호출의 교체 외에는 같다. self-hosted 집합·
필수 gate 수·PR proof 매핑은 보존했고 공통 helper와 frozen fixture도 byte 단위로 같다.
변경은 workflow·publisher·기존 회귀2개·이 기록의 정확5파일이다.
기능·빌드·테스트·push·CI 조작은 하지 않았다.
새 정확 SHA의 실제 native ARM 실행과 cache staging의 동일 filesystem admission은
Actions에서 확인해야 하며, 실패해도 기존 자원·소유권 조건을 완화하지 않는다.

## 2026-10-30 — dev의 독립 메시징 검증 통합

dev `abea63adf5140cb670a1880fa2c350727a0feda9`의 #639를 정상 merge로
보존했다. 새 `messaging-allocation-431.yml`은 원래부터 hosted X64/native ARM에서
실행하므로, 기존 routing 회귀가 이를 상속 self-hosted workflow로 오인하지 않도록
원본 commit의 **전체 YAML**과 대조한다. 두 표준 matrix label과 정확한 selector도
명시한다. 원래 fork 검사·event·명령·권한은 바꾸지 않으며 다른 workflow의 기존
trusted-source 검사도 유지한다. 전체 source entry 자원 로그 artifact도 함께 보존한다.
로컬은 AST·YAML/Bash 구문·edition2021 Rust 서식·diff만 확인했다. 새 정확 source의
기능·필수 PR 및 병합 후 검사는 별도 Actions 결과를 요구한다.
