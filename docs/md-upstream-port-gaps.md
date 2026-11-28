# dev-md: carrot-wip 대비 Rust 기능 미반영 목록

작성일: 2026-11-11. 사용자가 요청한 정적 코드 조사 결과다. 기기 시험 결과나 병합 승인 기록이 아니다.

## 비교 기준과 범위

| 구분 | 기준 |
| --- | --- |
| 원본 저장소·브랜치 | `ajouatom/openpilot:carrot-wip` |
| 원본 SHA | `1032f8641bca0a85c0cd3662fbd3d1eddc783684` — 조사 당시 원격 HEAD와 일치 |
| Rust 저장소·브랜치 | `bin9208/openpilot-rust:codex/md-upstream-sync`, base `dev-md` |
| 주 조사 SHA | `8dbe086532a5bcd7d25b9f7720b012ee67e0e069` |
| 작성 시 추가 확인 SHA | `e4b272a310195af49390a16ee1b668c015048907` |
| 기존 동기화 기록 | `rust/upstream/carrot-wip.json`의 `source_commit=0436c7f0a7c9741f55bbb175f68c53d0df0c85d5`, 상태 `partial; boot checked, driving not tested` |

`8dbe08653..e4b272a31`의 추가 커밋 두 개는 Jetlink 세션 해제 시 `ENODEV` 처리,
추론 응답 오류 설명, 주행 중 engine 요청 유지를 수정한다. 이를 새 누락으로 세지 않는다.
이 문서의 나머지 기능 누락을 해소하는 변경은 해당 추가 커밋들에 없다.

공통 조상 `43203371004e035bdb70a00a8dad29a4b657c6c3` 이후의 원본 Python 변경을 목록화했다.
`openpilot/`와 `opendbc_repo/opendbc/car/`에서 테스트 경로를 제외한 후보는 340개였다.
최근 동기화 구간뿐 아니라 기존 이식 기록, 메뉴 설정 212개, 등록 프로세스, 웹 API, 추가 메시지 필드와
Rust 구현·호출 경로를 대조했다. 파일명이나 설정 문자열의 부재만으로 기능 전체가 없다고 판단하지 않고,
대응 구현을 읽어 전체 누락과 부분 누락을 구분했다.

이 조사는 기능 차이를 찾기 위한 정적 조사다. 모든 함수·차종·조건의 동등성을 증명한 결과는 아니며,
목록에 없는 코드도 실기기 동작을 보장하지 않는다. `pending` 같은 오래된 기록만으로 누락을 확정하지 않았다.

### 전환 대상에서 제외

- 기존 C/C++ 라이브러리·Panda safety·펌웨어·네이티브 모델 실행 코드는 유지한다. Rust 재작성 항목으로 세지 않는다.
- 모델 준비·컴파일에 필요한 Python은 유지한다. 모델 준비 코드의 Python 잔존 자체는 누락이 아니다.
- 단, 위 구성요소를 호출하거나 설정·입출력을 전달하던 Python 실행 로직의 Rust 연결 누락은 포함한다.
- PC·Jetson 서버, Android 앱, 외부 보드 코드를 언어 통일 목적으로 전환하지 않는다. 기기 측 연결은 조사에 포함한다.
- 원본에서 제거됐거나 실행되지 않는 기능, 개발 도구·테스트·문서·CI의 언어 차이는 차량 기능 누락과 구분한다.
- Python GC 콜백·객체 정리 같은 언어별 구현은 그대로 옮길 대상이 아니다. 독립적인 관측 데이터 누락은 별도로 적는다.

이하 원본 경로는 위 원본 SHA, `rust/` 경로는 주 조사 SHA 기준이다.

## A. 기능 또는 실행 연결의 미반영

| ID | 기능 | 원본 근거 | Rust에서 남은 내용과 사용자 영향 |
| --- | --- | --- | --- |
| A01 | 자동 방향지시등 판단 | `openpilot/selfdrive/controls/lib/desire_helper.py`, `desire_lib/{boundary_indicator,boundary_lane,indicator_observer,manual_turn_indicator}.py` | `rust/crates/desire/src/`에는 레버 처리까지 있으나 경계 접근·수동 회전·모델 경로·예측 차선변경에 따른 새 점등 판단이 없다. 모델의 판단과 실제 램프 요청을 분리하는 처리도 필요하다. |
| A02 | ATC·BSD·취소 상태 연동 | `openpilot/common/atc_params.py`, 원본 `desire_helper.py`, `opendbc_repo/opendbc/car/hyundai/{rear_gap,lane_line_time}.py` | ATC 설정 이전 처리는 있으나 우측 차선변경 거리 설정의 실행 소비가 없다. OEM BSD의 신선한 입력, `laneLineInfoMonoTime`, 실제 크루즈 취소 latch의 점등 경로 전달도 빠져 있다. |
| A03 | 현대·기아 확장 SPAS 점등 | `openpilot/selfdrive/car/hyundai_spas.py`, `opendbc_repo/opendbc/car/hyundai/spas.py`, 원본 `card.py`·`hyundai/interface.py` | 기존 `card/src/brands/hyundai/canfd_maintenance.rs`의 SPAS 송신과 별개다. 새 설정·차량 호환성 판정·세션·패치/복원·응답 확인·시험 모드를 연결해야 한다. C/C++ safety는 유지하고 필요한 설정을 전달한다. |
| A04 | 차량 방향지시등의 물리 유지·음성 연결 | `openpilot/selfdrive/car/voice_indicator.py`, `carrot/bluetooth/physical.py`, 원본 `card.py`·`controlsd.py` | `physical-hold.json` lease, 차종·시간·운전자 override 검사와 `physicalBlinkerHold`, 음성 점등 요청의 최종 CAN 출력 연결이 없다. 기존 차선변경 점등 유지와 구분해야 한다. |
| A05 | 새 페달 인터셉터 | `opendbc_repo/opendbc/car/pedal_interceptor.py`, `hyundai/pedal_interceptor.py`, `hyundai/{interface,carstate,carcontroller,values}.py` | 레이 EV 프로필, 명시적 설정·차량/센서 식별, 수신 상태·고장·운전자 페달·순정 크루즈 우선권, 명령 송신이 필요하다. 기존 GM 등의 인터셉터 지원이 이 기능의 이식은 아니다. |
| A06 | 테슬라 자동 방향지시등 | `openpilot/selfdrive/car/{tesla_turn_signals,tesla_lane_signals}.py`, `desire_lib/tesla_turn.py`, `opendbc_repo/opendbc/car/tesla/{carstate,interface}.py` | 새 운전자 stalk 해석, 차량 설정·safety flag 전달, 요청 ID·준비 확인·세션·점등 피드백·취소·운전자 우선권이 빠져 있다. 기존 테슬라 조향·속도 설정은 별도로 구현돼 있다. |
| A07 | 테슬라 CAN 교통 신호 정지 | `opendbc_repo/opendbc/car/tesla/traffic_control.py`, `openpilot/selfdrive/controls/lib/tesla_traffic_stop.py`, 원본 `longitudinal_planner.py` | `teslaTrafficControl` 수신·시간 검사·설정부터 플래너의 감속·정지 유지까지 연결이 없다. 스키마·DBC를 가져온 것만으로 동작하지 않는다. |
| A08 | 새 Bluetooth 리모컨 동작 | `openpilot/selfdrive/carrot/bluetooth/{model,daemon}.py` | `rust/crates/bluetooth/src/`에는 `blinkerLeft/Right`와 별도 `blinker` 채널이 없다. `gestures` 선택, 빠른 단일 입력, 연결·설정 변경 당시 눌린 키의 동기화, 점등 요청의 원래 입력 시각 보존도 필요하다. |
| A09 | Carrot-B1 기기 측 연결 | `openpilot/selfdrive/carrot/bluetooth/b1_monitor.py`, `carrot/server/features/bluetooth.py`, `tools/carrot_b1/auto_update.py` | B1 등록·해제 API, 장치 선택·headless pairing·인증·GATT 감시·요청/ACK·출력 소유권·업데이트 연결이 없다. 보드 펌웨어의 Rust 재작성은 대상이 아니다. |
| A10 | 공유 지도·내비게이션 | `openpilot/selfdrive/navigation/{mapsd,renderer,tiles,pmtiles,vector_tile,mapbox_tiles,route,helpers,preview}.py`, `carrot/server/features/navigation.py` | `mapsd`가 manager에 없고 지도 생성·타일 처리·경로 위치 추적·미리보기 공급이 없다. `/api/navigation/search`, 목적지 GET/POST/DELETE, `/api/navigation/map.png`도 Rust 서버에 없다. |
| A11 | 내비게이션 모델 입력 | `openpilot/selfdrive/navigation/{map_client,source,jetlink_demand}.py`, 원본 `modeld.py`·`generic_model_runtime.py`·`jetlink/inputs.py` | `MapInput`, `nav_map`·`navigation`, 만료 시 입력 초기화와 경로 ID 전달이 없다. Rust eGPU worker는 scalar 입력 3개만 허용한다. 계기판 지도 모드 `7`도 허용 범위 밖이다. 내비 모델의 데이터 경로 전체를 이어야 한다. |
| A12 | 음성 비서 기기 서비스 | `carrot/server/features/assistant_{connection,drive,settings,tools}.py`, 대응 `services/assistant_*.py`, `desire_lib/assistant_lane.py`, `carrot/assistant_speed.py` | 연결 확인·권한 부여/철회·설정 제안/적용·Tools·주행 명령 API와 요청 소비 경로가 없다. 차선변경/속도 명령의 유효기간·중복 방지·권한·운전자 입력·차량 상태 조건도 함께 보존해야 한다. |
| A13 | 외부 모델 라이브러리·부팅 선택 | `openpilot/common/{big_model_catalog,jetson_model}.py`, `modeld/model_library.py`, `carrot/server/features/egpu_model.py` | 모델 선택 API·선택 저장·부팅 중 선택 고정·폐기 모델 캐시 정리·사용 중 모델 보호·Jetson 배포 선택이 없다. 원본 기본은 `N16 Nav`, Rust `usbgpu/src/model_delivery/state.rs`의 기본은 아직 `MD v3`다. 원본의 `Flip Flop`, `Light speed`, `Light speed 2` 선택도 같은 라이브러리 경로가 필요하다. |
| A14 | Jetlink 입력·USB 추가 경로 | `openpilot/selfdrive/modeld/jetlink/{shared_input,aio,scheduling,inputs}.py` | 연결 소유 RAM 공유 입력(`ram_input_v1`), 선택적 FunctionFS AIO의 기존 C 구현 호출, 세션별 USB completion 우선순위 helper 연결이 없다. 기본 Jetlink 연결 전체가 없다는 뜻은 아니다. 선택적 전송 최적화와 필수 모델 입력 연결을 구분한다. |

### A항목의 주요 Rust 확인 경로

- A01–A04: [desire](../rust/crates/desire/src/), [Hyundai card](../rust/crates/card/src/brands/hyundai/), [drivingd wire](../rust/crates/drivingd/src/wire.rs), [controlsd HUD](../rust/crates/controlsd/src/hud.rs).
- A05–A07: [Hyundai parameters](../rust/crates/card/src/brands/hyundai/), [Tesla parameters](../rust/crates/card/src/brands/tesla/parameters.rs), [Tesla state](../rust/crates/card/src/brands/tesla/state.rs), [longitudinal planner](../rust/crates/plannerd/src/longitudinal_planner/).
- A08–A09: [Bluetooth config](../rust/crates/bluetooth/src/config.rs), [decoder](../rust/crates/bluetooth/src/decoder.rs), [journal](../rust/crates/bluetooth/src/journal.rs), [input ioctl](../rust/crates/bluetooth/src/input_kernel.rs), [Bluetooth HTTP](../rust/crates/carrot-server/src/bluetooth_http/).
- A10–A12: [manager catalog](../rust/crates/manager-catalog/src/catalog.rs), [navd](../rust/crates/navd/src/), [carrot-man](../rust/crates/carrot-man/src/), [server application](../rust/crates/carrot-server/src/http.rs), [eGPU worker input layout](../rust/crates/usbgpu/src/worker.rs).
- A13–A14: [model defaults](../rust/crates/usbgpu/src/model_delivery/state.rs), [selection](../rust/crates/usbgpu/src/model/selection.rs), [model HTTP](../rust/crates/carrot-server/src/egpu_model.rs), [Jetlink model contract](../rust/crates/jetlink/src/model_spec.rs), [App selection](../rust/crates/jetlink/src/selection.rs), [FunctionFS reader](../rust/crates/jetlink/src/ffs.rs).

## B. 기존 구현에서 추가 변경이 미반영인 부분

| ID | 기존 기능 | 미반영 변경과 영향 | 원본 / Rust 근거 |
| --- | --- | --- | --- |
| B01 | 테슬라 속도 설정 CAN 송신 | 원본에 추가된 `canValid`·`vehicleSensorsInvalid`, timestamp > 0, 미래 시각 배제 조건이 없다. Rust의 `age <= limit`만으로는 음수 age도 허용된다. | 원본 `tesla/speed_limit_controller.py` / [speed_limit.rs](../rust/crates/card/src/brands/tesla/speed_limit.rs) |
| B02 | ONNX BSD 추론 | 추론 자체는 있으나 `OnnxBsdEnabled`를 소비하지 않는다. OFF 시 모델 해제·결과 초기화·세대 확인에 따른 진행 중 결과 폐기가 미반영이다. | 원본 `xiaoge/{v_asm_inference,v_asm_server}.py` / [settings.rs](../rust/crates/xiaoge/src/settings.rs), [shared.rs](../rust/crates/xiaoge/src/native/shared.rs), [vision](../rust/crates/xiaoge/src/native/vision/) |
| B03 | 기존 내비 수신·경로 계산 | 외부 내비와 Mapbox 경로 소유권 분리, 경로 ID·재전송, RAM의 `NavDestinationTemporary`, 토큰 갱신, 비동기 경로 요청 등 새 `navd` 동작이 미반영이다. fallback instruction도 원본의 새 우선 조건과 `valid` 설정에 맞지 않는다. | 원본 `navigation/source.py`, `carrot_man.py`, `carrot_serv.py`, `navd/{helpers,navd}.py` / [carrot-man wire](../rust/crates/carrot-man/src/wire.rs), [serv tick](../rust/crates/carrot-man/src/serv/tick.rs), [navd](../rust/crates/navd/src/) |
| B04 | eGPU 출력 이상 복구 | 단발 비정상 출력을 잠깐 미공개로 두고 순환 상태와 desire pulse 이력을 복원하는 처리가 없다. 원본의 100 ms 제한과 연속 이상 시 종료를 함께 보존해야 한다. 이전 예측의 timestamp를 새로 붙이는 처리가 아니다. | 원본 `modeld/{egpu_output_recovery,precompiled_runner,precompiled_worker}.py` / [worker.rs](../rust/crates/usbgpu/src/worker.rs), [usb_model.rs](../rust/crates/drivingd/src/usb_model.rs) |
| B05 | eGPU 오류 분류·재시도 | 초기화 중 TLB mapping timeout, 새 runtime의 USB timeout/hang 분류, `device_restart_required` 표시, 일회성 다음 부팅 retry request와 `/api/egpu/model/retry-verification`이 미반영이다. 기존의 제한적 자동 복구는 있다. | 원본 `modeld/{egpu_rejection,precompiled_model,egpu_recovery}.py` / [failure](../rust/crates/usbgpu/src/model_delivery/failure.rs), [retry](../rust/crates/usbgpu/src/model_delivery/failure/retry.rs), [recovery](../rust/crates/drivingd/src/recovery.rs), [HTTP](../rust/crates/carrot-server/src/egpu_model.rs) |
| B06 | DM 화면 표시 | 원본의 머리 자세·눈 개폐에 따른 눈 표시가 미반영이다. Rust는 기존 얼굴 crop과 camera frame freshness를 쓰는 표시를 유지한다. DM 판단 자체와 표시 차이를 구분한다. | 원본 `ui/dm_preview.py`, `ui/onroad/driver_preview.py` / [driver_preview](../rust/crates/ui-application/src/onroad/driver_preview/) |
| B07 | 새 상태 표시 | 점등 준비·요청·확인·종료 표시, B1 상시 배지, 실제 stop-object 출력에 대응하는 콘 표시가 없다. stop-object 제어 송신 자체에는 별도 구현이 있다. | 원본 `common/indicator_display.py`, `ui/{b1_badge,stop_object}.py` / [ui-application](../rust/crates/ui-application/src/), [cluster renderer](../rust/crates/cluster/src/renderer/) |
| B08 | Cluster P/R 화면 | 원본 HEAD 쪽 P/R 전용 화면·차종 이미지/모델 선택·날씨·후진 궤적 등이 미반영이다. 원본에서 비활성으로 둔 주행 UI 부분까지 활성화할 근거로 삼지 않는다. | 원본 `cluster/{cluster_renderer,cluster_scene,cluster_vehicle_models}.py` / [renderer core](../rust/crates/cluster/src/renderer/core.rs), [frame](../rust/crates/cluster/src/renderer/frame.rs), [HUD policy](../rust/crates/cluster/src/renderer/hud/mod.rs) |
| B09 | Cluster 출력 선택 | 원본에서 제거한 독립 Wi-Fi display 선택이 Rust의 registration 읽기·transport 선택 경로에 남아 있다. 옛 설정이 있을 때 원본과 동작이 달라진다. 원본의 Jetson HUD 전송까지 제거하라는 뜻은 아니다. | 원본 `cluster/main.py`, `cluster_autorun.py` / [runtime entry](../rust/crates/cluster/src/runtime/entry.rs), [output factory](../rust/crates/cluster/src/runtime/output/factory.rs), [usb wifi](../rust/crates/cluster/src/usb/wifi.rs) |
| B10 | H264 display 송신 | display의 QP 범위 요청, FFmpeg encoder/filter thread 지정, chunk 송신이 진행되는 동안 3초 deadline 연장이 미반영이다. Rust sender는 enqueue 시작 시 고정 deadline을 쓴다. | 원본 `cluster/cluster_h264_pipeline.py`, `cluster/main.py` / [VIDC controls](../rust/crates/cluster/src/h264/vidc/controls.rs), [sender](../rust/crates/cluster/src/h264/native_sender/worker.rs), [runtime args](../rust/crates/cluster/src/runtime_args/) |
| B11 | Wi-Fi 공존 | Jetson AP 사용 중 백그라운드 scan 억제, 일반 연결용 `wlan0` 우선 선택, Web network viewer의 `--rescan no`가 미반영이다. | 원본 `common/wifi_coexistence.py`, `system/ui/lib/wifi_manager.py`, `server/services/device_info.py` / [scan policy](../rust/crates/wifi/src/state.rs), [adapter](../rust/crates/wifi/src/initialize.rs), [Web network](../rust/crates/carrot-server/src/system/network.rs) |
| B12 | 차량 geometry 설정 | `CenterToFrontRatio` 계산은 있으나, 폐기된 `CenterToFrontRatioEnabled`가 false였다면 값을 0으로 돌리고 옛 key를 지우는 이전 처리가 없다. | 원본 `common/lateral_geometry_params.py`, `system/manager/manager.py` / [manager initialization](../rust/crates/manager/src/initialization.rs), [parameters](../rust/crates/manager/src/parameters.rs) |
| B13 | 설정 Web API | `nonzero_min` 보정이 없다. ATC 거리 등 '0은 OFF, 양수에는 최솟값 존재' 설정이 원본과 다른 값으로 저장될 수 있다. `voice_assistant` 변경 출처도 미등록이다. | 원본 `server/services/{params,param_changes}.py` / [params HTTP](../rust/crates/carrot-server/src/params_http.rs), [change source](../rust/crates/carrot-server/src/param_changes/mod.rs) |
| B14 | Xiaoge broadcast | 원본은 새 `APP_trafficControl`의 timestamp·DLC·종류·상태를 검사하고 필요한 교통정보만 보낸다. Rust는 옛 `DAS_road` 읽기와 광범위한 상태 broadcast가 남아 있다. | 원본 `carrot/xiaoge_data.py` / [Tesla decoder](../rust/crates/xiaoge/src/native/tesla.rs), [broadcast](../rust/crates/xiaoge/src/native/broadcast.rs) |
| B15 | procLog 메모리 관측 | paced smaps/PSS 수집은 있으나 `Private_Dirty` 수집과 `memPrivateDirty` 공개가 없다. Python GC callback 이식과는 별개다. | 원본 `system/{proclog_smaps,proclogd}.py` / [smaps](../rust/crates/runtime-core/src/procfs/smaps.rs), [wire](../rust/crates/proclogd/src/wire.rs) |
| B16 | 모델·지연 관측 | `orientationRate`의 모델 표준편차 공개, `modelCameraGap` 전후 프레임 기록, 같은 프레임에 상관된 `runtimeTiming.peak_sample`이 미반영이다. 새 주행 정책을 추가하는 항목은 아니다. | 원본 `modeld/{fill_model_msg,camera_gap,modeld}.py`, `common/runtime_diagnostics.py` / [model wire](../rust/crates/modeld/src/model_wire.rs), [drivingd](../rust/crates/drivingd/src/) |

## C. 설정과 메시지 연결 누락

원본에서 실제 소비하는 메뉴 설정 중 대응 Rust 실행 처리가 없는 것은 다음 15개다.
설정 카탈로그나 Params key 정의에 존재한다는 것만으로 구현됐다고 판단하지 않는다.

| 설정 | 관련 ID | 부족한 부분 |
| --- | --- | --- |
| `TeslaAutoTurnSignals` | A06 | 차량 설정·세션·출력 |
| `TeslaCanTrafficStop` | A07 | 차량 설정·교통 관측·플래너 |
| `TeslaManualLaneSignals` | A06 | 수동 차선변경 점등 보조 |
| `TeslaModelDesireSignals` | A06 | 모델 desire 점등 보조 |
| `TeslaModelPathSignals` | A06 | 모델 경로 점등 보조 |
| `AutoBlinkerControl` | A01, A04 | 자동 점등 모드와 출력 lease |
| `ManualLaneChangeIndicator` | A01 | 운전자 경계 접근 판단 |
| `ManualTurnIndicator` | A01 | 운전자 수동 회전 판단 |
| `ModelDesireIndicator` | A01 | 모델 desire·예측 차선변경 판단 |
| `ModelPathIndicator` | A01 | 모델 경로 판단 |
| `AutoTurnControlRightLaneChange` | A02 | manager 이전 처리 외 거리 설정의 실행 소비 |
| `CanfdBlinkerControl` | A03 | 차량 호환성·설정·세션 |
| `CanfdBlinkerTest` | A03 | 시험 모드 설정 소비 |
| `CommaPedalEnabled` | A05 | 차량 프로필·제어 설정 |
| `OnnxBsdEnabled` | B02 | OFF/ON에 따른 추론·결과 수명 관리 |

`ShowRouteInfo`와 `MapboxStyle`은 원본의 이번 Python 실행 처리에서도 소비를 확인하지 못했다.
Rust에 문자열이 없다는 이유만으로 위 15개에 포함하지 않았다.

추가 schema 중 원본 실행 코드가 생성하거나 소비하지만 Rust의 대응 읽기·쓰기가 없는 주요 필드는 다음과 같다.
필드 추가에 그치지 않고 생성 측·전달 측·최종 소비 측을 맞춰야 한다.

- 점등·음성: `assistantLaneSignalId`, `assistantLaneSignalRoute`, `autoTurnSignal`,
  `autoTurnSignalDirection`, `autoTurnSignalLaneChange`, `autoTurnSignalReadyRequestId`,
  `autoTurnSignalRequestId`, `autoTurnSignalStatus`, `turnSignalDriverDirection`, `turnSignalDriverValid`.
- 점등 관측: `indicatorIsolationMask`, `indicatorJournalMonoTime`, `indicatorPrepareDirection`,
  `indicatorPrepareSource`, `manualLaneIndicator`, `manualLaneIndicatorMonoTime`,
  `observedIndicator`, `observedIndicatorMonoTime`, `observedIndicatorSource`.
- 차량·내비: `cruiseCancelState`, `physicalBlinkerHold`, `laneLineInfoMonoTime`,
  `tbtRearInputsAvailable`, `tbtRightRearBlocked`, `teslaTrafficControl`, `stopLineDistance`, `carrotRouteId`.
- 관측: `memPrivateDirty`.

## D. 구현이 있어 기능 전체 누락으로 취급하지 않는 항목

아래에는 Rust 구현이 있다. 정적 확인이며, 모든 조건에서 동등하다거나 기기에서 통과했다고 주장하지 않는다.

| 기능 | Rust 근거 | 남은 항목과 구분 |
| --- | --- | --- |
| 레이더에서 비전으로 전환 확인 | `radard/src/controller/{range,update}.rs` | A07의 테슬라 교통 신호와 별개 |
| 두 레이더 기반 cut-in 보강 | `radard/src/trajectory_cutin/policy.rs`의 `radar_pair_entry` | A02의 OEM BSD 입력과 별개 |
| 신호 정지 후 출발·정지 기억 정리 | `plannerd/src/carrot/`, `plannerd/src/signal/start.rs` | 테슬라 CAN 신호 정지의 새 경로는 A07 |
| 차량·Panda·순정 SCC 고장 감시 | `selfdrived/src/vehicle_faults.rs`, controller 측 | 이것만으로 모든 차종의 고장 보고 일치를 보장하지 않음 |
| camera SCC MAIN과 stop-object 제어 | `card/src/brands/hyundai/`, `controlsd/src/hud.rs` | B07의 콘 표시는 미반영 |
| 확인된 정차 중 DM 평가 일시정지 | `dmonitoringd/src/controller.rs`의 `stationary_pause` | B06의 눈 표시와 별개 |
| ATC 폐기 설정 이전 | `manager/src/parameters.rs`의 `migrate_atc` | A02의 거리 설정 소비는 미반영 |
| 레버 규칙 | `desire/src/lever.rs`, `desire/src/helper.rs` | A01의 새 observer 일체와 별개 |
| CenterToFrontRatio geometry 계산 | `control-policy/src/geometry.rs`, `paramsd/` | B12의 옛 key 정리는 미반영 |
| ONNX BSD/차선 추론 | `xiaoge/src/native/vision/` | B02의 enable·수명 관리는 미반영 |
| 기본 Jetlink·eGPU fallback/제한 재시도 | `jetlink/`, `drivingd/src/recovery*.rs`, `usbgpu/` | A13–A14, B04–B05를 추가로 다룸 |

## E. 대응 시 취급과 한계

1. 코드 수정 시 이 문서의 고정 SHA와 작업 HEAD 차이를 확인하고, 이미 수정된 항목을 재구현하지 않는다.
2. 원본의 같은 기능을 묶어서 연결한다. 특히 점등·지도·모델 선택은 API나 schema 복사로 끝내지 않는다.
3. 기존 safety·pose·시간·운전자 우선권·임계값·실행 우선순위 조건을 보존한다.
4. 구현 존재, 정적 차이 해소, 실제 기기 동작 확인을 구분한다. 이번에 컴파일·Actions·기기 접속·주행은 하지 않았다.
5. 이 문서는 새 CI·감사 장치·전체 workspace 비교 시험을 요구하지 않는다.

원본의 기능 누락을 모두 보완하고 `carrot-wip`에 전체 PR을 내는 경우, GPT가 검토한다는 사실 자체는
거부 사유가 아니다. 승인 여부는 원본 동작 보존, 설치·업데이트·실패 후 복구 설명,
변경 SHA의 기기 결과, 원본 측 유지보수 방침에 달려 있다. 이는 승인 가능성에 대한 추론이며 원본 측 답변이 아니다.
이 문서 작성으로 원본 PR을 생성·제출·병합하지 않는다.

## 고정 참조

- [원본 carrot-wip 조사 SHA](https://github.com/ajouatom/openpilot/tree/1032f8641bca0a85c0cd3662fbd3d1eddc783684)
- [Rust 주 조사 SHA](https://github.com/bin9208/openpilot-rust/tree/8dbe086532a5bcd7d25b9f7720b012ee67e0e069)
- [작성 시 추가 확인한 Jetlink 수정](https://github.com/bin9208/openpilot-rust/commit/092d7d7a3864a51db5dcb5388ba1b7099fbd184b)
- [작성 중 추가된 Jetlink engine 요청 수정](https://github.com/bin9208/openpilot-rust/commit/e4b272a310195af49390a16ee1b668c015048907)
- [Rust 기존 동기화 기록](../rust/upstream/carrot-wip.json)

Device-Test-Not-Needed: 조사 기록만 추가한다. 실행 코드·설정·빌드·설치 경로를 변경하지 않는다.
