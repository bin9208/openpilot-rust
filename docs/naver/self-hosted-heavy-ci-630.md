# 무거운 CI의 셀프호스트 배치 — 2026-10-30 KST

실행 대단원은 [#630](https://github.com/bin9208/openpilot-rust/issues/630),
기존 공통 CI 작업은 [#605 / PR #606](https://github.com/bin9208/openpilot-rust/pull/606)이다.

사용자는 전체 ARM 검사의 실행 위치를 확인한 뒤 무거운 빌드·기능·ARM 검사를
셀프호스트로 모두 옮기고 GitHub 기본 러너에는 가벼운 검사만 남기라고 지시했다.
이 지시를 이전의 hosted/self-hosted 병렬 배치보다 우선한다.

## 배치와 보존 범위

- Rust의 실제 실행39job, 통합 하위 workflow의 실행13job과 runtime-build
  X64/ARM matrix는 `[self-hosted, Linux, X64, openpilot-rust]` 풀을 사용한다.
  등록된 `bin-actions`, `bin-actions2`, `bin-actions3` 모두 이 라벨을 가진다.
- 개발 중 dev에 병합된 PR #639의 messaging allocation X64/ARM 검사도 같은 풀과
  기존 ARM transport로 배치한다. 메시징 구현·고정 기준·실제 할당/원본 비교 명령은
  그대로 보존하며, 다른 작업의 진행 중 검사를 취소하지 않는다.
- 빠른 문법·정책·작은 회귀, 문서 검사, 필수 결과 집계는 hosted에 둔다.
  빠른 검사에 있던 ONNX/Gradle 실행은 기존 셀프호스트 Jetlink 검사로 이동한다.
- 기존 검사 명령·입력·필수 gate·needs·matrix를 보존한다. ARM user space의
  실제 비교는 QEMU/PRoot에서 수행하고 물리 ARM/AGNOS/기기 수용과 구별한다.
  다른 채팅의 소스·진행 중 CI와 공유 cache를 취소하거나 삭제하지 않는다.
- inherited publish/sync의 활성 상태와 대상 브랜치는 변경하지 않는다.

## 기존 구현 재사용

공통 기반은 PR #606의 정확 SHA
`5ac64068c0ff6419332ca90248bacab59c2310cf`에서 필요한 부분만 재사용한다.
`job-temp`, `rust-path`, `local-arm` action과 runner admission, native APT
source view, ARM transport, Cargo target ignore·회귀, 증가량 기반 디스크 검사를
가져온다. #606의 기기·사용자 문서나 별도 집중 workflow는 가져오지 않는다.

hosted용 Android/GHC/.NET 전체 삭제는 제거한다. 작업별 소유 임시 경로·target·
Android SDK만 정리하고, APT source/list는 작업별 공식 Ubuntu view를 사용한다.
실제 패키지 설치는 dpkg 잠금을 최대600초 기다려 같은 PC의 runner2/3 충돌을 줄인다.
명시적 ARM cross source를 쓰는 준비 단계도 source pin은 그대로 두고 metadata lists를
작업별 경로로 분리한다. 이 호출은 기본 APT wrapper의 source 선택 예외이므로 별도로
분리해야 하며, 고정 source 파일·ARM 패키지·서명 검증은 보존한다.
고정 Cargo/test 병렬 cap과25/35GiB 추가 floor 대신 실제 CPU와 작업별 증가량을 쓴다.
기기 설치의 별도 디스크 정책은 변경하지 않는다.

## #630 소스와 실제 검증 상태

9개 기능 수정은 [PR #631](https://github.com/bin9208/openpilot-rust/pull/631)로
dev `3fd7fd051b0af237fb8f75a4260e48707e3ae7c8`에 이미 반영됐다.
검증한 PR head `875e7a4b6172305dbc6ae4d16c78d5f02ba84fe3`와 tree가 동일하며,
[필수 PR Rust46job](https://github.com/bin9208/openpilot-rust/actions/runs/40895279139),
[통합16job](https://github.com/bin9208/openpilot-rust/actions/runs/40895279424),
fast/docs 및 전체 ARM 검사가 성공했다.

병합 후 [통합](https://github.com/bin9208/openpilot-rust/actions/runs/40908895379),
[fast](https://github.com/bin9208/openpilot-rust/actions/runs/40908894242),
[docs](https://github.com/bin9208/openpilot-rust/actions/runs/40908893929)는 성공했다.
[병합 후 Rust40908894073](https://github.com/bin9208/openpilot-rust/actions/runs/40908894073)는
이 러너 배치 변경 지시에 따라 이 작업 소유 실행만 취소했다. 취소를 성공으로 표시하지 않는다.

이번 CI 변경은 새 정확 SHA의 정상 PR 검사·셀프호스트 실행과 별도 병합 후 검증이
남아 있다. 로컬은 문법·서식·diff 및 읽기 감사만 수행한다. 실제 결과와 SHA/Actions는
#630 및 후속 PR에 기록한 뒤 개발 실행 대단원을 닫는다. 원래 실기 수용은 별도다.
