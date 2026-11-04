# 기존 raylib 입력과 독립 공유 provider — #599 (Actions 검증 대기)

상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598),
[환경 #599](https://github.com/bin9208/openpilot-rust/issues/599).
모델 준비 Python과 기존 C/C++ 라이브러리를 유지하면서 전체 native generation의
UI 입력을 준비한다. 현재 기본 시작/업데이트 경로에 연결된 완료 후보는 아니다.

## 원래 SDK와 공급 라이브러리

`uv.lock`의 `comma-deps-raylib` 버전은 `6.0.0.1.post103`이다. 기존 Rust 준비기가
고정한 원래 ARM wheel은21707647 bytes, SHA256
`d9b9bebb39167442bacf627958d57ae95e452550ced1b68337cbf2a73375856a`다.
원래 `py3-none-manylinux_2_28_aarch64` tag와 METADATA/WHEEL/RECORD를 보존한다.

native 준비는 원래 `raylib/install`의 `raylib.h`, `raymath.h`, `rlgl.h`,
`libraylib_comma.a`만 선택한다. 네 파일의 합은2384661 bytes다. wheel에 함께 들어 있는
43075208-byte Gallium, EGL/GLES/DRM 대체 라이브러리, desktop/headless archive와
Python shim은 prefix에 넣지 않는다. 원래 헤더의 라이선스 표기를 보존하며 wheel에
없는 별도 라이선스 파일을 생성하거나 외부 installer를 실행하지 않는다.

기존 release의 raylib shared provider는 고정된 별도 component로 재사용한다.
`rust/native-raylib-provider.json`은 원래 source
`a3d0e3a497b4ae4d23c7cf5ee2275177600bbb31`, 원래 archive SHA
`9c9c16a72bad163226388ef84327b0db1fa64bbe4828e64388e0a7e92d364291`와
manifest SHA `8adbc002388fb97420e4bef9dcf58a5ed75de4c4632a1bc0d10c555f1c2d39ee`를
고정한다. shared library1개, 원래 라이선스·third-party notices·build/provenance36개,
원래 manifest1개, 총38개 member2303785 bytes를 선택한다. 새 provenance/압축 크기는
이 payload 합과 구분하여 준비 보고서에 기록한다. 프로젝트 실행 파일은 포함하지 않는다.

`libopenpilot-raylib.so`의 SHA는
`f62d902f04c9b329d1a8f0741e9ca83dd7c3b92beddaea4a3ec393b1f4bafa44`다.
SDK의 `raylib.h`와 provider가 보존한 원래 header는 모두
`047e7255f93f8c34039cab906ad76136706b5c7b4c5b5b065d84141963ee9b6b`다.
이는 header 출처의 일치이며 기기 GPU 실행 성공을 뜻하지 않는다.

## 준비와 검증 방법

다음은 Actions/개발 검증용이며 기기 설치 명령이 아니다.

```sh
stage-startup-ui-raylib --prepare-native-inputs --root "$SOURCE_ROOT" \
  --cache "$RAYLIB_ASSET_CACHE" --store "$SDK_PREFIX_STORE"
```

원래 legacy staging/compiler CLI도 유지한다. native mode와 `--arch`/`--cxx` 등의
혼용은 다운로드 전에 실패한다. source lock의 해당 wheel URL/SHA/size를 준비 전후
확인한다. raw archive의 같은 byte를 인증·파싱하여 매번 기대 Tree를 독립적으로
도출하고, 내용별 lock/읽기 전용 prefix/작업별 증가량/fsync/원자 발행을 적용한다.
정상 반복은 자산 HTTP body/stored/copy bytes가 모두0이어야 한다. 이는 Actions cache
restore 전송량과 실기 업데이트 시간/디스크 사용량을 측정한 값이 아니다.

producer/consumer CLI는 [FFmpeg provider 절차](native-provider-components-599.md)의
`--plan`을 `rust/native-raylib-provider.json`으로 바꿔 사용한다. 두 profile의 inventory와
release namespace는 독립적이다. FFmpeg의 원래 URL/Tree/identity는 유지하며 raylib
profile을 FFmpeg prefix로 조합하는 요청은 거부한다. producer가 이미 검증한 원래
archive cache를 재사용하므로 두 번째 origin 다운로드는0이어야 한다. consumer는
독립 descriptor SHA와 작은 component만 소비한다. 공개 component release와 Git의
독립 descriptor pin은 아직 없으며 CI의 offline 후보 검사는 이를 대신하지 않는다.

`runtime-source-build.yml`은 기존 legacy admission 회귀, 새 source-lock/CLI/profile
거부 회귀와 Clippy를 실행한다. 실제 고정 wheel을 준비하고 반복 전송/복사0을 확인한다.
그 job이 새로 만든 prefix의 header를 같은 크기로 변조하고 Tree/receipt 둘 다 위조해도
raw에서 도출한 기대 metadata와 달라 실패해야 한다. `finally`에서 원래 세 파일/권한을
복구하고 다시 준비해 동일 Tree와 전송/복사0을 확인한다. 다른 cache는 건드리지 않는다.

`runtime-native-environment.yml`은 같은 고정 wheel/component를 QEMU/PRoot의 실제
AGNOS19.8 ARM user space에서 source-built ARM 준비기로 소비한다. 독립 host Tree와
일치하고 반복 전송/복사0이어야 한다. 현재 `startup-ui/native/raylib_loader.cc`를 원래
헤더로 AArch64 ET_REL object로 컴파일한다. 현재 loader의 required/forwarded symbol과
contract symbol이 실제 shared library에 모두 있는지 확인하고 원래 factory system
library까지 엄격한 ELF dependency/symbol/version audit를 실행한다. 누락은 실패로 남긴다.

검사 결과는 해당 SHA run의 `native-raylib-sdk-*.json`, `native-raylib-provider-*.json`,
Tree, 변조 거부 stdout/stderr, actual ARM commands log, loader object hash와 ELF audit를
함께 읽는다. 기존 360분 제한과 원래 모든 probe 명령/pin을 유지한다. GPU library를
`dlopen`하거나 화면을 초기화하지 않는다. `library_loaded=false`,
`graphics_execution_tested=false`, `physical_device_execution=false`를 명시한다.

로컬에서는 구문·서식·diff/읽기 감사만 수행한다. 전체 wheel 인증/새 준비기·빌드·실행은
Actions 결과 전까지 통과로 기록하지 않는다. 전체 UI/module 회귀와99개 generation의
빌드/seal/default start, 성능 개선 및 사용자 comma3X 자체 빌드·부팅·녹화·업로드·주행
수용은 별도 미완료다.
