# dev-md: 이어서 할 작업 (2026-11-11 저녁 기준)

다른 사람이나 다른 세션이 이어받을 수 있도록 현재 상태와 남은 일을 적는다.
관련 문서는 세 개다. 이 문서는 할 일의 순서와 맥락, 이어받는 방법을 다룬다.

| 문서 | 내용 |
| --- | --- |
| 이 문서 | 할 일의 순서, 기기 상태, 이어받는 방법, 주행에서 본 현상 |
| [md-upstream-port-gaps.md](md-upstream-port-gaps.md) | 기능 누락 목록 A01–A14, B01–B16, 설정 15개 (원본 `1032f8641` 기준) |
| [md-current-state-review-20261111.md](md-current-state-review-20261111.md) | Codex가 쓴 현재 상태 전수 조사. 새 결함 F01–F06, 원본 갱신분 U01–U03, 누락 목록의 현재 판정 (Rust `e8f9f91d8`, 원본 `bc8eb4c2e` 기준) |

## 목표

1. 원본 `ajouatom/openpilot:carrot-wip`의 기능을 Rust판에 **빠짐없이 같게** 옮긴다.
2. 그 뒤에 성능을 최적화한다.
3. 마지막에 원본에 PR을 낸다.

완료 기준은 [AGENTS.md](../AGENTS.md)대로 실제 기기에서의 동작 확인이다. 컴파일과 테스트 통과는 완료가 아니다.

## 기준

| 항목 | 값 |
| --- | --- |
| 작업 브랜치 | `bin9208/openpilot-rust:codex/md-upstream-sync` (base `dev-md`, PR #695) |
| 이 문서 갱신 시 코드 HEAD | `625361a7a` |
| 원본 비교 기준 | `carrot-wip` `bc8eb4c2e` (2026-11-11 14:34). 누락 목록 문서는 `1032f8641` 기준이고 그 뒤 12커밋은 아래 "원본 갱신분" 참고 |
| 이식 기록상 원본 기준 | `0436c7f0a` (`rust/upstream/carrot-wip.json`, `coverage_complete: false`). 갱신되지 않는 옛 기록이다 |
| 시험 기기 | comma 3X, 차량 HYUNDAI_IONIQ_5_PE |
| **기기에 올라가 있는 SHA** | **`eb70376eb`** (2026-11-11 17:56에 갱신 시작, 18:06 현재 빌드 중). 그 뒤 커밋(`bc615e62f`부터)은 기기에 없다 |

기기 접속 방법과 로그 위치는 저장소에 적지 않는다. 작업자 쪽 메모를 본다.

## 이 PC에서 이어받기

- 주 체크아웃은 `/home/fma1/rust/openpilot-rust`(브랜치 `dev`)이고, 이 작업 브랜치는 별도 worktree에 있다.
  `git worktree list`로 위치를 본다. 지금 worktree는 `/tmp` 아래라 재부팅하면 사라진다.
  사라졌으면 `git worktree prune` 뒤 `git worktree add <경로> codex/md-upstream-sync`로 다시 만든다. 커밋은 전부 push돼 있다.
- 원본은 주 체크아웃의 원격 추적 ref `carrot/carrot-wip`다. `carrot`이라는 remote 설정은 없다.
  갱신: `git fetch --no-tags https://github.com/ajouatom/openpilot carrot-wip:refs/remotes/carrot/carrot-wip`.
  원본 파일 읽기: `git show carrot/carrot-wip:<경로>`. 공통 조상은 `43203371004e`.
- 빌드·테스트는 `rust/`에서 한다.

  ```sh
  export CARGO_TARGET_DIR=/home/fma1/rust/cache/cargo-target
  cargo test --offline -q -p openpilot-card
  # carrot-server 테스트는 FFmpeg 라이브러리 경로가 있어야 실행된다
  LD_LIBRARY_PATH=/home/fma1/rust/cache/native-sdk/usr/lib/x86_64-linux-gnu cargo test --offline -q -p openpilot-carrot-server --lib
  # xiaoge의 실제 실행 코드는 native-skip-miri 기능과 호스트용 OpenCV가 있어야 컴파일된다
  OPENPILOT_OPENCV_ROOT=/home/fma1/rust/cache/opencv-signal-host \
  LD_LIBRARY_PATH=/home/fma1/rust/cache/native-sdk/usr/lib/x86_64-linux-gnu:/home/fma1/rust/cache/opencv-signal-host/install/lib \
    cargo test --offline -q -p openpilot-xiaoge --features native-skip-miri --lib
  # card의 주행 경로(core/step.rs 등)는 native 기능으로 컴파일된다
  cargo test --offline -q -p openpilot-card --features native
  ```

  `xiaoge`의 `inference::session` 테스트 1건은 모델 경로 환경변수가 없어 이 PC에서 실패한다(원래부터).

- `cargo fmt -p openpilot-card`는 `brands/hyundai/parser_inputs.rs`를 건드린다(원래 서식이 안 맞는 파일). 커밋에 넣지 말고 되돌린다.
- `carrot-server` 테스트는 worktree 루트에 `.omo/evidence/`를 만든다. 추적하지 않는 산출물이다. 커밋하지 않는다.
- **같은 worktree에서 Codex와 Claude가 동시에 작업한 적이 있다.** `git add`는 자기 파일 경로만 지정하고, 커밋 전에 `git status`를 본다.

## 확인한 사실: "옮겨진 기능"도 원본과 다르다

기능이 통째로 없는 것 외에, 옮겨진 코드가 원본과 다르게 동작하는 곳이 계속 나왔다.
전부 원본 Python과 한 줄씩 대조해서 찾았고, Rust 쪽 주석과 테스트는 틀린 동작을 "원본과 같다"고 적고 있었다.

| 커밋 | 차이 | 확인 수준 |
| --- | --- | --- |
| `092d7d7a3` | 이미 해제된 USB 장치를 다시 해제하면 `jetlinkd`가 종료됨. 추론 실패 사유 세 가지를 한 문구로 뭉쳐 찍음 | 차량에서 확인 |
| `e4b272a31` | 원본은 정차 중에만 모델 요청을 반납하는데 항상 반납함. 폰이 `NOT_READY`로 모든 프레임 거부 | 차량에서 확인 |
| `3b37fcabf` | 원본은 폰 응답을 3초 기다리는데 50ms를 넘으면 연결 전체를 끊음 | 차량에서 확인 |
| `10c5f287f` | 원본 modeld의 프레임 대기는 150ms인데 50ms로 구현됨 | 차량에서 확인 |
| `6446918b1` | 원본은 카메라 프레임 누락 때 폰 모델을 재시작하지 않는데, 누락마다 이력을 초기화함(분당 300회 이상). 조향 떨림의 원인으로 판단 | **기기에 안 올라감. 주행 필요** |
| `c3ade0d12` | 기기 빌드가 모델 파일에 skip-worktree를 붙여 다른 브랜치로 전환이 거부됨 | 기기에서 양방향 전환 확인 |
| `13971857d` | 로그 업로드 기본 목적지 세 곳이 원본 프로젝트 서버였음 | 기기에서 확인 |
| `84a93d14d` | 테슬라: 차량 버스가 있으면 `State::new`가 실패함(DBC에서 사라진 메시지 등록). 속도 송신이 `canValid`·센서 무효·미래 시각을 보지 않음. 측정값 유효성 판정, 단위 판정, 디지털 페달, TPMS 조건이 원본과 다름 | PC 테스트만. 테슬라 차량 없음 |
| `e8f9f91d8` | 설정 저장 때 `nonzero_min` 보정이 없음(`CenterToFrontRatio` 1–30이 그대로 저장됨). 변경 출처 `voice_assistant` 미등록 | PC 테스트만 |
| `0af3992cc` | 원본 `df90d3083`(11-11) 반영: 최종 정차 요청 중 멀어지는 선행차(`HUD_LEAD_INFO` 1)도 2로 보냄 | PC 테스트만. **시험 차량 해당, 주행 필요** |
| `63ee82761` | 동작 변경 없음. 느린 폰 프레임 로그에 단계별 시간을 추가 | PC 테스트만 |
| `a4a961a1f` | 리모컨 명령 파일에 명령이 둘 이상이면 첫 번째만 전달됨(`507625e05`의 캐시가 원인) | PC 테스트만 |
| `90874d7d1` | 폐기된 `CenterToFrontRatioEnabled`가 꺼져 있던 기기에서 저장된 비율이 그대로 적용됨(원본은 시작 때 0으로 돌리고 스위치를 지움) | PC 테스트만 |
| `eb70376eb` | `procLog`에 `memPrivateDirty`가 항상 0 (B15) | PC 테스트만 |
| `bc615e62f` | 폰 모델이 응답하는 프레임에도 내부 모델을 매번 실행함(F04). 원본처럼 건너뛰고, 폰 프레임을 잃으면 내부 모델을 초기화해 같은 프레임에 답함 | **주행 경로 변경. 주행 필요** |
| `30e4c168b` | `signalcolord`가 manager에 등록돼 있지 않아 `SignalAssistMode`를 켜도 관측기가 뜨지 않음(F01) | PC 테스트만 |
| `4e3d56ef8` | `OnnxBsdEnabled`를 읽지 않아 폴리곤이 설정돼 있으면 비전 BSD 추론이 항상 돎(B02). 원본 기본값은 꺼짐 | PC 테스트만 |
| `625361a7a` | 원본 `ec79cae36`의 차량 쪽: `carState.cruiseCancelState` 추가, CANCEL 래치 중 방향지시등 요청과 계기판 차선변경 램프 유지를 중단 | PC 테스트만. **시험 차량 해당, 주행 필요** |

**이어받는 사람에게:** "원본과 같다"는 주석·테스트 이름·커밋 메시지를 믿지 말고 원본 파일을 직접 읽는다.
특히 원본의 조건문(`IsOffroad and not IsOnroad` 같은 것)과 상수(대기 시간)가 빠지거나 바뀐 곳이 많았다.
성능을 위해 넣은 변경(`507625e05`의 캐시, `20d75c50d`의 노출 격자)도 원본과 동작이 달라진 곳이 있다.

## 할 일, 우선순위 순

### 1. Jetlink (폰 빅모델): 주행 가능한 상태로 만들기

- [ ] 기기를 현재 HEAD로 올리고 `6446918b1`을 차량에서 확인한다. 폰 모델 사용 중 직진 조향 떨림이 없어졌는지, rlog의 `camera dropped … frames` 횟수와 `drivingModelData.jetlink` 상태로 본다.
- [ ] 같은 주행에서 tmux 로그의 `Jetlink slow frame N: T ms; warp …, handoff …, roundtrip …, parse …`를 모은다(`63ee82761`). `roundtrip`이 대부분이면 폰·USB 쪽이고, `warp`나 `handoff`가 크면 콤마 쪽이다.
- [ ] 폰 왕복 시간이 50ms를 넘으면 modelV2가 16Hz 아래로 내려가 `commIssue`가 뜬다(주행 `00000ca1` 1번 구간에서 201회, 인게이지 해제). 폰 발열이 주원인으로 보이며 사용자가 쿨러를 달 예정이다. 그 뒤 다시 잰다.
- [ ] 주행 중 USB Type-C 연결이 물리적으로 끊긴 뒤 복구되지 않았다(커널 `USB Type-C disconnect`). 원인 미확인. 폰이 부팅 때부터 꽂혀 있으면 콤마가 폰에서 전류를 끌어 쓰는데(책상에서 300–450mA 측정), 원본은 포트가 비어 있을 때만 입력 전류를 끊는다. 전원 공급형 USB3 허브 사용 여부와 함께 확인한다.
- [ ] 포트 누락 A14: 원본은 modeld와 데몬 사이 영상을 공유 메모리로 넘긴다(`shared_input.py`). Rust는 프레임마다 소켓으로 복사한다.
- [ ] 원본은 폰 모델이 활성일 때 내부 모델을 돌리지 않는다. `bc615e62f`에서 원본처럼 바꿨다(검토 문서 F04). 주행으로 확인한다: 폰 모델 사용 중 콤마 발열·부하, 그리고 정차 상태에서 폰을 뽑았을 때 내부 모델로 돌아오는지(이때 `commIssue`가 뜨는 것은 원본과 같은 동작이다).

#### 폰 모델이 느릴 때의 처리: 원본과 대조한 결과 (2026-11-11)

사용자 질문: 폰 쪽 지연은 최대 69ms 정도였고 20→16프레임으로 떨어졌을 뿐인데 인게이지가 풀리는 것이 맞는 처리인가.

원본 코드로 확인한 것:

- `selfdrived`의 modelV2 주파수 하한은 20Hz × 0.8 = 16Hz다(`cereal/messaging/__init__.py` `FrequencyTracker`). 평균이 그 아래면 `commIssueAvgFreq`가 뜬다. Rust도 같은 0.8을 쓴다(`messaging/src/frequency.rs`).
- 원본 modeld는 카메라 프레임이 하나라도 누락된 프레임의 `cameraOdometry.valid`를 false로 낸다(`fill_model_msg.py` `fill_pose_msg`). 왕복 63ms면 프레임의 16–19%가 누락되므로 `locationd` 입력이 그만큼 무효가 된다. 주행에서 본 `locationdTemporaryError`·`commIssue`와 같은 경로다.
- 따라서 **폰 왕복이 50ms를 넘는 동안 인게이지가 풀리는 것은 원본도 같을 것으로 판단한다.** 같은 폰·같은 발열 상태에서 원본을 돌려 확인한 것은 아니다.

Rust가 원본과 다른 부분(지연에 얼마나 기여하는지는 재지 못했다):

| 항목 | 원본 | Rust |
| --- | --- | --- |
| 폰 모델 활성 중 내부 모델 | 실행하지 않음. 실패하면 내부 모델 상태를 초기화하고 돌아감 | `bc615e62f`부터 원본과 같음. 그 전에는 매 프레임 실행(같은 GPU에서 워프와 겹침) |
| 폰과의 요청·응답 | modeld 실시간 스레드(FIFO 54, 코어 7)가 직접 함 | `SCHED_OTHER` 작업 스레드가 함 |
| 워프한 영상 전달 | 연결이 소유한 공유 메모리에 바로 씀 | 소켓으로 복사(A14) |

정차 중 초반 수치는 폰 추론 24–30ms, 왕복 중앙값 37–41ms였다. 콤마 쪽 몫은 8–12ms로 추정되며, 폰이 55ms를 넘기면 콤마 쪽을 줄여도 50ms 안에 들어오지 않는다.

마지막 주행 뒤 사용자에게 물었고 답을 받지 못한 것:

1. 주행 중 폰 화면이 꺼져 있었는지, USB 관련 알림이 떴는지.
2. 폰을 콤마에 직접 꽂았는지, 전원이 들어가는 허브를 거쳤는지.
3. 16:17과 16:41쯤의 콤마 재부팅은 사용자가 한 것인지.

### 2. 캐스퍼 EV 인게이지 (`f6db3f2fe`)

- [ ] 시험자의 rlog를 받아 확인한다. 아직 차량 확인이 없다.
- [ ] 원본 `7d337b054`(2026-11-09)가 넣은 차단을 완화한 상태다. 검토 문서 F05에 따르면 `stockSccFault`는 즉시 해제·진입 차단 정의가 빠져 `permanent`만 남았고(캐스퍼로 한정하는 조건 없음), `pandaCanFault`는 정의를 유지한 채 트래픽이 없는 CAN 컨트롤러를 검출에서 제외했다. 원본과 다르게 만든 것이므로, 원본에 PR을 낼 때 이 판단을 따로 설명하거나 원본 쪽 수정으로 제안해야 한다.
- [ ] 카메라 FCA11 오류 표시의 근본 원인은 모른다. 원본 조사 보고서 `docs/casper_scc_input_20261109.md`(원본 저장소)의 가설(CAN 버스 1번 오류와의 연관)은 코드상 근거가 없었다. 원본은 11-11에 캐스퍼 후속 문서 세 개(`ecdd04393`, `19549c7c7`, `7d318b6ed`)를 더 올렸다. 아직 읽지 않았다.

### 3. 기능 누락 메우기

[md-upstream-port-gaps.md](md-upstream-port-gaps.md)의 항목을 아래 순서로 진행한다. 각 항목은 원본 코드를 읽고, 옮기고, 기기에서 확인한 뒤에 닫는다.

1. **B 항목 중 주행에 영향이 있는 것**
   - [x] B01 테슬라 속도 송신 조건: `84a93d14d`. 조건 세 개만이 아니라 `carstate.py`의 측정값 유효성 일체가 빠져 있었다. 기기 확인 없음.
   - [x] B13 설정 보정: `e8f9f91d8`. `nonzero_min`을 쓰는 설정은 `CenterToFrontRatio` 하나뿐이다(누락 목록의 "ATC 거리 등"은 틀림). 기기 확인 없음.
   - [x] B12 폐기 설정 이전: `90874d7d1`. 기기 확인 없음.
   - [x] B02 비전 BSD 켜고 끄기: `4e3d56ef8`. 기기 확인 없음.
   - [x] B15 `memPrivateDirty`: `eb70376eb`. 기기 확인 없음.
   - [ ] B04·B05 eGPU 출력 복구와 오류 분류
2. **시험 차량에 해당하는 A 항목:** A01–A04(자동 방향지시등 일체), A14(Jetlink 입력 경로), A13(모델 라이브러리). A01–A04는 아래 "자동 방향지시등을 옮기기 전에 알아야 할 것"을 먼저 읽는다.
3. **화면·관측:** B06–B10, B15, B16
4. **다른 차종과 부가 기능:** A05(페달), A06·A07(테슬라), A08·A09(Bluetooth·B1), A10–A12(지도·내비·음성 비서)
5. **C 항목의 설정 15개**는 위 기능을 옮기면서 같이 연결한다.

검토 문서의 새 결함:

- [x] F01 `signalcolord` 등록: `30e4c168b`. 시작 조건은 주행 중이고 `SignalAssistMode`가 1 또는 2. 설정을 읽지 못하면 시작하지 않는다.
- [x] F02 리모컨 명령 누락: `a4a961a1f`.
- [ ] F03 첫 설치에서 SDK·자산 압축 해제가 실패하면 화면과 6999 복구가 뜨지 않을 수 있다.
- [x] F04 Jetlink 활성 중 내부 모델 실행: `bc615e62f`. 주행 확인 필요(위 1번).
- [ ] F06 카메라 노출 샘플링이 원본(2×2, 운전자 2×4)과 다르다(전부 8×8). `20d75c50d`에서 성능 때문에 바꾼 것이다. "원본과 같게"가 먼저라면 되돌린다.

원본 갱신분(`1032f8641..bc8eb4c2e`, 12커밋):

| 원본 커밋 | 내용 | 상태 |
| --- | --- | --- |
| `df90d3083` | 최종 CAN FD 정차에서 멀어지는 선행차의 객체 상태 | 반영 `0af3992cc`. 주행 필요 |
| `bc8eb4c2e` | 레이 Comma Pedal 차량은 현대 고정 게인에서 제외하고 설정에 게인을 노출 | A05(페달)와 함께 옮긴다. A05 없이는 조건이 참이 될 수 없다 |
| `ec79cae36` | 크루즈 취소 때 자동 방향지시등 일시 정지, 예측 차선변경 점등 | 차량 쪽(`cruiseCancelState` 게시, 램프 요청·계기판 유지 중단)은 반영 `625361a7a`. 관측기·음성·테슬라 쪽은 A01·A12·A06과 함께 |
| `c8454993d` | P/R 계기판 화면 | B08 |
| `a04187511`, `e56e71473`, `9945cdb9d`, `e12057a89`, `e63b6f360`, `f07617011` | Jetson 성능 진단(`performance_log.py`, `daemon.py`)과 서명된 N16 런타임 배포 정보(`host_release.json`) | 기기 쪽 연결 미반영. Jetson 서버 Python은 옮길 대상이 아니다 |
| `ecdd04393`, `19549c7c7`, `7d318b6ed`, `ed157919a` | 캐스퍼·레이 조사 문서 | 코드 변경 없음 |

#### 자동 방향지시등(A01–A04)을 옮기기 전에 알아야 할 것

원본 코드를 따라가 본 결과, 이 기능은 한 덩어리가 아니라 출력 경로가 둘인 사슬이다.

- `desire_helper.py`의 `_update_params_periodic`: `lamp_mode = AutoBlinkerControl in (1, 2) and not CanfdBlinkerControl`. 경계 접근·수동 회전·모델 desire·모델 경로 관측기(A01)는 **`lamp_mode`일 때만** 켜진다. 즉 CAN으로 점등하는 `CanfdBlinkerControl`을 켜면 이 관측기들은 꺼진다.
- `lamp_mode`의 실제 점등은 Bluetooth 보드(B1, A08·A09)와 `physical-hold.json`(A04)을 거친다. B1이 없으면 A01을 옮겨도 차에서 보이는 것이 없다.
- CAN 점등(A03)은 `openpilot/selfdrive/car/hyundai_spas.py`(273줄)와 `opendbc/car/hyundai/spas.py`(216줄)다. 점등 의도는 별도 Python 작업 프로세스(`tools/jetlink/blink_source.py`, `blink_protocol.py`)가 만들고 card가 그 프로세스와 주고받는다. 주차 상태 시험 모드가 18가지(`CanfdBlinkerTest`) 들어 있다. 차량 ECU에 진단 세션을 여는 코드이므로 한 단계씩 차에서 확인하며 옮겨야 한다.
- `boundary_indicator_supported`는 `HYUNDAI_IONIQ_5_PE`에서만 참이다(원본 테스트 기준). 시험 차량이 유일한 대상 차종이다.
- 원본 분량: `desire_helper.py` 886줄, `desire_lib/` 관측기 5개 약 800줄, `hyundai_spas.py`·`spas.py` 약 490줄, `tools/jetlink/blink_*.py`, 그리고 `card.py`·`controlsd.py`·`hyundaicanfd.py`의 연결. Rust `desire` 크레이트는 레버·차선변경 FSM까지만 있다.

먼저 정해야 할 것: 시험 차량에 B1 보드가 있는지. 없으면 A03(CAN 점등)부터, 있으면 A01·A04·A08·A09부터가 맞다.

#### 공용 스키마가 원본과 어긋나 있다

원본에 PR을 내거나 원본 도구로 Rust판 로그를 읽으려면 먼저 풀어야 한다. 이번에 고치지 않았다.

| 파일 | 차이 |
| --- | --- |
| `openpilot/cereal/log.capnp` 이벤트 이름 | Rust는 `jetlinkLost @126`, `driverMonitorFallback @130`, `teslaTrafficStopWarning @134`. 원본은 `driverMonitorFallback @126`, `teslaTrafficStopWarning @130`. 같은 번호가 다른 이벤트를 뜻한다 |
| `openpilot/cereal/log.capnp` | Rust가 `JetlinkFrameStatus`를 추가해 `DrivingModelData @8`과 다른 구조체 `@27`에 넣었다. 원본에는 없다 |
| `openpilot/cereal/custom.capnp` | Rust는 `naviOwner` 등 10개를 `@33–@42`에 넣고 원본의 `positionFixMonoTime` 등 4개를 `@43–@46`으로 밀었다. 원본은 그 4개가 `@33–@36`이다 |
| `openpilot/cereal/services.py` | Rust에 `youtubeRoadEncodeData` 추가 |
| `openpilot/common/params_keys.h` | 키 집합은 같고 순서와 Rust 전용 키(`Jetlink*`, `OnnxComputeMode`)가 다르다 |
| `opendbc/car/car.capnp` | `625361a7a`로 원본과 같아졌다 |

사용자 문서(`docs/user/`)와 `carrot_settings.json`도 원본과 갈라져 있다. Rust 전용 내용이 섞여 있어 통째로 덮어쓸 수 없다.

누락 목록은 정적 조사라서 누락의 전부가 아니다. 이미 옮겨진 모듈도 위 표처럼 원본과 대조해야 한다. 대조가 끝난 모듈은 여기에 적어 둔다.

| 모듈 | 대조 상태 |
| --- | --- |
| `jetlink` 클라이언트·데몬·modeld 연결 | 연결·추론·대기 시간·프레임 누락 처리까지 대조함. 공유 입력, AIO, 디스플레이·HUD 송신, Wi-Fi 프로비저닝은 안 봄 |
| `selfdrived` 차량 결함 판정 | 대조함 (의도적으로 완화) |
| `card` 테슬라 `state.rs`·`speed_limit.rs` | `carstate.py`·`speed_limit_controller.py`와 대조해 맞춤. 방향지시등 레버(A06)와 교통 신호(A07) 부분은 없음. `steering.rs`·`controller.rs`는 `coop_steering.py`·`carcontroller.py`의 공통 조상 이후 변경분이 들어 있는 것만 확인 |
| `card` 현대 `canfd_acc.rs`의 `camera_acc` | 객체 상태 보정 조건과 원본 `test_stopping.py`의 진입 조건 9가지를 대조함. 함수 전체는 아님 |
| `carrot-server` 설정 쓰기(`/api/params/set`, 인트로 프리셋) | 보정 순서와 변경 출처 목록을 대조함 |
| `desire` `command.rs` | 원본 `CommandReader`와 대조함 |
| `manager` 시작 때의 설정 이전 | `manager_init`의 ATC·geometry 이전 두 개를 대조함. 나머지 초기화 순서는 안 봄 |
| `manager-catalog` 프로세스 목록 | 원본 61개 중 `mapsd`만 없다(A10). 목록 순서는 원본과 다르다(`jetlinkd` 위치 등). 시작 조건은 `signalcolord`만 대조함 |
| `drivingd`의 Jetlink 프레임 경로 | 원본 `JoiningModel.run`과 대조해 내부 모델 생략·실패 시 초기화를 맞춤. 합류 조건(`may_join`)과 워프 준비 과정은 안 봄 |
| `xiaoge` 비전 BSD | `v_asm_server.py`의 켜고 끄기·세대 확인·상태 응답을 대조함. 게이트 계산, 차선 인식, 테슬라 broadcast(B14)는 안 봄 |
| `runtime-core` smaps 수집 | `proclog_smaps.py`와 대조함 |
| `card` 크루즈 CANCEL 래치의 게시와 소비 | `card.py`·`hyundaicanfd.py`의 해당 줄만 대조함. `cruise.py` 전체는 안 봄 |
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
- [ ] 이 PC에서 `carrot-recovery` 테스트 7건과 `tombstoned` 1건은 보조 실행 파일이 없어 실패한다. `carrot-server` 테스트는 위 "이 PC에서 이어받기"의 라이브러리 경로를 주면 실행된다(25건 통과).
- [ ] `rust/crates/selfdrived/data/alerts.json`은 `openpilot/selfdrive/selfdrived/events.py`에서 생성하는 파일이다. 이번에는 둘을 손으로 맞췄다. 생성기(`rust/tools/generate_selfdrive_alerts.py`)로 다시 만들어 차이가 없는지 확인한다.
- [ ] `openpilot/selfdrive/carrot_settings.json`이 원본보다 뒤처져 있다(설명 문구와 `Long KiV` 표기 등 39줄). 원본 `bc8eb4c2e`·`ec79cae36`의 설정 변경과 함께 맞춘다.

## 작업 규칙 (이번에 정한 것)

- 강제 주행 스위치(`/tmp/openpilot_bench_ignition`)는 사용자가 요청할 때만 켠다. 대기 중인 백그라운드 명령 안에 넣지 않는다.
- DM 숨김 스위치(`/data/disable_dm_for_testing`)는 시험자 본인용이다. 설정 항목으로 만들지 않는다.
- 기기를 갱신하기 전에 정차 여부를 확인한다. 받은 직후 `sync`한다. 빌드가 끝난 것을 확인한 뒤에 다음 갱신을 한다.
- 안전 판정을 완화하는 변경과 기기의 파일 삭제는 사용자 승인 후에 한다.
- 기기 접속은 그 대화에서 사용자가 허락한 범위에서만 한다. 세션이 바뀌면 다시 묻는다.

## 기기 갱신 명령

기기에서(SSH 또는 당근웹 터미널), 차가 정차한 상태에서 실행한다.

```sh
sudo systemctl stop comma
cd /data/openpilot && git fetch origin codex/md-upstream-sync && git merge --ff-only origin/codex/md-upstream-sync
sync
git log --oneline -1
sudo systemctl start comma
tail -f /data/openpilot-build/build.log   # "Build complete"가 나올 때까지
```

## 주행 로그에서 jetlink 상태 보는 법

`rlog`의 `drivingModelData.jetlink`에 프레임마다 `source`(native/jetlink), `phase`(ACTIVE/PREPARING/LOST), `executionMs`가 있다.
구간별로 jetlink 프레임 수, 전환 횟수, `executionMs` 분포를 세면 폰 모델 사용 비율과 지연을 알 수 있다.
같은 구간의 `onroadEvents`에서 `commIssue`, 로그 메시지에서 `camera dropped … frames`를 같이 본다.
`jetlinkd`의 실패 사유와 느린 프레임의 단계별 시간은 rlog에 없고 tmux 로그와 `/dev/shm/carrot-jetlink.json`, `/dev/shm/carrot-jetlink-model.json`에만 남는다.
`/dev/shm/carrot-jetlink.json`의 `timings`는 폰이 보고한 추론·대기·합계 시간(µs)이다.
