# AGNOS 시스템 SDK 입력 검증 — #599 (Actions 검증 대기)

상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598),
[환경 #599](https://github.com/bin9208/openpilot-rust/issues/599).
native compiler executable뿐 아니라 사용한 시스템 헤더·라이브러리·보조 도구의
내용과 이름 공간도 빌드 입력으로 고정한다. 기존 Rust toolchain/provider의 일반
`Tree`는 절대 링크와 디렉터리 링크를 계속 거부한다.

## 입력의 출처와 범위

`rust/native-system-sdk.json`은 AGNOS `19.8-carrot-bt1`의 기존 raw image SHA
`375c5d22335770ac08750660bb5b29a3331c550d6a9875b35f36b88af792ea44`와
검사할 상대 경로를 고정한다. `--capture-system-sdk`는 **독립적으로 인증한 이미지의
별도 추출 디렉터리**에서 SDK receipt를 만든다. 이 명령 자체는 이미지의 출처를
인증하지 않는다. 소비할 receipt SHA도 외부에서 독립적으로 고정해야 한다.
변경된 live 시스템에서 새 receipt를 만들어 이전 신뢰를 대체할 수 없다.

기본 범위는 include, ARM 라이브러리, GCC/Clang/LLVM, CMake/pkg-config 정보,
loader 설정과 compiler/helper 경로다. 재귀 경로는 파일·디렉터리 구성 전체를
검사한다. `usr/lib`/`usr/local/lib`는 직접 파일과 즉시 하위 이름만 고정한다.
선택하지 않은 Python 하위 디렉터리의 bytecode는 SDK 전체 변경으로 간주하지 않는다.
이는 명시한 범위이며 아직 전체99개 runtime의 실제 compiler search closure를
입증한 목록은 아니다. 모델 준비용 Python과 기존 C/C++ 라이브러리는 유지한다.

SDK의 절대/디렉터리 symlink는 이미지 안의 논리 경로로 해석한다. host의 `/usr`로
따라가지 않는다. 링크 문자열, 실제 대상 file SHA·bytes·mode, 디렉터리 child 목록,
없던 경로의 부재를 함께 기록한다. 위쪽 탈출, compiler 이름 공간 밖의 링크,
40회 초과 링크 해석, set-id·특수 파일·초과 크기는 실패한다.
상한은100,000개 항목·16GiB 고유 파일 읽기·16MiB receipt다.

Rust 입력 도구의 capture는 출력 파일을 이미지 밖에 별도 게시하고 기존 파일을
덮어쓰지 않는다. 출력 byte 수만큼 공간을 먼저 확인하고 fsync 후 게시한다.
compiler recipe의 `input_trees`에서 SDK 종류를 명시하며 root는 실제 `/`여야 한다.
사용하지 않은 다른 sysroot를 검사해 native compiler를 통과시키지 않는다.
receipt SHA가 캐시 키와 `verified_input_trees` 보고서에 들어가며 빌드 전후 검증한다.
seal 경계에도 같은 파일 pin이 전달된다. SDK receipt로 Rust prefix나 runtime provider의
일반 `Tree`를 대신할 수 없다.

## 검사 방법

### 전체 native generation의 필수 입력 — 구현 추가, Actions 검증 대기

일반 build driver의 작은 격리 fixture와 전체 runtime 전달 경계는 구분한다.
전체99개/13그룹의 generation은 모든 그룹이 실제 `/` SDK와 SHA-pinned receipt를
선언해야 한다. 그룹별 SDK identity가 다르거나 한 그룹이라도 SDK를 누락하면 거부한다.
단순히 어떤 SDK receipt를 선언했을 때만 검사하는 선택 사항으로 남겨 두지 않는다.

전체 compiler 실행 전과 seal 전후에 receipt를 한 번 읽은 동일 byte에서 hash/parse하고,
보존한 **그 세대 소스**의 `rust/native-system-sdk.json`과 factory SHA·AGNOS·전체 root
profile이 같음을 확인한다. 자체 pin을 새로 만들어도 축소 profile·다른 image·일반 Tree를
대체 입력으로 사용할 수 없다. 실제 `/`의 내용 검사는 기존 driver의 빌드 전후 및
seal Pins의 검사를 유지한다. profile 검사만으로 live SDK 또는 전체 search closure를
확인했다고 표현하지 않는다.

boot/fallback에서는 Prepared의 그룹별 기록과 필수 SDK identity를 확인한다. compiler
입력 cache의 receipt를 다시 열지 않아 cache 정리로 정상 runtime의 선택이 깨지지 않는다.
실제 runtime/시스템 ELF·ABI·파일 검사는 기존대로 유지한다. 아직 전달 전인 native
prototype 중 SDK가 누락된 기록은 완전한 generation으로 수용하지 않는다.

foundation X64/ARM과 genuine AGNOS의 새4개 회귀는 실제99개 roster를 사용한다.
각13그룹의 누락, SHA 누락·그룹 간 불일치·unpacked root, 재pin 후의 축소 profile/다른
factory SHA, 다른 receipt 종류, 입력 cache 삭제 뒤 boot metadata 검사를 확인한다.
회귀의 owned image/receipt는 작은 fixture이며 진짜 factory SDK 또는99개 빌드가 아니다.
genuine의 실제 SDK·same-size 변조·복구 검사는 그대로 앞에서 별도 실행한다.

로컬에서는 구문·rustfmt·diff/읽기만 수행한다. 아래 기능 검사는 등록된
`bin-actions`의 Actions에서 실행한다.

- foundation의 `cargo test --locked -p openpilot-runtime-build`에 SDK 회귀7개를 포함한다.
  이미지 상대 절대/디렉터리 링크, 같은 크기 헤더·library 변경, mode·파일 추가·경로 부재,
  shallow 범위, escape/cycle/set-id/FIFO/OS 불일치, receipt 종류 분리와 CLI 게시 보존을 검사한다.
- genuine AGNOS workflow는 기존 image SHA 검사와 `debugfs rdump` 직후, guest 실행 전에
  host helper로 expected receipt를 만든다. 원래 compiler/driver4회/Rust prefix/전체 cereal/
  bootstrap seed 검사는 그대로 유지한다.
- 그 뒤 실제 ARM source-built driver에 `/` SDK 입력을 붙여 초기·반복 build를 실행한다.
  두 std 프로그램43/42, 반복 fresh·같은 artifact hash와 SDK SHA 보고를 확인한다.
- job 소유 추출 이미지의 `usr/include/stdio.h` 첫 byte만 같은 크기·mode로 바꾼다.
  driver 실패와 이전 프로그램43/42 보존을 확인하고 `finally`로 원래 byte를 복구한다.
  host helper로 전체 선언 범위를 다시 capture해 최초 receipt SHA와 같음을 확인한다.
  runner 시스템·기기·다른 세션의 파일은 변경하지 않는다.

개발용 capture 형태는 다음과 같다. 실행은 Actions에서 인증된 이미지에 수행한다.
receipt의 부모 디렉터리는 미리 존재해야 한다.

```sh
openpilot-runtime-inputs --capture-system-sdk \
  --root "$VERIFIED_EXTRACTED_AGNOS_IMAGE" \
  --plan rust/native-system-sdk.json --output "$SDK_RECEIPT_OUTSIDE_IMAGE"
```

`native-system-sdk*.json`, `runtime-driver-system-sdk-*.json`/stdout와 commands log를
정확 source SHA의 artifact로 남긴다. 성공 전에는 기능 통과로 기록하지 않는다.
QEMU/PRoot의 AGNOS 사용자 공간이며 물리 ARM CPU·kernel boot·기기 성능 검사가 아니다.

공개·독립 pin의 factory SDK receipt, 전체 provider 헤더/library 대응, full99개 native
build/seal, aggregate 자원·복구, 기본 시작/업데이트와 Python 모델 준비·녹화/기존 업로드,
PR/병합 후 검사와 사용자 comma3X 수용은 남아 있다. #598/#599는 열린 상태로 유지한다.
