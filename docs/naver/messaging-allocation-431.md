# 메시징 중간 할당 제거 독립 통합 — #431

작성: 2026-10-30 KST. 이슈: [#431](https://github.com/bin9208/openpilot-rust/issues/431).

## 범위와 분리

사용자는 #598 및 다른 진행 작업과 겹치지 않는 #431만 별도로 진행하도록 승인했다.
기준 dev는 `e445c9d19c3041bc79d8624589e2eb6ba7d061c6`이며,
작업 브랜치는 `codex/feat-431-messaging-allocation`이다.
기존 #296/PR343의 `2d8d027932dc880bd0acd00288b256ac8d7f1b89`에서
메시징 소스 2개·상태 회귀 1개·독립 계측 도구 1개만 재사용한다.
그 커밋의 Bluetooth·dev-tools·공용 CI 변경은 포함하지 않는다.

`SubMaster::update`는 수신한 메시지의 바이트를 빌린 iterator로 전달하여
중간 `Vec<Vec<u8>>` 수집을 없앤다. 모든 메시지를 파싱한 뒤 상태를 바꾸는
기존 순서를 유지한다. topic/frame/validity·주기·오류 처리 정책은 보존한다.
의존성 및 Cargo.lock 변경은 없다. dev-md의 변경을 dev로 옮기지 않는다.

## 검증 경로

- 로컬은 Rust 서식·Bash/YAML 구문·diff 확인만 수행한다.
- 전용 `Messaging allocation` Actions는 X64 및 native ARM에서 고정 기준과
  현재 소스를 각각 빌드하고 같은 독립 계측 도구를 각자의 rlib에 연결한다.
- empty/single/two/malformed의 정확한 행 구성과 의미 fingerprint를 비교한다.
  nonempty 두 사례에서 할당 호출·요청 바이트가 모두 줄어야 통과하며,
  기준/기준을 비교한 음성 대조는 반드시 실패해야 한다.
- 실제 메시징 all-target 테스트·strict Clippy·기존 원본 상태 비교를 실행한다.
  기존 비교의 `1e-12` 허용 오차와 malformed 배치의 상태 무변경 조건을 유지한다.
- 기존 필수 PR 5개 검사와 selfdrived/소비 데몬 검사, 전체 ARM 빌드는 그대로
  실행한다. 전용 집중 검증을 기존 필수 context로 복사하지 않는다.
- 정상 dev 병합 후 같은 소스의 별도 검사 결과를 확인한다.

전용 workflow와 helper만 추가하므로 #598/#630이 수정하는 공용 workflow를
변경하지 않는다. 러너는 기존 hosted X64/ARM을 쓰며 현재 가용 CPU 수를 반영한다.
준비·두 빌드·증거 저장의 예상 증가량 8 GiB를 확인하고 고정 25/35 GiB 정책은
새 경로에 도입하지 않는다. 기존 작업의 실행이나 cache는 정리하지 않는다.

## 상태와 완료 경계

현재 구현 분리 및 검증 준비 중이다. 이번 source의 기능 검사·ARM·정상 PR 통합·
병합 후 검사는 아직 성공하지 않았다. 과거 [할당 비교 성공](https://github.com/bin9208/openpilot-rust/actions/runs/39494388960/job/118875506156)은
재사용할 구현의 근거이며 현재 후보의 성공을 대신하지 않는다.

관측하는 값은 호스트/native ARM의 Rust allocator 요청량이다. 전체 peak memory,
장치 CPU·지연·주행 개선으로 확대하지 않는다. 완전 후보의 설치·기기 수용은
기존 #528에서 구분하며 이번 작업에서 기기에 접속하거나 배포하지 않는다.

Docs-Not-Needed: 사용자 조작·설정·기능을 바꾸지 않는 내부 할당 개선이다.
