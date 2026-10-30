# 콤마 자체 빌드·다운로드 재사용 검증 안내 — #598

## 2026-10-30 — workspace 테스트 기록의 source checkout 오염 분리 ([#645](https://github.com/bin9208/openpilot-rust/issues/645))

실제 실패와 테스트 기록 격리는 [#645](https://github.com/bin9208/openpilot-rust/issues/645)에서 추적한다.

고정 source `2375949c19c63667ad66de0f9ee7e2c90ad8ba70`의
[Rust 40961947131/workspace job123757140114](https://github.com/bin9208/openpilot-rust/actions/runs/40961947131/job/123757140114)은
`bin-actions`에서 01:12:43–01:38:16 UTC에 실행됐다. PR611의 원래 base는
`abea63adf5140cb670a1880fa2c350727a0feda9`, 실제 checkout은
`3f72cb97aa9791c95a484bbfd62d5749a37e805d`이며 tree는 source와 같다.
완료 로그 367,516bytes의 SHA256은
`2eaf7dad2ed7df8a6aa04bf896332a6963146a34ce768a544cb195d20d009660`이다.

같은 workspace 실행에서 먼저 성공한 carrot-server 테스트가 checkout 안의
`.omo/evidence/225-heartbeat/provider-controls/chunk-default-and-opt-in.json`을 생성했다.
뒤의 dev-tools `generated_mpc_cannot_use_the_unbound_foreign_provider_path`와
`independent_panda_firmware_requires_the_current_complete_source_contract`가 해당
untracked 파일을 source 불일치로 거부했다. dev-tools unit 결과는 85개 성공·2개 실패,
전체 명령은 exit101이다. source guard의 거부는 정상이며 disk admission·numpy·네트워크
실패로 해석하지 않는다. step 이름 첫 줄은 disk 검사지만 같은 step의 workspace test가
실패했다. 이 결과로 전체 필수 gate 성공이나 정상 병합을 주장하지 않는다.

carrot-server의 기존 HTTP/ureq 기록 두 곳과 process-supervision의 lock/PTY 기록
세 곳만 `CI_JOB_TEMP/test-evidence/`로 옮긴다. CI 환경이 없으면 HTTP/ureq는 소유
`tempfile` 디렉터리를, lock/PTY는 해당 테스트의 기존 소유 임시 디렉터리를 사용한다.
JSON payload·파일명·상대 하위 경로·fixture·명령·검사를 그대로 유지한다.
`tempfile = "3"`은 두 crate 모두 기존 `[dependencies]`에 이미 있으며 새 의존성이나
Cargo manifest/lock 변경은 없다. 기존 `rust-card-workspace-fixtures` 단계·path·archive
루트는 원문 그대로 유지한다. 바로 뒤의 별도 `always()` 단계가
`CI_JOB_TEMP/test-evidence/`만 `rust-workspace-test-evidence`로 수집하고, 파일이 없으면
기존 방식대로 경고한다. job-temp 정리 전 실패 시에도 별도 artifact로 증거를 보존한다.

source guard·ignore·삭제 정책·ABI·입력 pin·자원 기준은 변경하지 않는다. 로컬 확인은
Rustfmt·YAML/Bash 구문·기준 source 대비 diff와 payload/명령 보존 읽기 감사뿐이다.
unit·기능·build는 실행하지 않았다. 새 수정본의 정확한 Actions 결과는 아직 미검증이며
추가 사용자 승인에 따라 7개 파일만 한국어 로컬 커밋 하나로 보존한다.
push·취소·재실행·dispatch·PR 편집이나 부모의 future stack 적용은 하지 않는다.
Genuine의 기존 S 실행과 다른 진행 중 Actions는 그대로 보존한다.

## 2026-10-28 재개: 실제 실패 수정 #614/#615

기준 source `9e132db8da05c67678c84167f662c04fc989feae`의 실제 실패를 모아
[환경 회귀 #614](https://github.com/bin9208/openpilot-rust/issues/614)와
[전송 전 연결 실패 #615](https://github.com/bin9208/openpilot-rust/issues/615)에 기록했다.
원래 `cargo test`·clippy 명령을 복원하고 해당 단계가 실패했을 때만 나머지 테스트를
진단용으로 수집한다. 원래 실패 결과와 필수 gate는 유지한다. Xiaoge/planner 검사는
단계 위치 대신 필요한 결과 변수를 가진 유일한 aggregation 단계를 선택한다.
Android APK 검사 전에 Python을 준비한다.

webcam source 비교는 선택한 Python의 shared library 경로·실행 파일·`av` 버전을
확인한다. EGL 비교도 Python의 기존 library 경로를 보존하고 실패한 source/native의
stdout/stderr를 예외 전에 저장한다. EGL fixture 공간 검사는 폐기된25GiB 고정
잔여량 대신 해당 빌드 예상 증가량1GiB만 사용한다. loader 수정 성공과 EGL의 실제
원인은 수정 source의 Actions 결과로 확인한다.

native 입력은 수신 본문0바이트인 일시 연결 오류에만2·4·8초 간격, 최대3회,
전체1800초 안에서 재시도한다. 일부 body·영구 HTTP·크기/해시 오류는 계속 실패하며
기존 정상 자산을 덮어쓰지 않는다. 소유 curl 실행 fixture는 재시도 성공·횟수 소진·
부분 body·HTTP 오류·크기 상한 거부를 검사한다. 기능 검사는 Actions에서 수행한다.

로컬 확인은 Rust 서식, Python AST, YAML/Bash 구문과 diff다. 현재 수정의 기능 통과,
정상 병합·병합 후 검사, 공개 입력/index와 완전한 자체 빌드 후보는 아직 미완료다.
기기 시험은 #598 개발 완료 후 #528에서 사용자가 수행한다.

후속 읽기 감사에서 같은 OpenCV loader 덮어쓰기가 Xiaoge의 실제 source subprocess
세 곳에도 남아 있어 inherited library 경로를 보존했다. 공통 local-arm 셸도 실제로
선택된 X64/고정 ARM Python library를 보존하며 기존 compiler·guest pin은 유지한다.
Xiaoge의 폐기된25/35GiB 규칙은 비교별 예상 증가량128/512MiB로 바꿨다(#604).
이전 workspace job122715470848이 실제로 보고한 `native_provider.rs:981`의
`clippy::redundant_closure`도 원래 동작을 유지하는 `map(Path::new)`로 수정했다.
genuine의 자동 경로에 실제 변경된 `assets.rs`를 포함하여 후속 source 누락을 방지한다.

[bootstrap 검증 #616](https://github.com/bin9208/openpilot-rust/issues/616)은 실제
startup job122715470990의 ready timeout을 추적한다. 보존 artifact에서 provider당
31,728,032byte, launcher65,331,544byte와14개 manifest 항목, 총477,795,960byte를
확인했다. debug 빌드의 해시 작업이20초를 넘겼다는 것은 현재 추론이다. startup CI의
불필요한 debug 정보를 끄고 실제 `--verify-package`의 입력 byte·시간·종료 결과를
`bootstrap/ready/package-verification.json`에 추가한다. 원래20초 시작 deadline과
모든 manifest 검사·8개 기능 사례는 유지하며 새 source의 실제 결과로 확인한다.

## 입력·실행 검증 절차

동일 source의 foundation X64 job122828044322은 실패 뒤 나머지 target도 수집해
두 독립 회귀를 확인했다. [compiler root #619](https://github.com/bin9208/openpilot-rust/issues/619)는
비어 있는 suffix에도 `join("")`을 호출해 `/owned/Opencv/`를 만든다. root만 요구하는
환경 변수는 원래 root를 사용하고 FFmpeg/raylib/OpenCV/Eigen 경로 검사를 보강한다.
13개 group·provider 영향 범위·실제 하위 경로와 receipt/pin 검사는 유지한다.
[supervision fixture #620](https://github.com/bin9208/openpilot-rust/issues/620)은 이전 log
sentinel `normal.log`와 `normal` 사례의 stdout 파일 이름이 충돌한 문제다. 사례 출력은
`attempt-<case>.log`를 `create_new`로 만들며 기존 log byte 동일성 assertion과 실제
Cargo/하위 프로세스·다른 정상 프로세스·workspace lock·active 세대 보존을 유지한다.
운영 로그 손상으로 과장하지 않는다. 로컬 서식·diff만 확인하고 기능 성공은 새 Actions로 확인한다.

[ARM system Python #618](https://github.com/bin9208/openpilot-rust/issues/618)은
수정 source `d6a9b5f74f5e5132e0e06faba7c44cc2f19fe8cc`의 foundation ARM
job122828043659에서 확인했다. `/opt`의 고정 oracle Python은 있었지만 운영 명령 wrapper의
`/usr/bin/python3`가 없어 소유권/종료 회귀2개가 실제 취소 검증 전에 실패했다.
ARM 패키지 bootstrap에 Ubuntu system Python을 준비하고 운영과 같은 최소 환경에서
`/usr/bin/python3 -I -B -S`·ARM·ctypes를 확인한다. 고정 Python3.12.14 archive/hash/venv,
비특권 검사·원래 명령·owner death/SIGKILL·4MiB 출력 상한·offroad 정책은 유지한다.
로컬은 Bash 구문·diff만 확인하며 실제 성공은 정확 수정 source의 Actions에서 확인한다.
같은 Ubuntu24.04의 설치된 패키지 목록을 읽어 `_ctypes`가 `libpython3.12-stdlib`에
속함을 확인했다. wrapper는 `ctypes`를 사용하므로 `python3-minimal`만으로 준비하지
않고 해당 stdlib를 의존성으로 요구하는 `python3`를 설치한다.

2026-10-28의 genuine OpenCV prefix 실패는
[문자열 정렬 회귀 #617](https://github.com/bin9208/openpilot-rust/issues/617)로 추적한다.
고정185개 입력의 파일 SHA·기대 manifest SHA를 유지하고 Rust `PathBuf` component 순서를
원래 UTF-8 문자열 순서로 맞춘다. 두 순서의 실제 SHA와 수정/검사 범위는
[OpenCV prefix 기록](native-opencv-prefix-599.md)에 있다. 소유 회귀와 실제 original
prefix 준비·반복 재사용의 성공은 정확 수정 source의 Actions에서 확인해야 한다.

공개 입력 발행의 source/CI/artifact/파일 해시·기존 원격 asset 보존·중단/재개 검사는
[입력 발행 안내](native-input-publication-599.md)를 따른다. foundation의
`test_native_input_publication.py`는 소유 ZIP/API fixture와 별도 process group의 byte
상한/취소를 검사한다. fixture helper/component 파일은 실행하거나 실제 release에 보내지
않는다. 실제 읽기 검증/발행/공개 URL 검증과 index의 정상 PR·병합 후 결과는 별도로 확인한다.
PR 연결 정보가 후속 push로 바뀌는 경우는 원래 `GITHUB_EVENT_PATH`와 실제 checkout
commit을 보존한 작은 validation-context artifact로 검사한다. 실제 SHA-1/부모를 가진
shallow Git merge, artifact API/digest·보존 bytes 변조·원래 PR/base/attempt 차이와
main PR 유지 회귀도 같은 소유 fixture에 포함한다. 이 추가 회귀의 실행 성공은 아직 미확인이다.

Rust component 준비의 `memory limit reached` 실패와 추가 검사 절차는
[XZ decoder 한도 #612](native-rust-toolchain-599.md)에 기록했다. foundation의
`toolchain` integration test는128MiB 선언의 이전 한도 거부와 수정 후 준비·반복 재사용,
512MiB 거부/이전 세대·로그 보존을 확인한다. 실제 공식 archive와 compiler/Cargo 실행은
같은 수정 SHA의 genuine workflow에서 확인하며, 로컬 기능 시험으로 대신하지 않는다.

현재 Git index·기기별 정책·초기 빌드/기존 세대 부팅·native updater의 연결은
[bootstrap 실행 안내](native-bootstrap-seed-599.md)의 첫 절을 따른다. foundation의
`test_native_source_entry.py`는 metadata initial/repeat/docs/단일 provider 변경,
변조/공간 부족, 원자적 시작 기록과 기존 부팅에서 재빌드/Git index 읽기 없음을 검사한다.
기존 부팅 검사는 실제 exec 경계에서 멈추며 fake helper를 실행하지 않는다.
Rust의 selection/launcher 검사는 잘못된 초기 선택·손상된 active 보존과 protocol을 검사한다.
이 검사는 Actions에서 실행하며 로컬에서는 구문·서식·diff만 확인한다.

genuine workflow의 추가 `git-index-initial`/`git-index-reused`/`git-index-docs` 사례는 실제 seed/SDK와
15 provider descriptor의 후보를 작업 소유 Git fixture에만 커밋한다. factory isolated
Python에서 운영용 입력 선택기를 호출하고 그 설정으로 실제 native helper의 전체 입력
준비를 실행한다. 첫 metadata는 소유 artifact에서 공급하며, 그 초기 준비와 반복/docs를
구분한다. 반복/docs는 metadata 전송/저장0, provider/모델 재전송0·compiler fresh·모델
재생성0·composition 재복사0을 요구한다. 기존 직접 설정 사례의 초기 namespace와
metadata cache의 SDK receipt 경로가 다르므로 두 초기 준비를 같은 cache 결과로 추정하지 않는다.
`runtime-inputs-evidence/*-resolved.json`, 세 native 준비 JSON과 `summary.json`에서
workflow 원본 SHA와 별도 fixture SHA, index/config pin 및 host cgroup 증거를 구분한다.
공개 HTTP 자산 소비, source entry의 전체 build/sudo/부팅 검사를 대신하지 않는다.

검증 후 개발 환경에서 지원하는 진입 형태는 다음과 같다. 공개 input index와 seed가
준비되기 전의 기기 설치 명령이나 현재 기능 성공 증거가 아니다.

```sh
/usr/bin/python3 -I -B -S tools/native/source_entry.py \
  --source "$REQUESTED_SOURCE" --machine-config "$MACHINE_CONFIG" \
  --machine-configuration-sha256 "$MACHINE_CONFIG_SHA256"
```

정상 시작은 검증된 기기 설정을 읽는 `launch_openpilot.sh`의 native 경로다. 업데이트는
기존 updater가 현재 요청 source의 입력을 사용하며 이전 정상 설정을 덮어쓰지 않는다.
정확 SHA의 foundation/필수 PR 검사, genuine AGNOS user space의 원래 검사와 추가 입력 준비,
실제 전체 진입점·정상 boot/update/복구 검증을 각각 확인한 뒤 최종 기기 절차로 진행한다.

[전체18개 입력 Layout과99개 실제 빌드 연결](native-complete-inputs-599.md)에 eGPU 모델의
독립 cache, 완전한 source/data 검증, source-built 봉인/ABI 및 반복·docs·작은 logger 변경의
재사용 검사를 연결했다. 새 source의 Actions 실행 결과는 아직 미확인이다.

[모델 준비 wheel·factory compiler 안내](native-model-wheel-inputs-599.md)에 NumPy/zstandard
내용별 준비와 원래 C 라이브러리 대조, 실제 factory LLVM20.1/QCOM 경로의 Actions 검사를
추가했다. ONNX 전체 생성·생성물 cache와 전체 native runtime 수용은 계속 남아 있다.

모델 준비용 [ARM Python 입력과 실행 검사](native-python-inputs-599.md)를 추가했다.
공식 CPython3.12.16 archive를 내용별로 준비하고 실제 ARM interpreter/stdlib/ELF closure와
반복 전송·복사0을 Actions에서 확인한다. 새 결과와 전체 모델 환경·생성물 cache는 아직 대기다.

현재 source와 독립 Panda firmware의 입력871개·출력4개/원래 provenance를 결속하는
[Panda 준비·검사 안내](native-panda-inputs-599.md)를 추가했다. 현재 source binding,
공개 debug 서명·버전, 반복 전송/복사0을 Actions에서 검사하며 새 결과는 대기다.
전체 native 빌드/실기 flash·수용이 완료된 상태는 아니다.

기준일: 2026-10-28 KST. 상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598),
환경 [#599](https://github.com/bin9208/openpilot-rust/issues/599),
캐시 [#600](https://github.com/bin9208/openpilot-rust/issues/600),
전환분·성능·전달 [#601](https://github.com/bin9208/openpilot-rust/issues/601).

## 현재 검증 범위

빌드 기반 PR #602는 source `13d7ce113bf602b2be5b5abedb55c4debeb83a8c`의 전체68개
검사 성공 후 `dev`의 `e445c9d19c3041bc79d8624589e2eb6ba7d061c6`으로 병합했다.
[x64/ARM 집중 검사](https://github.com/bin9208/openpilot-rust/actions/runs/40274067028),
[필수 PR Rust 검사](https://github.com/bin9208/openpilot-rust/actions/runs/40274071261),
[필수 PR 통합 검사](https://github.com/bin9208/openpilot-rust/actions/runs/40274071270)는
성공했다. 병합 후 검사는 별도로 확인한다.

`openpilot-runtime-assets`는 대형 자산을 위한 내용별 캐시 준비 도구이며,
`openpilot-runtime-build`는 준비된 offline Cargo 의존성으로 지정 binary를 빌드한다.
전체 AGNOS recipe·toolchain 준비·ABI 봉인·세대 활성화·기존 startup/update 연결은
아직 완료되지 않았다. 아래 CI fixture 결과를 장치 설치 가능 증거로 사용하지 않는다.

## 개발 검증 실행 방법

원래103개 RadarCAN DBC를 source 입력과 결속해 별도 component로 재사용하는 명령,
Python 원본 재생성 oracle·실제 ARM Rust loader 및 Layout/봉인/시작 거부 회귀는
[DBC 준비 안내](native-dbc-inputs-599.md)를 따른다. 새 source의 Actions 성공은 아직 대기다.

모델 cache와 분리한 불변 export, 현재 Git·compiler source/출력 전체 검증, 모델이 빠진
native build/잘못된 Layout 거부와 봉인/source generation 시작 검사는
[모델 실행 입력 안내](native-model-runtime-inputs-600.md)를 따른다. 실제 전체 봉인과
comma 3X 부팅·녹화·업로드 수용은 아직 남으며 새 Actions 결과를 확인해야 한다.

내부 QCOM 주행/운전자 ONNX의 실제4개 pipeline 생성, 종류별 입력 cache와 docs/작은
logger 변경의 생성·QCOM 호출/전송0 검사는 [모델 생성·재사용 안내](native-model-generation-cache-600.md)를
따른다. 새 Actions 결과는 대기다. GPU 실행·전체 runtime 부팅/녹화/업로드와 사용자
comma 3X 수용을 이 카탈로그 검사로 대신하지 않는다.

현재 Git의 LFS SVG/ONNX4개 준비, 문서/한 객체 변경의 전송·복사 재사용과 실패/위조/
중단/동시 실행 거부 및 ARM 실제 byte 검사는 [LFS 입력 안내](native-source-lfs-599.md)를
따른다. raw source 준비를 모델 생성·펌웨어 준비·전체 runtime/기기 검증으로 표현하지 않는다.

실제 manager 시작 목록의 ffmpeg/ffprobe·클러스터 글꼴 준비, CLI lossless 왕복과
현재 Rust glyph/software-window의 정확 픽셀 비교는
[도구·글꼴 검사 안내](native-media-tools-font-599.md)를 따른다. 전체 runtime/기기 수용을
대신하지 않으며 새 Actions 결과는 대기다.

공용 library53개 경로 중 남은 SDL/TTF/글꼴 dependency/TurboJPEG11개, 원래 alias/notice,
실제 Rust virtual device·정확 JPEG 비교와 ARM ELF 검사는
[SDL·TurboJPEG 입력 안내](native-display-inputs-599.md)를 따른다. 전체 GUI/하드웨어/모델
회귀와 정상 기기 동작은 별도로 확인하며 새 Actions 결과는 아직 대기다.

생성 MPC 입력4662개와 원래 output/현재 source를 함께 묶어 기존 C/C++ solver를
재사용하는 조건, 변경·누락 거부 및 실제 ARM Rust/direct C ABI12 solve 비교는
[MPC 소스 결속 안내](native-mpc-source-bindings-599.md)를 따른다. 새 생성·전체 Python/
Cython planner 회귀와 실기 수용은 별도로 남으며 Actions 성공 전 실행 통과로 쓰지 않는다.

전체99개/13그룹의 전이/default feature에 따른 compiler 입력 목록과 필수 Tree 결속은
[compiler 입력 계약](native-compiler-input-contract-599.md)을 따른다. catalog audit의
compiler_inputs_bound와 compiled/activated를 구분하며 engineering plan을 전체 runtime
완료로 사용하지 않는다.

기존 OpenCV4.13.0의168개 source header와 원래 생성 설정 헤더2개 준비, 반복 압축 해제
생략/전송0/복사0과 ARM API object 검사는 [OpenCV 헤더 안내](native-opencv-headers-599.md)를
따른다. 같은 shared library5개와 headers/alias185행을 구성하고 실제 ARM Rust bridge를
검사하는 방법은 [OpenCV prefix 안내](native-opencv-prefix-599.md)를 따른다. 전체
webcam/Xiaoge 원본·모델 회귀와 실기 검증은 별도로 필요하다.

기존 UI raylib의 원래 헤더/static archive와 독립 shared provider 준비, 반복 전송/복사0,
캐시 영수증 위조 거부 및 실제 ARM loader 컴파일/정적 ABI 검사는
[raylib 입력 안내](native-raylib-inputs-599.md)를 따른다. GPU 화면 출력과 전체 runtime
시작 성공은 이 정적 검사로 대신하지 않는다.

기존 estimator Eigen3.4.0 입력을 원래 wheel에서 준비하고 필요한 wheel만 선택하는 방법과
전송/복사0·실제 C++ helper 검사는 [Eigen 입력 안내](native-eigen-inputs-599.md)를 따른다.
이 검사를 전체 locationd/paramsd 또는99개 runtime의 통과로 대신 쓰지 않는다.

전체 generation의 모든13그룹에 같은 실제 `/` SDK를 필수로 묶고, 그 세대 소스의
전체 SDK profile과 일치하는지 확인하는 회귀는 [SDK 검사 안내](native-system-sdk-599.md)를
따른다. profile 회귀와 실제 SDK namespace 검사, 전체 runtime 빌드/실기 수용을 구분한다.

FFmpeg6.1.1의 header/provider를 실제 Rust 인코더가 소비하는 단일 prefix와 원본5개
codec case의 정확 packet 비교는 [provider 검사 안내](native-provider-components-599.md)의
단일 prefix 절차를 따른다. 변경 SHA의 foundation 및 genuine 결과가 성공하기 전에는
실행 성공으로 판단하지 않는다. QEMU/PRoot AGNOS user space 결과는 실제 comma 3X의
설치·부팅·카메라/녹화·업로드 수용과 구분한다.

Rust compiler/Cargo/std 내용별 준비·전송0/복사0·stdlib 변조 검증은
[고정 native Rust 준비 안내](native-rust-toolchain-599.md)를 따른다. compiler 실행과
전체 runtime 수용은 별도 단계이며 새 source의 Actions 성공을 먼저 확인한다.

capnp SDK의 입력 pin·그룹별 도구 검사와 전체 원본 schema/wire 비교는
[SDK 안내](native-capnp-sdk-599.md)에 있다. 이 SDK 준비·예제는 CI 개발 검증이며,
장치 설치 가능한 전체 후보의 준비 완료를 뜻하지 않는다.
프로젝트의 `--prepare-capnp`는 같은 고정 .deb를 installer 실행 없이 준비한다.
검사에서는 독립 dpkg-deb의 vendor tree 전체 일치와 실제 source-built ARM 준비기의
초기/반복 전송0·반복 복사0·같은 tree SHA를 확인한 뒤 원본 wire 비교를 수행한다.

외부 Cargo crate의 원본 고정·내용별 준비와 driver의 directory-source 연결은
[Cargo 입력 안내](native-cargo-inputs-599.md)를 따른다. empty Cargo home의 실제 빌드,
반복/docs-only freshness와 source·lock 변조 거부를 Actions에서 확인한다.
genuine의 ARM 준비 검사는 host에서 만든 seed의 검증/재사용이며 새 기기 준비 실측이 아니다.

처음 Rust 입력 준비 도구를 얻는 순환은 [고정 bootstrap seed](native-bootstrap-seed-599.md)에
정리했다. 후보 추출·독립 descriptor pin·실제 AGNOS ABI·새 Rust prefix의 compiler 검사를
Actions에서 수행한다. 공개 seed/기본 시작과 전체 기기 수용은 아직 준비 완료가 아니다.

로컬에서는 `git diff --check`, scoped `rustfmt --check`, TOML/YAML 문법과 diff만
확인한다. 기능·빌드는 GitHub Actions에서 실행한다. 작업 branch push는
`Runtime source build foundation`의 x64/ARM matrix를 실행한다. 같은 변경의 성공한
검사를 반복하지 않는다. 의도적으로 별도 재현이 필요할 때만 다음 명령을 쓴다.

```sh
gh workflow run runtime-source-build.yml --repo bin9208/openpilot-rust --ref BRANCH
gh run list --repo bin9208/openpilot-rust --workflow runtime-source-build.yml --branch BRANCH
gh run view RUN_ID --repo bin9208/openpilot-rust --log-failed
```

CI의 실제 명령은 다음과 같다. 실제 fixture는 격리된 임시 Git checkout과 작은 입력을
사용하며 사용 중 runtime·기기·로그를 수정하지 않는다.

```sh
cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-runtime-build -- --nocapture
cargo clippy --manifest-path rust/Cargo.toml --locked -p openpilot-runtime-build --all-targets -- -D warnings
```

| 검사 | 합격 결과 |
| --- | --- |
| 같은 model/provider/toolchain 준비 반복 | 두 번째 전송 호출0, `transferred_bytes=0`, 기존 내용 동일 |
| provider 하나 변경 | 변경 provider만 전송, 기존 model/toolchain 전송0, 이전 provider 보존 |
| 잘못된 hash/길이·변조된 캐시 | 실패, 잘못된 blob 미발행, 다른 자산 보존 |
| 다운로드 중 실패·자원 부족·동시 owner | 실패, 기존 자산과 runtime 보존 |
| symlink cache/blob·비 HTTPS 입력 | 외부 파일 수정 없이 실패 |
| 문서만 수정한 새 source commit | Cargo `fresh=true`, 같은 binary hash, 보고서 source는 새 commit |
| runtime source 수정 | 수정 대상 재빌드·출력 변경, 관련 없는 target은 fresh/같은 hash |
| 미커밋 입력·빌드 중 source 변경 | 성공 보고서 발행 거부, 이전 성공 보고서 보존 |
| 압축 해제된 provider/도구체인 tree 변경 | 같은 크기의 파일 교체·권한 변경·파일 추가/누락·link 변경을 거부 |
| 변조 후 receipt만 재생성 | pinned receipt SHA가 다르므로 거부; 검증된 새 입력/receipt를 고정해야 별도 cache에서 빌드 |
| 의도적 compile failure | 이전 binary가 계속 실행되어 원래 출력 유지 |
| 빌드 frontend에 TERM 또는 KILL·worker KILL·IsOnroad 전환 | 실제 Cargo/build script/손자 프로세스만 종료, 정상 runtime 표식·로그 보존, workspace lock 반환 |
| 부팅 대기 기록의 부분 쓰기·다른 config·미검증 후보·동시 실행 | 기존 active pointer·대기 기록·로그 보존, 미검증 세대 활성화 거부 |
| updater용 Git checkout | 실행 소스와 다른 경로·자체 `.git` directory, source store의 object만 공유, dirty checkout 자동 reset 금지 |
| 실제 kernel memory 한도·파일 증가·frontend 부모 종료 | 한도 초과 또는 취소에서 detached 하위 작업 종료, 정상 프로세스·active/log 보존, 실패 기록 유지 |

자원 제한의 별도 집중 검사는 `native-resource-guard.yml`이다. 해당 작업 branch의 push로
실행하며 결과는 다음처럼 읽는다. 실제 root scope 권한 하강, 실제 메모리 할당/파일 증가,
worker/frontend/부모 강제 종료를 CI에서 검사한다. 준비·정리는 자기 job의 cgroup만
다룬다. production scope 준비는 자기 UID의 고정 경로만 허용하며 공유 controller는
변경하지 않는다. [자원 경계와 측정 한계](native-build-resource-guard-600.md)를 함께 확인한다.

```sh
gh run list --repo bin9208/openpilot-rust --workflow native-resource-guard.yml --branch BRANCH
gh run view RUN_ID --repo bin9208/openpilot-rust --log-failed
gh run download RUN_ID --repo bin9208/openpilot-rust --dir EVIDENCE_DIRECTORY
```

artifact의 `native-resource-tests.log`와 시도별 `resources.jsonl`을 함께 읽는다.
kernel memory peak, 한도 event와 실패/성공 상태를 확인하고 최종 행 누락은 성공으로
판정하지 않는다. 디스크 값은 filesystem 가용 공간 감소의 관측값이며 hard quota가 아니다.
이 검사는 X64 host kernel의 격리 근거다. comma 3X kernel 검증은 아래 실기 수용에 남아 있다.

native 빌드 worker의 원문 stdout/stderr는 시도별 `build.log`에 보존한다. updater로 전체
출력을 전달해 그 프로세스의 메모리를 늘리지 않는다. 실패 메시지는 경로와 최대64KiB의
원문 tail을 제공하므로 자세한 원인은 해당 파일과 `resources.jsonl`을 함께 확인한다.
큰 synthetic 출력 flood의 CI artifact에는 제한된 tail만 포함하며, 그 검사를 정상 runtime
부팅 성공으로 판정하지 않는다. 실제 배포 전 이 로그 경로와 자원 제한 검사도 통과해야 한다.

자산의 보고 시간·저장 byte 수는 준비 작업의 수치다. HTTP `transferred_bytes`는 본문
크기이며 TLS/header를 포함한 네트워크 전체 사용량이 아니다. host fixture 시간과
메모리·디스크 값을 콤마의 전체 runtime 수치로 확대하지 않는다.

실제 압축 해제 입력을 recipe에 연결할 때는 `openpilot-runtime-inputs`로 tree 목록을
생성하고 `inputs`에서 receipt SHA를 고정한 다음 `input_trees`에 연결한다. 출력은
입력 tree 밖에 저장한다. 이 도구는 원본 자산의 출처를 스스로 증명하지 않는다.
새 receipt를 신뢰할 근거는 고정된 source 또는 hash 검증한 자산과 그 준비 과정이다.
`verified_input_trees`가 비어 있는 보고서는 전체 toolchain/provider를 검증했다는
증거가 아니다. AGNOS 시스템 라이브러리·ABI closure는 별도 대상 검사를 유지한다.

[AGNOS 시스템 SDK 검사](native-system-sdk-599.md)는 image에서 만든 독립 pin과 실제
compiler root `/`를 연결한다. foundation 회귀7개와 genuine의 실제 build 초기/반복,
같은 크기 헤더 변조 거부·이전 실행 파일 보존·전체 선언 범위 복구 검사를 확인한다.
이 새 검사의 성공은 정확 source Actions 결과에서 별도로 기록한다.

[Codec 입력 검사](native-codec-inputs-599.md)는 기존 FFmpeg/libyuv의 원본 wheel pin을 유지하고
내용별 준비 초기/반복, 한 provider 변경 시 무관 prefix 보존, 로컬 receipt 위조 거부와 실제
AGNOS의 두 C/C++ API 실행을 확인한다. 전체 encoder/FFmpeg closure와 기기 수용은 별도다.

전체 runtime의 실제 b555 기준은 [FFmpeg6.1.1 header 검사](native-ffmpeg-headers-599.md)에서
따로 확인한다.140개 원래 header의 초기/반복·위조/누락 거부와 실제 ARM module object를
검사하며, 별도7.1 codec wheel이나 host header로 대체하지 않는다.

[Shared provider component 검사](native-provider-components-599.md)는 원래 foreign provider만
생산·소비하며 전체 runtime/source 재표기를 거부한다. 독립 descriptor pin·왕복/반복0과
실제 ARM의 header/shared ABI·엄격한 ELF·foreign CLI 실행을 확인한다. 공개 발행/HTTPS는 별도다.

2026-10-26 추가한 generation 시작/업데이트 연결은 아직 Actions 검증 전이다.
`--stage-update`는 source-built 후보를 봉인하고 기존 active ID와 config hash에 묶어
`pending.json`을 발행한다. launcher가 boot lock을 잡은 뒤에만 다시 검증·선택하고,
이전 세대 라이브러리가 섞이지 않도록 새 세대 loader 경로로 기존 lock handoff를 쓴다.
protocol 응답은 이 기능의 지원 여부이며, 정상 manager 부팅·모델·기록·업로드가
완료되었다는 시험 결과가 아니다. 최종 전체 AGNOS recipe와 아래 실기 수용은 남아 있다.

## 최종 사용자 기기 수용 시험

설정/source 연결 거부는 foundation의 `runtime-build/tests/configuration.rs`, pending 회귀,
launcher의 실제 handoff 회귀와 updater의 작업별 Context 회귀를 확인한다. 최초 설정의
SHA·요청 commit/tree·candidate source가 이어져야 하며, 변경 거부 뒤 이전 선택·pending·
log byte가 유지되어야 한다. 이 metadata/CLI 검사는 정상 업데이트·부팅 수용을 대체하지 않는다.

그 이전의 첫 도구 재빌드 검증은 [bootstrap 경로](native-bootstrap-seed-599.md)를 따른다.
foundation의 SDK pin/가짜 controller 거부, kernel workflow의 실제 권한 하강과 실패 로그,
genuine workflow의 ARM13개 도구 initial/reused/docs/small 결과 및 private cgroup JSONL을
각각 확인한다. X64 host kernel의 peak를 AGNOS kernel 또는 comma3X의 값으로 표현하지 않는다.
13개 준비 도구의 성공과 전체99개 runtime/정상 부팅 준비 완료는 구분한다.

현재 도구와 전체 입력 준비의 연결은 [운영용 준비 경로](native-bootstrap-seed-599.md)를
따른다. foundation은 실제 Cargo feature를 바꾼 executable 실행/metadata 대조와 기존
schema3 report 호환, 잘못된 selection pin의 시작 전 거부, Python 입력/출처 회귀를 검사한다.
genuine workflow는 원래 명령 뒤 별도 hard cgroup에서 운영용 provider/model 준비의
initial/reused/docs를 실행하고, compiler 입력9종·runtime 역할18종·전체 catalog가
결속됐는지 확인한다. `runtime-inputs-evidence/summary.json`, 시도별
`runtime-inputs/result.json`/`operations.json`, `resources.jsonl`과 실제 명령 종료 상태를
함께 읽는다. 생성/전송/복사0, 기존 선택/log 보존은 성공한 정확 SHA의 결과로만 인정한다.
새 구현은 아직 기능 검증 대기이며, 이 입력 준비 성공도 전체99개 빌드·정상 부팅 수용을
대체하지 않는다.

전체 요청 실행기는 같은 [bootstrap 안내](native-bootstrap-seed-599.md)의
`source_runtime.py`를 따른다. 현재 도구로 입력을 준비한 뒤 정확 source/config/pin의
전체 build/seal 결과를 받으며, 업데이트는 기존 부팅 설정 아래에 새 세대를 pending으로
등록한다. foundation의 `test_native_source_runtime.py`는 tool hash/mode/inventory,
seed 준비 중 source drift, 원래 요청자 종료와 실행기 SIGKILL, 출력 byte bound를 검사한다.
kernel resource 검사에는 원래 요청자/중간 실행기 중 각각 하나만 종료되고 다른 하나가
살아 있는 두 사례가 추가된다. 실제 build/부팅 성공으로 확대 해석하지 않는다.

정확 소스 `2b896bd0e32764ddc7f19bcda1ef935ac79ec2c7`의 실제 loggerd/logmessaged
X64/ARM 각각6개 비교는 [#608 결과](native-source-stamp-reuse-608.md)에 있다. 문서 변경은 두
출력 fresh/같은 hash, loggerd만 수정하면 무관한 logmessaged fresh/같은 hash,
legacy source stamp 보존을 확인했다. 전체99 runtime/AGNOS/실기 수용은 별도다.

전체 계획의 컴파일 입력 연결 검사는 [native 입력 조합기](native-input-bindings-599.md)의
Actions 절차를 따른다. 실제9개 입력 역할의 pin·13그룹 환경·반복 plan·발행 거부를
검사한다. 실행 자산/모델·전체 build·기기 수용의 준비 완료 판정은 별도로 유지한다.

[실행용 foreign library 검사](native-runtime-companions-599.md)는 네 component의 독립
pin·반복 전송/복사0, 실제 ARM 수치·기하·압축 API와 ELF,13그룹 계획의 runtime Tree
연결을 확인한다. acados 생성 solver 두 개는 별도 필수 준비/회귀가 남는다.

전체 build 목록·현재 Cargo 소스 대조와 입력별 recipe 생성 명령은
[native runtime 계획 안내](native-runtime-plan-599.md)에 있다. 이 도구의
`compiled=false`/`activated=false`는 실제 의미 그대로이며, plan 생성 성공을 전체
runtime 빌드나 사용자 기기 시험 준비 완료로 판정하지 않는다.

사용자가 선택한 최종 수용 장치는 **comma 3X**다. C3X의 실제 AGNOS 및 QCOM/eGPU 경로,
모델·provider 구성을 기록하고 C4 또는 Ubuntu ARM 결과로 그 수용을 대신하지 않는다.

정확 소스의 전체 AGNOS build·seal·업데이트/복구 CI와 후보 준비 후 사용자가 수행한다.
현재 부분 도구를 차량에 설치하는 명령은 제공하지 않는다. 최종 후보의 branch·SHA와
실제 지원 명령은 준비된 후보의 기록으로 고정한다.

1. 기기 종류, AGNOS 버전, 시작 branch/SHA, model/provider/toolchain ID, 남은 공간을
   기록한다. 기존 정상 세대와 로그를 보존하고 주차 상태에서 시작한다.
2. 해당 SHA의 프리빌트 runtime release가 없는 소스로 처음 빌드한다. 전송 byte 수,
   빌드 대상·소요 시간·peak 메모리·디스크 증가량과 source/result manifest를 저장한다.
3. 같은 소스로 반복하고 문서만 변경한 커밋으로 갱신한다. 대형 자산 전송0, 전체
   runtime 재다운로드0, 빌드 대상 fresh, 결과 hash가 동일한지 확인한다.
4. 작은 runtime 수정 후 갱신한다. 변경 대상과 필수 의존 대상만 재빌드했는지 확인한다.
   provider 하나를 갱신할 때도 나머지 model/toolchain 전송0을 확인한다.
5. 준비된 실패 주입 절차로 build 실패·취소·동시 실행·자원 부족·잘못된 자산·전원 중단을
   확인한다. 검증되지 않은 세대로 전환되지 않고 기존 세대와 로그가 보존되어야 한다.
6. 정상 업데이트 후 재부팅해 기본 화면·manager·모델·카메라·녹화를 확인한다. 기존
   업로드 경로로 한 구간을 업로드해 결과 페이지와 실제 로그의 실행 SHA·모델·오류를
   대조한다. 기기별 결과를 분리하고 주행 수용은 사용자가 기록한다.
7. 같은 장치·모델·기능·입력·전원/냉각 조건으로 선정 성능 변경 전후를 비교한다.
   CPU·메모리·지연·프레임 누락과 기능 회귀의 실제 측정값/한계를 남긴다.

2026-10-28 사용자 지시에 따라 #598은 개발·정확 소스의 CI·정상 병합과 병합 후 검사,
검증된 완전한 후보 및 테스트 절차 제공을 마친 뒤 닫고 사용자 기기 테스트로 넘긴다.
실제 자체 빌드·정상 부팅·기록·업로드와 성능 수용은 기존 #528에서 별도로 기록한다.
#598의 개발 완료나 CI 성공을 실제 기기 수용으로 대신 체크하지 않는다.

## 2026-10-29 실패 묶음과 후속 검증

`b0d1d9f2efe0fa6a11abf877ce3ac35999899b75`의 fast·integration은 성공했지만 foundation,
genuine, kernel, docs와 여러 runtime job은 같은 `path_is_empty` E0658로 실패했다.
`PathBuf` suffix에 stable `as_os_str().is_empty()`를 사용하고 fast에 Rust1.94.0의
runtime-build 전체 target 타입 검사를 추가한다(#619). 기존 필수 명령은 유지한다.
ARM 소유권 fixture의 자식은 최소 환경에서 실행 가능한 `/usr/bin/python3 -I -B -S`를
사용한다. 부모 oracle의 `/opt` Python loader를 production 격리 환경에 섞지 않는다(#618).

구소스 genuine40691750328은 이미 실패 종료됐으며 취소할 실행은 없었다. 그 실패의
companion 부모 디렉터리 누락은 Actions 소유 work에서 고친다(#621).
Planner X6440703120388/job122883492732의 SIGILL은 원인 미확정이며 실제 PC/CPU/역추적을
실패 artifact에 추가한다(#622). ARM/job122883492855는 원본·Rust 각2회/121메시지의
출력이 같지만 유효 게시0으로 실패했다. model_done 간격 평균135.45/77.54ms를 확인했고
동일 fixture 입력의 생성·직렬화를 timed 전송 앞에 옮긴다. 기존40입력/주기/bit 비교와
유효 게시/trigger/overlay 필수 기준은 유지한다. planner helper에 남은 고정25GiB만
제거하고 기존128/512MiB operation 예상 증가량을 유지한다(#558).

사용자는 이 작업의 구소스 검사 취소와 최신 수정본 우선 검증을 승인했다. 다른 세션과
최신 source 검사는 보존한다. 자동화598은 삭제됐으며 새 주기 프롬프트를 만들지 않는다.
이 수정의 기능 성공·필수 gate 통과·완전 후보 준비는 아직 확인 전이다.

## 필수 helper의 잔여 고정 공간 규칙 읽기 감사 — #623

필수 Rust/Integration workflow가 직접 부르는14개 Python helper에서 남은25GiB 항을
제거하도록 수정한다. JPEG/USB GPU/Params/location 빌드의 원래1/2GiB 예상 증가량,
Xiaoge memory의2GiB, GEOS의 실제 payload+1MiB, recovery의 growth+25MiB,
webcam의 명시 reserve_mib와 RadarCAN의 실제 growth는 유지한다. 원본·수치·유효성·
메모리/보안 검사와 명령은 그대로다. local AST/diff만 확인했으며 정확 후속 source의
Actions 결과와 정상 PR/병합 후 근거는 아직 확인 전이다. 현재 d0 검사는 보존한다.

## 기존 X64 ABI 실패를 긴 ARM 준비 전에 확인 — #622

이전SIGILL은 Python 원본 ABI가 evidence/input.json을 남기기 전 종료한 실패다.
같은 generator/NumPy/Cython/wheel/원본 생성·컴파일 도구를 재사용한 작은 원본 ABI 검사를
genuine의 긴 ARM 입력/전체 빌드 전에 추가한다. 실패132를 그대로 반환하고 같은 실패만
GDB로 재현해 CPU/PC/명령어/library/역추적을 남긴다. 기존 Rust의 원본/native bit 비교와
모든 genuine ARM 명령·pin은 유지한다. 진단 성공이나 source-only smoke를 필수 전체
통과로 대신하지 않으며 해당 후속 정확 source 결과는 아직 확인 전이다.

## 실제 runner의 ISA와 수치 연산 경로 — #622/#624

source `46f5758e24160cce167ab19ab9e8c05828a20d66`의
[genuine40814444977/job123252682506](https://github.com/bin9208/openpilot-rust/actions/runs/40814444977/job/123252682506)은
긴 ARM 준비 전에 원본 ABI의 SIGILL을 재현했다. GDB PC는
`libblasfeo.so:blasfeo_dvecse+35`, 명령은 `vbroadcastsd %xmm0,%ymm1`이다.
실제 Xeon E3-1230 V2에는 AVX만 있고 AVX2/FMA는 없으나 wheel의 헤더는
`X64_INTEL_HASWELL`과 AVX2/FMA 필수를 지정한다. 진단 artifact는
[12872915150](https://github.com/bin9208/openpilot-rust/actions/runs/40814444977/artifacts/12872915150)이다.

X64 Actions 비교용 C SDK를 실제 CPU에 맞춰 소스 빌드한다. acados/BLASFEO/HPIPM/
qpOASES revision은 기존 license source와 acados gitlink를 모두 대조하며,
[고정 upstream recipe](https://github.com/commaai/dependencies/blob/d78ef2d42d1cfbda02eabec706839a70bc762145/acados/build.sh)의
같은 C 소스와 QPOASES 설정을 사용한다. AVX 호스트는 `X64_INTEL_SANDY_BRIDGE`,
그 밖에는 `GENERIC`을 선택한다. 원래 generator wheel/파일 검사와 ELF/파일 목록·SHA
검사는 유지한다. 실제 SDK revision/compiler/recipe/output SHA를 원래 생성 결과의
별도 provenance에 기록하며, 한 job의 소유 임시 경로에서만 immutable SDK를 재사용한다.
기기용 ARM wheel/provider와 그 출력 pin은 바꾸지 않는다.

`build_plannerd_acados.py`는 고정 ARM MPC source contract의 입력이므로 원래
`a3d0e3a497b4ae4d23c7cf5ee2275177600bbb31`의 bytes로 복원한다.
SHA256은 `9f18eda8bb1764f77227c5a4cbaf12258add346c7833abfcf545761ce71e8199`다.
CI는 명시적인 `plannerd_ci_builder.py -- <원래 builder> <원래 인자>`를 사용해
원래 main과 전체 인자를 실행한다. solver와 원본 Cython binding 모두 기존128MiB
작업 증가량을 적용하며, source contract 자체나 원래 generation namespace는 완화하지
않는다. 명령 보존 회귀와 frozen builder의 실제 Git 원문 bytes 회귀로 경계를 확인한다.

같은 source의 [estimation job123254262542](https://github.com/bin9208/openpilot-rust/actions/runs/40814449746/job/123254262542)은
lagd의 상수/mask/주기/sparse 비교가 실패했다(#624). 고정 NumPy2.5.3의
X86_V2 complex multiply는 곱셈/덧셈을 각각 반올림하지만 Rust는 항상 `mul_add`를
사용했다. 고정 원본의 X86_V3/ASIMD dispatch와 같은 CPU 조건으로 융합 경로를
선택하고, 비융합 결과와 실제로 달라지는 cancellation 회귀를 더한다. 입력·PocketFFT·
허용 오차·유효성·delay 정책은 유지한다. 결과에 실제 NumPy CPU/ufunc dispatch도
남긴다. 이 수정의 기능 성공은 후속 정확 source의 Actions에서 확인해야 한다.

host transport 기록은 x86_64 Linux `7.0.2-6-pve`, AArch64 binfmt handler 없음이다.
QEMU/PRoot component 성공이 실제 root/sudo를 포함한 전체 source entry 성공을 뜻하지
않으며, 해당 잔여 검증과 완전 후보·발행·정상 병합 조건은 계속 남아 있다.

## 최소 ARM 도구와 라이선스 표시의 실제 실패 — #625/#626

PR head `c7e7c80deafc729839e163f3022dd197d7cc52a9`, 실행 merge
`83515e47f74198c7a1059ac8d64dc51424c8ca49`의
[WebRTC ARM job123265711271](https://github.com/bin9208/openpilot-rust/actions/runs/40818463694/job/123265711271)은
두 native 서비스/예제 빌드·library5개·Clippy를 통과한 뒤 `file: command not found`로
exit127 종료했다. 이어질 소유권·미디어·HTTP 비교는 아직 실행되지 않았다. 공통 ARM
bootstrap에 `file`을 설치하고 실제 system Python을 ARM ELF로 분류해 도구/실행 가능성을
느린 Cargo 빌드 전에 확인한다. 원래 모든 명령과 조건은 유지한다.

같은 head의 [genuine job123265022423](https://github.com/bin9208/openpilot-rust/actions/runs/40818458078/job/123265022423)은
실제 C source를 받은 뒤 license 대조에서 끝났다. 고정4개 Git blob을 읽어 비교한 결과
qpOASES 원문의 form feed9개만 고정 notice에서 문자 `^L`로 표시되어 있었다.
원문26436bytes의 SHA는 `5df07007198989c622f5d41de8d703e7bef3d0e79d62e24332ee739a452af62a`,
기존 notice26445bytes의 SHA는 `e5d44cfd3a10f2f0e54b262b46089845c9ef27d288ab678240d7ef6d73d58b19`다.
upstream revision과 raw source SHA, 기존 notice SHA,9개 표시 변환 뒤 전체 bytes 일치를
각각 확인한다. source contract의 notice와 sources.json은 그대로 유지하며 원문도
CI artifact에 따로 보관한다. 원문/notice 변조와 이미 표시한 notice를 raw 원문으로
제출하는 경우를 거부하는 회귀를 추가한다. 검사 자체를 생략하거나 whitespace를
일반적으로 정규화하지 않는다.

후속 source는 짧은 host ABI 검증을 먼저 확인하고 같은 commit으로 PR #611의 필수
검사를 진행한다. 각 workflow의 동일 source 중복 등록은 피하며 source와 실제 결과를
다시 읽는다. 지금 확인한 것은 기존 실행의 부분 성공과 실제 실패 원인이며, 후속
source의 기능·전체 gate·설치 후보 성공은 아직 확인 전이다.

## CI cache의 작업 사이 재사용 오류 — #627

`eacfe059180c17c827f2236a16642a4124409ae9`의 Genuine `40833691796` attempt1은
원래 Planner ABI와 같은 고정 C 소스의 host SDK 빌드는 통과했으나 FFmpeg 소스의
`curl 28`로 실패했다. 같은 ID의 실패 job을 재시도했으며 전체 성공은 아직 아니다.

실제 cache API와 toolkit 소스를 읽어 별도 설정 오류를 확정했다. workflow의 cache path가
매번 달라지는 `/tmp/openpilot-ci.<random>/native-*`였으므로 key가 같아도 version이 달라졌다.
같은 ref/key의 이미지 cache가 attempt1/2에서 각각 약1.01GB로 새로 저장됐고 version은
`f45bab628f2b6ff89a6dad9916620c06c75ee5c69a595abcc452b8d10bf9b3f7`/
`09e1761945950d42790ecd4aa85ff68c8cc35035e5ab5c7b7ed365adc0a4d19c`다.
단일 작업 내부의 반복 전송0은 이 작업 사이 재사용 성공을 뜻하지 않는다.

고정된 ignored workspace staging의 `payload`만 Actions archive에 넣고, 준비 도구 실행
전후에는 job 전용 입력 폴더로 같은 filesystem에서 원자 이동한다. lease·inode·runner와
기존 파일을 확인하며 다른 작업이나 기존 원격 cache는 채택·삭제하지 않는다. 취소 시
job 소유 payload만 되돌리고 정상 job-temp 정리로 연결한다. 실제 원래 검증 명령과
자산 SHA/byte 및 다운로드 검증은 유지한다. 서로 다른2개 Actions job의 실제 hit와
소유 archive·취소·충돌·redirect 회귀를 추가했다. 로컬 AST/YAML/diff만 수행하며
이 새 source의 실제 cache hit·필수 검사·병합 후 성공은 아직 확인 전이다.

근거: [실제 실행](https://github.com/bin9208/openpilot-rust/actions/runs/40833691796),
[cache version 구현](https://github.com/actions/toolkit/blob/main/packages/cache/src/internal/cacheUtils.ts),
[archive 경로 구현](https://github.com/actions/toolkit/blob/main/packages/cache/src/internal/tar.ts).

## 같은 source 다운로드의 반복 시간 초과 — #628

Genuine `40833691796` attempt2/job123323861172도08:04:31–08:05:45 UTC에
같은 FFmpeg source 준비의 `curl 28`로 실패했다. 먼저 같은 실행을 코드 변경 없이
재시도했지만, 각 실행의 기존4회 연결 재시도 후에도 실패했다. 이 기록은 FFmpeg의
전역 장애나 header 검증 실패를 주장하지 않는다.

원래 URL의 zero-body 일시 연결 실패가 모두 소진된 경우에만 Buildroot 보관본의
같은 release tarball을 사용한다. 원래 source URL·SHA·10,458,600byte와 header140개
전체 inventory 및 provider pin은 바꾸지 않는다. mirror도 HTTPS와 같은 SHA/byte를
검사한 뒤에만 cache에 발행하며 부분 body·HTTP/크기/hash 오류는 대체 근거가 아니다.
전체1800초 예산과 origin별4회 제한, 실패한 모든 body 전송량 보존을 유지한다.
소유 curl fixture 회귀와 실제 header 준비·재사용은 새 source의 Actions에서 확인한다.
로컬 rustfmt/AST/YAML/diff만 확인했으며 기능 성공은 아직 주장하지 않는다.

[Buildroot 원래 해시 기록](https://lists.buildroot.org/pipermail/buildroot/2024-April/751360.html)은
기존 `8684f4b00f94b85461884c3719382f1261f0d9eb3d59640a1f4ac0873616f968`와 같다.
보관본 HEAD의200/같은 길이는 읽기 증거이며 실제 body/140-header 성공을 대신하지 않는다.

`2c5b78b011017bebe45aaf175e867bfaf1418978`의 push fast `40840421557`은 Rust1.94
공유 build driver all-target typecheck와 새 cache 소유/취소/archive 회귀7개가 통과했다.
전체97개 중 기존 Cargo target view fixture 한 개가 실제 cleanup 명령을 호출할 때
`GITHUB_ACTION_PATH`를 주지 않아 `/../../../tools/ci/native_cache.py`를 찾지 못했다.
실제 Actions가 주는 action 경로를 해당 소유 fixture에도 전달한다. 원래 생성·소스
cleanliness·정리/다른 파일 보존 검사는 유지하며 새 정확 source에서 다시 확인한다.

## DBC 생성기의 정확 source pin 보존 — #629

`e591ee13beab02bfa5d81f41b0d947ffd02e9578`의 Genuine `40841326495`/job123338218920은
기존53개 library component·MPC·Panda 단계를 통과한 뒤09:49:47 UTC에 DBC 생성 입력
불일치로 실패했다. 고정143개 입력 중 `radarcan_prepare_assets.py`의25GiB floor 제거만
bytes를 바꾼 원인이었다. DBC 정의·생성 계산·runtime 감지/lead 선택은 바꾸지 않는다.

생성기 원래 bytes(SHA256 `392dd87d54bfea8fe160a9c3e7cf6d5c2c888b4be2aee94822b1417b1b31d450`)와
143행 source pin `60d1c67b28395a25ec103a2e3481317fd136a78f2ccb0f7dce821055c7456d05`를
유지한다. CI는 명시적인 `radarcan_ci_builder.py --` adapter가 원래 bytes를 확인한 뒤
resource 조건/오류 문구 두 줄만 기존 operation 성장량으로 바꾼다. 실제 free 값과
나머지 생성 코드·argv는 유지한다. 모든 원래 DBC 출력 SHA 비교와 부족/변조 거부를
Actions에서 실행하며 로컬에서는 AST/YAML/diff만 확인한다.

검증된 original archive의 기존 cache save도 첫 foreign-only export 직후로 옮긴다.
이후 다른 입력이 실패해도 완료된 원본 검증/전송을 다음 시도에서 재사용하기 위한
순서 수정이다. 원래 key/bytes/hash 검증을 유지하며 이전 project runtime을 현재 결과로
캐시하지 않는다. 새 source의 실제 재사용·필수 검사·정상 병합/병합 후 결과는 남아 있다.

## 원래 cereal wire 예제의 실제 compile 회귀 — #632

정확 source `a5e2d10f7ea882f7edf4453c481c8dc5b5f9ffb5`의
[workspace job123396315060](https://github.com/bin9208/openpilot-rust/actions/runs/40857362214/job/123396315060)은
native package 거부 검사를 통과한 뒤 원래 workspace/all-target Clippy에서 실패했다.
`native_roundtrip.rs`의 union reader에 없는 `get_car_state`/`get_log_message` 호출이
E0599의 원인이다. 생성된 현재 API와 기존 caller를 대조하고 `which()`의
`CarState`/`LogMessage` variant로 payload를 읽도록 두 접근만 고친다.

원래 message 생성, valid/timestamp/숫자/한국어 text, schema·capnp pin과 원본 Python의
전체 wire bytes 비교를 유지한다. 다른 variant나 잘못된 값은 계속 실패한다.
로컬은 Rustfmt·diff/읽기 대조만 수행한다. 새 source의 원래 Clippy·example build와
기존 genuine의 full-schema 원본 왕복/wire·재사용 비교가 통과해야 수용한다.
이전 source의 부분 성공으로 대신하지 않으며 전체 #598은 계속 미완료다.

같은 source의 [PR fast job123391576559](https://github.com/bin9208/openpilot-rust/actions/runs/40857362282/job/123391576559)은
101개 회귀 중 새 DBC 출력 검사의 원래 Tesla 생성 child가 `opendbc`를 import하지
못해 실패했다. 부모 테스트의 `sys.path`는 새 Python process에 전달되지 않는다.
테스트가 실행하는 실제 generator child에 기존 opendbc source 경로를 `PYTHONPATH`로
명시한다. production generator·고정143개 입력·출력 SHA와 실제 free-byte 검사는
유지한다. 이는 #629의 회귀 환경 보완이며 출력 비교를 생략하거나 pin을 바꾸지 않는다.
기능 재검증은 새 정확 source의 원래 Actions에서 수행한다.

## 2026-10-30 — 향후 입력 Git pin 검사의 Xiaoge Python 경로 보존 준비

dev-md의 별도 source `ce20f2582dfdc5ca77776e58ba23a52c28f90655`에서
[X64 job123744005591](https://github.com/bin9208/openpilot-rust/actions/runs/40957390360/job/123744005591)이
설치된 numpy를 첫 policy import에서 찾지 못했다. provider 준비가 선택 Python의
library 경로를 덮어쓰는 명령은 현재 dev 대상 source S237에도 동일하게 있다.
현재 S237의 Xiaoge job123757139846는 성공했으므로 그 실행을 실패나 수정 성공으로
재표기하지 않는다. 현재 S237·Genuine·원격 실행은 유지한다.

향후 공개 입력 Git pin P에만 적용할 전용 proof checkout에서 기존 Webcam과 같은
X64 전용 7줄 단계를 준비했다. provider 준비 뒤 선택 `pythonLocation/lib`와 provider
경로를 함께 보존하고, 명시 interpreter의 numpy import·경로·버전을 확인하여 후속
단계에 넘긴다. source Python peer 세 곳의 기존 LD tail 보존은 S237에 이미 있으므로
그 파일들은 바꾸지 않는다. 원래 명령·provider/모델 pin·픽셀·tensor·IPC·timeout·ARM
및 최종 gate는 유지한다. [#614](https://github.com/bin9208/openpilot-rust/issues/614)에
연결하며 실제 import·전체 source 비교의 성공은 적용한 P의 Actions에서 확인한다.
로컬은 YAML·inline Python AST·Bash 구문·diff만 확인하며 기능 검사는 실행하지 않는다.
