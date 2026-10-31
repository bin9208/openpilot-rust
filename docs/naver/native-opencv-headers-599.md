# 기존 OpenCV4.13.0 헤더 준비 — #599 (Actions 검증 대기)

상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598),
[환경 #599](https://github.com/bin9208/openpilot-rust/issues/599).
기존 C/C++ 라이브러리를 재사용하는 전체 native 빌드를 위해, 원래 provider의 설치
헤더를 준비한다. OpenCV 또는 모델 준비 환경을 새로 빌드·전환하는 작업은 아니다.

## 원래 입력

기존 OpenCV source revision은 `fe38fc608f6acb8b68953438a62305d8318f4fcd`, codeload
archive95450266 bytes, SHA256
`6a7508554941c1a698c243b2212b2985ce59a65a3e0348d53c2158607d801e61`다.
기존 hardening patch SHA는
`7d17439697d9786b43b2a44dc9eecea34381adb6d2f14c6892266b6699a49aec`다.
버전·소스·patch·생성 설정을 최신 일반 OpenCV 설치본으로 바꾸지 않는다.

기존 release source `a3d0e3a497b4ae4d23c7cf5ee2275177600bbb31`의 runtime manifest SHA는
`8adbc002388fb97420e4bef9dcf58a5ed75de4c4632a1bc0d10c555f1c2d39ee`다. 이 manifest가
고정한 `rust/bin/licenses/opencv/native-libraries.sha256`의 SHA는
`d34c38a3ae14a51624759d526a825e596297ff203ad3ad6861cd08a32798934d`다. 그 파일의 원문과
170개 헤더의 source selector/SHA/size 및 생성 헤더 원문을
`rust/native-opencv-headers.json`에 고정했다. compact Rust member inventory SHA는
`2b59ab600acfff15039709ff4ca395ec02a2723c440208be7d91334465f81c63`다.

설치 헤더 합계는4629080 bytes다.168개는 고정 archive의 원래 설치 대상 source를 선택한다.
이 중 SSE header는 기존 patch와 같은 변환을 메모리에서 적용하며 원래 입력 SHA와
기존 patch 출력 SHA가 모두 정확히 같아야 한다. patch 자체와 source 전체를 수정하거나
외부 `patch`/CMake/compiler를 실행하지 않는다. 기존 shared library의 hardening을
바꾸거나 새로 빌드했다고 기록하지 않는다.

다음 두 생성 헤더는 원래 설치본의 byte를 보존하고 기존 공개 hash manifest와 대조한다.

| 헤더 | bytes | SHA256 |
|---|---:|---|
| opencv2/cvconfig.h |3269| `d86babae9fcae249a465ba2358837dba91c78a1d52e18639df9790c1b7377d2a` |
| opencv2/opencv_modules.hpp |479| `3cc030a42cb62ed2d01e8d540bc87433499b1ac9cba204538262dfd816134077` |

원래 LICENSE/COPYRIGHT, patch, header plan과 기존 native hash manifest도 prefix에
보존한다. 원래 manifest는 다른 기존 provider 파일도 기술하므로 `provenance/`에 원문으로
보존한다. 이 헤더 전용 prefix에 원래 shared library 또는 전체 빌드 receipt가 있다고
표현하지 않는다. `install/lib`, 프로젝트 실행 파일, Python shim을 만들지 않는다.

## 준비와 검사 방법

다음 명령은 Actions/개발 검증용이며 기기 설치 명령이 아니다.

```sh
build-runtime-opencv --prepare-released-headers \
  --plan "$SOURCE_ROOT/rust/native-opencv-headers.json" \
  --cache "$OPENCV_SOURCE_CACHE" --store "$HEADER_PREFIX_STORE"
```

원래 build-runtime-opencv의 native provider 빌드 CLI는 유지한다. 새 mode에 기존
`--source`/compiler 인자를 혼용하거나 고정 source/헤더/생성 설정을 바꾸면 자산 준비 전에
실패해야 한다. 기존 경로 또는 source library SHA 검사를 제거하지 않는다.

기대 Tree는 source에 독립 고정한 전체 header inventory에서 도출하며 로컬 Tree/receipt를
신뢰 근거로 사용하지 않는다. 최초에는 같은 byte의 원본 archive SHA를 인증한 뒤
안전한 경로/중복/타입/크기/압축 해제 상한을 검사하여 선택 파일만 준비한다. 이후 준비는
raw archive 및 실제 prefix Tree의 hash를 다시 확인하고, 전체 source를 다시 풀지 않는다.
기존 공통 publisher의 lock/읽기 전용 파일/실제 예상 증가량/fsync/원자 발행을 사용한다.

`runtime-source-build.yml`은 고정 source/selector/생성 헤더/manifest 변조와 CLI 혼용
거부, 기존 legacy OpenCV 검사와 함께 Clippy를 실행한다. 실제 공개 archive의 초기/반복
준비, header170개/정확 생성 설정, 반복 HTTP body/stored/copy0 및
`source_archive_decompressed=false`를 확인한다. job이 새로 만든 `cvconfig.h`를 같은
크기로 변조하고 Tree/receipt도 함께 위조하면 거부해야 한다. `finally`에서 세 파일과
권한을 복구하고 동일 Tree/전송0/복사0/재압축해제 없음으로 복귀하는지 확인한다.

`runtime-native-environment.yml`은 같은 raw cache를 QEMU/PRoot AGNOS19.8의 source-built
ARM 준비기로 소비한다. 독립 host Tree와 초기/반복 결과를 비교한다. 기존 core/dnn/
imgproc/imgcodecs/videoio의 선언을 사용하는 작은 C++ API 파일을 factory g++로 AArch64
ET_REL object로 컴파일하여 원래 생성 설정과 전체 header include 경로를 검사한다.
이 object는 실행하지 않으며 현재 Rust consumer 전체의 빌드·link·수치 회귀를 뜻하지 않는다.

완료한 run의 `native-opencv-headers-*.json`, Tree, 변조 거부 stderr와 실제 ARM commands
log/API source·object hash를 확인한다. 자산 HTTP byte는 Actions cache restore 전송량과
구분한다. 반복 압축 해제 생략은 이 준비기의 동작이며 콤마 전체 시간/CPU 개선 실측이 아니다.

로컬에서는 구문·서식·diff/읽기만 수행했다. 원래 공개 archive의 SHA/길이를 bounded
stream으로 읽어 확인했으며 파일 저장·압축 해제·새 준비기·compiler 실행은 하지 않았다.
새 기능/compile 성공은 Actions 결과가 나와야 기록한다. 같은 기존 OpenCV shared
provider의 [독립 component/prefix와 ARM bridge 검사](native-opencv-prefix-599.md)는 별도
단계이며 해당 Actions 결과를 확인해야 한다. 실제 consumer의 전체 원본·모델 회귀와
전체99개 generation 빌드/seal/default startup, 사용자 comma3X 자체 빌드·부팅·녹화·
업로드·주행 수용은 아직 미완료다.
