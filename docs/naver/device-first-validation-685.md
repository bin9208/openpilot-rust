# 빠른 집중 검사와 실기 수용 중심 검증 전환

2026-11-05 KST. 추적: [#685](https://github.com/bin9208/openpilot-rust/issues/685).

## 문제와 결정

#598은 긴 전체 CI 뒤에도 기존 설치의 생성 파일, 디스크 여유와 메모리 admission 때문에
실제 기기 빌드가 반복 중단됐다. 실제 설치 상태·자원 조건을 빠른 재현에 먼저 반영하고,
수정 브랜치에서 실제 기기 빌드·정상 부팅·변경 동작을 확인한 뒤 병합한다.
몇 시간짜리 전체 workspace/원본 비교/ARM·AGNOS 회귀는 필수 조건에서 제외하고
명시적으로 필요한 경우의 수동 진단으로 보존한다. 실행 중인 다른 작업은 취소하지 않는다.

## 구현

- `focused-validation.yml`: hosted X64, 변경 문법/서식과 선택한 집중 Python/Rust 검사.
  단일 job 15분 timeout, 작은 Rust 명령은 5분 이내로 제한한다. 전체 workspace를 실행하지 않는다.
- `user-docs.yaml`: 기존 짧은 문서 검사. dev-md도 별도 파일로 동일 검사를 실행한다.
- `device acceptance`: 현재 PR HEAD의 사용자 실기 결과 필드를 검사한다. 기기에 접속하지 않는다.
  문서/CI 정책/발행 gate만 변경한 경우 경로와 구체적인 생략 사유를 확인한다.
- 다른 workflow의 push/PR 자동 실행을 제거하고 dispatch/call만 남긴다. 전체 검사 내용을
  삭제하거나 성공으로 바꾸지 않는다. 필요한 산출물 준비·hash/ABI/rollback 보호는 유지한다.
- 입력 발행은 실제 입력 producer/ARM foundation/resource 검사와 정확 PR event 영수증을
  유지하되, 전체 Rust/통합 gate 대신 집중 x86 gate를 확인한다. 입력은 실기 시험의 선행
  준비물이므로 아직 기기에서 시험할 수 없는 입력 자체에 최종 기기 수용을 요구하지 않는다.
- runtime release 발행은 보호된 병합 SHA의 새 짧은 context를 확인한다.
  발행을 위한 전체 회귀를 다시 요구하지 않는다.

## 적용 및 검증

dev 적용은 [PR #686](https://github.com/bin9208/openpilot-rust/pull/686), dev-md 적용은
[PR #687](https://github.com/bin9208/openpilot-rust/pull/687)로 분리한다. 정확 SHA/Actions 결과와
원격 보호 규칙 readback은 [#685의 최신 기록](https://github.com/bin9208/openpilot-rust/issues/685)에 남긴다. dev-md runtime 소스를 dev에 가져오지 않는다. 로컬은 구문·서식·diff만
검사하며 기능 검사는 Actions에서 수행한다. 이 정책/CI 변경은 기기 실행 코드를 바꾸지 않아
기기 반복 시험을 생략한다. #598/#528의 실제 기기 빌드·부팅 수용은 별도이며 미완료다.
