# dev-md: 이어서 할 작업 (2026-11-11 기준)

다른 사람이나 다른 세션이 이어받을 수 있도록 현재 상태와 남은 일을 적는다.
기능 누락 목록은 [md-upstream-port-gaps.md](md-upstream-port-gaps.md)에 있고, 이 문서는 그것을 포함한 전체 할 일의 순서와 맥락을 다룬다.

## 목표

1. 원본 `ajouatom/openpilot:carrot-wip`의 기능을 Rust판에 **빠짐없이 같게** 옮긴다.
2. 그 뒤에 성능을 최적화한다.
3. 마지막에 원본에 PR을 낸다.

완료 기준은 [AGENTS.md](../AGENTS.md)대로 실제 기기에서의 동작 확인이다. 컴파일과 테스트 통과는 완료가 아니다.

## 기준

| 항목 | 값 |
| --- | --- |
| 작업 브랜치 | `bin9208/openpilot-rust:codex/md-upstream-sync` (base `dev-md`) |
| 이 문서 작성 시 HEAD | `6446918b1` |
| 원본 비교 기준 | `carrot-wip` `1032f8641` (2026-11-10) |
| 이식 기록상 원본 기준 | `0436c7f0a` (`rust/upstream/carrot-wip.json`, `coverage_complete: false`) |
| 시험 기기 | comma 3X, 차량 HYUNDAI_IONIQ_5_PE |

기기 접속 방법과 로그 위치는 저장소에 적지 않는다. 작업자 쪽 메모를 본다.

## 이번 작업에서 확인한 사실: "옮겨진 기능"도 원본과 다르다

기능이 통째로 없는 것 외에, 옮겨진 코드가 원본과 다르게 동작하는 곳이 실제 주행에서 계속 나왔다.
전부 원본 Python과 한 줄씩 대조해서 찾았고, Rust 쪽 주석과 테스트는 틀린 동작을 "원본과 같다"고 적고 있었다.

| 커밋 | 차이 | 확인 수준 |
| --- | --- | --- |
| `092d7d7a3` | 이미 해제된 USB 장치를 다시 해제하면 `jetlinkd`가 종료됨. 추론 실패 사유 세 가지를 한 문구로 뭉쳐 찍음 | 차량에서 확인 |
| `e4b272a31` | 원본은 정차 중에만 모델 요청을 반납하는데 항상 반납함. 폰이 `NOT_READY`로 모든 프레임 거부 | 차량에서 확인 |
| `3b37fcabf` | 원본은 폰 응답을 3초 기다리는데 50ms를 넘으면 연결 전체를 끊음 | 차량에서 확인 |
| `10c5f287f` | 원본 modeld의 프레임 대기는 150ms인데 50ms로 구현됨 | 차량에서 확인 |
| `6446918b1` | 원본은 카메라 프레임 누락 때 폰 모델을 재시작하지 않는데, 누락마다 이력을 초기화함(분당 300회 이상). 조향 떨림의 원인으로 판단 | **미확인. 주행 필요** |
| `c3ade0d12` | 기기 빌드가 모델 파일에 skip-worktree를 붙여 다른 브랜치로 전환이 거부됨 | 기기에서 양방향 전환 확인 |
| `13971857d` | 로그 업로드 기본 목적지 세 곳이 원본 프로젝트 서버였음 | 기기에서 확인 |

**이어받는 사람에게:** "원본과 같다"는 주석·테스트 이름·커밋 메시지를 믿지 말고 원본 파일을 직접 읽는다.
특히 원본의 조건문(`IsOffroad and not IsOnroad` 같은 것)과 상수(대기 시간)가 빠지거나 바뀐 곳이 많았다.

## 할 일, 우선순위 순

### 1. Jetlink (폰 빅모델): 주행 가능한 상태로 만들기

- [ ] `6446918b1`을 차량에서 확인한다. 폰 모델 사용 중 직진 조향 떨림이 없어졌는지, rlog의 `camera dropped … frames` 횟수와 `drivingModelData.jetlink` 상태로 본다.
- [ ] 폰 왕복 시간이 50ms를 넘으면 modelV2가 16Hz 아래로 내려가 `commIssue`가 뜬다(주행 `00000ca1` 1번 구간에서 201회, 인게이지 해제). 폰 발열이 주원인으로 보이며 사용자가 쿨러를 달 예정이다. 그 뒤 다시 잰다.
- [ ] 주행 중 USB Type-C 연결이 물리적으로 끊긴 뒤 복구되지 않았다(커널 `USB Type-C disconnect`). 원인 미확인. 폰이 부팅 때부터 꽂혀 있으면 콤마가 폰에서 전류를 끌어 쓰는데(책상에서 300–450mA 측정), 원본은 포트가 비어 있을 때만 입력 전류를 끊는다. 전원 공급형 USB3 허브 사용 여부와 함께 확인한다.
- [ ] 포트 누락 A14: 원본은 modeld와 데몬 사이 영상을 공유 메모리로 넘긴다(`shared_input.py`). Rust는 프레임마다 소켓으로 복사한다.
- [ ] 원본은 폰 모델이 활성일 때 내부 모델을 돌리지 않는다. Rust는 매 프레임 둘 다 돌린다. 부하와 동작 차이를 확인한다.

### 2. 캐스퍼 EV 인게이지 (`f6db3f2fe`)

- [ ] 시험자의 rlog를 받아 확인한다. 아직 차량 확인이 없다.
- [ ] 원본 `7d337b054`(2026-11-09)가 넣은 `pandaCanFault`·`stockSccFault` 차단을 완화한 상태다. 원본과 다르게 만든 것이므로, 원본에 PR을 낼 때 이 판단을 따로 설명하거나 원본 쪽 수정으로 제안해야 한다.
- [ ] 카메라 FCA11 오류 표시의 근본 원인은 모른다. 원본 조사 보고서 `docs/casper_scc_input_20261109.md`(원본 저장소)의 가설(CAN 버스 1번 오류와의 연관)은 코드상 근거가 없었다.

### 3. 기능 누락 메우기

[md-upstream-port-gaps.md](md-upstream-port-gaps.md)의 항목을 아래 순서로 진행한다. 각 항목은 원본 코드를 읽고, 옮기고, 기기에서 확인한 뒤에 닫는다.

1. **B 항목 중 주행에 영향이 있는 것:** B01(테슬라 속도 송신 조건), B04·B05(eGPU 출력 복구와 오류 분류), B13(설정 보정)
2. **시험 차량에 해당하는 A 항목:** A01–A04(자동 방향지시등 일체), A14(Jetlink 입력 경로), A13(모델 라이브러리)
3. **화면·관측:** B06–B10, B15, B16
4. **다른 차종과 부가 기능:** A05(페달), A06·A07(테슬라), A08·A09(Bluetooth·B1), A10–A12(지도·내비·음성 비서)
5. **C 항목의 설정 15개**는 위 기능을 옮기면서 같이 연결한다.

그 문서는 정적 조사라서 누락의 전부가 아니다. 이미 옮겨진 모듈도 위 표처럼 원본과 대조해야 한다. 대조가 끝난 모듈은 이 문서에 적어 둔다.

| 모듈 | 대조 상태 |
| --- | --- |
| `jetlink` 클라이언트·데몬·modeld 연결 | 연결·추론·대기 시간·프레임 누락 처리까지 대조함. 공유 입력, AIO, 디스플레이·HUD 송신, Wi-Fi 프로비저닝은 안 봄 |
| `selfdrived` 차량 결함 판정 | 대조함 (의도적으로 완화) |
| 나머지 전부 | 안 봄 |

### 4. 기기에서 본 미해결 현상

- [ ] tmux에 `Failed to set gpio 49 value: Operation not permitted`가 계속 찍힌다.
- [ ] `qRoadEncodeData: dropping frame waiting for audio initialization, queue is too large`가 주행 중 반복된다.
- [ ] 주행 상태 진입 때 `qcomgpsd`가 종료 코드 1로 죽는 경우가 있다.
- [ ] `9a153cabb`의 `loggerd` 압축 레벨·`camerad` 노출 샘플링 변경은 효과를 재지 못했다. `pandad` CPU 28%의 원인도 프로파일링하지 못했다.
- [ ] 외부 클러스터 화면은 15fps에서 코어 1개의 66–70%를 쓰고, 대부분이 JPEG 압축(프레임당 33–39ms)이다.
- [ ] 책상 위에서는 차량 CAN이 없어 `card`가 대기하고 modeld가 추론하지 않는다. jetlink 합류는 차에서만 재현된다.
- [ ] 책상 전원(5V)으로 주행 상태를 켜면 기기가 전압 부족으로 꺼진 적이 있다(PMIC UVLO).

### 5. 정리

- [ ] 시험 기기의 `/data/openpilot-native`(19GB)는 쓰는 곳이 없다. 삭제는 사용자가 한다.
- [ ] 원본 빌드를 돌리고 Rust로 돌아오면 추적 파일 6개(글꼴·DBC)가 수정됨 상태로 남는다. Rust 쪽에서 무시하거나 복원할지 정해야 한다.
- [ ] 이 PC에서 `carrot-recovery` 테스트 7건과 `tombstoned` 1건은 보조 실행 파일이 없어 실패한다. `carrot-server` 테스트는 라이브러리 경로 문제로 실행되지 않는다.
- [ ] `rust/crates/selfdrived/data/alerts.json`은 `openpilot/selfdrive/selfdrived/events.py`에서 생성하는 파일이다. 이번에는 둘을 손으로 맞췄다. 생성기(`rust/tools/generate_selfdrive_alerts.py`)로 다시 만들어 차이가 없는지 확인한다.

## 작업 규칙 (이번에 정한 것)

- 강제 주행 스위치(`/tmp/openpilot_bench_ignition`)는 사용자가 요청할 때만 켠다. 대기 중인 백그라운드 명령 안에 넣지 않는다.
- DM 숨김 스위치(`/data/disable_dm_for_testing`)는 시험자 본인용이다. 설정 항목으로 만들지 않는다.
- 기기를 갱신하기 전에 정차 여부를 확인한다. 받은 직후 `sync`한다. 빌드가 끝난 것을 확인한 뒤에 다음 갱신을 한다.
- 안전 판정을 완화하는 변경과 기기의 파일 삭제는 사용자 승인 후에 한다.

## 주행 로그에서 jetlink 상태 보는 법

`rlog`의 `drivingModelData.jetlink`에 프레임마다 `source`(native/jetlink), `phase`(ACTIVE/PREPARING/LOST), `executionMs`가 있다.
구간별로 jetlink 프레임 수, 전환 횟수, `executionMs` 분포를 세면 폰 모델 사용 비율과 지연을 알 수 있다.
같은 구간의 `onroadEvents`에서 `commIssue`, 로그 메시지에서 `camera dropped … frames`를 같이 본다.
`jetlinkd`의 실패 사유는 rlog에 없고 tmux 로그와 `/dev/shm/carrot-jetlink.json`, `/dev/shm/carrot-jetlink-model.json`에만 남는다.
