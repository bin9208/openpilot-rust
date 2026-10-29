# 독립 지원 서비스 병행 실행 묶음 — 2026-10-29 KST

실행 대단원: [#630](https://github.com/bin9208/openpilot-rust/issues/630).
전체 현황: [#522](https://github.com/bin9208/openpilot-rust/issues/522).

## 사용자 요청과 결정

진행 중인 [#598](https://github.com/bin9208/openpilot-rust/issues/598)의 종료를
기다리지 않고 진행할 수 있는 기존 이슈를 하나의 실행 대단원으로 모은다.
기존 대단원은 #523~#528의6개와 #598을 합쳐7개였으며, 대단원8 #630을 추가했다.
기존 분류·이력·하위 이슈 번호와 완료 조건은 보존한다.

아래9개를 선정했다. 대부분 기존 수정 구현과 개별 검증 근거가 있으므로
새로 재작성하지 않고 현재 기준의 필요한 변경·검증·정상 dev 통합을 처리한다.
과거 집중 성공은 최신 전체 후보 성공으로 대신하지 않는다.

| 이슈 | 내용 | 주 대상 |
|---|---|---|
| #392 | 음성 클립 경계와 유한 반복 초과 | soundd 정책·playback 회귀 |
| #394 | 이전 페어링 응답이 새 요청 화면을 지우는 경쟁 | Bluetooth BlueZ agent/state |
| #395 | 테더링 취소 후 소유 자손 프로세스 정리 | Wi-Fi tether |
| #398 | 모뎀 helper의 EOF·취소·자손 수명 | modem helper/PPP/runtime |
| #363 | tombstoned의 group 정리와 leader reap 순서 | apport |
| #351 | multipart 업로드의 중복 전체 파일 버퍼 제거 | web-upload multipart |
| #353 | snapshot xattr 버퍼 재사용 | bootlog snapshot |
| #397 | Wi-Fi snapshot의 불필요한 깊은 복사 제거 | Wi-Fi state |
| #439 | Cookie 헤더의 문자열 중간 할당 제거 | LPA HTTP cookies |

#395/#397은 같은 Wi-Fi 실행 단위로 소유권을 맞춘다. 상태는 #630의
체크리스트 한 곳에서 관리한다. 원래 native 분류 부모는 #524/#526으로 유지한다.

## 분리 확인과 한계

2026-10-29 20:34 KST에 다음 정확 head와 전체 PR 파일 목록을 조회했다.

| PR | 대상 | head |
|---|---|---|
| #611 | dev | `a5e2d10f7ea882f7edf4453c481c8dc5b5f9ffb5` |
| #606 | dev | `ec418df39d3b5820f2751978e3898f0a4d858cbc` |
| #603 | dev | `b42e5eaeec5a13bf83f24ccd31ab5e3364eb2d47` |
| #494 | dev-md | `868ea1cfff9fb46606a5e814a776b0ea93bff820` |

이4개 PR의 파일 합집합444개에 선정 런타임8개 crate 경로의 직접 교집합은0개다.
별도 결함 PR #500의 전체292개 파일에도 해당 crate 변경이 없다.
현재 원격 dev는 `e445c9d19c3041bc79d8624589e2eb6ba7d061c6`, 기존 PR343 후보는
`126fdbea36598566d1040be0a35a365c909be4e4`다.

`Cargo.lock`의 각 package 의존성 목록을 dev와 기존 후보에서 비교했다.
#393은 soundd의 회귀용 rustix/tempfile 추가, #450은 현재 dev에 없는
stats_alloc 추가가 필요하여 이번 묶음에서 제외했다. #392는 #393과 같은
기존 커밋에 있으므로 정책/playback 변경만 분리하고 manifest/lock을 섞지 않는다.
#398은 기존 rustix의 feature 설정을 유지하되 새 패키지 의존성으로 확대하지 않는다.

확인 범위는 현재 변경 경로와 의존성이다. 기능/빌드 성공이나 실기 효과의
증거가 아니며, 공유 러너·공통 helper·최종 dev 통합의 조율은 여전히 필요하다.
각 실행 단위에서 최신 head와 fixture/검증 경로를 다시 확인한다.
공용 lock/workspace/CI/provider/bootstrap/설치 경로 변경이 필요해지면
그 항목은 독립 병행 범위에서 보류하고 필요한 검사를 생략하지 않는다.

## 실행·종료 기준

- 최신 dev의 별도 worktree/이슈 브랜치에서 필요한 기존 변경만 재사용한다.
  #343 전체 전환과 중단된 다른 작업을 포괄 재개하지 않는다.
- 로컬은 읽기·문법·서식·diff 확인만, 기능/원본 비교·빌드·strict Clippy·ARM은
  GitHub Actions에서 수행한다. 다른 세션의 실행과 cache를 보존한다.
- 정확 소스의 필수 PR 검사·정상 dev 병합·별도 병합 후 검사와 관련 문서를 확인한다.
  필요하면 PR은 영향 범위로 나누되 사용자용 현황은 #630에 모은다.
- 개발·검증·통합9개를 마친 뒤 개발 실행 대단원 #630을 닫는다.
  원래 실기 조건은 원래 이슈 및 #528에 남기고 실제 충족 전에는 해당 이슈를 닫지 않는다.
- #598의 기존 완료 조건과 후보에 새 작업을 추가하지 않는다.
  새 독립 결함은 별도 이슈로 기록하고 이9개 묶음을 계속 확대하지 않는다.

## 이번 처리 결과

- #630을 kind:epic/track:dev/area:functions/area:optimization으로 생성했다.
- #522의 native 하위 이슈로 연결하고 #522/#524/#526에 실행 묶음 링크를 추가했다.
  기존 본문·제목·열린 상태와 원래 하위 부모는 유지한다.
- 런타임 소스 변경, 기능 테스트/빌드, Actions 실행·취소, PR 병합·배포·기기 작업은
  이번 이슈 구성에서 수행하지 않았다. 이 문서는 로컬 작업 기록이다.

조회 snapshot·선정 감사·변경 전후 이슈 본문·확인 결과는 private ignored
`.analysis/archive/2026-10-29/independent-support-batch-630/`에 보관한다.
기존 작업 파일은 수정하거나 정리하지 않는다.

## 2026-10-29 개발 착수 후 의존성 보완

실제 패치 분리에서 #398의 `spawn_session_redirected_with_env`와 #395의
`spawn_session_inherited_with_env`가 현재 dev에 없는 것을 확인했다.
선정 당시 crate 경로 대조에서 놓친 공용 API 의존성이다. #611/#606의 실제
`rust/crates/process-supervision` diff는0개다. 해당 `launch.rs`에 기존 구현의
두 새 메서드와 소유 session/stdio 회귀만 추가하는 최소 예외를 #630에 기록했다.
기존 API/StreamMode/호출의 동작을 변경하지 않고 새 의존성·lock 변경도 없다.
공용 CI/provider/bootstrap/설치 경로의 격리는 유지한다.

현재 dev의 hosted 고정 CI를 중복 전환하지 않기 위해 별도 검증 controller
브랜치에서 정확 후보 SHA의 집중 검사를 준비한다. controller는 개발 PR에
병합하지 않으며 집중 결과는 필수5개 context와 정상 병합 후 검사를 대신하지 않는다.
