# 작업 브랜치의 전체 AGNOS workflow ARM 측정 (#643)

2026-10-30 KST 사용자가 빌드 도구 두 개의 짧은 비교를 넘어 실제 AGNOS 작업 브랜치의
빌드를 ARM에 배정하고 시간을 측정하도록 요청했다.

선택한 작업 브랜치는 `codex/feat-599-complete-runtime-inputs`, 기준 SHA는
`dff230060a2ccec0cc58c3d09822b9a026beff1f`이다. 이 커밋에서 별도 실험 브랜치
`codex/feat-643-agnos-arm-full`을 만들었다. 선택한 브랜치와 실행 중인 x86 CI는 보존한다.

새 `Full AGNOS build on GitHub ARM` workflow는 원본
`.github/workflows/runtime-native-environment.yml`의 준비·컴파일·검증·입력 구성·증거
수집·정리 단계를 모두 유지한다. GitHub `ubuntu-24.04-arm`만 선택한다. 별도 Fast checks
push 실행은 사용자 정책대로 제거한다.

## ARM 실행에 필요한 변경

- 호스트 planner 생성기를 `aarch64` 대상으로 실행한다.
- 원본과 같은 PRoot5.5.0 소스를 ARM에서 빌드하고 동일 AGNOS 이미지에 사용한다.
- ARM에서는 QEMU를 넣지 않는다. 기존 x86 실행 경로는 보존한다.
- 명시한 ARM 실험 환경에 한해 원본 self-hosted 전용 입장 조건을 GitHub ARM에도
  허용한다. 실제 커널 cgroup, 비특권 실행, 소유권·SHA·자원 검증은 유지한다.
- 실제 허용 CPU 수로 병렬도를 정한다. 1job 강제나 fallback을 사용하지 않는다.
- 전체 job 시간, 준비부터 증거 수집까지의 시간, 원본 AGNOS 내부 명령 구간 시간을
  기록한다. 실패 시 실제 실패 단계와 그때까지의 시간으로 보고한다.

런타임 구현과 의존성 pin을 바꾸지 않고 CI 실행 환경만 조정한다. 정확한 측정 커밋 SHA는
새 workflow의 실행 SHA이며 위 작업 브랜치 기준 커밋과 함께 기록한다. 사용자 설치·장치
부팅·주행 수용의 증거로 취급하지 않는다. 입력 파일의 publication 후보와 artifact를
원본처럼 보존하지만 릴리스 발행이나 설치는 실행하지 않는다.

로컬은 Python/YAML/Bash 문법과 diff만 확인하고 실제 실행은 Actions에서 수행한다.
결과가 나올 때까지 이슈를 완료 처리하지 않는다.

이슈: https://github.com/bin9208/openpilot-rust/issues/643

사용자는 이 전체 비교가 끝나고 GitHub ARM이 기존 QEMU보다 훨씬 빠른 것으로 확인되면
ARM 대상 빌드를 모두 GitHub ARM으로 전환하도록 추가 지시했다. 측정 중에는 ARM 작업
배정과 QEMU 사용 경로를 읽기 감사하고, 조건이 확인되면 별도 CI 변경과 실제 Actions
검증을 진행한다. 비교가 끝나기 전 전환 완료로 표시하지 않는다.
