# #600 자체 빌드의 자원·취소 경계

상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598),
자산·증분·복구 [#600](https://github.com/bin9208/openpilot-rust/issues/600)의 추가 구현이다.
이 변경은 검증 대기이며 comma 3X 설치 후보가 아니다.

## 실행과 게시

Config의 필수 `resources`는 작업별 RAM 한도, 정상 runtime용 가용 RAM 여유,
쓰기 대상 filesystem별 예상 증가 한도와 남겨 둘 공간을 지정한다. 모든 쓰기 filesystem을
포함해야 하며 같은 filesystem의 별칭은 같은 예산이어야 한다. 고정25/35GiB 기준은 없다.
시스템 MemAvailable과 가시적인 상위 cgroup 한도를 함께 확인한다.

`openpilot-native-build-scope`만 root로 실행한다. 고정된 자기 UID의
`/sys/fs/cgroup/openpilot-native-build-UID` 또는 v1 memory mount 아래 같은 이름만 허용한다.
공유 root controller는 이미 memory가 활성화돼 있어야 한다. 이 도구는 자기 빈 parent의
memory controller·소유권만 준비하고, 자신만 `control` leaf로 이동한 뒤 UID/GID와
supplementary groups를 내려 같은 소스의 `openpilot-runtime-native`로 exec한다.
컴파일러·Git·모델 준비는 root로 실행하지 않는다. 기존 manager/updater/runner는 이동하지 않는다.
2026-10-27 사용자가 root scope 변경을 포함한 #598 재진행을 명시적으로 승인했다.
로컬에서 실제 controller를 생성하거나 기기를 조작하지 않았다.

frontend가 만든 private `build-PID-random` cgroup에 worker가 먼저 들어간다.
v1/v2의 hard RAM 한도, 계층 정책, swap 제한을 확인하고 없으면 빌드를 거부한다.
상위 프로세스 사망·취소·offroad 상태 변경은 worker 및 다른 process group/session을
만든 하위 작업까지 정리한다. frontend의 부모인 scope/sudo 대기 프로세스가 죽는 경우도
parent-death signal로 중단한다. cgroup 삭제·신호는 자기 시도에만 적용한다.

worker는 봉인된 candidate receipt만 남긴다. frontend는 worker 종료, 자원 최종 확인과
하위 작업 정리를 마친 뒤 source/config/seal을 다시 확인해야 pending update를 게시한다.
실패나 취소에서 기존 active generation, 정상 프로세스와 로그를 교체하지 않는다.

## 측정 범위와 한계

각 `generations/build-attempts/attempt-*/resources.jsonl`은 초기 admission, 실행 중 상태,
실패 또는 성공을 fsync해 남긴다. 강제 종료·전원 중단으로 최종 행이 없는 기록을 성공으로
해석하지 않는다. candidate도 frontend의 최종 확인 없이 pending으로 선택되지 않는다.

- 메모리는 worker 진입 뒤 하위 프로세스·file cache를 포함한 커널 cgroup charge의 최대값이다.
  v2 `memory.peak` 또는 v1 `memory.max_usage_in_bytes`가 필수다. frontend VmHWM은 별도로
  기록하며 단일 프로세스 RSS를 전체 빌드 peak로 표시하지 않는다.
- 디스크는100ms 간격 filesystem 가용 공간 감소의 관측 최대값이다. 동시 writer의 증가도
  포함하며 hard filesystem quota나 순간 peak를 완전히 포착한 값으로 표시하지 않는다.
  한도/여유가 부족하면 빌드를 중단한다. 초기 증가량 예측과 여유값은 실제 작업에 맞게 정해야 한다.
- controller를 제공하지 않는 커널에서는 격리 없는 fallback으로 실행하지 않는다.
  이 개발 환경·CI의 측정값을 comma 3X의 메모리/디스크 예산으로 복사하지 않는다.

## Actions 검증

`native-resource-guard.yml`은 등록된 bin-actions X64 host kernel에서 source-built scope의
실제 권한 하강을 확인하고, 별도 job-owned kernel cgroup에서 mandatory ignored test를
명시적으로 실행한다. 일반 디렉터리를 controller로 흉내 내거나 환경이 없을 때 성공 처리하지 않는다.
일반 workspace 테스트는 별도로 가짜 controller 거부를 검사한다.

실제 메모리를 할당하는 정상/초과, 파일 증가, worker SIGKILL, frontend SIGKILL,
frontend 부모 SIGKILL을 검사한다. 독립 process group의 하위 작업 종료와 이전 active/log의
바이트 보존, 무관한 정상 프로세스 생존을 확인한다. 커널 peak/event와 시도별 JSONL을 보존한다.
CI 준비·정리는 job owner/source/inode를 대조한 자기 cgroup만 대상으로 한다.
production의 자기 UID scope는 다음 빌드에 재사용하며 공유 controller와 다른 job을 변경하지 않는다.

Rust/ARM 회귀는 기존 `runtime-source-build.yml`의 원래 명령을 그대로 실행한다.
ARM QEMU/PRoot는 ARM user space 검증이며 실제 AGNOS kernel 또는 comma 3X가 아니다.
이 변경의 기능/빌드/Clippy는 Actions 결과 대기다. 로컬에서는 서식·문법·diff만 확인했다.
전체 native toolchain/provider 준비, 전체 runtime 빌드, 기본 부팅 경로, 사용자 녹화/업로드 수용은
별도 완료 조건으로 남아 있다.
