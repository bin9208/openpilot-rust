# 기존 Eigen 입력의 내용별 준비와 선택 — #599 (기능 검증 대기)

상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598),
[환경 #599](https://github.com/bin9208/openpilot-rust/issues/599).
모델 준비 Python과 기존 C/C++ 코드를 유지하고, estimator의 원래 Eigen 입력을 준비한다.
새 solver/수학 정책으로 바꾸지 않는다.

## 원래 입력과 사용 그룹

`uv.lock`의 Eigen3.4.0 wrapper commit은
`40e5d76de1b33a86c5181b63db6782d8f06da1da`다. wrapper의 고정 pyproject는 원래
`commaai/dependencies`의 `eigen/v3.4.0` ARM release wheel을 선택한다.
[원래 release](https://github.com/commaai/dependencies/releases/tag/eigen/v3.4.0)의
asset470585660은2250657 bytes, SHA256
`b02ee704aa964d9d7bafb53cc0e096680e4a3b4f6fb1cd98b0a85c7298f3dc0a`다.
이는 library 자체의 upstream Git commit이라고 바꿔 표현하지 않는 wrapper/source pin이다.

원래 wheel의 `eigen-3.4.0.data/purelib/eigen/install/`에 있는 native 파일과 원래
dist-info METADATA/WHEEL/RECORD 등 배포 metadata를 준비한다. tag는 원래
`py3-none-linux_aarch64`이며, 로컬 shim이 다시 만든 `py3-none-any` wheel로 대체하지 않는다.
원본 native header의 copyright/라이선스 표기는 byte 그대로 보존한다. 원래 wheel에는
별도 COPYING/license 파일이 관찰되지 않았다. install 밖의 Python shim과 relicense script는
준비하거나 실행하지 않는다. pip·Python package installer를 호출하지 않는다.

catalog의04 locationd,06 paramsd는 기존 C++ rednose helper에서
`<eigen3/Eigen/Dense>`를 사용한다. compiler 입력은 다음과 같다.

| 소비자 | 환경 | 값 |
|---|---|---|
| locationd native/solver | `LOCATIOND_EIGEN_INCLUDE` | 검증된 Eigen prefix root |
| paramsd native/solver | `PARAMSD_EIGEN_INCLUDE` | 같은 검증된 Eigen prefix root |

root 아래의 `eigen3`를 include 경로에 또 덧붙이지 않는다. 각 소비 그룹에만 이 완전 Tree와
독립 SHA-pinned receipt를 연결한다. 다른 group/model cache를 불필요하게 바꾸지 않는다.
최종 native bindings/default startup 연결 및 전체 module 회귀는 아직 별도 미완료다.

## 필요한 wheel만 준비

기존 Rust 준비기의 CLI 이름을 유지하고 명시적인 `--packages` 선택을 추가했다.
기본은 원래 FFmpeg7.1.0/libyuv1922.0 두 wheel 그대로다. final runtime의 FFmpeg6.1.1
header/shared provider는 [별도 고정 경로](native-provider-components-599.md)를 계속 사용한다.

아래는 Actions 또는 준비된 개발 환경용이며 장치 설치 명령이 아니다.

```sh
stage-encoder-target --prepare-wheels --packages eigen \
  --root "$SOURCE_ROOT" --cache "$EIGEN_ASSET_CACHE" --store "$NATIVE_PREFIX_STORE"

stage-encoder-target --prepare-wheels --packages libyuv \
  --root "$SOURCE_ROOT" --cache "$CODEC_ASSET_CACHE" --store "$NATIVE_PREFIX_STORE"
```

`eigen`, `libyuv`, `ffmpeg`만 선택할 수 있다. 빈 이름·중복·알 수 없는 이름, 기존 staging/
compiler 인자와 혼용을 거부한다. 선택한 uv.lock row의 버전/wrapper를 준비 전후 확인한다.
선택하지 않은 row 변경은 해당 input을 다운로드하거나 재준비하게 만들지 않는다.

기존 archive 인증/ZIP 중복·경로·타입·크기·metadata 검사, raw에서 매번 도출한 기대 Tree,
내용별 lock/읽기 전용 파일/작업별 실제 예상 증가량/fsync/원자 발행을 그대로 사용한다.
기존 두 wheel의 identity/Tree를 바꾸지 않는다. 반복 준비와 기존 pair에서 libyuv만 선택한
준비는 HTTP body/stored archive/copy bytes가0이어야 한다. 이 값은 준비기의 자산 HTTP
계측이며 GitHub Actions cache restore 전송량 또는 실기 성능으로 바꿔 쓰지 않는다.

## 검사와 남은 범위

- foundation X64/ARM QEMU는 기존 회귀·Clippy·원래 두 wheel 명령을 유지한다. 선택한
  provider만 존재하는 fixture에서 준비/반복/선택 row 변조 거부·다른 row 변경 재사용과
  잘못된 CLI 선택의 조기 실패를 추가한다. 실제 CLI는 고정 공개 Eigen wheel의 초기/반복,
  Python shim 제외와 기존 libyuv prefix의 동일 identity/Tree·전송0·복사0을 확인한다.
- genuine은 같은 원본 raw cache를 ARM source-built 준비기로 소비하고 독립 host Tree와
  비교한다. 반복 전송/복사0 및 실행 뒤 재검증을 확인한다. 실제 factory g++로 기존
  `rednose/helpers/ekf_sym.cc`를 AArch64 ET_REL object로 컴파일하고, Eigen3.4.0을 고정한
  작은 LDLT 사례를 실행해 정확한 결과/잔차0과 엄격한 ELF audit를 확인한다. object의
  `-O0` 컴파일은 header/API 확인이며 전체 release 최적화·filter 수치 회귀가 아니다.
- `native-eigen-*.json`/Tree, `native-libyuv-only.json`, 실제 commands log와 cpp build/run/
  ELF audit 기록을 정확 SHA artifact로 남긴다. Python CI script는 이 Rust CLI의 검사
  orchestration일 뿐 장치의 preparation/runtime 경로가 아니다.

로컬은 구문·서식·diff/읽기만 수행한다. 공개 ZIP metadata92327 bytes의 범위 읽기로
원래 prefix/tag/제공 파일 목록을 관찰했으며 파일 추출·준비기·installer는 실행하지 않았다.
전체 archive 인증과 새 기능/compile/실행은 Actions 결과 전까지 성공으로 기록하지 않는다.
genuine도 X64의 QEMU/PRoot AGNOS user space이며 물리 ARM·기기·완전한 estimator 회귀·
99개 generation build/seal/start·성능·comma3X 수용은 미완료다.
