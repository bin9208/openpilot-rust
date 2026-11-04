# 기존 ARM codec 입력의 내용별 준비 — #599 (검증 대기)

상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598),
[환경 #599](https://github.com/bin9208/openpilot-rust/issues/599).
기존 `stage-encoder-target`에 `--prepare-wheels`를 추가해 기존 C/C++ 라이브러리를
독립된 내용별 prefix로 준비한다. 새 언어 전환이나 codec 동작 변경은 없다.

## 유지한 원본 입력

`encoder_stage::packages()`와 현재 `uv.lock`의 원래 version/dependency-wrapper commit을
먼저 대조한다. 원본 ARM wheel은 다음과 같다.

| 입력 | 고정 원본 SHA256 | 압축 bytes |
| --- | --- | ---: |
| FFmpeg7.1.0 | `ce758b64c0343574e18ab97cbcff1e29868c66ccc5e0c866b5970266451203d4` | 11743600 |
| libyuv1922.0 | `1659e55a357f732836e5ed2a17fa65e715a11bf9bfa6d305e71fd7ac00a8e475` | 267013 |

wrapper commit은 FFmpeg `b9732165bcf5a3fab83b05994187802a0d115b6e`, libyuv
`28c3c2a2444232aeeaf989c33fd333ce74e6fc90`다. 이것을 upstream C/C++의 실제 build
compiler나 source attestation으로 바꾸어 표현하지 않는다. 원래 commaai/dependencies
release URL과 SHA·크기를 유지하며 wheel의 pip/setup.py는 실행하지 않는다.

원본 wheel의 install 하위 파일만 준비하고, 원래 dist-info의 METADATA/WHEEL/RECORD와
포함된 notice도 데이터로 보존한다. package 이름·version·ARM tag를 확인한다.
읽기 감사에서 이 두 원본 wheel의 dist-info에는 license 본문 파일이 없었다.
기존 원본 metadata 보존과 최종 배포의 전체 license/source notice closure는 별개이며,
이 준비 결과로 최종 runtime의 notice 완성을 주장하지 않는다.

## 재사용과 검증

기존 ZIP member 검증기를 legacy staging과 공유한다. 전체 중앙 directory record 수를
확인해 중복 이름을 숨기지 못하게 하고, 경로 탈출·파일/디렉터리 충돌·symlink·set-id·특수
파일·초과 payload를 거부한다. 새로운 mode는 정확 SHA로 인증한 raw bytes를 메모리에서
해석한다. 원본 ZIP과 로컬 추출 receipt 중 어느 것을 기대값으로 쓰는지 혼동하지 않는다.

자산 blob은 기존 SHA cache에, prefix는 각 package의 원래 version/source-wrapper/raw SHA
identity별로 보관한다. 다른 package나 현재 runtime commit은 그 prefix 선택 키가 아니다.
반복 때도 원본에서 전체 기대 Tree를 다시 계산하고 로컬 Tree·metadata·파일 목록·mode를
대조한다. 파일과 두 로컬 receipt를 함께 다시 써도 raw 원본을 대신할 수 없다.

파일은444/555로 만들고 소유 임시 경로에서 검증/fsync한 뒤 rename-NOREPLACE로 게시한다.
내용별 nonblocking lock과 NOFOLLOW 검사를 사용한다. 손상 hit는 실패하며 자동 교체하지
않는다. 실패 시 이전 prefix/runtime/source/log를 건드리지 않고 준비 결과를 활성화하지 않는다.
공간은 raw download와 실제 prefix payload·파일/디렉터리 수·receipt의 작업별 예상 증가량으로
판정한다. 해당 기존 codec staging의 고정25/35GiB floor도 최신 지침대로 제거했다.

반복 보고서의 `asset.transferred_bytes`, `asset.stored_bytes`, `copied_bytes`는0이어야 한다.
전송량은 HTTP body이며 GitHub cache-service 전송량이나 TLS/header 전체 사용량이 아니다.
`copied_bytes`는 새 prefix의 입력 payload와 보존 metadata/provenance byte 수다.
host 준비 결과의 `compiler_executed=false`/`runtime_activated=false`를 그대로 유지한다.

## Actions 검사 방법

로컬은 구문·rustfmt·diff/읽기만 수행한다. 아래는 Actions의 개발용 준비 명령이며 기기
설치 명령이 아니다. cache/store의 부모는 미리 있어야 한다.

```sh
stage-encoder-target --prepare-wheels --root "$SOURCE_ROOT" \
  --cache "$NATIVE_CODEC_ASSET_CACHE" --store "$NATIVE_CODEC_PREFIX_STORE"
```

- foundation: 기존 encoder staging 회귀를 모두 유지하고 CLI의 잘못된 lock 거부를 추가한다.
  `native_codec_inputs`의 소유 ZIP 회귀4개는 초기/반복/한 provider 변경·무관 prefix 보존,
  같은 크기 header와 두 로컬 receipt 위조, 잘못된 자산/경로/metadata, 동시 owner와
  generation symlink 거부를 검사한다. 실제 고정 wheel 초기/반복도 준비한다.
- genuine: host에서 만든 독립 Tree와 비교하며 실제 source-built ARM stager가 같은 원본을
  새 prefix로 처음/반복 준비한다. warm raw cache body0·repeat copied0을 확인한다.
  실제 AGNOS g++로 libyuv I420 white→ARGB 결과와 FFmpeg avutil header/library version ABI를
  링크·실행한다. 원래 compiler·driver4case·Rust·전체 cereal·bootstrap·system SDK 검사는 유지한다.
- `native-codec-first/reused.json`, Tree와 `native-codec-*.json/jsonl/txt`, commands log를 정확
  SHA의 artifact로 확인한다. 아직 새 기능의 성공을 가정하지 않는다.

이 준비는 FFmpeg wheel의 static 기본 입력이다. 원래 필요한 libswscale·전체 대응 header,
OpenCV/raylib·그 밖의 provider를 대체하지 않는다. 별도 runtime의 FFmpeg6.1.1 shared/CLI를
7.1로 자동 변경하지 않는다. catalog00의 cluster native-media와01의 encoderd가 같은 codec
의존성을 사용한다.03의 webcam/webrtc도 FFmpeg를 사용하므로 전체 build의 실제 추가 closure와
입력 그룹 배정은 계속 확인해야 한다. 무관한 모델을 compiler 입력에 함께 묶지 않는다.

QEMU/PRoot 사용자 공간의 두 외부 API 확인은 전체 codec 회귀·전체99개 runtime·물리 ARM
CPU·AGNOS kernel boot·기기 성능을 입증하지 않는다. 전체 native build/seal·기본 시작/업데이트/
복구·Python 모델 준비·녹화/기존 업로드, 정확 source PR/병합 후 검사와 사용자 comma3X 수용은
남아 있다. #598/#599는 열린 상태다.
