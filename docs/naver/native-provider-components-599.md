# 기존 FFmpeg shared provider의 독립 component — #599 (검증 대기)

상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598),
[환경 #599](https://github.com/bin9208/openpilot-rust/issues/599).
기존 C/C++ provider를 현재 runtime commit과 독립된 작은 자산으로 재사용한다.
원래 provider의 source/ABI를 유지하고 프로젝트 실행 파일은 새 소스에서 빌드한다.

## 원본과 선택 범위

`rust/native-ffmpeg-provider.json`은 기존 source
`a3d0e3a497b4ae4d23c7cf5ee2275177600bbb31`의 공개 archive를 고정한다.
압축 크기는1155969168 bytes, SHA는
`9c9c16a72bad163226388ef84327b0db1fa64bbe4828e64388e0a7e92d364291`이다.
이는 **일회성 component 생산용 원본**이며 소비 경로의 다운로드 대상이 아니다.

고정36개 member는 FFmpeg6.1.1 shared library24개 이름/alias, libvpx9의2개 이름/alias,
외부 FFmpeg/ffprobe CLI2개, FFmpeg/vpx notice·원래 build/package metadata7개와 원래 runtime
manifest다. alias17개를 제외한 payload는23934709 bytes이며, 새 provenance와 gzip/TAR
metadata는 별도다. 실제 압축 component 크기는 Actions 결과로 기록한다.

member의 source/destination/SHA/bytes/mode/link 목록 자체를 고정 SHA로 검증한다.
원래 manifest SHA `8adbc002388fb97420e4bef9dcf58a5ed75de4c4632a1bc0d10c555f1c2d39ee`와
source/AGNOS, 선택 member의 원래 manifest 소속도 확인한다. 현재 runtime source로 다시
표기하거나 old project executable을 선택할 수 없다. 원래 manifest는 provenance 데이터다.

읽기 감사에서 FFmpeg library와 CLI는 이 component의 libvpx.so.9 및 AGNOS의 x264/OpenSSL/
zlib/libc/libm/loader를 사용했다. 이는 static DT_NEEDED 확인이며 실제 ABI 성공은 Actions의
엄격한 ELF 검사와 실행 결과에서 따로 확인한다. 해당 라이브러리나 요구 버전을 완화하지 않는다.

## 생산과 소비의 구분

`build-runtime-ffmpeg --export-released-provider`는 내용 cache에서 원래 archive를 인증하고,
고정 foreign member만 임시 prefix로 선택한다. raw source의 다른 프로젝트 파일은 쓰거나
실행하지 않는다. 예상 source/alias·종류·크기·hash·mode·manifest와 archive 한도를 검사한다.
별도 prefix를 원자적으로 게시하고 결정적인 gzip/TAR component 및 descriptor **후보**를 만든다.
원래 notice/build record/source는 보존한다. exporter는 provider를 실행하거나 release를 발행하지 않는다.

`--prepare-released-provider`는 descriptor의 **독립적으로 고정한 SHA**를 요구한다.
descriptor/source/target/AGNOS/expected Tree/owner release URL·압축 SHA·크기를 검증한 뒤
해당 component만 준비한다. 원래1.1GB archive를 가져오는 코드는 이 소비 경로에 없다.
지금은 검증된 공개 component와 독립 Git pin이 아직 없으므로 기본 시작에 연결하지 않는다.

기대 Tree는 고정 원본 member 목록에서 계산한다. 로컬 receipt를 함께 위조해도 기대값을
대체하지 못한다. 추가/프로젝트 member·경로 탈출·잘못된 alias·종류/mode/hash/크기·손상
metadata를 거부한다. alias는 모든 file 검증 후 고정된 내부 file 대상으로만 만든다.
내용별 lock/NOFOLLOW/private temp/fsync/rename-NOREPLACE를 사용하고 기존 prefix와 로그를 보존한다.

압축 component 상한은64MiB다. source 생산은8GiB, component 소비는128MiB의 스트림 확장
상한을 사용한다. 디스크는 raw asset와 실제 prefix/member 수·archive 출력의 작업별 예상
증가량으로 판정하며 고정25/35GiB 기준은 쓰지 않는다. 생성 실패 시 기존 결과를 덮어쓰지 않는다.

반복 component HTTP body/stored/copy byte 수는0이어야 한다. URL은 내용 hash의 별도
`native-provider-ffmpeg611-<SHA>` namespace이며 source/current runtime SHA와 혼동하지 않는다.
새 provider descriptor는 해당 provider의 자산만 선택한다. 나머지 provider/모델과 전체
group 배정은 전체 native coordinator에서 별도로 묶어야 한다.

## Actions 검사 방법

### 실제 Rust 소비에 필요한 단일 prefix — 구현 추가, 기능 검증 대기

`build-runtime-ffmpeg --prepare-released-prefix`는 위의 실제 header/provider 준비기를
다시 호출한다. 두 기대 Tree의 내용 SHA를 확인하고 `include/`와 `lib/`가 함께 있는
하나의 `FFMPEG_DIR`를 만든다. 임의 local receipt 두 개를 받는 별도 신뢰 우회 경로는 없다.
양쪽 원본 provenance·notice·alias를 보존하고 같은 경로의 내용/mode/type이 다르면 거부한다.

identity는 profile/target/AGNOS와 두 입력 Tree SHA에만 의존한다. 원래 provider source는
그대로 남고 현재 runtime commit이나 입력 prefix의 절대 경로를 identity에 넣지 않는다.
읽기 전용444/555 file은 같은 filesystem에서 hardlink한다. EXDEV일 때만 배타적·길이 제한
복사를 사용한다. 별도 provenance 복사량과 file hardlink량을 구분하며, 반복 준비는
양쪽 raw 전송/stored/copy 및 합성 prefix copy/link가 모두0이어야 한다.

기존 archive 준비기와 공유하는 publication helper가 내용별 lock, 작업별 실제 payload와
metadata의 예상 증가량, 임시 directory, fsync, rename-NOREPLACE, 기존 generation의 완전
member 검사를 유지한다. 합성 입력에는 가짜 HTTP asset receipt를 만들지 않는다.
초기/반복·경로 독립 identity·동일 notice 중복·alias·충돌·입력 변조·generation 추가 member의
회귀를 foundation의 X64/ARM QEMU에서 실행한다. 로컬은 구문·서식·diff만 확인한다.

```sh
build-runtime-ffmpeg --prepare-released-prefix \
  --header-plan rust/native-ffmpeg-headers.json --provider-plan rust/native-ffmpeg-provider.json \
  --descriptor "$VERIFIED_DESCRIPTOR" --descriptor-sha256 "$INDEPENDENT_DESCRIPTOR_SHA256" \
  --header-cache "$FFMPEG_SOURCE_CACHE" --provider-cache "$COMPONENT_CACHE" --store "$PREFIX_STORE"
```

genuine 검사에서는 독립 host/ARM prefix Tree가 일치해야 한다. 실제 `openpilot-encoderd`를
현재 Git 소스와 원래 catalog의 native/visionipc-ion feature로 release 빌드한다. 기존
`openpilot-runtime-build`에 Rust/Cargo vendor/capnp/libyuv/합성 prefix의 완전 Tree와
AGNOS SDK의 독립 SHA를 전달한다. 같은 recipe의 반복 binary는 fresh/동일 hash여야 한다.
이때 프로젝트 binary를 옛 runtime에서 가져오지 않는다.

현재 소스의 `encoder_codec_trace`와 기존 원본 C++ method oracle도 같은 FFmpeg6.1.1/
libyuv/JPEG로 빌드한다. 기존5개 lossless/H.264/JPEG case의 압축 packet bytes 및 metadata를
정확 비교하며 허용 오차나 codec 설정은 바꾸지 않는다. 기존 host `--native` 명령도 유지한다.
별도 prefix 선택은 `--ffmpeg-prefix`와 `--libyuv-prefix`를 함께 사용한다. command별
timeout은 기존 host120초, QEMU600초이며 결과 receipt에 기록한다. runtime validity나
실시간 deadline을 완화한 값이 아니다. 실제 binary/example/oracle은 엄격한 ELF 검사를 거친다.

기존 #608 ARM release 초기 빌드6386611ms가90분보다 길었으므로, 원래 command/pin을
유지한 genuine job에 실제 인코더 단계를 추가하며 상한을360분으로 늘렸다. 이 예산은
실기 시간/성능이나 이번 job의 성공 근거가 아니다. `native-encoder-*.json/jsonl`과 codec
`receipt.json`/원문 log에서 각 단계 결과를 확인한다. 별도 daemon 시작·99개 전체 runtime
빌드·정상 부팅·녹화/업로드·물리 ARM·comma 3X 수용은 여전히 별도 미충족 항목이다.

로컬은 구문·서식·diff/읽기만 수행한다. 아래는 개발용 형태이며 device 설치 명령이 아니다.
cache/store/output의 부모는 미리 있어야 한다. 기존 output은 대체하지 않는다.

```sh
build-runtime-ffmpeg --export-released-provider --plan rust/native-ffmpeg-provider.json \
  --cache "$ORIGIN_CACHE" --store "$ORIGIN_PREFIX_STORE" --output "$NEW_COMPONENT_EXPORT"

build-runtime-ffmpeg --prepare-released-provider --plan rust/native-ffmpeg-provider.json \
  --descriptor "$VERIFIED_DESCRIPTOR" --descriptor-sha256 "$INDEPENDENT_DESCRIPTOR_SHA256" \
  --cache "$COMPONENT_CACHE" --store "$COMPONENT_PREFIX_STORE"
```

- foundation 회귀4개는 고정 source/member 제약, 원래 project code의 생산·소비 제외,
  component 왕복과 반복 body/copy0, descriptor pin/옛 source 재표기 거부, 변조 file/alias/
  추가 project member 거부와 이전 원본 prefix 보존을 확인한다. 소유 fixture는 실행하지 않는다.
  기존 FFmpeg CLI 회귀도 유지하고 독립 descriptor pin 누락의 조기 거부를 추가한다.
- genuine host는 원본 archive를 cache로 준비해 후보를 만든다. 별도 cache에 `--offline-archive`
  로 후보를 seed하고 독립 export stdout의 descriptor SHA로 소비한다. 첫 seed 복사 bytes와
  반복0을 구분하며 이 결과를 공개 HTTP component 다운로드 성공으로 표현하지 않는다.
- 실제 source-built ARM consumer는 warm component cache만 이용하고 원래 archive를 받지 않는다.
  독립 host Tree와 비교하고 기존6.1.1 header와8개 shared library의 실제 version ABI를
  링크한다. 현재 소스의 ELF auditor로 symbol/version/dependency를 검사한 후 ABI 프로그램과
  foreign FFmpeg/ffprobe `-version`을 실행한다. 실행 뒤 prefix를 재검증한다.
- `native-provider-export.json`, 후보 archive/descriptor, consumer 초기/반복, Tree,
  `native-provider-*.json/jsonl/txt` 및 commands log를 정확 source artifact에서 확인한다.

새 기능 성공은 아직 미확정이다. 모든 job은 등록된 bin-actions이고 ARM은 QEMU/PRoot 사용자
공간이다. 공개 component 발행·독립 Git pin·실제 HTTPS 다운로드/재시도 검증은 남아 있다.
이 component는 [6.1.1 header](native-ffmpeg-headers-599.md)와 함께 compiler input에 연결해야 한다.
OpenCV/raylib 등 다른 provider, full99개 source build/seal·aggregate 자원·기본 시작/업데이트/
복구·Python 모델 준비·녹화/기존 업로드·정확 source PR/병합 후·사용자 comma3X 수용도 남는다.
#598/#599는 열린 상태로 유지한다.
