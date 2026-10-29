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
