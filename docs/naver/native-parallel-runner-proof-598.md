# #598 병렬 runner의 발행 검증 결속

- 작업: [#598](https://github.com/bin9208/openpilot-rust/issues/598)
- 기준 source: `fb33b6503bfd74b75e262d7148505b1381e8f8da`
- 2026-10-29 사용자가 GitHub-hosted Actions와 `bin-actions`, `bin-actions2` 병렬 사용을
  명시적으로 허용했다. 이전 `bin-actions` 단독 지침보다 이 승인이 우선한다.

## 변경 범위

일반 PR의 workspace·integration·fast·docs gate는 공통 `tools/ci/runner_policy.py`의
API job 정책으로 판정한다. 승인된 두 self-hosted runner와 `ubuntu-24.04`,
`ubuntu-24.04-arm` hosted labels를 허용한다. Genuine·foundation·kernel gate와
선택한 실제 producer 및 publisher 실행은 두 self-hosted runner로 제한한다.
Workflow 라우팅과 공통 정책 helper는 다른 담당자의 변경을 통합한다.

PR validation context는 기존 schema와 run·attempt·job·source·base·merge object를
그대로 유지한다. 생성 시 실제 Actions 환경을 검사하고, 발행 시 독립적으로 조회한
해당 run의 실제 proof job 한 개와 보관된 `runner_name`, `runner_environment`를 대조한다.
job 이름·완료·성공과 API labels 정책도 함께 확인한다. 따라서 보관 JSON에
`github-hosted`만 적거나 다른 허용 runner의 이름으로 바꾸어 통과시킬 수 없다.

Source SHA, 동일 PR·base 교집합, Git merge object의 전체 bytes, artifact digest·소속·
만료, 전체 입력 파일의 SHA·크기, 필수 job 이름과 결론은 변경하지 않았다.

## 회귀와 검증 한계

기존 source 귀속 fixture에 실제 API labels를 추가하고 다음 회귀를 보강했다.

- hosted X64·ARM64 및 `bin-actions2`의 일반 검사 허용과 불변 context 결속
- 다른 self-hosted 이름, 누락·잘못된 labels, 실제 runner와 context 이름 불일치 거부
- self-hosted job을 hosted context로 위장하는 경우 거부
- native producer gate와 publisher 실행의 self-hosted 제한 유지
- 실제 API-pinned context artifact를 읽는 경로에서도 runner 이름 불일치 거부

로컬에서는 변경 Python 세 파일의 AST와 `git diff --check`만 확인한다.
공통 helper는 부모 통합 시 연결되며, 이 브랜치에는 중복 구현하지 않는다.
기능 테스트·빌드·CI dispatch·취소·원격 push·발행은 실행하지 않는다.
기능 성공은 부모 통합 후 정확한 source의 Actions 결과로 확인해야 한다.

2026-10-29 추가 사용자 결정으로 `bin-actions3`도 실제 등록 API에서 확인해 같은
self-hosted 허용 목록과 양성 receipt 회귀에 포함했다. 미등록 이름을 거부하는 음성
회귀는 고정된 미등록 fixture 이름으로 유지한다. #630 전용 실제 job은 중단하지 않으며
공통 CI의 무거운 검사는 세 자체 runner pool을 우선 사용한다.
