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

## 2026-11-01 중간 증거

- provider 계약 검사 `a2d5f93839fcfc900c0774abe406fe5b1cc922fc`, [Actions 41235682321](https://github.com/bin9208/openpilot-rust/actions/runs/41235682321): 기존·신규 안전성/동등성 16개 통과. 뒤따른 Clippy의 불필요한 참조 1건 때문에 전체 실행은 실패했다. 해당 경고는 `49ec9d63d`에서 수정하고 실제 ARM 비교를 [41235936846](https://github.com/bin9208/openpilot-rust/actions/runs/41235936846)에 요청했다. 성능 목표 달성은 아직 확인하지 않았다.
- recovery 비교 명령과 evidence 업로드를 변경하지 않고 독립 hosted X64 작업으로 이동했다. workspace의 fmt/Clippy/전체 Cargo test/build는 유지한다. 최종 `rust checks`와 입력 공개 gate 모두 recovery 성공 및 별도 원본 PR 실행 영수증을 요구한다.
- PTY EOF와 `Popen.poll()` 사이의 경합을 확인했다. 최초 수정의 원본 파일 변경은 MPC의 전체 원본 contract를 무효화해 되돌렸다. 비교 listener의 명시적 관측 경계에서 해당 자식을 잠금 밖에서 최대 2초 회수하며 실제 종료 코드만 전달한다. 원본 함수 본문·고정 source contract와 native 비교 assertion은 유지하고, 관측 전 `None`과 실제 자식 종료 코드를 별도 기록한다. 0/7을 구별하는 실제 자식 재현을 Actions에 추가했다. 런타임 정책·권한·입력 제한은 바꾸지 않는다.

## 독립 case 실행 구조

- UI는 정책·기하 5개, 픽셀 5개, lifecycle·안전 3개로 나눈다. 각 그룹은 별도 프로세스·Xvfb·DISPLAY·TMPDIR·IPC prefix·출력 경로를 소유한다. 전체 13개가 중복·누락 없이 성공해야 최종 PASS다.
- Card/CAN은 원래 shared·브랜드·CAN 비교의 명령·인수를 그대로 case 목록으로 옮긴다. 실제 affinity와 가용 메모리/cgroup·합산 예상 디스크 증가량으로 동시 수를 정한다. IPC lifecycle은 reference의 완전한 성공 영수증을 읽은 뒤에만 시작한다.
- Cargo/test의 새로운 전역 제한을 추가하지 않는다. 폐지된 25/35 GiB 고정 규칙이 남아 있던 Card 공통 admission과 UI safety의 사전 검사만 작업별 증가량으로 바로잡는다.
- Actions 회귀 검사는 기존 소스의 모든 case/argv와 새 목록을 비교하고, 실제 자식 프로세스의 동일 임시 파일명·IPC prefix 격리 및 실패 전파를 검사한다. 실제 UI/Card 성공과 병목 개선은 전체 필수 Actions에서 추가 확인해야 한다.

## AGNOS 구성과 encoder 재사용 실패 원인

- 기존 #598의 `44fbc7a34526950063713fd8255b96b0e543f554` [Actions 41222239366](https://github.com/bin9208/openpilot-rust/actions/runs/41222239366)은 encoder 재사용의 `fresh` assertion에서 실패했다. 보존 artifact의 Cargo fingerprint는 `openpilot/system/hardware/comma/agnos.json`의 원래 dangling symlink를 매번 missing으로 판정했다. 초기/재사용 encoder 바이트는 같았으나 실제 불필요한 재빌드가 발생했으므로 성공으로 인정하지 않는다.
- logging/loggerd의 build script는 사라진 tracked 입력의 기존 부모를 관측하도록 고친다. Git HEAD/index/ref와 실제 소스 변경 감지는 유지한다. 실제 Cargo 재사용 검사에 dangling symlink와 링크 변경을 포함한다. `fresh` assertion을 제거하거나 재시도하지 않는다.
- AGNOS는 공통 준비·encoder·seed·각 구성요소·model·최종 layout/production의 시간과 실패를 기록한다. seed 준비 이후 독립 bootstrap 도구 증명과 구성요소를 같은 작업에서 병행한다. bootstrap의 기존 네 경우/13개 도구·원래 argv/검증 조건은 유지한다. 합산 memory/disk/affinity admission이 부족하면 순차 실행한다. bootstrap의 별도 target/store와 출력만 사용하며 두 그룹 성공 후 최종 단계에 진입한다.
