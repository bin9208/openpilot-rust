# #598 작업 브랜치의 APT·native 입력 캐시 적용 (#644)

2026-10-30 사용자가 #598 작업 브랜치에도 APT 및 native 입력 로컬 캐시를 모두
반영하고, 현재 실행은 유지하며 다음 작업부터 적용하라고 명시했다.

- 적용 대상: [PR #611](https://github.com/bin9208/openpilot-rust/pull/611)의
  `codex/feat-599-complete-runtime-inputs`.
- 적용 전 source: `27547f08c6daf7deac99e35b0f6e1d0c8ffb0298`.
- 재사용한 구현: [#644](https://github.com/bin9208/openpilot-rust/issues/644),
  `d0a57af32d1e43ad1f4a879803f15b0fee722afc`의 helper·cache action·회귀 검사.

## 반영 내용

`job-temp`의 시작 부분에 지속 다운로드 경로 연결과 실제 해석된 Actions SHA의
cache 보강을 추가한다. #598의 validation-context 생성·검증·artifact, owner 검사,
target/output 격리와 기존 정리 절차는 그대로 유지한다. 일반 작업은 이미 설정된
러너의 cache를 사용하며 Listener 설정 변경이나 재시작 명령을 호출하지 않는다.

native APT 기본 호출은 공식 source bytes와 native architecture로 구분한 지속
lists·metadata·deb archives를 사용한다. 매번 저장소 업데이트와 서명을 검증하고
동일 source의 작업은 파일 잠금으로 직렬화한다. 기존 explicit cross source,
APT_CONFIG와 guest APT 선택은 유지한다.

`native-cache` action은 self-hosted 작업에서 이름과 정확 key의 로컬 입력을 독립
copy/reflink로 복원·저장한다. 원래 immutable source pin·길이·형식 검증과 작업별
소유권은 유지하고 GitHub cache 전송을 줄인다. 다른 실행 환경을 위한 기존
archive staging 경로도 보존한다. 기존 runtime·compiler·SDK·model manifest는
변경하지 않는다.

집중 검증은 기존 `native-cache-reuse.yml`의 두 job을 등록된 bin-actions에서
실행한다. 첫 job은 실제 캐시/원자 저장·동시 writer·취소/owner 회귀와 기존 CI
정책을 검사하고, 두 번째 job은 다른 private 경로에서 exact local hit·동일 body와
실제 APT index `Hit:`를 확인한다. source SHA와 cache backend를 artifact에 기록한다.

## 현재 실행 보존과 검증 범위

반영 시작 시 진행 중인 작업은 AGNOS
[40991065022/job123853333691](https://github.com/bin9208/openpilot-rust/actions/runs/40991065022/job/123853333691),
ARM 및 workspace
[40991071475](https://github.com/bin9208/openpilot-rust/actions/runs/40991071475)다.
이 실행을 취소하거나 재시작하지 않는다. 기존 워크플로의
`cancel-in-progress: false`와 pending 보존을 유지해 새 commit 이후 작업이
새 helper를 사용하도록 한다. 러너의 routing 라벨이나 현재 checkout은 바꾸지 않는다.

로컬은 Python AST·YAML·Bash 구문과 diff만 확인한다. 기능 결과는 새 source의
Actions에서 확인하고 정확 SHA·실행 URL을 #644에 추가한다. 이전 #644 source의
성공을 새 #598 source의 성공으로 대신하지 않는다. PR 전체 필수 검사·병합 및
ARM/AGNOS runtime/실기 수용은 cache 집중 검증과 별도로 유지한다.

## Fast checks push 제거의 #598 반영

앞선 push 제거는 다른 작업 브랜치에만 반영되어 #598에는 기존 push 트리거가
남아 있었다. `ed1061eb5f6bdc540a70c8a2fbc84071f64388b0`에서
[push40997288026](https://github.com/bin9208/openpilot-rust/actions/runs/40997288026)과
[PR40997291909](https://github.com/bin9208/openpilot-rust/actions/runs/40997291909)가
각각 실행된 원인이었다.

사용자 재확인에 따라 #598에서도 push 트리거를 제거한다. PR 필수 Fast checks와
수동 실행은 유지하고, 기존 frozen event 검사에는 이 push 제거 한 경우만 명시적으로
반영한다. 입력 발행 gate도 더 이상 생성되지 않는 push Fast checks를 기다리지
않도록 수정하되, 정확 source의 PR Fast checks·원래 PR/base receipt와 나머지
발행 필수 검사는 유지한다. push 성공 기록으로 PR 검사 부재·실패를 대신할 수
없음을 회귀로 확인한다. 캐시 변경과 동시에 들어온 native ARM 변경도 보존한다.
