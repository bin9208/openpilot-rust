# ARM 태블릿 AGNOS 빌드 비교 (#643)

2026-10-30 KST 사용자 요청으로 `bin-Arm-tablet`(runner ID25)에 공통
`openpilot-rust` 라벨을 추가하고 API로 확인했다. 등록 라벨은 `self-hosted`,
`Linux`, `ARM64`, `openpilot-rust`다. ARM 작업의 선택자는
`[self-hosted, Linux, ARM64, openpilot-rust]`이며 X64 작업에 ARM을 섞지 않는다.

사용자는 1job 비교 재실행을 금지했다. 실제 프로세스 CPU affinity 수를 Cargo/test
병렬도로 사용한다. CPU가 하나만 허용되면 1job으로 실행하지 않고 원인을 보고한다.

사용자가 지정한 비교 대상은 `bin-actions3`의
[AGNOS 실행40904508393](https://github.com/bin9208/openpilot-rust/actions/runs/40904508393),
job123556239177, source `9b8c6337b56b80b79788ebc389b6622b484fe0c7`이다.
이 실행은 약3시간 후 bootstrap 검증에서 실패했으므로 완료 빌드 시간으로 취급하지 않는다.
#608 logger 개별 시험이나 #642 합성 workspace로 대체하지 않는다.

태블릿은 Android 위 PRoot Ubuntu 환경이다. 같은 AGNOS19.8-carrot-bt1 파일시스템을
QEMU 없이 실행할 수 있는지 먼저 실제 Actions에서 확인한다. 기존 원본의 kernel
resource guard와 ABI 판정을 완화하거나 Ubuntu에서의 일반 Rust 빌드를 AGNOS 성공으로
표시하지 않는다. 원래 소스·실행 중인 다른 runner 작업은 변경하지 않는다.

첫 확인은 외부 Actions 없이 공통 ARM selector로 native PRoot 실행, 실제 CPU affinity,
도구와 kernel/cgroup 상태를 기록한다. #642의 checkout/upload Action 추출 실패가 남아
일반 모든 CI에 즉시 호환된다고 주장하지 않는다. 로컬은 YAML/Bash/Python 문법과 diff만
확인하며 기능·ARM 실행은 Actions에서 수행한다.

이슈: https://github.com/bin9208/openpilot-rust/issues/643
