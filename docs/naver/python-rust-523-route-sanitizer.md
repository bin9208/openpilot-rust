# #523 route sanitizer 원본 비교·전용 CI 준비

작성: 2026-10-24. 상위 작업: [#523](https://github.com/bin9208/openpilot-rust/issues/523),
[#296](https://github.com/bin9208/openpilot-rust/issues/296).
조사 기준 HEAD: `97f7ad36dfa0919b4e5c216fb16f44ed29161663`.
작업 트리: `/home/fma1/.codex/worktrees/python-rust-523/openpilot-rust`.

## 범위와 상태

기존 `rust/crates/dev-tools/src/route_sanitizer.rs`를 재포팅하지 않고,
`tests/route_sanitizer_original.rs`에 독립 원본 비교 5개를 추가했다.
원본 `openpilot/tools/lib/sanitizer.py`, native 구현, 기존 unit fixture와 Cargo는 수정하지 않았다.
후속 작업에서는 `.github/workflows/python-rust-523-sanitizer.yml`만 새로 작성했다.
기능 실행·원본 캡처·빌드·Clippy는 **UNRUN**이다. 로컬 검사는 문법·fmt·diff만 수행하며,
아래 등록과 실제 실행은 부모의 다음 `bin-actions` batch에 속한다.

현재 HEAD의 공개 모듈에는 이미 `route-sanitizer-native` feature 조건이 있다.
처음 전달된 no-feature 설명 대신 실제 소스의 조건에 맞춰 테스트도 같은 feature를 요구한다.

## 독립 원본 경로와 관측값

테스트는 준비된 Python을 `-I -B`로 실행하고 새 private `sys.pycache_prefix`를 지정해
기존 checkout의 stale bytecode도 배제한다. 환경은 `/usr/bin/env -i`로 비우고,
HOME·TMPDIR·XDG_CACHE_HOME과 cwd를 테스트 소유 private temp로 지정한다.
입력·출력은 JSON stdin/stdout이며, route·기기·사용자 데이터·네트워크를 사용하지 않는다.
설치나 dependency 준비 명령도 테스트 안에서 실행하지 않는다.

`openpilot.cereal`의 실제 `__init__.py`가 원본 full `log.capnp`, `custom.capnp`,
`deprecated.capnp`와 저장소 `opendbc_repo/opendbc/car/car.capnp`를 로드한다.
축약 `rlog.capnp`, frozen oracle, archived code를 실행하지 않는다.
stub은 sanitizer가 import하는 `openpilot.tools.lib.logreader`의 타입 주석
`LogIterable`·`LogMessage` 두 이름뿐이다. `sanitize_vin`, `sanitize_msg`, `sanitize`는
실제 원본 모듈에서 import하여 그대로 호출한다. VIN 수식을 Python harness에 복사하지 않았다.

`ROUTE_SANITIZER_ORIGINAL_JSON` 로그에는 입력 wire hex, 원본 출력 wire hex,
직접 읽은 필드·float32 bits·identity·pull trace, Python/pycapnp 버전,
실제 interpreter·pycapnp Python/extension 파일 SHA-256을 **마지막 시점에 한 번** 남긴다.
테스트 자체의 이 metadata는 provider의 before/after 불변성을 검증하지 않는다.
원본·schema·native 구현·기존 unit·새 테스트의 source SHA-256을 실행 전후 비교하고,
Rust 쪽 독립 파일 읽기 결과와도 일치시킨다.
현재 원본 관측값을 아직 생성하지 않았으므로 성공 캡처나 원본 삭제 근거로 사용할 수 없다.

## 테스트 5개

| selector | 입력과 잡아내는 차이 |
| --- | --- |
| `original_vin_utf8_codepoint_boundaries` | 빈 값, 1/5/6/7글자, ASCII, 다중 byte, combining mark, ZWJ, embedded NUL, 최대 Unicode scalar, 개행 등 14개. 순수 함수와 실제 carParams wire 양쪽에서 UTF-8 byte 절단·grapheme 절단 오류를 검출한다. |
| `original_car_params_copy_preserves_full_wire_and_float_bits` | +0/−0, 최소·최대 subnormal, 최대 finite, ±infinity, payload가 있는 quiet NaN 8개. header, 문자열, flags, tuning list, firmware Data도 함께 확인한다. |
| `original_non_car_messages_preserve_identity_and_fields` | CAN·pandaStates·pandaStateDEPRECATED·직접 sanitize_msg에 넣은 logMessage 4개. 원본 Python 객체 identity, native Borrowed variant와 CAN Data 주소, 필드·전체 wire를 확인한다. |
| `original_service_filter_order_and_laziness` | 필터 대상과 보존 대상이 섞인 9개 중 6개 출력. 생성 시 iter 호출, next 전 무소비, 출력별 pull 횟수, 순서·중복·EOF를 비교한다. |
| `original_malformed_boundaries_are_lazy_and_leave_inputs_unchanged` | 잘못된 VIN UTF-8, 알 수 없는 union tag, list 대신 struct인 CAN payload, 필터되는 잘못된 logMessage payload, 잘린 framing 5개. decode 실패와 lazy next 실패를 구분하며 오류 후 다음 입력 처리·원본 불변을 확인한다. |

정상 출력은 타입을 모르는 Cap'n Proto `Reader::canonicalize()`로 **전체 wire graph**를 비교한다.
typed `to_dict()` 재구성, 숫자 정규화, field mask, 허용오차를 사용하지 않는다.
CarParams는 출력 사본의 VIN만 원래 값으로 복원한 뒤 입력 전체 canonical wire와도 비교한다.
직접 읽은 sentinel 값과 float bits를 별도로 검사하므로 wire 비교만 통과시키는 필드 손실을 허용하지 않는다.

malformed 비교는 오류 여부·발생 시점·소비 횟수·불변성에 한정한다.
Python 예외 class/message는 JSON에 보존하지만 Rust 오류 문자열과 같다고 주장하지 않는다.

## unknown field와 identity 증거의 한계

새 full-schema 원본 비교가 미래 schema의 unknown field 보존을 검증했다고 주장하지 않는다.
공통 canonicalization의 일치만으로 unknown field 손실을 판정하지 않으며, 해당 범위는 기존
`route_sanitizer_tests.rs::unknown_event_and_car_params_data_and_pointer_fields_survive`의
raw struct 크기·미지 data word·미지 pointer text sentinel 독립 assertion에 별도로 남긴다.
기존 `non_car_params_borrowed_and_owned_arenas_pass_through_unchanged`의 owned arena 주소 검사,
borrowed 경로 검사와 나머지 unit도 변경하지 않았다.
새 테스트의 unknown union tag 경계는 unknown field 보존 검증과 구분한다.
기존 unit도 이번 작업에서 실행하지 않았다. CI 결과 전에는 해당 보존을 실증했다고 주장하지 않는다.

## 부모 CI 통합 계약

Cargo 등록 제안이며 이번 작업에서는 실제 Cargo 파일을 수정하지 않았다.

```toml
[[test]]
name = "route_sanitizer_original"
required-features = ["route-sanitizer-native"]
```

기존 준비 venv에 프로젝트 pin `pycapnp==2.1.0`이 필요하다. NumPy·requests·LogReader의
런타임 dependency는 필요 없다. `ROUTE_SANITIZER_ORACLE_PYTHON`을 해당 venv Python의
**절대 경로**로 전달해야 하며, 누락 시 skip/fallback 없이 실패한다.
Python package 설치·Cargo/schema compiler 준비·growth admission은 부모가 기존 절차로 처리한다.

```bash
ROUTE_SANITIZER_ORACLE_PYTHON="$CI_JOB_TEMP/venv/bin/python" \
  cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-dev-tools \
  --features route-sanitizer-native --test route_sanitizer_original -- --nocapture
```

실제 CI에서 `5 passed; 0 failed; 0 ignored`를 확인하고 stdout의 5개 group JSON을 보존해야 한다.
feature/플랫폼 누락으로 발생한 `0 tests`는 통과 증거가 아니다. Linux target에서 실행한다.
기존 unit의 unknown/identity 증거가 필요하면 아래 두 selector를 별도로 실행할 수 있다.

```text
--lib route_sanitizer::tests::unknown_event_and_car_params_data_and_pointer_fields_survive -- --exact
--lib route_sanitizer::tests::non_car_params_borrowed_and_owned_arenas_pass_through_unchanged -- --exact
```

실제 caller 교체·caller closure·원본 소스 삭제·차량 검증은 이번 작업 범위 밖이며 완료를 주장하지 않는다.

## 로컬 확인

- `rustfmt +1.94.0 --check --edition 2021 rust/crates/dev-tools/tests/route_sanitizer_original.rs`: 통과.
- 내장 Python 문자열의 `ast.parse`: 통과. 프로젝트·pycapnp import나 원본 함수 호출은 수행하지 않았다.
- 새 두 파일의 `git diff --no-index --check /dev/null FILE`: whitespace 진단 없음.
  새 파일 차이가 있으므로 no-index 명령의 종료값 1은 diff 존재를 뜻한다.
- 원본 비교·기능 테스트·빌드·Clippy·CI: **UNRUN**. Cargo 등록과 실제 5-test 결과 확인은 부모가 수행한다.

## 전용 sanitizer workflow 후속

`.github/workflows/python-rust-523-sanitizer.yml`은 해당 workflow 파일이
`codex/feat-523-python-rust`에 push될 때 실행한다. 커밋·push·dispatch는 이번 작업에서 하지 않았다.
repository는 `bin9208/openpilot-rust`로 제한하고, 첫 step에서 실제 `RUNNER_NAME=bin-actions`,
self-hosted·Linux·X64·x86_64와 trusted push/ref를 확인한다.
원본 5개·기존 library 15개·strict Clippy를 모두 새로 실행하며 과거 성공 cohort를 재사용하지 않는다.

기존 bundle/focus의 pinned `job-temp` action, private checkout, Bash workspace 분리,
source/index seal, 실패 후 artifact upload, 업로드·seal 성공 후 소유 경로만 정리하는 패턴을 재사용했다.
HOME·Cargo registry·target·Rustup home·cache·venv는 job 소유 경로에 둔다.
원본 source와 공유 SDK, 다른 job의 경로에는 정리 명령을 적용하지 않는다.

### 실행 selector와 실패 독립성

- 원본: `--test route_sanitizer_original -- --nocapture`, 정확히 `5 passed; 0 failed; 0 ignored`와 5개 JSON group을 요구한다.
  JSON에서 14 VIN·8 float·4 identity·9→6 lazy stream·5 malformed 관측 수를 다시 확인하고 group별 파일로 보존한다.
- library: `--lib route_sanitizer::tests:: -- --nocapture`, 정확히 `15 passed; 0 failed; 0 ignored`를 요구한다.
- Clippy: `--lib --test route_sanitizer_original -- -D warnings`.
  공통 옵션은 `--locked -p openpilot-dev-tools --no-default-features --features route-sanitizer-native`다.

세 step은 `!cancelled()`와 공통 SDK 준비 성공만을 조건으로 둔다. 원본 step은 추가로 venv 준비를 요구한다.
원본 비교 실패 후에도 library와 Clippy를 실행하고, library 실패 뒤에도 Clippy를 실행한다.
`continue-on-error`로 실패를 성공 처리하지 않는다. venv 설치 실패 시에도 native library·Clippy는 시도한다.
after source/index·Rust SDK·venv/interpreter seal은 서로 다른 `always()` step으로 실행하고,
그중 하나가 실패해도 outcomes 기록과 artifact upload를 시도한다.

### 공유 Rust와 provider의 실제 before/after 범위

공유 SDK는 `/home/actions/.rustup/toolchains/1.94.0-x86_64-unknown-linux-gnu` 하나만 읽는다.
기존 Fast의 실제 rustc/cargo/rustfmt/clippy-driver SHA pin을 사용하고 `rustc -vV`의 release/host를 확인한다.
Cargo는 SDK의 절대 경로로 호출하고 `RUSTC`·`RUSTDOC`도 해당 SDK의 절대 경로다.
Rustup proxy/install/component 변경이나 SDK 복사를 하지 않는다.
SDK의 전체 regular file SHA-256과 symlink target·directory·device/inode·owner/mode·size·mtime/ctime roster를
실행 전후 각각 채취해 비교한다. atime은 읽기 때문에 달라질 수 있으므로 seal에 포함하지 않는다.

`actions/setup-python`은 `3.12.15`로 고정하고 private venv에 `pycapnp==2.1.0`만 추가 설치한다.
NumPy나 requests를 설치하지 않는다. pip download cache를 남기지 않고 binary wheel만 허용한다.
설치 후 원본 실행 전에 venv **전체** regular file SHA-256과 symlink/directory를 포함한 위와 같은 roster를 만든다.
`venv/bin/python`의 symlink-resolved 실제 interpreter 경로·파일 SHA-256·device/inode·owner/mode·size·mtime/ctime도
별도로 채취하고, 세 check 시도 후 같은 항목과 정확히 비교한다.
pycapnp extension/Python 파일·pip metadata·venv entry roster는 이 전체 seal 안에 포함된다.
after 수집은 해당 Python을 실행하지 않고 `find`·`sha256sum`·`readlink`·`stat`로 수행한다.
before baseline 누락이나 불일치는 seal 실패이며 provider/SDK qualification을 주지 않는다.

따라서 provider 불변성의 근거는 테스트 마지막 hash metadata가 아니라 **해당 workflow가 실제로 성공한 before/after seal**이다.
현재 workflow는 UNRUN이므로 그 불변성이 실증되었다고 아직 주장하지 않는다.
interpreter 밖의 전체 base stdlib·OS loader/library를 봉인했다고도 주장하지 않는다.

### 60 GB volume과 growth 검토

조사 기준 HEAD의 `git ls-tree -r -l HEAD`에서 추적 blob 합계는 **740,489,522 bytes / 19,344개**였다.
기존 97f7 실행의 보존 artifact에서 source admission과 preparation 사이의 가용 공간 차이는
bundle **1,197,252,608 bytes**, focus **1,196,265,472 bytes**였다.
이는 두 시점 사이 전체 filesystem free 변화이며 checkout 단독 사용량의 정확한 측정이라고 보지 않는다.
shallow `.git`·working tree·inode/block 여유를 포함하는 기존 **2 GiB source admission을 유지**한다.
이번 workflow는 checkout 후 private source의 실제 `du -sx` 할당량과 추적 blob 합계를 별도 기록하고,
source가 그 2 GiB 추정을 넘으면 추가 준비 전에 실패한다.

checkout과 별도로 private Cargo registry/target, venv와 proof에 **2 GiB preparation growth**를 admission한다.
관련 두 경로의 실제 filesystem available을 각각 검사한다.
Rust SDK를 복제·설치하지 않고 기존 capnp/CC/C++/pkg-config를 요구해 apt 증가를 만들지 않는다.
debug info·incremental을 끄고 build jobs는 2로 제한한다. 이 preparation 값은 다음 실제 run에서 확인할 추정이며
현재 sanitizer build peak를 측정한 값이 아니다. 종료 시 disk·private output 할당량·memory/cgroup을 보존한다.
volume 확장·공유 cache 삭제·다른 job source 회수나 admission 완화를 전제로 하지 않는다.

### 증거와 정리

`diagnostic-sanitizer-RUN_ID-RUN_ATTEMPT` artifact에 source/index 전후, SDK/venv/interpreter 전후,
세 check의 개별 로그, 원본 group JSON, runner·tool version/hash·capacity·outcomes를 보존한다.
원본 step이 실패하면 앞서 출력된 원본 JSON은 개별 log 안에 남는다.
cleanup은 artifact-id/digest 유효성과 upload 성공, source/index·SDK·provider seal 성공을 모두 요구한다.
private source root의 owner token·device/inode·UID·canonical path·nonsymlink를 다시 확인한 뒤 해당 root만 지운다.
그 성공 뒤에만 같은 job의 `job-temp`를 정리한다. 공유 SDK·외부 Python interpreter에는 쓰기/삭제를 하지 않는다.
artifact outcomes에는 `original_retired=false`, `caller_closure=false`, `inventory_credit=false`를 명시한다.

workflow YAML, 각 run block의 Bash 문법, 내장 Python 문법만 로컬에서 확인한다.
실제 원본 5개·library 15개·strict Clippy·provider 전후 불변성은 모두 **UNRUN**이다.


## 0e28 실제 실패와 다음 수정

[40087242329/job120852565522](https://github.com/bin9208/openpilot-rust/actions/runs/40087242329/job/120852565522)은
실패다. 세 check는 독립 실행됐지만 original5·library15 모두 잘못된 generated API와
raw schema wrapper 때문에 컴파일 전에 멈췄고, Clippy도 동일한 오류다. 두 테스트 파일에서
`init/get_kp_b_p` 및 `capnp::schema::StructSchema::new(raw).get_proto()`로 수정한다.
fixture bits·모든 assert·production sanitizer는 변경하지 않는다. 기능 결과는 여전히 **UNRUN**이다.

artifact12557518748/493,413B/SHA256
`ab503357cdb0ad47040263bf79a82ead6464a7f9192ea6457b1428fbe21feb04`의45 CRC/source/index 및
SDK/venv 내용·identity 전후 일치를 확인했다. 부모3.12.15 확인만으로 env-i 자식의 동일 runtime을
보증하지 않고, #531 수정처럼 명시된 단일 library directory를 전달해 실제 자식3.12.15 및
provider mapping·전후 hash를 확인한다. 원본 source 삭제와 inventory credit은 아직 없다.


## 2026-10-24 a134 원본 수용과 소스 삭제 후보

`a134b32c5d50d346994d671c0882b147175021dc`의
[Actions 40089943421/job120860396114](https://github.com/bin9208/openpilot-rust/actions/runs/40089943421/job/120860396114)에서
실제 CPython3.12.15/pycapnp2.1.0 원본5, native15 및 strict Clippy가 통과했다.
source/index, 전체 Rust SDK 및 private venv/interpreter의 전후 내용·identity seal도 일치했다.
artifact12558278976, 517926B, SHA256
`1595e9bb5886c925c1ab6016f936fa1d3df670374e9921d6113d56f162e7c9e5`의
52개 member CRC와 seal을 부모가 별도로 확인했다.

5개 원본 JSON을 재직렬화하지 않고 `tests/fixtures/route-sanitizer-original/`에 보존한다.
모든 파일의 전체 SHA256을 native checker에 고정한다. 현재 native 구현과 unit 및 full IDL8개는
실제 capture의 source SHA와 일치해야 하며, 원본 Python/provider 경로는 역사 metadata로만 읽는다.
기존5개 비교 본문을 유지하고 삭제 source의 파일·디렉터리·broken symlink 재출현을 거부하는
음성 통제1개를 추가했다. 비교와 전후 seal을 모두 통과한 뒤 별도 frozen-check 접두사를 출력한다.

`openpilot/tools/lib/sanitizer.py` 하나를 삭제한 후보이며 inventory는 아직 pending이다.
tracked source에서 이 모듈의 실제 import/caller가 없는지 재검색하고 있다. 다음 CI는 Python 준비·원본 실행 단계를 제거하고 source 부재·frozen5+음성통제1·strict Clippy·SDK/source 전후 seal을 확인한다.
이미 성공한 unchanged native15는 위 artifact와 source pin으로 재사용한다.
삭제 후 CI는 **UNRUN**이다. #523 전체 또는 사용자 기기 수용 완료를 의미하지 않는다.


삭제 후 gate의 raw strace는 frozen-test Cargo 자식 트리만 기록한다. 미해석 경로를 누락하는
부분 정규식에 의존한 Python0 receipt를 만들지 않으며 Clippy까지 포함한 전체 실행체의
non-Python 수용을 주장하지 않는다. 수용 대상은 원본 모듈 부재, 원문 JSON에 대한 native API
비교 및 source/fixture 전후 seal이다. execveat는 명시적 실패 분기로 거부한다.
