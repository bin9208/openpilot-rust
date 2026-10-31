# dev-md CI 의존성 병목 개선 — #649

추적: [#649](https://github.com/bin9208/openpilot-rust/issues/649).

## 범위와 원래 증거

`dev-md`는 콤마에 설치되는 최종 브랜치이자 최상위 통합 브랜치다. 이번 변경은 별도 `codex/fix-649-dev-md-ci-dag` → `dev-md` PR로만 반영한다. `dev`/`main`으로 가져오지 않는다. 차량 배포·런타임 정책·모델 정밀도·자동 릴리스 범위를 바꾸지 않는다.

기준은 `96c69111e53fbeaa49d629c634830353bb823cf7`이다. 통합 CI의 50개 job 정의는 앞선 job에서 파일·출력을 받지 않으면서 하나의 사슬로 연결돼 있었다. 각 job은 독립 checkout/setup을 수행하며 `download-artifact`나 job output 소비가 없다. 과거 성공 실행의 경과 시간은 10시간 26분 33초였으나 입력이 다른 과거 실행을 개선 완료의 기준으로 삼지 않는다.

동일 제품 소스·기존 환경의 새 기준 실행: [CI 41236691654](https://github.com/bin9208/openpilot-rust/actions/runs/41236691654), 정확 SHA `96c69111e53fbeaa49d629c634830353bb823cf7`. 수동 CI이므로 marked push 전달은 실행되지 않는다. 전체 성공과 대기·순수 실행·재실행 전 공백을 분리해 후보와 비교해야 한다.

## 변경

- source/user-docs/정책 검사가 성공하면 자체 입력을 준비하는 Integration·독립 Rust 모듈·generic ARM 빌드가 병행한다. 서로 다른 실행 환경을 가진 matrix의 고정 직렬 제한도 제거한다. runner 종류·모든 원래 명령·기능 검사·산출물 이름·실패 조건은 유지한다.
- `integration gate`, `rust checks`, `rust aarch64 build`의 전체 성공 집계는 그대로 유지한다. source 검사 `check mapped user docs`를 포함한 서버 필수 검사 네 개는 바꾸지 않는다.
- `runtime-delivery`는 원래처럼 실제 dev-md PR 병합 SHA에 대한 push이고 `[runtime-package]`가 있어야 실행된다. 모든 원래 검사가 성공해야 도달하며 수동 package workflow·입력 공개·production 경계를 유지한다. 이 CI 변경에 marker를 추가하지 않는다.
- package 사전 검사에서 폐지된 `fast checks` 요구만 제거한다. 서버에 실제 적용된 나머지 필수 검사 네 개의 보호 정책 확인은 유지한다. 새 CI 회귀 검사와 이 기록의 정확한 두 경로만 기존 CI 변경 허용 목록에 추가한다.
- 새 SHA별 concurrency와 `cancel-in-progress: false`로 기존 실행을 보존한다. 새 장비·유료 러너·러너 서비스 복구는 포함하지 않는다.

## 완료 조건

- [ ] 새 DAG의 모든 원래 job·step·argv·환경·artifact와 네 gate 보존 검사 성공.
- [ ] 별도 dev-md PR의 전체 검사 성공과 실제 독립 job 동시 실행 확인.
- [ ] 같은 조건의 새 기준 실행과 후보의 순수 실행·배정 대기·전체 경과 비교.
- [ ] 네 필수 gate와 성공 후 marked 전달·수동 경계 보존 확인.

로컬은 YAML/Python 구문 및 diff만 확인한다. 기능·성능·ARM 검증은 GitHub Actions에서 수행한다. 성능 목표 미달이나 필수 검사 실패가 있으면 완료 처리하지 않는다.
