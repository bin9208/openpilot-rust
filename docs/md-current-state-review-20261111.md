# dev-md 현재 상태 전수 조사 — 2026-11-11

사용자 요청으로 클로드 수정·정리 이후의 코드, 원본과의 차이, 남은 일을 기록한다.
코드를 수정하거나 기기에 접속하지 않았다. 이 문서의 정적 확인을 기기 시험·주행 통과로 해석하지 않는다.

## 1. 기준과 조사 범위

| 항목 | 기준 |
| --- | --- |
| 저장소·작업 브랜치 | `bin9208/openpilot-rust:codex/md-upstream-sync` |
| PR 기준 트랙 | `dev-md`; `dev` 변경은 섞지 않음 |
| 조사 시작 SHA | `40c7807294d04eab772d962e019a85630871bb8f` |
| 조사 중 추가 확인 SHA | `84a93d14d`, `e8f9f91d8`, `0af3992cc`, `63ee82761`, `a4a961a1f` |
| 이 문서의 코드 기준 SHA | **`a4a961a1f690f2a0a71abdc1e47b3d2ba4c99d36`** |
| 원본 | `ajouatom/openpilot:carrot-wip` **`bc8eb4c2e1aac3023c99f3914e657ed9638beab7`**; 조사 시 원격 HEAD 확인 후 fetch |
| 이전 기능 조사 | [md-upstream-port-gaps.md](md-upstream-port-gaps.md): Rust `8dbe08653`, 원본 `1032f8641` 기준 |
| 기존 작업자 인계 | [md-port-handoff.md](md-port-handoff.md): 코드 기준 `6446918b1` |
| 실제 기기 범위 | 인계 문서에 comma 3X / `HYUNDAI_IONIQ_5_PE`로 기록. 이번 조사에서 직접 확인하지 않음 |

조사 도중 다른 세션이 테슬라·설정·CAN FD 정지 보정과 Jetlink 지연 기록을 커밋했고,
이 조사에서 발견한 Bluetooth 묶음 명령 문제도 수정했다. 해당 12개 파일의 추가 차이도 읽어 반영했다.
기준 SHA 이후 변경과 다른 세션의 미커밋 수정은 포함하지 않는다.
기존 문서는 당시 상태를 남기고, 이 기준 SHA의 판정은 이 문서를 참조한다.

조사한 범위는 다음과 같다.

- `rust/crates/`의 Cargo crate 109개, Rust 소스 파일 2,995개를 목록화하고 선언·빌드 대상·프로세스·외부 호출을 검색했다. vendored dependency 소스는 이 개수에서 제외한다.
- 공통 조상 이후 원본 `openpilot/`, `opendbc_repo/opendbc/car/`의 테스트 경로 외 Python 변경 후보 342개를 목록화했다.
- 메뉴 설정 212개, 원본 manager 등록 61개와 Rust 등록 59개, 주요 웹 API와 schema 소비 여부를 대조했다.
- 정상 시작→SDK/자산 준비→컴파일→설치→런처→manager, 업데이트·복구, Jetlink 추론·fallback 경로를 읽었다.
- 삭제 커밋과 후속 수정, 시험 전용 동작, 기존 기능 누락 목록, 작업자 인계의 기기 보고를 구분했다.

**전수 목록·정적 연결 조사이며, 2,995개 파일의 모든 수식·차종·분기를 줄마다 동등 비교한 것은 아니다.**
깊게 읽은 실행 경로와 발견한 차이를 아래에 명시한다. 함수명·테스트명·주석의 '원본과 동일'은 증거로 삼지 않는다.

C/C++ 라이브러리·펌웨어, 모델 준비에 필요한 Python, 외부 Jetson/폰/Android 코드는 언어 통일을 위한
재작성 대상으로 세지 않았다. 이들과 기기 측 Rust 실행 코드의 연결은 조사 대상이다.

## 2. 현재 구조와 정리된 부분

### 자체 빌드와 정상 시작

| 단계 | 현재 코드에서 확인한 동작 | 근거 |
| --- | --- | --- |
| 진입 | ARM64에서 `device_build.sh`를 실행하고 성공하면 `openpilot-launch`로 진입 | [launch_openpilot.sh](../launch_openpilot.sh) |
| 변경 없음 | `rust/.device-built`가 현재 HEAD와 같고 launcher가 있으면 빌드를 건너뜀 | [device_build.sh](../rust/tools/device_build.sh) |
| 입력 | Git에 포함된 분할 압축 SDK·자산을 풀고 ID가 같으면 재사용. 전체 입력 묶음 재검증은 이 경로에 없음 | `device-inputs/MANIFEST`, 같은 스크립트 |
| 컴파일 | offline Cargo release 한 번에 선택된 패키지·바이너리를 빌드. 실패 시 jobs `6→4→3→2`로 재시도 | `device_build.sh`의 `compile`·`for jobs` |
| 자원 | C++는 `flock`으로 직렬화. 컴파일 중 CPU 상한 변경과 compiler의 `oom_score_adj` 조정 | [device_cxx.sh](../rust/tools/device_cxx.sh), `device_build.sh` |
| 설치 | 실행 파일별 임시 복사 후 rename. 마지막에 HEAD stamp 기록. 예전 package 디렉터리·manifest 삭제 | `device_build.sh` 후반 |
| 업데이트 | `openpilot-runtime-install` 위치에 설치한 shell helper가 Git switch/merge를 수행하고 다음 시작에 변경분 빌드 | [device_stage_update.sh](../rust/tools/device_stage_update.sh) |
| 런처 | 자체 빌드 stamp가 있으면 옛 package manifest 전체 검사 대신 stamp 기반 identity 사용. OS·모델 준비 후 manager 시작 | [bootstrap.rs](../rust/crates/runtime-launcher/src/bootstrap.rs) |

빌드 설정에는 66개 `-p`, 88개 `--bin`이 기록돼 있다. 자체 빌드 스크립트가 installer 패키지·바이너리를
제외하므로 실제 선택은 **65개 패키지·87개 바이너리**이며, updater helper는 shell로 별도 설치한다.
Cargo의 자동 `src/main.rs`/`src/bin/` 규칙을 포함해 이름을 대조했으며 정의 없는 선택 이름은 찾지 못했다.
이는 컴파일 성공 판정은 아니다. 선택하지 않은 library crate 상당수는 의존성으로 빌드된다.

### 검증 삭제의 실제 범위

- `79219669a`: 파일 1,523개 변경, 1,474개 삭제, 49개 수정, 195,657줄 삭제.
  다수의 테스트·fixture·검증 문서·upstream 기록과 `disk-space` crate 등이 대상이다.
- `99e182102`: Jetlink 연결 재구성과 함께 추가 테스트·기존 검증 경로 일부를 정리했다.
- 현재 커밋의 `.github/workflows/`에는 `focused-validation.yml`, `user-docs.yaml` **두 개만** 있다.
- 남은 필수 job 이름은 `focused x86 checks`, `device acceptance`, `check mapped user docs`다.
  이번 조사에서 Actions를 실행하거나 결과를 반복 조회하지 않았다.
- 테스트가 전부 사라진 상태는 아니다. 남은 inline 테스트가 있고, 조사 중 추가된 다섯 커밋에도 해당 수정의 테스트가 추가됐다.

**옛 코드가 파일로 남은 것과 정상 시작 때 호출되는 것은 다르다.**
`runtime-installer` transaction, package manifest 검사와 overlay 호환 코드는 남아 있다.
정상 자체 빌드는 stamp로 package 검사를 우회하지만, launcher의 overlay activation은 여전히 호출된다.
과거 staging journal/ready 파일이 있으면 예전 transaction 경로를 탈 수 있다. 모델 준비·산출물 읽기의
검사도 별도로 남는다. 이를 '모든 검증 제거'나 '옛 전체 검증이 매번 실행됨'으로 표현하면 부정확하다.

## 3. 새로 확인한 실행 차이와 결함

아래는 수정 제안서가 아니라 현재 코드에서 확인한 동작이다. 이번 세션에서는 고치지 않았다.

### F01. 신호 관측 실행 파일이 정상 manager에 연결되지 않음

- 원본 `process_config.py`는 `started and requested()` 조건으로 `signalcolord`를 등록한다.
- Rust는 `openpilot-signalcolord` 바이너리와 `signal-observer` 구현을 가지고 빌드 대상에도 넣는다.
  그러나 manager catalog·binding·시작 코드에서 이 프로세스 등록과 정상 호출을 찾지 못했다.
- 원본 61개와 Rust 59개의 등록 차이는 **`mapsd`, `signalcolord`**다. 전자는 이전 A10에 포함됐지만,
  후자는 이전 조사에서 별도 누락으로 기록하지 못했다.
- 영향: 파일로 구현된 신호 관측기가 사용자 설정에 따라 자동 실행되는 연결이 빠져 있다.
  신호 인식·정지 판단 코드 전체가 없다는 뜻은 아니다. 수동 실행과 표준 시작을 구분해야 한다.
- 근거: [manager catalog](../rust/crates/manager-catalog/src/catalog.rs), [observer main](../rust/crates/signal-observer/src/main.rs),
  [build args](../rust/ci/runtime/build-args.json), 원본 `openpilot/system/manager/process_config.py`.

### F02. Bluetooth 묶음 명령 소비 결함 — 조사 중 코드 수정됨

- 발견 시점 `63ee82761`의 `CommandReader::read`는 파일 stamp를 먼저 저장한 뒤 첫 유효 action에서 반환했다.
  다음 호출은 파일이 그대로면 즉시 `None`을 반환했다.
- 한 파일에 아직 처리하지 않은 유효 명령이 둘 이상 있으면, 첫 반환 뒤 남은 명령은 파일이 다시 써지기 전까지
  읽지 않았다. writer의 `prune`도 이벤트 수가 바뀔 때만 다시 쓰므로 정기 재작성에 의존할 수 없었다.
- 원본은 20 ms마다 payload를 다시 읽고 `seen`으로 이미 처리한 것만 제외하므로 같은 파일의 다음 명령을 소비할 수 있다.
- 당시 영향: 묶음 수신 시 크루즈·차선·주의 명령 일부가 지연되거나 TTL 안에 소비되지 않을 수 있었다.
  조건에 따른 정적 결함 확인이며, 기기에서 버튼 누락을 재현한 결과는 아니다.
- **현재 코드 반영:** 다른 세션의 `a4a961a1f`가 파일 끝까지 읽은 경우에만 stamp를 저장하도록 수정했다.
  이 diff와 추가 테스트 코드를 읽었다. 이번 조사에서 테스트를 실행하거나 기기 동작을 확인하지 않았다.
- 근거: [command.rs](../rust/crates/desire/src/command.rs)의 stamp 검사·`return Some(action)`,
  [journal.rs](../rust/crates/bluetooth/src/journal.rs)의 `prune`, 원본 `carrot/bluetooth/model.py`의 `CommandReader`.

### F03. 첫 준비 실패와 빌드 실패 후의 복구 서비스 수명에 빈틈

- `start_ui()`는 spinner 실행 파일이 있어야 recovery도 시작한다.
  새 checkout에 실행 파일이 없다면 첫 호출은 아무것도 하지 않고, SDK를 먼저 풀고 자산을 나중에 푼 뒤 다시 호출한다.
- 따라서 **기존 바이너리가 전혀 없는 첫 설치에서 SDK/자산 압축 해제가 실패**하면 화면·6999 서비스가 준비되지 않을 수 있다.
- `fail()`은 오류 창 호출 후 `stop_ui()`로 자신이 시작한 recovery를 종료하고 exit한다. EXIT trap도 같은 종료를 수행한다.
  오류 창이 떠 있는 동안과 그 창이 끝난 뒤를 구분해야 한다.
- 영향: compile 이전의 6999 실행 경로는 생겼지만, 초기 준비 실패와 실패 후 지속 접근까지 보장된 것은 아니다.
- 근거: [device_build.sh](../rust/tools/device_build.sh)의 `start_ui`(40행), `fail`(56행), SDK 해제(108행), 자산 해제(118행).

### F04. Jetlink가 선택돼도 내부 모델을 매 프레임 실행함

- Rust `drivingd`는 외부 추론을 시작한 뒤 조건 없이 `runtime.infer()`를 실행하고, `external.finish()`가 게시할 결과를 고른다.
- 원본 `JoiningModel.run()`은 외부 결과가 성공하면 바로 반환하며 내부 모델 실행은 그 뒤 fallback 경로에 있다.
- 영향: 활성 외부 모델과 내부 모델을 동시에 계산하는 부하·실행 순서 차이가 있다.
  이것이 발열·지연의 주원인이라는 실측 결론은 이번 조사에서 내리지 않았다.
- 근거: [daemon.rs](../rust/crates/drivingd/src/daemon.rs)의 277행 이후,
  [runtime.rs](../rust/crates/jetlink/src/runtime.rs)의 `finish`, 원본 `modeld/jetlink/model.py`.

### F05. 안전 이벤트 차이는 두 부분을 분리해서 봐야 함

- `stockSccFault`: 원본에는 `immediateDisable`, `noEntry`, `permanent`가 있다.
  Rust 브랜치의 Python 이벤트 정의와 TICI/MICI `alerts.json`에는 **`permanent`만** 남아 있다.
  이 정의에는 캐스퍼 차종으로 한정하는 조건이 없다.
- `pandaCanFault`: immediate-disable/no-entry 정의는 **남아 있다**. 대신 `carried_traffic=false`인 CAN controller를
  fault monitor에서 건너뛰도록 검출 범위가 바뀌었다. 두 이벤트의 차단을 모두 삭제했다고 쓰면 틀리다.
- 영향: 검증 도구 삭제와 별개의 제어 정책 차이다. 기존 인계 문서도 의도적 완화로 기록한다.
  이번 세션에서 이를 승인·복원·확대하지 않았다. 현재 SHA의 캐스퍼 기기 결과도 확인하지 않았다.
- 근거: `f6db3f2fe`, [vehicle_faults.rs](../rust/crates/selfdrived/src/vehicle_faults.rs),
  [health_events.rs](../rust/crates/selfdrived/src/controller/health_events.rs), [alerts.json](../rust/crates/selfdrived/data/alerts.json),
  [브랜치 events.py](../openpilot/selfdrive/selfdrived/events.py).

### F06. 카메라 노출 샘플링은 원본과 다름

- 원본 `camera_qcom2.cc`는 일반 2×2, 운전자 2×4 간격으로 exposure 값을 계산한다.
- Rust `FrameState`는 모든 카메라에 8×8을 사용한다. `86897aeab`에서 잠시 원래 간격으로 돌렸다가
  `20d75c50d`에서 다시 8×8로 바꾼 현재 상태다.
- 영향: 단순 언어 변환 외의 동작 변경이다. 노출 분포·기기 CPU에 대한 효과를 같은 조건에서 확인해야 하며,
  소스 주석의 오차·속도 수치를 이번 기기 검증 결과로 취급하지 않는다.
- 근거: [frame_state.rs](../rust/crates/camerad-runtime/src/frame_state.rs), 원본 `openpilot/system/camerad/cameras/camera_qcom2.cc:243`.

## 4. 이전 기능 누락 목록의 현재 판정

[기존 문서](md-upstream-port-gaps.md)의 원본 경로·상세 설명은 유지한다. 아래는 기준 SHA에서의 변경 상태다.
'남음'은 해당 실행 연결의 누락을 다시 확인했다는 뜻이며, 모든 하위 조건의 동등성 검증을 뜻하지 않는다.

| 기존 ID | 기능 | 현재 상태 |
| --- | --- | --- |
| A01 | 자동 점등 판단 | 남음: 경계·수동 회전·모델 경로/예측 observer 연결 |
| A02 | ATC 거리·BSD·취소 상태 | 남음: 이전 설정 처리는 있으나 실제 소비·메시지 연결 부족 |
| A03 | 확장 SPAS | 남음: 기존 SPAS 송신과 새 세션·복원은 별개 |
| A04 | 물리 유지·음성 점등 | 남음: lease와 최종 차량 출력 연결 |
| A05 | 페달 인터셉터 | 남음: 새 프로토콜·레이 EV 설정·상태·출력 |
| A06 | 테슬라 자동 점등 | 남음. `84a93d14d`의 일반 차량 상태 수정과 구분 |
| A07 | 테슬라 CAN 교통 신호 정지 | 남음 |
| A08 | Bluetooth 점등·빠른 제스처 | 남음. F02의 기존 명령 소비 수정과는 별개 기능 |
| A09 | B1 기기 서비스 | 남음: 등록·인증·감시·출력·업데이트 연결 |
| A10 | 공유 지도·내비 API | 남음: `mapsd` 등록도 없음 |
| A11 | 내비 모델 입력 | 남음: `nav_map`, `navigation`, 경로 ID, 계기판 모드 7 |
| A12 | 음성 비서 API·기기 명령 | 남음 |
| A13 | 외부 모델 라이브러리 | 남음: 원본 기본 N16 Nav, Rust 기본 MD v3와 선택 관리 차이 |
| A14 | Jetlink 추가 입력·USB 경로 | RAM 공유 입력·AIO·completion helper 연결은 남음. 기본 연결 오류 수정과 구분 |
| B01 | 테슬라 속도 송신 보호 조건 | **코드 반영 확인:** `84a93d14d`에 CAN/센서 검사, 0·미래 timestamp 배제. 기기 결과 미확인 |
| B02 | ONNX BSD enable | 남음: 추론은 있지만 `OnnxBsdEnabled`에 따른 중단·해제·결과 폐기 부족 |
| B03 | 내비 경로 우선권·임시 목적지 | 남음 |
| B04 | 단발 eGPU 출력 복원 | 남음 |
| B05 | eGPU 오류 분류·다음 부팅 재시도 | 남음 |
| B06 | DM 눈 표시 | 남음: 기존 얼굴 crop 표시 유지 |
| B07 | 점등 상태·B1·정지 콘 표시 | 남음. stop-object 제어 송신은 있음 |
| B08 | P/R 계기판 | 남음: 원본 최근 화면·자산 연결 |
| B09 | 옛 독립 Wi-Fi display 선택 | 남음: 원본에서 제거한 경로가 Rust 정상 선택에 잔존 |
| B10 | H264 QP·진행 기반 timeout·thread 옵션 | 남음 |
| B11 | Wi-Fi 공존 | 남음: AP 중 scan 억제와 station 선택 |
| B12 | geometry 옛 설정 이전 | 남음: `CenterToFrontRatioEnabled` 정리 |
| B13 | `nonzero_min`·음성 변경 출처 | **코드 반영 확인:** `e8f9f91d8`에 보정 호출과 `voice_assistant` 출처 추가. 기기 결과 미확인 |
| B14 | Xiaoge 새 테슬라 교통정보 broadcast | 남음 |
| B15 | Private_Dirty 공개 | 남음 |
| B16 | 모델 표준편차·카메라 공백·동일 프레임 peak 기록 | 남음. `63ee82761`의 Jetlink 단계별 지연 로그 추가와는 구분 |

기존 설정 15개에 대한 실행 연결 부족도 남아 있다. B01·B13 수정은 이 15개 설정의 신규 기능 연결과 별개다.
원본에서도 Python 소비를 확인하지 못한 `ShowRouteInfo`, `MapboxStyle`을 누락 개수에 더하지 않았다.

### 원본이 갱신되며 추가된 차이

원본 `1032f8641..bc8eb4c2e`는 12커밋이다. 외부 서버·도구의 변경을 전부 기기 Rust 재작성으로 세지 않는다.

| ID | 추가 차이 | 근거·현재 영향 |
| --- | --- | --- |
| U01 | 최종 CAN FD 정지의 객체 상태 보정 | **코드 반영 확인:** 원본 `df90d3083`의 `HUD_LEAD_INFO` 0/1→2 보정이 `0af3992cc`에 추가됐다. ACC/정지 조건과 실제 lead 거리·속도 보존을 코드에서 확인. 기기 결과 미확인 |
| U02 | 레이 Comma Pedal PID 조절 | 원본 `bc8eb4c2e`는 레이 페달 차량을 현대 고정 gain에서 제외하고 설정 화면도 해당 차량에 gain을 노출한다. Rust는 현대 전체 고정 gain을 유지. A05에 연결해서 처리할 새 차이 |
| U03 | Jetson 성능 route 진단과 배포 pin | 원본 `9945cdb9d` 계열에 power/clock/scheduler 성능 관측·`jetsonPerformance` rlog 연결과 새 signed release pin이 있다. 이 연결과 선택 경로는 미반영. `63ee82761`에 추가된 warp/handoff/roundtrip/parse 지연 로그는 별개로 반영됐다. Jetson 서버 Python을 Rust로 다시 쓸 항목은 아님 |

## 5. 클로드 후속 수정과 기기 증거의 경계

인계 문서의 '차량에서 확인'은 **작업자 기록을 인용한 상태**다. 이번 조사에서 원시 로그·기기에 접근해 독립 검증하지 않았다.
그 결과를 뒤에 추가된 SHA의 결과로 옮겨 쓰지 않는다.

| 커밋 | 코드 변화 | 증거 상태 |
| --- | --- | --- |
| `092d7d7a3` | 해제된 USB의 `ENODEV` 처리, 추론 응답 오류 구분 | 인계 문서에 차량 확인 기록 |
| `e4b272a31` | 주행 중 engine 요청 유지 | 인계 문서에 차량 확인 기록 |
| `3b37fcabf` | peer 응답 대기 3초와 modeld budget 분리 | 인계 문서에 차량 확인 기록 |
| `10c5f287f` | modeld frame wait를 150 ms로 맞춤 | 인계 문서에 차량 확인 기록 |
| `6446918b1` | 카메라 누락 때 peer 모델을 매번 reset하지 않음 | 인계 문서에 **주행 미확인**으로 명시 |
| `c3ade0d12` | 모델 파일의 skip-worktree 문제 정리 | 인계 문서에 기기 양방향 branch 전환 확인 기록 |
| `13971857d` | 업로드 기본 목적지 분리, 원본 Discord 자동 전송 제거 | 인계 문서에 기기 확인 기록 |
| `f6db3f2fe` | 캐스퍼를 위해 fault 검출·차단 정책 변경 | 인계 문서에 차량 미확인, F05 참고 |
| `84a93d14d` | 테슬라 측정값·TPMS·디지털 페달·속도 송신 조건 수정 | 이번 조사에서 코드 차이 확인. 테스트 실행·기기 결과를 대신 주장하지 않음 |
| `e8f9f91d8` | `nonzero_min` 보정과 음성 변경 출처 | 이번 조사에서 코드 차이 확인. 기기 결과 미확인 |
| `0af3992cc` | CAN FD 최종 정지 때 멀어지는 lead도 제어 가능한 객체 상태로 보정 | 이번 조사에서 코드 차이 확인. 기기 결과 미확인 |
| `63ee82761` | 느린 Jetlink frame의 warp·worker 대기·왕복·parse 시간을 분리 출력 | 이번 조사에서 코드 차이 확인. 지연 개선이나 기기 결과를 뜻하지 않음 |
| `a4a961a1f` | F02의 Bluetooth 묶음 명령 캐시 결함 수정 | 이번 조사에서 코드 차이 확인. 테스트 실행·기기 결과 미확인 |

인계 문서의 미해결 기기 현상은 다음과 같다. 원인 확정 기록으로 쓰지 않는다.

- 폰 모델 지연과 `commIssue`, 물리 USB Type-C disconnect 후 복구 문제.
- GPIO 49 권한 오류, 오디오 초기화 대기 중 영상 frame drop, onroad 진입 시 `qcomgpsd` 종료.
- 외부 cluster JPEG 부하, `pandad` CPU 사용률. 현재 코드만으로 원인을 단정하지 않는다.
- 책상 전원의 전압 부족 종료와 차량 CAN 부재. 책상에서의 대기를 정상 주행 검증으로 세지 않는다.
- 더 이상 사용하지 않는다는 기기 경로의 정리와 원본↔Rust 전환 후 글꼴·DBC 추적 파일 변경. 이번에 삭제·복원하지 않았다.

## 6. 의도적인 원본 차이·시험 장치·기록의 부정합

| 항목 | 현재 상태와 해석 |
| --- | --- |
| DM 시험 파일 | `/data/disable_dm_for_testing`이 있으면 세션 DM 비활성 값을 설정한다. 파일이 없으면 이 조건은 작동하지 않는다. MD 브랜치가 항상 DM 정상이라는 단정에 이 예외를 빠뜨리지 않는다. 이번에 파일을 만들거나 기기 상태를 조회하지 않음 |
| 책상 ignition | `/tmp/openpilot_bench_ignition`이 있고 Panda 메시지가 비어 있지 않으면 ignition에 합산. 실제 ignition만 보는 원본과 다른 시험 경로. 이번에 켜지 않음 |
| 로그 압축 | Rust zstd level 3, 원본 level 10. 저장량·CPU tradeoff 변경이다. 소스 주석의 6.8배·7% 수치를 동일 조건 기기 실측으로 재확인하지 않음 |
| Params cache | namespace mtime을 10 ms 간격으로 보고 값은 최대 1초 재사용. 같은 프로세스 핸들끼리 공유. 직접 파일을 덮어쓰는 외부 writer는 즉시 관측되지 않을 수 있음. 캐시 자체가 무조건 잘못됐다는 뜻은 아님 |
| 업로드 | 기본 수신 서버를 이 포트의 서버로 변경. 원본 Discord 전송은 명시적 설정이 있어야 함. 원본에 PR할 때는 프로젝트별 기본값 정책으로 설명할 차이 |
| 남은 package 호환 코드 | 자체 빌드 stamp 경로에서는 전체 package 검사를 하지 않지만 옛 staging 활성화 코드·모델 준비 검사·binding의 파일 존재 검사는 남음. 검증 문자열 개수만으로 제거 여부를 판정하지 않음 |
| `rust/port-status.json` | 59개 프로세스 모두 `production_replaced=false`인 옛 기록. 실제 manager는 Rust binary binding을 사용하므로 현재 실행 선택과 맞지 않음. 이 파일이 참조한 서로 다른 문서 69개 중 33개가 현재 checkout에 없음 |
| `rust/upstream/carrot-wip.json` | 원본 기준이 여전히 `0436c7f0a`, partial. 최신 원본 SHA나 현재 기능 완성도를 나타내지 않음 |
| 생성 alerts 기록 | `alerts.json`의 `events.py`·`realtime.py` hash는 기준 SHA 파일과 일치, 두 capnp schema hash는 불일치. 이것만으로 alert 내용 오류를 확정하지 않음. 생성 파일 기록과 코드가 어긋난 상태라는 사실만 기록 |
| 인계 문서 표현 | fault 완화는 F05처럼 stock SCC의 차단 삭제와 Panda 검출 범위 변경으로 분리해야 정확함. 두 이벤트 차단 정의를 모두 지운 상태는 아님 |

## 7. 영역별 조사 범위: 109개 crate 목록

아래 모든 crate의 manifest·소스 목록·관련 검색을 포함했다. 상태는 해당 영역의 핵심 실행 경로를 읽은 범위다.
새 결함이 없다고 보증하는 표가 아니다.

| 영역 | 포함 crate | 현재 판정·심층 확인 범위 |
| --- | --- | --- |
| 시작·패키지·복구 | `runtime-launcher`, `runtime-installer`, `runtime-package`, `runtime-version`, `runtime-core`, `startup-recovery`, `startup-ui`, `checkout-status`, `registration`, `bootlog` | 자체 빌드→런처, stamp, overlay, 복구 수명 확인. F03 및 옛 transaction 분기 |
| 관리·업데이트 | `manager`, `manager-catalog`, `process-supervision`, `managed-entry`, `updated`, `agnos` | 등록 59개·binding·종료·Git update 연결 확인. F01, mapsd, 옛 설정 이전 |
| 하드웨어·네트워크 | `hardwared`, `hardware-control`, `hardware-info`, `amplifier`, `modem`, `lpa`, `wifi` | 시험 ignition과 네트워크 공존 차이 확인. GPIO·모뎀·전원은 실제 하드웨어 미검증 |
| 카메라·센서·GPS | `camerad`, `camerad-runtime`, `camera-kernel`, `sensord`, `qcomgpsd`, `ublox`, `timed` | 노출 샘플링·기기 의존 경로 확인. F06. 실제 센서·GPS 시작 결과는 미확인 |
| CAN·차량 | `pandad`, `panda-usb`, `panda-spi`, `panda-spi-linux`, `can`, `card` | 차량 fault 입력, Hyundai 정지 객체, Tesla 후속 diff, SPAS·페달 연결 확인. 전 차종 패킷 동등성 미검증 |
| 제어·계획·추정 | `selfdrived`, `controlsd`, `control-policy`, `plannerd`, `radard`, `radarcan`, `paramsd`, `locationd`, `lagd`, `torqued`, `calibrationd` | fault 이벤트, impact reboot 비활성화, geometry, 신호 출발·레이더 전환·cut-in 구현 확인. F05/U01/U02와 기존 누락 |
| 모델·외부 연산 | `drivingd`, `modeld`, `model-runtime`, `usbgpu`, `jetlink`, `signal-observer`, `signal-numerics`, `onnx-runtime`, `opencv-runtime`, `desire` | Jetlink begin/infer/finish와 reset·timeout, 모델 선택·입력 layout 확인. F01/F04, 내비 모델·복구 누락 |
| DM | `dmonitoringd`, `dmonitoringmodeld`, `monitoring` | 정차 pause와 시험 비활성 파일 확인. 실제 운전자 감시·경고 동작은 기기 결과 필요 |
| IPC·설정·진단 | `params`, `params-typed`, `cereal`, `msgq`, `messaging`, `logging`, `logmessaged`, `crash-reporting`, `tombstoned`, `proclogd`, `journald` | 설정 캐시·메시지 연결·잔존 검증·기록 상태 확인. B15/B16. IPC 스트레스·하드웨어 시간 특성 미검증 |
| 기록·인코딩·전송 | `loggerd`, `encoderd`, `log-format`, `micd`, `portaudio`, `deleter`, `uploader`, `web-upload`, `dashcam-upload`, `cweb-push`, `athena`, `statsd` | 압축·업로드 기본값, 오디오 대기 관련 코드·보고 구분. 실제 서버 전송·영상/오디오 기록 미시험 |
| UI·계기판 | `ui-application`, `ui-framework`, `cluster`, `cluster-wifi`, `soundd`, `feedbackd`, `jpeg`, `sdl2`, `beepd`, `impact-dashcam` | DM 표시·P/R·지도 모드·H264 queue·Wi-Fi 선택 확인. 렌더링·화면·소리 실기 미시험 |
| 내비게이션 | `carrot-man`, `carrot-navi`, `carrot-state`, `navd` | 외부 경로/Mapbox 우선권, 목적지·지도·모델 입력 연결 확인. A10/A11/B03 |
| 웹·Bluetooth·보조 서비스 | `carrot-server`, `carrot-recovery`, `bluetooth`, `xiaoge`, `webrtc`, `webcam`, `webjoystick`, `bridge`, `control-tools` | 등록 API·설정 소비·B1·음성·ONNX enable·명령 cache 확인. F02, 기존 A08/A09/A12/B02/B14 |
| 공통 라이브러리 | `pocketfft`, `http-transport` | 의존성·구현 존재 확인. 수치·통신의 모든 조건 동등성은 이번 조사에서 증명하지 않음 |
| 개발 도구 | `dev-tools` | 남은 docs 검사·생성 도구와 삭제 범위 확인. 제품 기능 이식 목록과 분리 |

기존 C/C++ 의존성이 있다는 이유로 위 항목을 미완으로 표시하지 않는다. 26개 `build.rs`와 네이티브 링크·바인딩 경로를
목록화했으며, 이 조사로 새 SDK·검증기·테스트 체계를 만들지 않았다.

## 8. 다음에 이 기록을 사용할 때

1. 먼저 문서의 코드 SHA와 작업 HEAD 차이를 확인한다. B01·B13·U01을 다시 미구현으로 취급하지 않는다.
2. 기기에서 보고된 막힘을 우선한다. F03은 빌드·복구, F05는 제어 정책 차이, F01/F04는 실행 연결·동작 차이다. F02는 발견 후 코드 수정 기록이다.
3. 원본 업데이트 U01–U03과 기존 기능 누락은 해당 기능을 옮길 때 함께 다룬다. 원본 기준 SHA만 올려서 반영했다고 쓰지 않는다.
4. 구현 확인, 원본 차이, 작업자 기기 보고, 현재 SHA 기기 결과를 구분한다. 실제 결과가 없으면 항목을 닫지 않는다.
5. 이 문서에는 코드 수정·새 CI·몇 시간짜리 전체 비교 실행·기기 설치·파일 삭제를 수행했다는 의미가 없다.

이번 작업의 산출물은 이 문서다. 기능 수정이나 기기 시험을 수행하지 않았으므로 새 사용자 버전·실사용 가능 판정은 하지 않는다.

## 고정 참조

- [조사 기준 Rust](https://github.com/bin9208/openpilot-rust/tree/a4a961a1f690f2a0a71abdc1e47b3d2ba4c99d36)
- [조사 기준 carrot-wip](https://github.com/ajouatom/openpilot/tree/bc8eb4c2e1aac3023c99f3914e657ed9638beab7)
- [검증 정리 커밋](https://github.com/bin9208/openpilot-rust/commit/79219669a)
- [테슬라 후속 수정](https://github.com/bin9208/openpilot-rust/commit/84a93d14d643d799b20081a997025a0e19779e97)
- [설정 보정 후속 수정](https://github.com/bin9208/openpilot-rust/commit/e8f9f91d82df322b5ec86eda2b954576e6cd8255)
- [CAN FD 정지 후속 수정](https://github.com/bin9208/openpilot-rust/commit/0af3992cc)
- [Jetlink 단계별 지연 기록](https://github.com/bin9208/openpilot-rust/commit/63ee82761)
- [Bluetooth 묶음 명령 후속 수정](https://github.com/bin9208/openpilot-rust/commit/a4a961a1f690f2a0a71abdc1e47b3d2ba4c99d36)

Device-Test-Not-Needed: 고정 SHA의 정적 조사 기록만 추가한다. 실행 코드·설정·설치 동작을 변경하지 않는다.
