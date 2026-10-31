# 전체 runtime의 FFmpeg 6.1.1 헤더 준비 — #599 (검증 대기)

상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598),
[환경 #599](https://github.com/bin9208/openpilot-rust/issues/599).
전체 runtime의 기존 shared provider는 FFmpeg6.1.1이다. 별도 ARM encoder 검사의
FFmpeg7.1 wheel 준비와 구분하고 원래 버전·기능을 유지한다.

## 실제 기준과 원본 pin

기존 `build-target-groups-b55552db.sh`는 `cross-env.sh`의7.1 설정 뒤에
`FFMPEG_DIR=.../complete-ffmpeg-headers/verified-prefix`를 지정했다. 이 prefix는
**6.1.1의140개 정확한 public header와 기존 released shared library**를 사용했다.
앞선 읽기에서 base cross-env만으로 전체 빌드의 FFmpeg 버전을 추정하면 안 된다.

`rust/native-ffmpeg-headers.json`은 다음 원본을 고정한다.

- FFmpeg6.1.1 source `10458600` bytes, SHA
  `8684f4b00f94b85461884c3719382f1261f0d9eb3d59640a1f4ac0873616f968`.
- 기존 provider source `a3d0e3a497b4ae4d23c7cf5ee2275177600bbb31`의 released archive
  SHA `9c9c16a72bad163226388ef84327b0db1fa64bbe4828e64388e0a7e92d364291`.
- 그 runtime manifest SHA
  `8adbc002388fb97420e4bef9dcf58a5ed75de4c4632a1bc0d10c555f1c2d39ee`와 FFmpeg
  build record SHA `fa7815aa65aa90a18fc55d80a36e348c3a0bbb7ad7b46d197390cd600574df28`.
- public header140개의 SHA·bytes, 대응 shared library24개 이름/alias의 SHA.
  목록 자체의 compact JSON SHA도 코드에 고정해 entry 삭제·교체를 허용하지 않는다.

source와 일치하는138개 header는 upstream archive에서 읽는다. 생성 header 두 개는 원래
공개 provider의 byte를 그대로 사용하며 위140개 목록의 SHA·크기로 재확인한다.
`avconfig.h`는 little-endian/fast-unaligned AArch64이고 `ffversion.h`는6.1.1이다.
host에서 새로 configure한 값이나7.1 header로 대체하지 않는다.

기존 configure 진단 실패를 source/library 새 빌드 성공으로 사용하지 않았다. 이 mode는
compile/configure/make/pip/activation 없이 입력만 준비한다. `provider_libraries_prepared=false`,
`actual_target_execution=false`, `compiler_executed=false`를 보고한다.24개 library hash는
대응 기준이며 이 mode가 그 library를 이미 준비·실행했다는 뜻이 아니다.

## 보관·재사용

기존 codec wheel과 동일한 private publication helper를 재사용한다. 인증한 raw source에서
매번 기대 Tree를 다시 계산하며 로컬 receipt를 기대값으로 신뢰하지 않는다. SHA/크기,
압축 해제 상한·중복/탈출 경로, 선택 header/notice의 file 종류·mode, 누락·변조를 거부한다.
generated header도 같은 검증을 거치며 원본의 COPYING4개와 LICENSE.md를 함께 보존한다.

raw source만 기존 SHA cache에서 받고, 별도 prefix의 include·notice·provenance를444로
보관한다. prefix는 내용별 lock/NOFOLLOW/private temp/fsync/rename-NOREPLACE로 발행한다.
작업별 실제 prefix 예상 증가량으로 공간을 확인하고 기존 prefix·정상 runtime·로그를
덮어쓰거나 지우지 않는다. 반복 HTTP body/stored/copy byte 수는0이어야 한다.
입력 준비에는 기존 약1.1GB runtime archive를 다운로드하지 않는다.

원래 provider/source의 이름은 provenance에 보존하며 현재 runtime source로 다시 표기하지
않는다. 같은 입력은 현재 runtime commit과 독립적으로 재사용한다. 이후 실제 compiler
recipe에서는 이 Tree receipt의 SHA와 대응 library를 함께 고정해야 한다.

## 검사 방법

로컬은 문법·rustfmt·diff/읽기만 수행한다. 아래 명령의 기능 실행은 Actions에서 한다.
cache/store의 부모는 미리 존재해야 한다.

```sh
build-runtime-ffmpeg --prepare-released-headers \
  --plan rust/native-ffmpeg-headers.json \
  --cache "$NATIVE_FFMPEG_SOURCE_CACHE" --store "$NATIVE_FFMPEG_HEADER_STORE"
```

- foundation은 기존 모든 검사와 FFmpeg builder 회귀를 유지한다. 새 header 회귀3개는
  고정 목록/생성 설정/source/library pin 변경 거부, 원본 준비 초기/반복·copy/body0·같은
  크기 변조 거부, 누락/다른/링크 header의 발행 거부와 이전 prefix 보존을 확인한다.
  CLI는 변경 plan·기존 build 인수 혼합을 cache 생성/외부 실행 전에 거부한다.
- 실제 고정 source에서140개 header를 처음/반복 준비하고 Tree와 전송/복사량을 남긴다.
- genuine은 실제 ARM source-built preparer로 같은 일을 수행해 host 독립 Tree와 비교한다.
  실제 AGNOS g++로8개 public module interface와 정확 version/endian/word-size 조건을
  compile하고 little-endian AArch64 relocatable ELF를 확인한다. 이후 다시 입력을 확인한다.
  이 object는 링크·실행하지 않는다. library 실행·전체 codec 회귀로 확대하지 않는다.
- 정확 source artifact의 `native-ffmpeg-headers-first/reused.json`, Tree,
  `native-ffmpeg611-*.json/jsonl/txt`, commands log와 native object hash를 확인한다.

모든 job은 bin-actions의 등록 labels이며 ARM은 QEMU/PRoot 사용자 공간이다. 새 기능의
실제 통과는 Actions 결과 전까지 미확정이다.140개 header identity와 module object 검사가
물리 ARM CPU·AGNOS kernel boot·기기 자원/정상 실행을 대신하지 않는다.

기존6.1.1 shared provider와 그 ABI/dependency closure의 내용별 준비, OpenCV/raylib 등
다른 provider, full99개 native build/seal·aggregate 자원·기본 시작/업데이트/복구·Python
모델 준비·녹화/기존 업로드·정확 source PR/병합 후·사용자 comma3X 수용은 남아 있다.
#598/#599는 열린 상태로 유지한다.
