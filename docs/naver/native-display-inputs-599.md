# SDL·TurboJPEG 실행 입력과 실제 Rust ABI — #599 (검증 대기)

상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598),
환경 [#599](https://github.com/bin9208/openpilot-rust/issues/599),
캐시 [#600](https://github.com/bin9208/openpilot-rust/issues/600).
기존 C/C++ 라이브러리를 재사용하고 원래 runtime/모델 동작을 유지한다.

## 원래 자산과 준비

원본 source `a3d0e3a497b4ae4d23c7cf5ee2275177600bbb31`, archive1155969168 bytes/
SHA `9c9c16a72bad163226388ef84327b0db1fa64bbe4828e64388e0a7e92d364291`, 원본 전체
manifest241671 bytes/SHA `8adbc002388fb97420e4bef9dcf58a5ed75de4c4632a1bc0d10c555f1c2d39ee`를
그대로 사용한다. 원본 manifest의 공용 library 경로53개 중 아직 준비되지 않은11개를
두 개의 별도 닫힌 profile로 추가한다. 이 목록은 실제 배치·전체 runtime 실행 증거와 구분한다.

| profile | 원래 library 경로 | member | payload bytes | 고정 inventory SHA |
| --- | --- | --- | --- | --- |
| sdl228-runtime | SDL2/SDL2_ttf alias포함4, FreeType/HarfBuzz/libpng/Brotli2 등9 | 21 | 5548932 | bae47a4de03ac143d6c3cdcd0e719012b14b76989588b01c9d249b32b69e540f |
| turbojpeg3141-runtime | libturbojpeg.so.0 및 원래 target2 | 7 | 1108484 | 4480e350f6f09164b20145db7e3206481394c53129f3772bc27c817e9a3a3f02 |

SDL 원래 notice11개와 TurboJPEG notice/provenance4개, 각 원본 전체 manifest도 보존한다.
원래 alias3개의 target/hash를 유지한다. SDL dependency의 Brotli1.0.9는
[Carrot 서버의 Brotli1.1.0](native-runtime-companions-599.md)과 별도이며 바꿔 끼우지 않는다.
외부 wheel의 hashed library를 AGNOS factory의 다른 파일로 대체하지 않는다.
프로젝트 Rust 실행 파일이나 생성 모델/solver는 이 profile에 넣지 않는다.

기존 build-runtime-ffmpeg export/prepare API의 strict 원본 archive/member/hash/mode/
alias/manifest 검증을 재사용한다. source_commit은 원래 provider source로 유지한다.
profile별 URL namespace/descriptor/component SHA를 분리하며 기존 profile identity는 바꾸지
않는다. host의 기존 원본 cache에서 작은 component를 export하고 소비자는 독립 pin으로
작은 component만 준비한다. 공개 component 배포와 HTTPS 소비 검증은 아직 남았다.

host prepare_native_runtime_companions.py는 기존4개와 새2개를 준비한다. 원본 manifest의
53개 root library 경로가 모든 provider plan의 원본 source 경로 합집합과 정확히 같은지도
대조한다. guest는 기존4개 API 증거에 새2개 runtime input Tree를 더하고 source-bound
MPC가 partial acados를 대체한다. 이 절의6개에 후속
[FFmpeg 도구·글꼴](native-media-tools-font-599.md)2개를 더해 전체13그룹/99개 계획에는
실행 입력8개 Tree를 만든다. 이후 [Git LFS](native-source-lfs-599.md) Tree1개를 더해
최종 계획에는9개를 연결한다. media 후속은 실제 glyph/software-window 비교도 추가한다.
다른 세션/기존 검증과 partial component 증거는 보존한다.

## 검사 방법

변경 branch push가 기존 foundation 및 genuine AGNOS workflow에 자동 연결된다.

```sh
gh run list --repo bin9208/openpilot-rust --workflow runtime-source-build.yml --branch BRANCH
gh run list --repo bin9208/openpilot-rust --workflow runtime-native-environment.yml --branch BRANCH
gh run view RUN_ID --repo bin9208/openpilot-rust --log-failed
```

foundation의 기존 provider roundtrip/변조/독립 pin 검사에 production profile/member/alias/
notice/foreign 경계 검사를 추가한다. 실제 Rust 예제2개는 x64/ARM strict Clippy로 검사한다.
genuine AGNOS의 source-built ARM 준비기는 host와 같은 Tree·원래 source를 확인한다.
초기/반복 준비 모두 HTTP body/offline seed0, 반복 복사0이어야 한다.

같은 고정 Rust/Cargo/vendor와 실제 factory C/C++ 도구로 현재 source의 예제2개를 native
release locked/offline 빌드한다. 실제 SDL Rust API가 소유한 virtual joystick/controller를
20회 생성·입력·snapshot·해제하고 기존 device 개수를 복구한다. 가짜 provider를 사용하지 않는다.
별도 C++ reference는 원래 SDL2.28.4/TTF2.20.1 및 TTF 초기화/정리 상태를 검사하고 원래
TurboJPEG3.1.4.1의 C API로 JPEG를 만든다. 실제 Rust TurboJpeg API는4개 입력
(1x1/q1,2x1/q100,63x35/q82,128x64/q75)을 각각20회 library/40회 compressor 생성·
정리와 invalid length/color 거부로 실행한다. C reference와 JPEG 파일 바이트가 정확히
같아야 한다. 품질·subsampling·색 변환 수학을 바꾸지 않는다.

실제 maps에서 각 원래 library 경로와 Python library 미적재를 확인한다. 실제 Rust
예제/C++ reference/원래 SO8개를 strict AArch64 ELF·의존성 검사하고, 같은 build 반복의
Cargo fresh/같은 executable SHA와 API 호출 후 provider 재준비0을 확인한다. artifact의
native-display JSON/JSONL/maps/cases/commands log를 정확 source SHA로 확인한다.
host 작업별 예상 증가량은 component payload4배+16MiB, guest는 예제 빌드2GiB다.
전체 runtime의 실제 최대 자원 측정으로 표현하지 않는다.

로컬에서는 Rustfmt/Python AST/TOML/YAML/JSON/hash·diff 읽기만 수행한다. 기능·빌드는
등록된 bin-actions(self-hosted/Linux/X64/dev-md-local)의 Actions에서만 실행한다.
genuine guest는 x64 위 QEMU/PRoot ARM user space이며 물리 ARM CPU/AGNOS kernel 부팅이나
comma3X의 화면/기기 성능이 아니다. 실제 glyph 렌더링·hardware GPU·전체 모듈/원본 모델
회귀와 전체 Layout/99개 native build/seal/default update/start/record/upload·복구, 정상
PR/병합 후 검사와 사용자 comma3X 수용은 계속 남는다. 새 변경의 CI 결과는 아직 대기다.
