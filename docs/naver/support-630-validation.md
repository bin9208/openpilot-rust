# #630 독립 집중 검증 controller 기록

대상: [#630](https://github.com/bin9208/openpilot-rust/issues/630).
작성: 2026-10-29 KST. 기능·원본·ARM 성공 기록이 아니라 구현·정적 검토 기록이다.

## 현재 소유권과 실제 환경

검증 브랜치는 `codex/feat-630-validation-controller`이며 dev/#598로 PR·병합하지 않는다.
상위가 등록된 Rust port workflow ID `370230005`의 이 브랜치 버전을 관리한다.
기본 브랜치의 `rust.yml`과 #598/PR611·606·603·494 파일은 이 작업에서 수정하지 않는다.

상위 packages v1은 `00cd872f2`로 push했다고 전달받았다. 전달 당시 packages 실행
[40869070812](https://github.com/bin9208/openpilot-rust/actions/runs/40869070812)는 queued였으며
이 기록으로 현재 상태나 성공을 주장하지 않는다. 해당 v1의 실제 환경은 다음과 같다.

- `bin-actions` self-hosted X64 실행, 공용 Rustup의 Rust `1.94.0` pin을 사용한다.
- `setup-python`의 `3.12.14`에서 job별 private venv를 만든다.
- candidate별 Cargo target·증거 디렉터리는 `$SUPPORT_ROOT` 아래 private 경로다.
- packages는8crate와 additive API 담당 `openpilot-process-supervision`을 포함한다.
- 고정25GiB floor 대신 작업별 예상 증가량을 확인하며 자동 cleanup은 없다.
- HOME은 재정의하지 않는다. 다른 실행·cache를 중단/삭제하지 않는다.
- source/ARM 단계는 상위의 후속 commit·dispatch 전에는 실행된 것으로 간주하지 않는다.

이번 담당의 최종 쓰기 범위는 새 `tools/ci/support630-source.sh`,
`tools/ci/support630-arm.sh`, 이 문서다. 이전 미사용 `support630.py`는 상위 요청으로
삭제했다. 상위가 관리하는 workflow/packages/resource·prep 파일은 수정하지 않는다.

## source 스크립트 계약

상위가 `SOURCE_ROOT`(정확 후보 checkout), `CARGO_TARGET_DIR`(private target),
`EVIDENCE`, `SOURCE_PYTHON`(private Python 3.12 venv), `SUPPORT_ROOT`,
`RUNNER_TEMP`와 native prerequisites를 제공한다. `SOURCE_SHA`도 제공하면 HEAD 일치를
추가 확인한다. 기본 실행은 등록 `bin-actions`에서만 허용한다.

```sh
bash "$GITHUB_WORKSPACE/controller/tools/ci/support630-source.sh"
```

입력 중8crate·공용 관련 소스/원본/checker의 실제 파일 SHA256, HEAD와 tracked index를
기록한다. `--locked`로8crate와 process-supervision·logmessaged·PortAudio의 필요한
library/bins/examples를 빌드한다. 원본 Params는 기존 `build_params_python.py`로,
msgq는 기존 `build_msgq_python.py`로 별도 증거 경로에 빌드한다. Params의 json11 wheel
hash/architecture 검증과 provenance, 각 비교기의 기존 assertion·deadline을 보존한다.

| 모듈 | 그대로 실행하는 비교 진입점 |
|---|---|
| web-upload | `check_web_upload.py`, `check_web_upload_timeouts.py`, `web_upload_eintr_compare.py` |
| bootlog | `check_bootlog_snapshot.py`의 기존10시나리오 |
| tombstoned | `check_tombstoned_reference.py`, `check_tombstoned_safety.py`, `check_crash_sdk_transport.py`, `check_tombstoned_runtime.py`, `check_tombstoned_shutdown.py` |
| soundd | 후보의 `check_soundd.py`, `check_soundd_daemon.py`; 미수정 원본/의도한 #392 변화 구분 유지 |
| Bluetooth | `check_bluetooth_runtime.py` 전체; BlueZ·prompt 실제 deadline 포함 |
| Wi-Fi | `check_wifi_policy.py`, `check_wifi_runtime.py`, `check_wifi_callback_ordering.py` |
| modem | `check_modem_process_boundary.py`, `check_modem.py`; 마지막 실제 RED/GREEN 도구에서도 원본 비교 |
| LPA | `check_lpa.py`의 PTY/TLS/profile/notification 비교 |

추가로 실제 production LPA/Wi-Fi 할당 probe, multipart loopback/RSS probe와
Bluetooth3·Wi-Fi3·modem15·tombstoned3 회귀, bootlog xattr 및 sound playback을 실행한다.
modem은 `helper-lifetime-tests`와 정확한 `MODEM_SOURCE_PYTHON`을 지정한다.
기존 tombstoned 비교기가 exit0/`EMULATION_LIMITS`를 내더라도 manifest가 `PASS`이고
모든 비교가 일치하지 않으면 이 controller는 실패한다.

old-source RED는 private baseline 사본에서만 실행한다. 기준 dev
`e445c9d19c3041bc79d8624589e2eb6ba7d061c6`의 sound policy·BlueZ agent/state·apport와
multipart 구현을 현재 의존성/회귀에 연결한다. BlueZ에는 새 test module만 붙인다.
먼저 `--no-run` 컴파일 성공을 요구하고 실제 libtest 실패/지정 회귀 문자열을 검사한다.
modem은 `1364e48c96e888fa5d178c5eb2132d135cf30765`와 후보의 기존
`check_modem_helper_lifetime.sh`를 그대로 사용한다. 컴파일 실패는 기능 RED가 아니다.
bootlog의 missing-API 컴파일 실패를 RED 증거로 사용하지 않는다.

## source 추가 준비와 산출물

native host prerequisites는 상위가 설치한다. 기본 목록 외 `git`, `curl`, `binutils`,
C++/Cython용 Python headers와 `libzmq3-dev`가 필요하다. 원본 Params 빌드에는
`--zmq-include /usr/include`를 지정한다. source 스크립트는 apt를 실행하지 않는다.

private venv에만 다음 pins를 설치/확인한다. 공용 Python 환경이나 HOME을 바꾸지 않는다.
`numpy==2.5.3`, `pycapnp==2.1.0`, `Cython==3.2.4`, `setuptools==82.0.1`,
`pyzmq==27.2.0`, `zstandard==0.25.0`, `requests==2.34.2`, `aiohttp==3.13.3`,
`anyio==4.12.1`, `PyJWT==2.15.1`, `cryptography==50.0.1`, `pyserial==3.5`,
`jeepney==0.9.0`, `pytest==9.0.2`, `sentry-sdk==2.55.0`, `cffi==2.1.1`.

임시 IPC/fixture는 짧고 고유한 `/tmp/s630.*`에 둔다. 길이 제한이 있는 bootlog의
원본 endpoint도 이곳을 쓰며 결과 JSON/log는 `$EVIDENCE/source/bootlog`로 보존한다.
marker 경로는 `PRIVATE_TEMP_DIRECTORY`에 기록한다. 자동 삭제는 하지 않는다.
후속 정리는 증거 업로드 후 상위가 해당 소유 경로만 확인해 처리한다.

`$EVIDENCE/source`에는 command argv·각 exit/log, Python freeze, native/provider
provenance, input hashes, baseline source hashes, probe/ELF hashes, 원본 비교 결과,
HEAD/index 전후 검사와 `outcome.json`을 기록한다. 큰 설치/빌드/사본 전에 실제
대상 filesystem의 가용량과 예상 증가분을 기록하며 고정 reserve는 사용하지 않는다.

상위의 후속 실행 명령은 다음과 같다. 이 담당은 실행하지 않았다.

```sh
gh workflow run 370230005 --repo bin9208/openpilot-rust \
  --ref codex/feat-630-validation-controller \
  -f source_sha="$CANDIDATE_SHA" -f phase=source
```

## ARM 범위와 현재 제약

2026-10-29 재개 지시에 따라 ARM 담당은 `support630-arm.sh`와 이 절만 수정했다.
현재 등록 runner는 id23 `bin-actions2`, Linux/X64이며 상위가 추가한
`support-630` label을 사용한다. controller의 `[self-hosted, Linux, X64, support-630]`
선택과 별개로 스크립트도 실제 runner 이름·self-hosted·X64를 확인한다.
GitHub hosted 예외는 승인되지 않았고 이 스크립트는 해당 경로를 실행하지 않는다.

기존 dev `rust.yml:arm64`의 workspace GNU/musl 빌드 및 특정 다른 모듈 QEMU gate와,
#630 영향9package GNU ARM 빌드를 구분한다. ptrace/pidfd 수명 기능 검사는 host 담당이다.
새 PRoot/QEMU transport, ARM 전체 모듈 syscall 회귀, 새 공용 action은 추가하지 않았다.

### fresh runner용 기존 provider 도구 재사용

현재 dev의 FFmpeg helper는 내부 25GiB floor를 사용하므로 그대로 실행하지 않는다.
허용받은 #611의 정확 commit `fb33b6503bfd74b75e262d7148505b1381e8f8da`를
`$SUPPORT_ROOT/arm630.*/provider-tools`에 별도 checkout하고 변경 없이 읽기 재사용한다.
런타임 candidate에 공용 source·workspace·lock·CI/provider를 merge하거나 복사하지 않는다.
이 외부 checkout에는 growth-only `openpilot-disk-space`가 연결돼 있다.

실행 전 다음 파일 SHA256을 확인한다.

| 외부 helper 입력 | SHA256 |
|---|---|
| `rust/crates/dev-tools/src/ffmpeg_build.rs` | `c9935c88da7212cbad9e9ee51997ea4e764fe17360cd83bb98b4d88b5049e490` |
| `rust/crates/disk-space/src/lib.rs` | `160390714225d2fc2d425182baed86343a133800c54055e44f88ee12cb6e1a94` |
| `rust/Cargo.lock` | `c78df868dbff23470a79445719dd6a078d9f5cb2a6a9704cd6bfdf0a899598f8` |

기존 `build-loggerd-ffmpeg` CLI를 `--locked`로 호출한다. FFmpeg는 기존 pin인
`6.1.1`, `https://ffmpeg.org/releases/ffmpeg-6.1.1.tar.xz`, SHA256
`8684f4b00f94b85461884c3719382f1261f0d9eb3d59640a1f4ac0873616f968`를 사용한다.
기존 logger-minimal-static의 codec/configure 옵션, LGPL 고지, 소스/헤더/파일 hash,
AArch64 target predefine 검사와 phase별 growth admission을 그대로 사용한다.
새 configure recipe를 작성하거나 #598 도구 전환을 재구현하지 않는다.

### prerequisite와 APT 격리

parent controller가 제공하는 `SOURCE_ROOT`, `CARGO_TARGET_DIR`, `EVIDENCE`,
`SOURCE_PYTHON`, `SUPPORT_ROOT`를 그대로 받는다. host는 기존 기준과 같은 Ubuntu24.04이며
Rust1.94.0, `aarch64-unknown-linux-gnu`, GCC/G++13, GNU binutils와 glibc2.39/LP64를
명시적으로 확인한다. compiler/linker/ar/ranlib 실행 파일 SHA와 실제 package 버전을 남긴다.
이는 GNU Ubuntu ABI이며 AGNOS의 loader/glibc 호환성을 검증했다는 뜻이 아니다.

상위 전달에 따르면 첫 새 runner host
[40874907549](https://github.com/bin9208/openpilot-rust/actions/runs/40874907549)는
APT PPA404로 실패했고 #633에서 controller `c918f07a7`의 private 공식 Noble scope로
분리했다. ARM도 이 준비가 먼저 끝난 뒤 같은 `$SUPPORT_ROOT/ubuntu.sources`,
`apt-lists`, `apt-archives`를 재사용한다. 공식 archive/security HTTPS URI·Noble suites·
Ubuntu archive keyring 경로를 확인하고 source/keyring hash를 기록한다.
호스트 전체 `apt update`, 기본 PPA 수정, cache 삭제나 lock 중단은 하지 않는다.

ARM이 추가로 설치하는 핵심 패키지는 `gcc-13-aarch64-linux-gnu`,
`g++-13-aarch64-linux-gnu`, `binutils-aarch64-linux-gnu`,
`libc6-dev-arm64-cross`, `libstdc++-13-dev-arm64-cross`다. 기존 helper의 host build용
make/cmake/pkg-config/clang/capnproto/libcapnp/libzmq/curl/git/xz/liblzma/libssl/zlib도
명시적으로 설치·기록한다. 설치는 같은 private APT scope의 `--no-upgrade --no-remove`다.

### 실제 빌드 흐름과 증거

기본 `SUPPORT630_ARM_MODE=gnu-cross-build`이며 별도 사전 `FFMPEG_DIR`는 필요 없다.
1) cross prerequisites와 Rust target을 준비하고, 2) 외부 pinned helper를 host 전용
private Cargo target에서 빌드·실행하며, 3) 생성한 FFmpeg prefix의 전체 receipt hash와
세 static library의 모든 ELF object가 AArch64인지 확인한다.
4) 이 prefix·GCC13 linker·target sysroot를9package release 빌드에 연결하고,
5) Cargo JSON의 실제 artifact를 통해9package 전부에서 실행 파일이 생성됐는지 확인한다.
각 ELF의 ELF64/little-endian/AArch64, GNU loader `/lib/ld-linux-aarch64.so.1`,
GLIBC 요구 상한2.39 및 실제 파일 SHA256을 기록한다. ARM 파일은 실행하지 않는다.

정확 명령은 스크립트와 `$EVIDENCE/arm/commands.txt`에 있다. 호출은 기존 controller의
`phase=arm`이며 별도 runtime CI 변경이나 설치/배포를 요구하지 않는다.
`helper-and-abi.json`, `ffmpeg-provider.json`, provider `build-record.json`과 로그,
빌드 JSON, `elf-artifacts.json`, candidate HEAD/index 전후, 결과/실패 단계가 artifact에 남는다.
외부 helper의 source/index 불변도 확인한다. 실패를 성공·skip으로 바꾸지 않는다.

예상 증가분은 native 설치4GiB, Rust/host tool4GiB, helper checkout2GiB,
FFmpeg build6GiB, 영향 ARM build8GiB를 해당 작업 직전 실제 filesystem에 대조한다.
고정25/35GiB floor는 없다. HOME을 재정의하거나 다른 세션의 source/CI/cache를 만지지 않는다.
private root와 산출물은 자동 삭제하지 않는다.

이 변경은 문법·heredoc Python AST·diff 검토만 완료했다. 다운로드, helper compile,
FFmpeg build, target link/ELF 검사는 아직 실제 Actions 결과가 없다. 새 runner의 실제
OS/패키지·네트워크·cross dependency·FFmpeg wrapper link·빌드 시간은 미확인이다.
기존 GNU/musl full gate, PR/병합 후 필수5개 검사, 사용자 기기 수용은 별도로 남아 있다.

## 로컬 확인 및 남은 검증

두 shell 파일의 `bash -n`, 포함된 Python heredoc의 `ast.parse`, 담당 diff/공백 검토만
수행했다. 기능·Cargo build/test/Clippy·원본·ARM 및 다운로드 실행은 하지 않았다.
commit/push/dispatch와 다른 채팅 메시지도 보내지 않았다.
source의 최신 정확 SHA 기능 결과, baseline runtime RED/GREEN, provider 준비 결과와
ARM 대안의 실제 실행 가능성은 상위 Actions에서 확인해야 한다.
