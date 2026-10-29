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

## 2026-10-29 — ARM source command 초기화 실패의 제한된 진단

source `b1c6040749910d8e161b34233a3069f318be0d49`의
[foundation ARM job123495075956](https://github.com/bin9208/openpilot-rust/actions/runs/40887511364/job/123495075956)은
bin-actions2의 X64 → PRoot/QEMU AArch64, uid1000 환경이었다.
`test_native_source_runtime.py`의 SIGKILL 검사는 자식 준비 전에 wrapper가 exit1로
종료됐고, original-owner 검사도 `CalledProcessError`로 실패했다. 이 시점에는
owner-death 이후 자식 정리와 4MiB capture 한도까지 검사가 도달하지 못했다.
단순 10초 초기화 예산 초과나 CPU thread 설정 문제로 확정하지 않는다.

`OwnedCommands.run()`은 stderr를 임시 파일로 받았다가 예외의 `stderr`에 담지만,
unittest traceback은 그 bytes를 출력하지 않는다. 해당 임시 파일도 반환/오류 시
닫히고 artifact 목록에 없으므로 기존 로그에서는 내부 원인을 복원할 수 없다.
실패 source와 `924710ae8ac25e52dace839cc9ba89f2fd0fc4e6`의 production driver·
회귀 파일은 각각 Git blob이 같다. `prctl`/부모 PID/owner identity 검증 또는 exec
실패 중 어느 것인지는 실제 stderr를 확인해야 한다.

두 실패 case에서만 `CalledProcessError.stderr`의 앞4096바이트를 decode하여 stderr에
출력하고 원래 예외를 다시 발생시킨다. SIGKILL case는 테스트가 만든 중간 Python
process에서 예외가 발생하므로 그 code 안에서 출력한다. production 출력 정책,
원래 argv·10초/5초 예산·non-root/owner/독립 process 정리 단언과 4MiB 한도는 유지한다.
로컬에서는 모듈과 생성된 `-c` code/body의 AST 및 diff만 검사하고 기능 실행은 하지 않는다.

부모에게는 같은 `offline-cargo-reuse` ARM matrix의 local-arm 준비와 자원 admission 뒤,
Rust toolchain 설치·빌드 전에 아래 두 case만 진단 선행 step으로 실행하도록 제안한다.
기존 `openpilot-local-shell`과 `LOCAL_CI_GUEST_IDENTITY=unprivileged`를 사용하며,
전체 회귀 step을 삭제하거나 native-host ARM 성공으로 대체하지 않는다.
이 변경에는 workflow 수정·CI dispatch·원격 push를 포함하지 않는다.

```sh
python3 -m unittest -v \
  tools.ci.tests.test_native_source_runtime.SourceRuntimeTests.test_driver_sigkill_also_stops_the_non_root_download_command \
  tools.ci.tests.test_native_source_runtime.SourceRuntimeTests.test_original_owner_death_stops_only_the_dedicated_command_and_bounds_capture
```

## 진단 브랜치 전용 hosted X64 → ARM QEMU 실행

`native-source-command-diagnostics.yml`은
`codex/fix-635-source-command-diagnostics` push와 명시 dispatch만 받으며 job도
같은 저장소·브랜치 및 기존 trusted source 조건으로 제한한다. hosted `ubuntu-24.04`
X64에서 기존 job-temp/local-arm의 고정 PRoot·Ubuntu ARM rootfs·Python 준비를 그대로
사용하고 `LOCAL_CI_GUEST_IDENTITY=unprivileged`로 위 두 case만 실행한다.
host 설정 확인 step은 Bash, 테스트는 기존 `openpilot-local-shell {0}`이다.
Rust toolchain 설치·빌드나 공장 image 준비 step은 추가하지 않는다. 공통 local-arm의
기존 bootstrap은 바꾸지 않으므로 그 내부 rustup-init 준비와 모든 입력 pin도 유지한다.

job 예산은30분이며 두 case 내부의 원래 예산·argv·수용 기준은 변경하지 않는다.
stdout/stderr는 합쳐 앞64KiB만 console과 job 소유 로그에 남긴다. 초과 출력은 끝까지
읽어서 출력 한도가 unittest에 SIGPIPE를 보내지 않으며 pipefail로 원래 실패를 유지한다.
always 경로는 local-arm finish, 제한된 로그와 실행 identity/도구 version artifact,
기존 job-temp 소유권 검증 cleanup 순서다. self-hosted heavy 실행을 추가하지 않는다.

공통 routing 검사는 이 workflow 이름을 hosted 목록에 넣고 이 job 하나만 generic ARM
분기보다 앞에서 hosted X64로 대조한다. 원본 fixture·기존 gate·workflow는 수정하지 않는다.
이 workflow는 내부 오류 확인용이며 공개 입력·전체 source build·factory ARM·기기 정상
시작의 성공 증거가 아니다. 원인 확정 뒤 main에 이 workflow를 통합할 필요는 없다.
로컬은 AST/YAML/Bash 구문·diff만 확인하며 push·dispatch·기능 실행은 부모가 담당한다.


진단 branch 전용 `native-source-command-diagnostics.yml`을 추가한다. hosted X64의
원래 pinned local-arm PRoot/QEMU·unprivileged 환경에서 위 두 case만 실행하고
실패를 그대로 유지한다. 실제 ARM host 성공이나 전체 source build 성공을 대신하지
않는다. Rust build와 factory image download 없이 동일 source command를 진단하며
기존 self-hosted heavy 검사를 점유하지 않는다. job별 stdout/stderr는 기존4096바이트
진단과 두 case의 로그로 제한하고 원래 owner·SIGKILL·10/5초 단언을 유지한다.


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
