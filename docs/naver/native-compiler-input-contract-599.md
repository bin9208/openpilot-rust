# 전체 runtime의 compiler 입력 계약 — #599 (Actions 검증 대기)

상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598),
[환경 #599](https://github.com/bin9208/openpilot-rust/issues/599).
전체99개/13그룹 roster의 provider 누락과 불필요한 group 연결을 검사한다. 모델 준비
Python과 기존 C/C++ 라이브러리를 유지하며 production daemon/수학 정책을 바꾸지 않는다.

## 실제 소스의 의존성과 입력

`rust/native-runtime-catalog.json`의 각 group에 `compiler_inputs`를 기록한다. key는
필요한 환경 변수이고 value는 해당 입력 Tree root 아래의 suffix다. 예를 들어 libyuv의
값은 `lib`, capnp는 `usr/bin/capnp`, 직접 prefix를 가리키는 값은 빈 문자열이다.

| 입력 | 적용 그룹 |
|---|---|
| 고정 Cargo vendor |00..12 모두 |
| capnp compiler/SDK |00..10 |
| FFmpeg와 factory libclang |00,01,03,07,08,09,10 |
| libyuv |00,01 |
| raylib header/static SDK |00,01,02 |
| webcam OpenCV prefix |03 |
| opencv-runtime/Xiaoge OpenCV prefix |04 |
| locationd Eigen |04 |
| paramsd Eigen |06 |

기존 factory 시스템 SDK는 별도 계약으로13개 그룹 모두에 같은 literal `/`와 같은
SHA-pinned snapshot을 요구한다. `LIBCLANG_PATH=/usr/lib/llvm-18/lib`는 이 SDK에 묶는다.
일반 provider는 정확한 prefix Tree를 묶어야 하며 `/`만 지정해 대신할 수 없다.

roster의 직접 feature 목록만으로 판단하지 않는다. 실제 locked/offline Cargo metadata의
local dependency 선언, default feature, rename, optional `dep:` 활성화, strong/weak feature
forwarding을 따라 필요한 입력을 계산해 추적 catalog와 비교한다. dev-only dependency는
runtime build에 넣지 않는다. 현재 소스에는 조건부 local dependency와 local build dependency가
없다. 지원하지 않는 조건부 local edge나 mixed build/runtime feature context를 만나면
명시적으로 실패하며 임의로 모든 provider를 모든 group에 연결하지 않는다.

구체적으로 bootlog→loggerd 때문에08에, manager→bootlog→loggerd 때문에09에도 FFmpeg가
필요하고, carrot-server 때문에10에도 필요하다. UI의 기본 feature는00/02의 raylib를
활성화한다. 이 경로를 빼거나 사용하지 않는
12에 FFmpeg를 추가한 catalog는 실제 metadata 대조에 실패해야 한다.

feature 의미와 metadata field는 Cargo의 [공식 feature 설명](https://doc.rust-lang.org/cargo/reference/features.html)과
[metadata 형식](https://doc.rust-lang.org/cargo/commands/cargo-metadata.html)을 기준으로 한다.
이는 registry 전체 feature resolver를 대체하거나 실제 컴파일을 증명하는 기능은 아니다.

## 전체 generation에서의 필수 검사

환경 값은 absolute path여야 한다. suffix를 제외한 정확한 Tree root가 recipe의
`input_trees`에 있고 그 receipt가 `inputs`에 SHA로 고정되어야 한다. capnp executable이나
libyuv의 lib 하위 경로를 고정할 때도 실제 전체 SDK/prefix Tree를 묶는다. 환경 변수만
설정하거나 다른 root·단일 marker·상위 SDK로 생략할 수 없다.

이 metadata 검사는 전체 generation의 build/seal 및 보존된 generation 검증에 적용한다.
실제 driver는 기존대로 입력 파일/Tree를 빌드 전후 검사한다. 부팅 시에는 보존된 보고서와
입력 결속 metadata를 검사하며 제거된 build cache를 다시 열거나 compiler를 실행하지 않는다.
Tree 결속만으로 provider의 출처·원본 ABI를 인증하지 않는다. 고정된 기존 준비기의 raw
asset 검증과 source/toolchain/provider/결과·ABI 검사는 계속 별도로 필수다.

`openpilot-runtime-plan --check`는 필요한 입력을 `compiler_inputs`로 보고하고 아직 plan이
없으므로 `compiler_inputs_bound=null`이다. `--bindings … --output …`의 결과는 모든
compiler 입력 결속이 있으면 true, 없으면 false와 `compiler_inputs_error`를 보고한다.
작은 marker로 만든 engineering plan의 발행은 유지하되 전체 generation 수용과 구분한다.
false인 계획은 전체 build/seal/boot gate를 통과하지 못한다. true도 실제 빌드/실기 성공은 아니다.

## 검사 방법과 증거

로컬은 구문·서식·diff/읽기만 수행한다. Actions의 foundation은 기존 runtime-build 전체
회귀/Clippy를 유지하고 다음 검사를 포함한다.

- 실제 Cargo metadata에서99개/13그룹의 입력 matrix와 기본/전이 feature를 대조한다.
- rename/default/optional/weak forwarding, dev-only 제외, build context와 조건부 edge의
  잘못된 혼합을 작은 metadata fixture로 검사한다.
- 각 group의 모든 입력에서 환경·Tree·receipt pin을 하나씩 제거하면 거부한다.
  잘못된 suffix/외부 prefix를 `/`로 대신한 결속도 거부한다.
- engineering marker plan은 compiler_inputs_bound=false로 보고한다. 이 fixture는 실제
  factory/provider/전체 runtime 빌드 증거가 아니다.

genuine은 실제 AGNOS ARM user space에서 source-built CLI의 metadata 대조에 더해 고정
Cargo로13개 그룹 각각 `cargo tree --locked --offline --target aarch64-unknown-linux-gnu
--edges normal,build --prefix none --format '{p}|{f}'`를 실행한다. 실제 package 선택과
feature 인자를 유지하고 Cargo가 출력한 feature/provider 목록이 catalog와 일치하는지
독립적으로 대조한다. 명령·원문 tree/stderr/hash와 result.json을 보존한다. 공식
[cargo tree 설명](https://doc.rust-lang.org/cargo/commands/cargo-tree.html)도 완전한 build
동등성을 보장하지 않으므로 결과에 compiled=false/full_runtime_build=false를 명시한다.

run 완료 후 foundation의 runtime-build test log/native-runtime-catalog.json, genuine의
native-project-catalog.json/native-compiler-features/ 및 command log를 확인한다. 기존
명령/pin/360분 제한과 bin-actions/X64·별도 ARM QEMU 실행을 유지한다. 실제99개 build/
seal/default update/start/recovery, 공개 자산/pin, 모델·기록·기존 업로드·성능과 사용자
comma3X 수용까지 #598은 계속 미완료다.
