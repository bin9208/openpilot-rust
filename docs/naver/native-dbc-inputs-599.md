# 현재 소스에 결속한 원래 DBC 데이터 — #599/#598

이 변경은 RadarCAN의 수식·파서·레이더 선택을 바꾸지 않는다. 원래 생성 데이터만
별도 내용별 component로 준비한다. 전체 native Layout/빌드·봉인·실기 수용은 남아 있다.
새 구현의 기능 검사는 Actions 결과를 확인해야 한다.

## 원본과 재사용 조건

원본 runtime source는 `a3d0e3a497b4ae4d23c7cf5ee2275177600bbb31`이며,
원래 archive SHA256 `9c9c16a72bad163226388ef84327b0db1fa64bbe4828e64388e0a7e92d364291`,
manifest SHA256 `8adbc002388fb97420e4bef9dcf58a5ed75de4c4632a1bc0d10c555f1c2d39ee`를
유지한다. 기존 전체 runtime의 Rust 실행 파일은 이 component에 포함하지 않는다.

`rust/native-dbc-source-contract.json`은 DBC와 원래 generator/template 140개,
package 초기화 파일, 기존 준비기 및 MIT license의 Git 입력143개를 결속한다.
선택 Git entry와 NUL 구분자를 포함한 SHA256은
`60d1c67b28395a25ec103a2e3481317fd136a78f2ccb0f7dce821055c7456d05`다.
원본과 현재 소스의 입력이 같음을 읽기 감사로 확인했다. 문서나 독립 Rust 수정은 재사용하고,
DBC/generator/license/준비기 수정·추가는 새 검증 데이터를 준비하기 전 거부한다.

원래 103개 DBC와 생성 기록104개는 5,036,510 bytes다. 각각 원래 manifest의 hash와
일치하며, 생성 기록의 `source_sha256` 140행도 현재 Git blob과 모두 같다.
생성 기록의 여유 공간/성장량은 원래 제작 당시의 기록으로 보존하며 현재 측정값으로
바꾸지 않는다. MIT 원문은 별도 source contract에도 포함한다.

## 준비와 검증 방법

Actions에서 빌드한 `build-runtime-ffmpeg`의 기존 provider 명령을 재사용한다.

```sh
build-runtime-ffmpeg --export-released-provider --source-root CLEAN_SOURCE \
  --plan rust/native-dbc-provider.json --cache ORIGIN_CACHE \
  --store ORIGIN_PREFIX_STORE --output NEW_EXPORT
build-runtime-ffmpeg --prepare-released-provider --source-root CLEAN_SOURCE \
  --plan rust/native-dbc-provider.json --descriptor EXPORT/provider.json \
  --descriptor-sha256 INDEPENDENT_DESCRIPTOR_SHA --cache COMPONENT_CACHE \
  --store PREFIX_STORE --offline-archive EXPORT/radarcan-dbc-runtime.tar.gz
openpilot-runtime-inputs --verify-dbcs --root CLEAN_SOURCE --store PREPARED_PREFIX
```

공개 component 발행 전 CI는 인증한 원본 archive에서 한 번 추출한 독립 component를
offline seed로 소비한다. 공개 HTTPS 전송이나 새 기기 초기 준비를 통과한 결과가 아니다.
소비자는 전체 원본 archive를 요구하지 않으며 반복 전송·archive 저장·prefix 복사0을
검사한다. source root 없는 요청은 준비 전에 거부한다. 고정25/35GiB 대신 이 component의
payload·metadata 및 별도 oracle/consumer 빌드의 예상 증가량으로 공간을 검사한다.

foundation의 `dbc_inputs` 여섯 회귀와 provider 회귀는 실제 소유 Git commit으로
문서/Rust 변경 재사용, generation 입력 변경/추가/dirty 거부, 출력 누락·변조·탈출·추가·link
거부와 core CLI를 검사한다. 가짜 출력은 제어 흐름 회귀용이며 실제 DBC 검증이 아니다.

genuine Actions는 host와 QEMU/PRoot ARM에서 실제 component의 tree와 현재 source binding을
대조한다. 별도 CI oracle은 **Git 추적 파일만** 소유 임시 package에 복사하고 원래 Python
generator로103개를 재생성해 byte/hash 전체를 대조한다. 사용자 source/ignored generated
DBC/bytecode는 복사하거나 수정하지 않는다. warm/runtime 준비는 generator를 실행하지 않는다.
ARM에서는 실제 RadarCAN `Databases` consumer 예제를 같은103개에 실행한다.

전체 runtime 경로는 비싼 build 전 Layout, 봉인 전 매핑, 실제 봉인 출력과 시작 시 retained
source/실제 데이터 전체를 다시 검사한다. 시작 검증은 compiler/provider cache를 요구하지 않는다.
새 구현은 아직 Actions 대기이며 ARM user space 결과는 물리 ARM CPU·AGNOS kernel·차량
검증이 아니다. 기존 데이터와 동작을 그대로 쓰므로 NAS 레이더 서비스 코드는 변경하지 않는다.
