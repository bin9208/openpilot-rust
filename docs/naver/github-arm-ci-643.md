# GitHub ARM 빌드 전환 초안 (#643)

사용자는 전체 AGNOS 비교를 마치고 기존 QEMU보다 GitHub ARM이 훨씬 빠르면 ARM
빌드를 GitHub ARM으로 전환하도록 요청했다. 전체 비교는
[이슈643](https://github.com/bin9208/openpilot-rust/issues/643)에서 진행하며, 이 초안은
`dev`의 CI 변경만 준비한다. 비교 조건이 확인되기 전 적용 완료로 취급하지 않는다.

## 적용 범위

- generic aarch64 전체 GNU/musl 빌드와 정적 IPC probe를 `ubuntu-24.04-arm`에서 실행한다.
- X64 workspace의 genuine QCOM 컴파일·오프라인 스케줄링 테스트 두 파일을 전용
  GitHub ARM job으로 옮기고, workspace가 그 성공을 필수로 요구하도록 연결한다.
- 동일 compiler SHA256·ELF/ABI 검증·타깃 빌드 명령·IPC 검사와 기존 나머지 workspace
  검사를 보존한다. staging/probe CLI는 명시적 native 실행과 기존 QEMU 형식을 지원한다.
- 자동 병렬도를 사용한다. 1job으로 제한하지 않는다.
- 작업 브랜치 push의 별도 Fast checks를 제거하고 PR 필수 검사와 수동 실행은 유지한다.

이전부터 GitHub ARM이던 matrix와 RadarCAN/Carrot Navi ARM job은 유지한다. 아직 dev에
들어오지 않은 #599의 AGNOS/factory/source-entry 변경은 원래 작업 브랜치에서 별도로
진행한다. #599의 런타임 구현을 CI 전환만을 위해 dev에 포함하지 않는다.

## 검증 상태

동일 QCOM staging/probe 코드의 실험
[40956391986](https://github.com/bin9208/openpilot-rust/actions/runs/40956391986)에서
genuine QCOM Rust5개·pytest6개와 factory 권한 검사가 통과했다. generic ARM 전체는
[40962761699](https://github.com/bin9208/openpilot-rust/actions/runs/40962761699)에서
52분42초에 통과했고 실제 CPU affinity는0..3이었다. 이는 해당 실험 SHA의 증거이며
이 dev 초안의 최종 SHA 검사 대신 사용하지 않는다.

로컬은 Python/YAML/Bash 문법·Rust 서식·diff 읽기 점검만 수행한다. 이 초안은 아직
push/PR 생성 전이며, 최종 비교와 조건부 적용 및 해당 SHA의 Actions 결과를 추가한다.
실제 기기 설치·부팅·주행은 수행하지 않는다.
