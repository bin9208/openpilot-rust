# 실행용 foreign library의 내용별 준비 — #599 (검증 대기)

상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598),
환경 [#599](https://github.com/bin9208/openpilot-rust/issues/599),
자산·세대 [#600](https://github.com/bin9208/openpilot-rust/issues/600).
기존 [독립 provider](native-provider-components-599.md)의 닫힌 inventory를 확장한다.
모델 준비 Python과 기존 C/C++ 라이브러리를 유지하며 runtime 로직/수학을 바꾸지 않는다.

기존4개에 추가한 SDL·TurboJPEG2개 component와 원래 공용 library53개 경로 합집합/
실제 Rust API 검사는 [display 입력 안내](native-display-inputs-599.md)를 따른다.
FFmpeg 도구·FreeSans2개 component와 CLI/실제 glyph 검사는
[media 입력 안내](native-media-tools-font-599.md)를 따른다.
일반 Git/firmware와 구분하는 LFS4개 파일의 Tree1개는 [LFS 입력 안내](native-source-lfs-599.md)를 따른다.

생성 MPC는 이 foreign profile에 넣지 않는다. 별도 source-bound profile의 생성 입력/
output 계약과 실제 Rust solver ABI 검사 후속은
[MPC 소스 결속](native-mpc-source-bindings-599.md)을 따른다. 새 후속의 Actions 결과는 대기다.

## 원본과 선택 범위

기존 provider와 같은 원본 source `a3d0e3a497b4ae4d23c7cf5ee2275177600bbb31`,
원본 archive1155969168 bytes/SHA
`9c9c16a72bad163226388ef84327b0db1fa64bbe4828e64388e0a7e92d364291`,
원본 runtime manifest SHA
`8adbc002388fb97420e4bef9dcf58a5ed75de4c4632a1bc0d10c555f1c2d39ee`를 유지한다.
선택한 모든 파일을 원본 manifest hash와 대조하고 license/provenance를 보존한다.
원래 source SHA를 현재 runtime SHA로 덮어쓰지 않는다.

| 독립 profile / plan | 선택 member | 고정 payload bytes | 실제 내용 |
| --- | --- | --- | --- |
| `numpy253-openblas` / native-numerics-provider.json |22 |29061403 | NumPy2.5.3 wheel의 OpenBLAS ILP64·gfortran·manifest·전체 기존 notice |
| `geos313-shared` / native-geos-provider.json |8 |5999768 | 기존 Shapely2.1.2의 GEOS3.13.1/CAPI1.19.2·manifest·notice |
| `brotli110-shared` / native-brotli-provider.json |7 |1115802 | 기존 AGNOS의 Brotli1.1.0 세 library·manifest·copyright·이미지 출처 |
| `acados022-libraries` / native-acados-provider.json |13 |4289640 | 기존 acados0.2.2.post103/BLASFEO/HPIPM/qpOASES foreign library5개·notice·역사 provenance |

위 payload에는 profile마다 원본 runtime manifest241671 bytes가 포함된다. 실제 gzip
component 크기/전송량은 Actions 준비 보고서에서 별도로 확인한다. namespace와 descriptor
pin이 profile마다 달라 하나를 준비해도 다른 자산의 cache identity를 바꾸지 않는다.
기존 FFmpeg/raylib/OpenCV의 inventory·URL·Tree·identity는 그대로다.

원본 card/controlsd/radarcan/torqued-numerics의 각21개 path→hash 집합이 같음을 읽기
감사로 확인했다. 한 개의 `numerics/` component를 준비하고 최종 Layout에서 네 runtime
경로가 같은 immutable object를 사용하도록 연결한다. 이 변경은 component 하나만 만드는
단계이며 최종 네 경로의 실제 배치·저장 공간 절감 검증은 아직 남아 있다.

GEOS의 예전 절대 환경 경로 파일은 `provenance/original-geos-environment.txt`로 옮긴다.
실제 새 prefix는 준비 보고서 root에서 계산한다. 과거 환경 경로를 새 실행 설정으로 쓰지 않는다.

## acados 생성 solver는 별도 필수 단계

이 profile은 `libacados_ocp_solver_lat.so`와 `libacados_ocp_solver_long.so`를 포함하지 않는다.
이 두 파일은 프로젝트의 MPC 정의에서 생성한 C 결과이므로 foreign library 선택만으로 현재
source와의 일치를 입증할 수 없다. 기존 manifest/build provenance는 각각
`provenance/original-acados-manifest.json`, `provenance/original-acados-build.json`에
역사 기록으로 보존한다. profile 안에 실행용 `plannerd-acados/manifest.json`을 만들지 않아
부분 library가 완전한 solver artifact로 검증될 수 없게 한다.

읽기 감사에서 원래 두 Python MPC 정의 hash가 현재 source와 같은 것은 확인했다.
이 사실만으로 generator/환경/생성 C/빌드 결과의 전체 연결을 확정하지 않는다. 다음에는
현재 generator 입력과 환경을 묶어 native 생성·컴파일 또는 입력이 같은 결과의 엄격한
재사용을 구현하고 전체 solver manifest/원본 MPC 회귀를 확인해야 한다. 추가 언어 전환을
선행하지 않으며 기존 C/C++와 준비 Python 경로를 유지한다.

## 준비·검사 방법

기존 `build-runtime-ffmpeg --export-released-provider`/`--prepare-released-provider`의
`--plan`으로 위 plan을 선택한다. descriptor의 독립 SHA,64MiB component 한도,
archive 경로/종류/size/mode/hash/alias, 실제 전체 Tree, cache lock·읽기 전용 원자 발행과
반복 전송/복사0 검사는 기존 공통 경로를 재사용한다. 새 실행 파일/의존성/lock 변경은 없다.

`tools/ci/prepare_native_runtime_companions.py`는 **Actions 전용**이다. 기존 producer의
원본 cache를 재사용해 각 component를 별도로 내보내고 독립 descriptor pin으로 두 번
소비한다. 각 작업에서 실제 payload 기반 `4*payload+16MiB` 추가량을 admission한다.
이전 원본의 HTTP body/stored bytes0, consumer의 원본 archive 미다운로드와 반복 전송/
복사0을 확인한다. 공개 release/독립 Git pin·공개 HTTPS 시험은 이 단계가 아니다.

genuine AGNOS workflow는 host의 작은 component cache/export를 guest에 연결한다.
이 절의4개에 display2개/도구·글꼴2개 및 별도 source-bound MPC가 추가되며 guest에1.1GB 원본 runtime은
연결하지 않는다. source-built ARM 준비기가 각
component의 전체 Tree를 host 예상값과 비교하고 초기/반복/실제 API 호출 후 다시 검증한다.

- foundation은 production profile의 고정 inventory/namespace·원본 source 유지·변조 거부와
  기존 공통 provider 회귀·Clippy를 실행한다.
- ARM C++ fixture는 현재 Rust FFI와 같은 ILP64 float/double dot·matrix,2×2 SVD,
  GEOS 선 길이/중간점, Brotli1.1.0 quality11/window22 round trip을 확인한다.
  OpenBLAS thread는1로 고정한다. acados는 현재 Rust planner가 요구하는 core symbol9개만
  확인하며 solver를 만들거나 실행하지 않는다. 모델/수학 허용 오차를 바꾸지 않는다.
- 기존 strict `runtime-elf-audit`로 fixture와 foreign SO 전체의 AArch64/DT_NEEDED/
  version/strong-symbol closure를 확인한다. 실제 daemon·전체 MPC·module 회귀는 별도다.
  API 호출 때 `/proc/self/maps`도 저장해 실제 component 경로의 library가 매핑되고
  libpython이 섞이지 않았는지 확인한다. guest 경로와 PRoot host bind 경로를 구분한다.
- 이 절의 네 runtime Tree를 `native-companion-runtime-inputs.json`에 SHA-pin한다. 후속에서
  display2개/도구·글꼴2개/LFS1개와 [Panda](native-panda-inputs-599.md)1개를 더하고 partial acados를 source-bound MPC로 대체한10개 Tree를 기존13그룹
  계획의 `runtime_inputs`에 연결한다. 이 입력은 compiler recipe/cache에 섞지 않는다.
  동일 입력의 plan/bindings/audit byte 반복 일치 검사도 유지한다.

artifact는 `native-companion-*.json`, `native-companion-exports`, 각 Tree receipt,
ARM source/compile/API/ELF 보고서와 commands log다. 로컬에서는 Rustfmt·Python AST·YAML/
원본 명령/queue/runner/timeout diff·정적 plan/hash 읽기만 확인한다. 새 준비기·C++·기능은
로컬에서 실행하지 않는다. host x64 bin-actions/guest QEMU·PRoot AGNOS ARM user space를
물리 ARM CPU/AGNOS kernel 부팅/comma3X 성능 또는 사용자 수용으로 표현하지 않는다.

전체 provider/data/model/Python 준비·생성 MPC·Layout·99개 native build/seal/default update/
정상 start/record/upload/복구·실측 자원/성능·정확 PR/병합 후 검사와 사용자 comma3X 수용은
계속 남아 있다. 실행 자산 일부의 준비가 전체 runtime 완료를 뜻하지 않는다.

## 2026-10-29 초기 소비 경로 실패 (#621)

genuine40691750328/job122844676184에서 numerics export 뒤 nested cache/store 부모 누락으로
첫 consume이 ENOENT 종료됐다. Actions 전용 준비기가 work 안의 두 부모를 명시적으로
생성한다. 공통 provider의 단일 leaf 생성과 전체 경로/hash/mode 검증을 유지한다.
정확 수정 소스의 초기·반복·실제 AGNOS API 성공은 아직 확인 전이다.
host 소비뿐 아니라 guest의 companion/display/media 세 store도 같은 단일 leaf 생성
계약을 사용한다. 관련 호출 전부를 대조해 guest work의 세 부모도 명시적으로 생성한다.
공유 cache는 host가 이미 준비한 bind를 사용하며 다시 만들거나 삭제하지 않는다.
