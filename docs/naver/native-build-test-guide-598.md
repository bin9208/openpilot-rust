# 콤마 자체 빌드·다운로드 재사용 검증 안내 — #598

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
