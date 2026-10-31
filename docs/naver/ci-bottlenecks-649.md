# 검증을 유지하는 CI 병목 개선 — #649

추적: [#649](https://github.com/bin9208/openpilot-rust/issues/649), [#598](https://github.com/bin9208/openpilot-rust/issues/598), [#600](https://github.com/bin9208/openpilot-rust/issues/600), [#605](https://github.com/bin9208/openpilot-rust/issues/605).

## 현재 소스와 우선순위

- 2026-11-01 사용자가 이 작업을 우선하고 다른 세션의 기존 #598 구현을 보류했다.
- 기준 소스는 PR #611의 `44fbc7a34526950063713fd8255b96b0e543f554`다. 기존 입력과 encoder fresh 회귀 수정은 보존한다.
- 별도 작업 트리의 `codex/fix-649-ci-bottlenecks`에서 구현한다. 기존 체크아웃의 미커밋 수정과 실행 중인 Actions는 보존한다.
- `bin-actions2/3`는 사용자 정비 중이다. 러너 복구·서비스 시작·기기 접속을 하지 않는다. 온라인 복구는 사용자가 직접 한다.
- dev-md는 콤마 설치용 최상위 통합 브랜치다. CI DAG 변경은 별도 dev-md PR로 검증하며 dev/main으로 역통합하지 않는다.

## 구현과 검증 상태

- [ ] 같은 원본의 15개 provider를 한 번 순회하고 검증된 작은 component를 재사용한다.
- [ ] 기존 단일 provider의 descriptor/archive/tree와 동등성을 Actions에서 확인한다.
- [ ] 실제 같은 입력·러너에서 cold 12분 이하, warm 3분 이하를 확인한다.
- [ ] recovery를 독립 검사로 옮기고 공개 gate에 같은 source/base/실제 execution 증거를 결속한다.
- [ ] PTY 종료의 원래/네이티브 실패를 정확히 재현하고 재시도/허용값 완화 없이 수정한다.
- [ ] AGNOS 단계 계측과 자원 admission 기반 독립 그룹, UI/Card case 병렬화를 확인한다.
- [ ] dev-md의 불필요한 직렬 needs와 오래된 fast checks 요구를 정리하고 기존 4개 gate·전달 조건을 검증한다.

로컬은 Rust 서식·구문과 diff만 확인한다. 기능·빌드·성능·ARM/AGNOS 결과는 Actions로 확인한다. 구현이나 실행 등록만으로 성공 처리하지 않는다.

## Provider batch 인터페이스

`build-runtime-ffmpeg --export-released-provider-batch --plan ... --plan ... --source-root ... --cache ... --store ... --output ...`는 같은 원본의 profile을 일괄 준비한다. 기존 단일 명령은 유지한다.

batch의 profile별 Export는 기존 필드를 유지하며 `origin`은 작은 component에서 원본 manifest/provenance와 현재 plan을 검증한 prefix다. `origin.asset`은 실제 소비한 component 파일의 영수증이며 원본 전체를 새로 내려받았다고 주장하지 않는다. 원본 전체의 SHA와 실제 순회·전송량은 batch 최상위에 따로 기록한다.

캐시 적중에도 작은 archive의 전체 멤버·크기·mode·alias·manifest를 검사하고 현재 source contract를 다시 확인한다. 저장 영수증을 기대값의 근거로 삼지 않는다. 기존 프로젝트 실행 파일은 선택하지 않으며 source-bound MPC/Panda/DBC/eGPU 입력에는 기존 계약을 유지한다.
