# 시작에 필요한 FFmpeg 도구·클러스터 글꼴 — #599 (검증 대기)

상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598),
환경 [#599](https://github.com/bin9208/openpilot-rust/issues/599),
캐시 [#600](https://github.com/bin9208/openpilot-rust/issues/600).
실제 manager-catalog의 cluster binding은 프로젝트 Rust 실행 파일 외에도 ffmpeg/ffprobe와
rust/bin/cluster-replay/freesansbold.ttf를 요구한다. 이 외부 C 도구/글꼴은 별도 자산이며
현재 source의 Rust binary를 다시 빌드하는99개 catalog roster와 구분한다.

## 고정 원본과 경계

앞선 [library 입력](native-display-inputs-599.md)과 같은 원본 source
a3d0e3a497b4ae4d23c7cf5ee2275177600bbb31, 원본 archive1155969168 bytes/
SHA9c9c16a72bad163226388ef84327b0db1fa64bbe4828e64388e0a7e92d364291와 전체 manifest
SHA8adbc002388fb97420e4bef9dcf58a5ed75de4c4632a1bc0d10c555f1c2d39ee를 고정한다.

| profile | 선택 | member | payload bytes | inventory SHA |
| --- | --- | --- | --- | --- |
| ffmpeg611-tools | FFmpeg6.1.1/ffprobe, 원래 notice/provenance5개와 전체 manifest | 8 | 887747 | adf3b43e0b3bd371c4a9c278a704b8607da0523fcbf79f0fe4eaec5905d19de6 |
| freesans-pygame252 | 원래 클러스터 FreeSans98600 bytes, GPL/출처/notice4개와 전체 manifest | 6 | 396904 | ed2fa6bed647ae89a98e4742c9c5c083a5584261219b8165c57cf0e786b5854d |

FFmpeg SHA는 f2cc8824d08d9d4d4e6beb36e89d10ef69bb0a8c2ea7f1840f23ca456b1847c8,
ffprobe SHA는 bb8992db3348212b870083099f60c2771dbe2d490b76205c824899637beb4e46,
글꼴 SHA는 bf9251269f11e4b35582a99376096deee3d0c4973a459cbd97b0bebc01f440ad다.
원래 manifest 소속/크기/모드/hash와 독립 component descriptor/전체 Tree를 검증한다.
기존 profile의 inventory/identity는 바꾸지 않는다. 생성 모델/Panda firmware/프로젝트 Rust
실행 파일을 외부 도구로 취급하거나 다른 source의 Rust binary를 가져오지 않는다.

기존 generation 봉인은 외부 executable 중 ffmpeg/ffprobe만 별도로 허용하고 실제 ELF와
의존성을 검사한다. 이 제한은 유지한다. host는 기존 origin content cache에서 작은
component를 export하고 소비자는 작은 component의 독립 pin만 사용한다. 원본 source와
compiler provenance를 현재 SHA로 다시 쓰지 않는다. 공개 component 배포/소비는 남았다.

## 검사 방법

변경 branch push가 foundation 및 genuine AGNOS 검사에 자동 연결된다.

```sh
gh run list --repo bin9208/openpilot-rust --workflow runtime-source-build.yml --branch BRANCH
gh run list --repo bin9208/openpilot-rust --workflow runtime-native-environment.yml --branch BRANCH
gh run view RUN_ID --repo bin9208/openpilot-rust --log-failed
```

foundation은 닫힌 production profile·notice·파일 모드·프로젝트 실행 파일로의 치환 거부와
기존 공통 provider 회귀를 실행하고 실제 Rust glyph 예제를 x64/ARM strict Clippy로 검사한다.
genuine은 source-built ARM 준비기로 host와 같은 Tree, 초기/반복 전송/offline seed0,
반복 복사0을 확인한다. 이 절의8개 Tree에 후속 [Git LFS](native-source-lfs-599.md) Tree1개를
더한9개를 전체 계획에 연결한다.
partial acados는 기존 source-bound MPC로 대체하며 compiler recipe와 runtime input을 구분한다.

실제 원래 FFmpeg/ffprobe를 같은 고정 FFmpeg prefix로 실행한다. FFV1와 libx264 lossless
모드로 각각16x16 YUV420P3프레임을 기록하고 ffprobe의 codec/dimensions/pixel format/
frame count를 확인한다. 실제 FFmpeg로 decode한 전체1152 bytes가 입력과 같아야 한다.
입력을 기다리는 자기 자식 process의 maps에서 원래8개 FFmpeg library 경로를 확인한다.
검사 중 생성한 child만 관리하고 기존 작업/기기/로그에는 연결하지 않는다.

같은 guest에서 현재 Rust SDL gui 예제를 native release locked/offline 빌드한다.
원래 FreeSans를 size12/18·normal/bold·ASCII/Å/β로4회 렌더링하고 실제 software window에
표시·capture한다. 별도 C++ reference는 공개 C API와 opaque SDL handle만 사용한다.
글꼴 이미지와 window capture의 dimensions/전체 RGBA bytes가 각각 정확히 같아야 한다.
실제 maps의 원래 SDL/TTF 경로와 Python library 미적재, 도구2개/실제 Rust 예제/C++ reference의
strict AArch64 ELF·의존성, 반복 Cargo fresh/동일 executable SHA와 API 후 provider 재준비0도
검사한다. source·commands·JSON/JSONL·probe·pixel/영상·maps artifact를 정확 SHA로 확인한다.

host 작업별 예상 증가량은 component payload4배+16MiB, guest 실제 예제 빌드는2GiB다.
전체 runtime의 실측 자원·물리 GPU·Korean glyph coverage 또는 사용자 수용을 뜻하지 않는다.
로컬은 서식/문법/JSON/hash/diff 읽기만 수행하며 새 helper/codec/GUI를 실행하지 않는다.
Actions는 등록된 bin-actions(self-hosted/Linux/X64/dev-md-local), guest는 x64 위
QEMU/PRoot AGNOS ARM user space다. 물리 ARM CPU/AGNOS kernel 부팅/comma3X 실행과 구분한다.

## 전체 입력과 남은 작업

소스 자산 allowlist17개를 읽기 감사한 결과9개 font/image는 일반 Git blob,
alert SVG2개/Xiaoge ONNX2개는 Git LFS pointer, Panda firmware4개는 생성물이다.
일반 Git 자산은 retained source 자체로 보존하고 LFS는 현재 OID/size로 준비해야 한다.
Panda 생성물을 LFS 또는 외부 library로 취급하지 않는다. 이 후속은17개 전체 준비를
구현한 단계가 아니다. 모델은 Python 준비·입력 계약/의존성/소스·실제 생성물 검증을 유지한다.

정확 source CI 성공/정상 PR·병합 후 검사, 전체 모델·데이터·Layout/99개 native build/seal,
기본 update/start/record/upload/복구와 자원·성능, 전체 원본 회귀 및 사용자 comma3X 자체
빌드/부팅/기록/기존 업로드/주행 수용은 계속 남는다. #598은 모든 조건 충족 전 열어 둔다.
