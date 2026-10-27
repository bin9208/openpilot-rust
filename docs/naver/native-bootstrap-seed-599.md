# 첫 입력 준비 도구의 고정 seed — #599 (검증 대기)

상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598),
[환경 #599](https://github.com/bin9208/openpilot-rust/issues/599).
Rust compiler와 Cargo 원본을 준비하는 Rust 도구를 처음 얻는 순환을 분리한다.
고정된 작은 `openpilot-runtime-inputs` seed를 재사용하고 현재 프로젝트 런타임은 소스에서
빌드한다. 소비하는 프로젝트 commit마다 seed를 새로 발행할 필요는 없다.

현재 변경은 후보 추출·검증·소비 검사를 구현한 단계다. 검증된 공개 seed release,
그 descriptor SHA의 독립된 Git pin과 전체 native build/start 연결은 아직 남는다.
`rust/tools/ensure_runtime.sh`의 기존 기본 경로와 런타임 선택은 이 변경에서 바꾸지 않았다.

## seed에서 현재 소스의 준비 도구를 빌드하는 경로

seed의 새 `--bootstrap-tools-scope`는 root에서 자기 UID의 기존 고정 cgroup scope만
준비하고 UID/GID·supplementary groups를 내린 뒤 같은 seed의 frontend로 exec한다.
Python 진입점 `build-tools`가 실제 AGNOS/기본 loader ABI와 독립 descriptor pin을
확인하고 원래 요청자의 PID/start ticks를 전달한다. 요청자 종료·취소·offroad 변경은
기존 supervision 경로로 하위 작업까지 중단한다. root에서 compiler/Git/model을 실행하지 않는다.

큰 Rust/Cargo 준비와 현재 도구의 소스 컴파일은 private cgroup worker 안에서 실행한다.
현재 source의 Rust/Cargo plan·lock, 독립 SHA에 고정한 factory SDK snapshot과 source의
전체 SDK profile을 검증한다. 변경된 live `/`를 새 기대값으로 capture하지 않는다.
memory controller가 없거나 일반 디렉터리인 경우 격리 없는 fallback을 하지 않는다.
작업별 RAM/디스크 예산·여유, worker log와 kernel peak/관측 디스크 증가량을 기존
`resources` 구현으로 기록한다. 작은 seed 자체 준비는 기존 크기 상한·streaming 경로다.
worker의 일반 compiler 임시 파일은 cache 안의 별도 `tmp`를 사용해 같은 쓰기 예산에 넣는다.
Config도 한 번 읽은 같은 byte로 parse/hash하고 Python→root gateway→frontend→worker에
그 SHA를 전달한다. 도중 재작성·바꿔치기는 결과 검증과 bundle 발행 뒤에도 거부한다.

현재13개 도구는 runtime-build의8개 CLI와 provider 준비용 FFmpeg/raylib/OpenCV CLI,
ELF auditor와 codec wheel 준비 CLI다. 모델/외부 provider가 필요한 전체 runtime을 먼저 요구하지 않는다.
검증된 동일 Rust/Cargo/SDK 아래의 Cargo cache를 재사용하고 native incremental source
stamp 모드를 명시한다. source를 보존하고 실제 결과를 불변 object에 복사한 뒤 tool bundle에
hardlink한다. bundle ID에는 binary·recipe·compiler·입력 tree identity를 묶으며 요청 commit을
넣어 문서 변경마다 새 binary를 만들지 않는다. hit에서도 기존 최초 검증 출처/manifest를
유지하고 이번 요청 source와 별도로 보고한다. 이 bundle은 runtime 세대나 active 선택이 아니다.

검증된 seed와 독립 SDK pin·기기별 resource policy로 작성한 Config가 있는 개발 환경의 형태는
다음과 같다. 공개 seed/기기 설치 후보가 준비되었다는 안내가 아니다.

```sh
python3 tools/native/bootstrap_seed.py build-tools \
  --manifest "$VERIFIED_SEED_DESCRIPTOR" --manifest-sha256 "$PINNED_SEED_DESCRIPTOR_SHA256" \
  --cache "$NATIVE_BOOTSTRAP_CACHE" --config "$VERIFIED_BOOTSTRAP_CONFIG" --source "$CLEAN_SOURCE"
```

Config에는 별도 source/object/generation/Params store, cache, 독립 SDK receipt/SHA,
작업별 source/compiler/object 예상 증가량과 기존 `resources::Policy`가 필요하다.
CI fixture의3GiB 한도·여유값을 comma3X의 생산 예산으로 복사하지 않는다.

foundation은 별도 store·SDK pin·가짜 controller 거부를 검사한다. kernel resource workflow는
실제 root gateway의 권한 하강/출력 보존과 X64에서의 의도된 AGNOS 거부를 확인한다.
genuine workflow의 추가 사례는 별도 job-owned **X64 host kernel cgroup**에서 원래
AGNOS **ARM user space/QEMU/PRoot**를 사용한다. 처음/반복/docs/작은 실제 CLI 수정의
13개 Cargo 결과와 byte hash, 원래 bundle 출처, 자산 HTTP 전송0, 기존 선택/log sentinel
보존을 확인한다. 작은 변경은 실제 help marker/hash가 바뀌고 나머지12개는 fresh여야 한다.
fixture Git commit과 실제 GitHub source SHA를 분리하며 원래 checkout은 바꾸지 않는다.
raw 입력은 이미 인증된 별도 cache를 bind하므로 공개 seed의 최초 HTTP 전송 시험은 아니다.
기존 AGNOS 명령·입력 pin·runner·360분 상한과 모든 원래 사례는 유지한다.

새 경로는 아직 정확 source의 기능 검증 전이다. 이 첫 준비 도구의 성공이 전체99개
runtime의 기본 bootstrap·업데이트·부팅·기록·업로드 완료를 뜻하지 않는다. 공개 seed 및
SDK/component의 독립 Git pin, provider/모델 준비 orchestrator의 실제 검증과 기존 기본 시작 경로의
연결, 전체 수용·필수 PR/병합 후 검사·사용자 comma3X 시험은 계속 남아 있다.

## 현재 도구에서 전체 입력 준비로 연결 — 2026-10-28, 기능 검증 대기

현재 소스의 `--bootstrap-tools-scope`/frontend는 선택적인 `runtime_inputs`를 받는다.
고정 seed에 주는 최초 설정은 이 필드를 생략한다. seed로 현재 도구를 얻은 다음 현재
도구로 전체 입력 준비를 실행한다. 오래된 seed에 새 운영용 준비 코드를 의존하지 않는다.
13번째 `stage-encoder-target`은 libyuv/Eigen의 기존 wheel 입력을 준비하며 Python 설치
스크립트나 C/C++ 재컴파일을 실행하지 않는다.

```json
"runtime_inputs": {"path": "/absolute/runtime-input-selection.json", "sha256": "INDEPENDENT_SELECTION_SHA256"}
```

selection의 schema1은 `providers`, `budgets`, `compiler_growth_bytes`를 갖는다.
`providers`는 ffmpeg/raylib/opencv/numerics/geos/brotli/acados/sdl/turbojpeg/ffmpeg-tools/
freesans/panda/mpc/dbc/usbgpu의15개 역할을 모두 요구한다. 각 값은 독립 pin의
`{"path":"/absolute/provider.json","sha256":"..."}`이며, 이미 검증해 보관한 component가
있을 때만 `offline_archive`를 추가한다. `budgets`도 경로와 독립 SHA를 요구하고 실제
작업별 예상 증가량·산정 근거를 제공한다. 공개 component/pin이 아직 검증·발행되지
않았으므로 이 예시는 현재 기기 설치 후보나 공개 다운로드 완료 안내가 아니다.

비특권 worker가 factory SDK/현재 Git source/설정 pin을 검증한 뒤 현재 도구를 내용별
bundle에서 확인하고, 고정 ARM Python3.12.16의 `-I -B -S`와 별도 빈 bytecode 경로로
`rust/tools/native_runtime_inputs.py`를 실행한다. compiler/provider/model 하위 프로세스도
같은 hard cgroup·원래 요청자의 종료/취소·offroad 감시 안에 남는다. Python 모델 준비와
기존 C/C++ provider는 유지하고 전체 runtime archive를 대체 입력으로 내려받지 않는다.

준비 경로는 compiler 입력9종, provider15종, 원래 모델 생성물, source LFS, 별도 eGPU PKL의
전체18개 runtime 역할을 연결한다. source-built model compiler3종의 feature/profile은
`openpilot-runtime-build --cargo-metadata`의 실제 Cargo message와 실제 executable SHA에서
가져온다. 기존 schema3 build report와 generation reader 형식은 유지한다. 모델 생성은
원래 producer와 검증된 consumer를 거치며 반복/docs에서는 생성 cache를 재검증한다.

시도별 `runtime-inputs/result.json`은 compiler/provider/model 준비 보고서, 전체18역할
composition, catalog/bindings/99개 대상 build-plan, `native-config.json`의 SHA를 묶는다.
현재 source/config가 전후 바뀌거나 입력 일부가 없으면 실패하며 active/runtime/log는
선택하지 않는다. 다음 full build는 반환된 native configuration과 그 SHA로 기존
`openpilot-native-build-scope --build`를 사용하고, 봉인·ABI·검증된 세대 선택은 그대로 필요하다.

Actions에 실제 비특권 ARM userspace의 initial/reused/docs 준비를 별도로 추가했다.
원래 ARM 명령/pin을 끝까지 실행한 뒤 job-owned host cgroup에서 운영용 경로를 실행한다.
반복/docs의 model compiler fresh, QCOM 호출/모델 생성0, provider 전송/복사0, 동일한
composition과 link/copy0, 기존 선택/log 보존을 검사한다. host kernel peak/시간/디스크
증가량은 시도별 JSONL에 남긴다. 이는 물리 ARM CPU·AGNOS kernel·기기 부팅 검증이 아니다.

## 신뢰 경계와 저장

`tools/native/bootstrap_seed.py`는 AGNOS에 있는 Python 표준 라이브러리로 첫 도구를
준비한다. 별도 pip 설치나 먼저 설치된 프로젝트 Rust helper를 요구하지 않는다.
descriptor는 다음 내용을 묶는다.

- 정확한 seed source commit/tree와 준비 도구 역할, AArch64/AGNOS19.8-carrot-bt1.
- Cargo.lock·고정 Rust/Cargo 입력 plan·검증된 Cargo tree SHA, 실제 rustc/Cargo hash와 feature.
- helper ELF와 원래 프로젝트/Rust 배포본/해당 Cargo 의존성의 license notice SHA·정확 byte 수.
- 실제 기본 loader의 interpreter/library 경로·canonical 대상·hash/크기/mode와 readelf/ldd 도구.

소비자는 descriptor의 **독립적으로 고정한 SHA**를 제공한다. release 옆에서 받은 checksum을
자동으로 신뢰하지 않는다. `manifest.sha256`은 producer의 후보 식별/검토용 파일이며,
공개 사용 전에 검증된 값을 Git의 bootstrap pin으로 반영해야 한다.
descriptor의 source는 seed의 원래 소스다. 현재 runtime source로 다시 표기하지 않는다.

helper64MiB/notice16MiB/descriptor1MiB 한도와 정확 SHA·크기·ELF header를 확인한다.
내용별 lock·별도 임시 경로·fsync·rename-NOREPLACE로 게시하고, 기존 hit도 전체 고정 파일과
descriptor를 다시 검증한다. descriptor와 helper를 함께 다시 써도 외부 pin을 대체하지 못한다.
반복은 전송0/stored0이며, 같은 descriptor의 잠긴 소유 staging만 회수한다. 게시된 세대와
다른 descriptor/다른 이름의 데이터는 제거하지 않는다. 실패 시 기존 runtime/로그를 건드리지 않는다.

준비만 수행한 보고서는 `seed_executed=false`, `compiler_executed=false`,
`runtime_activated=false`다. x64에서 파일을 준비한 결과를 ARM 실행으로 표현하지 않는다.
`--verify-native`는 실제 AArch64/AGNOS, 고정 ABI 파일과 도구, 옮긴 helper의 기본 loader mapping을
재검증한다. 이 seed ABI는 모든 runtime/provider/header/compiler SDK의 검증을 대체하지 않는다.

## 개발 검증과 실행 형태

아래는 검증된 descriptor/pin이 있는 개발 환경의 준비 명령이다. 기기 설치/첫 수용 안내가
아니며, 현재 공개 seed가 준비되었다는 의미도 아니다. cache의 부모는 미리 존재해야 한다.

```sh
python3 tools/native/bootstrap_seed.py prepare \
  --manifest "$VERIFIED_SEED_DESCRIPTOR" --manifest-sha256 "$PINNED_SEED_DESCRIPTOR_SHA256" \
  --cache "$NATIVE_BOOTSTRAP_CACHE" --verify-native
```

source-built producer의 `export`는 실제 AGNOS에서 clean source/실제 native helper/검증된 Cargo
vendor/Rust 배포본 license를 입력받는다. 준비 도구가 사용하는 Cargo dependency graph의
normal/build 의존성을 따라 notice를 보존하고 source·binary·ABI가 전후 달라지면 실패한다.
출력은 helper/notice/manifest/checksum 후보다. 코드가 release를 생성하거나 장치에 설치하지 않는다.

foundation에는 ELF 모양의 소유 fixture4개가 있다. 독립 descriptor pin, 고정 seed 재사용,
helper/로컬 descriptor 변조, 잘못된 역할·source·URL·크기·body·ELF, 공간·동시 owner·staging/
캐시 symlink·목록 손상과 기존 입력 보존을 검사한다. fixture 파일은 실행하지 않는다.
별도 HTTPS 검사는 기존의 고정 공개 text403 bytes를 사용하며 실제 공개 seed 다운로드가 아니다.

genuine AGNOS workflow는 기존 명령·입력 pin·모든 원래 회귀를 유지한다. 그 결과 뒤에 실제
source-built ARM helper를 추출하고 별도로 고정한 descriptor로 후보 파일을 처음/반복 준비한다.
옮긴 helper의 실제 ABI를 확인하고, 그 helper로 새 Rust prefix와 기존 고정 Cargo 입력을 준비한다.
새 prefix의 compiler/Cargo로 실제 std 프로그램2개를 빌드·실행해43/42, 반복 fresh/같은 hash를
확인한다. producer와 consumer는 다른 경로의 실행 파일을 사용한다.

이 검사에서 seed 파일은 소유 artifact 경로로 공급하므로 `transferred_bytes=0`이다.
공개 GitHub release의 실제 seed 전송/재시도·캐시 재사용은 발행 후 별도로 확인해야 한다.
기존 준비용 라이선스/입력과 자산 cache/전송 측정, host/guest/source/emulation 기록은 유지한다.
QEMU/PRoot 사용자 공간이며 물리 ARM CPU/AGNOS kernel/실기 성능으로 표현하지 않는다.

현재는 새 정확 source의 Actions 실행 전이며 기능 성공을 가정하지 않는다. 전체 build worker의
aggregate 자원·취소/전원 중단/복구는 기존 #600 guard와 연결해 검증해야 한다. 전체 native OS
SDK/provider·기존 Python 모델 준비, full99개 source build/seal/기본 시작·업데이트/복구·녹화·
기존 업로드, 정확 source PR/병합 후 검사와 사용자 comma3X 수용까지 #598은 열린 상태다.
