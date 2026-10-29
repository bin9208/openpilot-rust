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

## 첫 PR 검증과 dev 기준 갱신 — 2026-10-30 KST

첫 source `064390b4b24b9bbd2b9db003740e97a39524a83d`의 실제 검사 merge는
`1b997fe3220110b08821f58aa146985452d607f8`이며 두 tree는
`6f6d55f5d8fa689d86698ee7e49cd3e96a869d6f`로 같다.
[집중40900673465](https://github.com/bin9208/openpilot-rust/actions/runs/40900673465)의
X64/native ARM은 모두 성공했다. 1,000회 갱신에서 single 할당 호출은
7,000→6,000·요청 bytes는920,000→896,000, two topics는
10,000→9,000·1,472,000→1,424,000이다. empty/malformed 의미 fingerprint와
상태 무변경을 보존했다. 원본 상태11,205회 대조는 양쪽 최대 오차0이며,
메시징 전체 테스트·strict Clippy·기준/기준의 미개선 거부도 통과했다.
게시 artifact의 실제 결과·실행 파일 hash·현재 소스 checksum을 읽어 확인했다.

[전체 Rust40900673352](https://github.com/bin9208/openpilot-rust/actions/runs/40900673352)의
workspace는 기존 [#484](https://github.com/bin9208/openpilot-rust/issues/484)의
별도 tunnel unit fixture 경합으로 실패했다. `child_driver`가 아직 비어 있는
`descendant.pid`를 읽어 `ParseIntError { kind: Empty }`를 냈다.
carrot-recovery 소스는 이번 PR에서 변경하지 않았다. 실패 실행과 #484 상태를
보존하며 #431의 기능 결함 또는 production 종료 결함으로 확대하지 않는다.
같은 source의 fast/docs/integration 및 selfdrived·Card 등 다른 검사는 성공했고,
ARM의 GNU 빌드와 후속 musl 작업은 당시 계속 진행 중이었다.

2026-10-30 01:10 KST에 별도 PR631이 dev `3fd7fd051b0af237fb8f75a4260e48707e3ae7c8`로
병합되어 PR639은 BEHIND가 됐다. dev의 strict 보호 규칙에 맞춰 해당 통합 기준을
작업 브랜치에 merge한다. 기존 #630 변경을 새로 구현하거나 그 파일을 편집하지 않는다.
이 갱신에서 메시징 소스·회귀·계측의 bytes는 그대로이고, 비교 기준 e445도 유지한다.
공용 CI는 이미 dev에 반영된 내용을 상속하며 PR639의 독자 diff는 계속 #431의7개 파일이다.
이전 source의 성공을 갱신된 source의 필수 성공으로 대신하지 않고 전체 검사를 확인한다.
최신 실행·병합 후 결과는 [#431](https://github.com/bin9208/openpilot-rust/issues/431)에
정확 source와 Actions 링크로 이어서 기록한다.
