# 기존 OpenCV 공유 라이브러리와 검증 prefix — #599 (Actions 검증 대기)

상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598),
[환경 #599](https://github.com/bin9208/openpilot-rust/issues/599).
[원래 헤더170개](native-opencv-headers-599.md)에 같은 기존 OpenCV4.13.0 shared provider를
연결한다. 모델 준비 Python과 C/C++ 라이브러리를 유지하는 전체 native 빌드의 중간 단계다.

## 고정한 provider와 prefix

`rust/native-opencv-provider.json`의 원래 release source는
`a3d0e3a497b4ae4d23c7cf5ee2275177600bbb31`이다. 원래 archive SHA는
`9c9c16a72bad163226388ef84327b0db1fa64bbe4828e64388e0a7e92d364291`, runtime manifest SHA는
`8adbc002388fb97420e4bef9dcf58a5ed75de4c4632a1bc0d10c555f1c2d39ee`다. library5개,
alias10개, 라이선스/notices/source/build provenance14개와 원래 manifest1개, 총30개
member를 선택한다. regular payload 합은21520422 bytes이며 압축·추가 provenance 크기는
실제 export 보고서에 별도 기록한다. 프로젝트 실행 파일은 없다.

member inventory SHA는
`aba0eb2961db6b8d19b245bae8138a34f28e42b21eec008553ee95c1f270c15a`다. FFmpeg/raylib와
독립된 고정 profile `opencv413-shared`, release namespace `native-provider-opencv413-…`를
사용한다. profile/파일 선택을 바꾸면 고정 inventory 검증에 실패한다. 기존 두 profile의
Tree/URL/identity는 유지한다.

prefix에는 `install/include/opencv4`, `install/lib`, 원래 notices/provenance와
`native-libraries.sha256`가 있다. 기존 두 consumer가 같은 root를 사용한다.

| 소비자 | 환경 변수 |
|---|---|
| opencv-runtime/Xiaoge native bridge | `OPENPILOT_OPENCV_ROOT` |
| webcam native capture | `OPENPILOT_WEBCAM_OPENCV_ROOT` |

새 compiler manifest는 header170개+library5개+해시가 같은 alias10개, 총185행이다.
파일별 경로와 SHA는 기존 구성과 같다. 경로순 정렬한 새 manifest SHA는
`bdc3c7f0ea23ea1e15272906478d92433997c8f36a4fe2bd39ab167211be2d06`다. library 행부터
기록한 기존 복원 cache의185행 manifest와는 정렬 순서 때문에 파일 자체 SHA가 다르다.
원래 release의199행 manifest는 SHA
`d34c38a3ae14a51624759d526a825e596297ff203ad3ad6861cd08a32798934d` 그대로 notices와
provenance에 보존한다. 새 manifest를 원래 build receipt라고 표시하지 않는다.

2026-10-28의 [정렬 회귀 #617](https://github.com/bin9208/openpilot-rust/issues/617)은
source `eb7ac90182cf4a81294547ef0da09f0f67ff6c84`의
[genuine 실패](https://github.com/bin9208/openpilot-rust/actions/runs/40685061209/job/122806456685)로
확인했다. `PathBuf` component 순서가 `core/affine.hpp`를 `core.hpp`보다 먼저 두어
185행의 SHA가 `8fe80dcff866a62cfa95f5b9d7950b17b312d9f3e436391ec404e0a514f2f4b2`가 된다.
고정 원본185개 경로/SHA의 읽기 대조에서 UTF-8 문자열 순서는 기존 고정 SHA
`bdc3c7f0ea23ea1e15272906478d92433997c8f36a4fe2bd39ab167211be2d06`과 일치한다.
manifest key만 문자열로 정렬하고 비 UTF-8 경로는 거부한다. 고정값·파일·alias
검증은 유지하며 두 header 경로가 함께 있는 실제 sha256sum/hardlink/재사용 회귀를
보강했다. 로컬 서식·diff만 확인하고 실제 준비·재사용 성공은 수정 source의 Actions로 확인한다.

기존 build.rs의 sha256sum, 정확4.13.0 alias 검사와 C++ bridge 컴파일 경로를 유지한다.
prefix는 모든 입력 Tree/receipt를 검증하고 readonly 파일을 hardlink한다. EXDEV에서는
exclusive bounded copy·동일 hash 검증·fsync를 적용한다. 생성 metadata 덮어쓰기, 파일
충돌과 잘못된 alias를 거부한다. identity는 입력 Tree/manifest SHA에 의존하며 현재 commit/
경로에는 의존하지 않는다. 같은 발행 engine을 쓰면서 기존 FFmpeg metadata tuple과
Tree/identity는 유지한다.

## 준비 명령과 공개 상태

다음 명령은 Actions/개발 검증용이며 기기 설치 명령이 아니다.

```sh
build-runtime-ffmpeg --export-released-provider \
  --plan "$SOURCE_ROOT/rust/native-opencv-provider.json" \
  --cache "$ORIGIN_CACHE" --store "$ORIGIN_PREFIX_STORE" --output "$COMPONENT_OUTPUT"

build-runtime-ffmpeg --prepare-released-provider \
  --plan "$SOURCE_ROOT/rust/native-opencv-provider.json" \
  --descriptor "$DESCRIPTOR" --descriptor-sha256 "$INDEPENDENT_DESCRIPTOR_SHA256" \
  --cache "$COMPONENT_CACHE" --store "$PROVIDER_PREFIX_STORE" --offline-archive "$COMPONENT"

build-runtime-opencv --prepare-released-prefix \
  --header-plan "$SOURCE_ROOT/rust/native-opencv-headers.json" \
  --provider-plan "$SOURCE_ROOT/rust/native-opencv-provider.json" \
  --descriptor "$DESCRIPTOR" --descriptor-sha256 "$INDEPENDENT_DESCRIPTOR_SHA256" \
  --header-cache "$OPENCV_SOURCE_CACHE" --provider-cache "$COMPONENT_CACHE" \
  --store "$PREFIX_STORE"
```

producer는 이미 FFmpeg/raylib에서 인증한 고정 origin cache를 재사용한다. 반복 origin
HTTP body/stored bytes는0이어야 한다. consumer는 작은 component와 독립 descriptor SHA만
소비하며1.1GB origin runtime archive를 다운로드하지 않는다. 공개 release와 Git의 독립
descriptor pin은 아직 없으므로 offline 후보 검사를 공개 다운로드 성공으로 쓰지 않는다.
기존 legacy OpenCV library build CLI도 유지한다. FFmpeg/raylib profile 또는 독립
descriptor pin을 생략한 조합은 입력 준비 전에 거부한다.

## 검사와 남은 수용

foundation은 기존 provider/FFmpeg prefix 회귀와 새 profile 분리, OpenCV manifest·실제
sha256sum·alias/hash·inode 재사용, 생성 metadata 충돌과 외부 alias 거부, CLI profile 혼용
거부 및 Clippy를 실행한다. 단위 fixture는 작은 owned 파일이며 실제 OpenCV 실행은 아니다.

genuine host는 실제 고정 component export/consumer와 prefix 초기/반복 준비를 실행한다.
ARM QEMU/PRoot AGNOS19.8에서는 source-built 준비기로 같은 component를 소비하고 독립
host Tree와 비교한다. 반복 자산 HTTP/stored/copy/link bytes가0이고 헤더 source의
재압축해제가 없어야 한다. 이는 Actions cache restore 전송량이나 기기 성능 실측이 아니다.

실제 `openpilot-opencv-runtime`의 `opencv_trace`와 C++ FFI bridge를 고정 toolchain으로
ARM release 빌드한다. 기존 example에 owned BGR→gray, 상수 image resize, bounds3개 사례를
실행해 정확 byte/결과와 maps의 실제 native library 경로를 확인한다. 별도 C++ ABI helper는
5개 기존 library를 link하여 버전/기본 pixels/BMP roundtrip/빈 DNN Net/닫힌 VideoCapture를
확인한다. 카메라를 열지 않는다. 모든 root와 factory/FFmpeg library에 기존의 엄격한
ELF architecture/dependency/strong symbol/version 검사를 적용한다.

Cargo 반복 결과는 같은 binary hash와 fresh=true, prefix는 동일 Tree/복사0/link0이어야
한다. maps에서 원래 prefix의 library mapping과 Python shim이 없음을 확인한다. PRoot가
표시할 수 있는 정확한 guest/host bind 경로만 허용한다. artifact의 provider/prefix 보고서,
Tree, 실제 ARM commands/build messages/ELF audit/maps/owned 사례 결과와 source/binary
hash를 함께 읽는다.

로컬은 구문·서식·diff/읽기만 수행하며 새 준비기/컴파일/기능은 Actions 결과 전까지 통과로
기록하지 않는다. 전체 webcam/Xiaoge의 원본 Python·실제 모델 회귀, 장치 capture, 전체99개
generation build/seal/default update/start/recovery, 녹화·기존 업로드·성능 및 사용자 comma3X
자체 빌드/부팅/녹화/업로드/주행 수용은 아직 미완료다. #598은 계속 OPEN이다.
