# #523 F02 maintained offline exporter의 native bookkeeping 연결

작성: 2026-10-26. 읽기 기준 HEAD: `73e17ff10ec9be7a7001ab9cbf415a779c436f78`.
상태: **실제 caller 연결 patch 구현, native 절대 원본 동등성·Rust build/test/Clippy·GHA UNRUN**.
전체 tinygrad/frontend/compiler 전환 완료가 아니며 Python source 삭제 credit는0이다.

## 실제 소비자와 선택한 최소 경계

현재 checkout에는 `model_export/launch.py`나 `native_writer.py`가 없다. 실제 launch 선택은
`offline_schedule.launch_dimensions`, offline writer는 같은 파일의 `emit_schedule`이다.
기존 `offline-qcom-compile --library ...`는 source stdin→genuine A630 binary stdout 경계다.
CPU writer `export::emit_cpu`는 별도 native CPU backend에서 소비되며 이번에 변경하지 않았다.

실제 경로는 다음과 같다.

```text
build_offline_model_pipelines.py → offline_publish → export_pipeline/export_model
  → emit_schedule: 원래 Tensor/UOp memory planning/to_program/genuine kernel compile
  → 같은 compiler.command.argv + --export-plan
  → 기존 offline-qcom-compile → compiler_schedule::export::prepare
     ├ compiler_buffers::descriptor / OfflineBufferTable
     │  └ 기존 export::BufferTable의 alias/read/write/initialized-range/raw packing
     ├ compiler_launch::launch_dimensions
     └ compiler_schedule::pipeline_entrypoints
  → native weights/views/launch/entrypoints → 기존 weights.bin/graph.json writer
```

단순 메타데이터 확인용 새 독립 CLI는 만들지 않았다. 기존 실행 파일의 새 모드를
maintained `emit_schedule`가 **무조건 호출**한다. native packed bytes가 실제 weights 파일이
되고 native allocations/views/launch/entrypoints가 실제 manifest와 반환값에 들어간다.
native 실행 실패·protocol 오류·원본 불일치는 destination 생성 전에 실패한다.
legacy Python 성공으로 되돌아가는 fallback이나 선택적 성공 경로는 없다.

인계 중에는 genuine Python table/launch/entrypoint 계산을 독립 비교로 남긴다. native로
예상 결과를 전송하지 않고, 실제 descriptor·initializer·view/read/write event·program
source/binary·원래 launch 인자·post-planning prefix/stage end만 보낸다. native event view
identity와 전체 allocations/views/weights/launch/entrypoints가 Python 원본과 같아야 쓴다.
숫자 비교는 canonical JSON으로 int/float/bool 차이도 보존한다. **이중 계산 비용이 남는다.**

Tensor/UOp identity 취득, original memory planner, constant pruning의 retained membership과
stage end 계산, rendering/to_program, compiler invocation, kernel deduplication, 파일 writer와
offline publication은 아직 Python에 남는다. 이번 schedule 연결은 그 뒤 실제 emitted prefix를
entrypoint로 투영하는 단계다. wire에서 post-preparation 순서 identity를 부여하며, raw UOp
constant-retention 전체를 Rust로 옮겼다고 주장하지 않는다. live/captured CPU export도 미전환이다.

## 변경 파일과 protocol

- `src/compiler_schedule.rs`: 기존 등록된 모듈에 feature-gated `export` leaf만 연결.
- 새 `src/compiler_schedule/export.rs`: strict typed JSON v1 입력과 native plan 처리.
- `src/bin/offline-qcom-compile.rs`: 기존 source compiler 모드는 유지하고, 뒤에 붙는
  `--export-plan` 모드 추가. JSON 모드는 provider/GPU를 열거나 커널을 실행하지 않는다.
- `rust/tools/model_export/offline_schedule.py`: 기존 emit 과정에서 실제 event를 기록하고
  같은 command로 native plan을 받아 독립 비교 후 native 결과를 기존 writer에 전달.
- 새 `tests/compiler_export_caller.rs`: 일반4개와 genuine Python caller ignored1개.
- 이 문서. Cargo/lib.rs/WF/Focus/원장/commit/push/GHA는 수정하지 않았다.

writer 요청은 `version`, `initialized`, `events`, `programs`, `launches`,
`emitted_by_source`, `stage_ends`만 받는다. descriptor는 정확한 `id(buffer.base)`와
allocation bytes/offset/length/device/allocated metadata를 담는다. initializer key는
`id(mapping_key)`여서 **alias key를 base key로 잘못 바꾸지 않는다**. 동일 key의 마지막 값,
lazy missing/wronglen, read-before-write, 서로 다른 동일 내용 base, QCOM/host·QCOM:0의
기존 descriptor 경계, raw all-zero/negative-zero/무 padding packing은 기존 core를 재사용한다.

dimension은 integer(decimal string)/float/bool을 명시한다. 유효 integer 범위는 기존 Rust
launch API의 i128이다. nonfinite float는 JSON 경계에서 거부한다. JSON stdin/stdout은 각각
256MiB, events는1,048,576개, 나머지 roster는65,536개, distinct storage는1GiB,
packed weights는128MiB로 제한한다. 이는 새 bounded handoff의 제한이며, 이보다 큰 모델의
지원/성능을 주장하지 않는다. hex와 독립 Python 비교로 인한 추가 메모리/시간도 미실측이다.
128MiB weights의 hex만으로256MiB가 되어 JSON overhead까지 wire에 들어간다고 보장할 수
없다. 실제 모델 fit는 미검증이며 이번 회귀 추가에서 cap을 늘리지 않았다.
기존 `CompilerCommand.timeout_seconds`를 그대로 사용하며 CI 기한을 늘리지 않는다.

## 기존 command 계약과 모든 확인한 소비자

새 환경변수나 binary/Cargo 등록은 필요 없다. 기존 stage는
`dev-tools/src/qcom_compiler_stage.rs`에서 새 native ELF를 복사하고 full SHA를 기록한다.
`command.json`은 다음 순서이며 뒤 인자를 그대로 frontend에 전달한다.

```text
qemu-aarch64 -cpu cortex-a57 LOADER --library-path LIBRARIES
  STAGED/offline-qcom-compile --library STAGED/libllvm-qcom.so
```

`offline_schedule`가 이 argv 뒤에 `--export-plan`만 붙인다. 기존 `.compile(source)`의
argv/source→binary ABI와 pinned QCOM provider는 바꾸지 않았다. 새 모드가 필요한 모든
확인된 소비자도 같은 기존 command를 사용한다.

| 소비자 | 연결/필요 조치 |
| --- | --- |
| `build_offline_model_pipelines.py --compiler-command` | 기존 JSON array와 timeout 유지. emit에서 새 모드 필수 |
| `test_offline_qcom_schedule.py`의 두 export_model 소비자 | 기존 `QCOM_COMPILER_COMMAND` 유지. rebuilt native ELF를 stage한 뒤 기존 검사 실행 |
| `test_offline_qcom_compiler.py`의 genuine source compile 두 검사 | 기존 raw mode 유지. 새 모드 인자를 직접 전달하지 않음 |
| `.github/workflows/rust.yml` workspace의 pinned ARM compiler stage | 이미 같은 기존 binary를 build/stage한다. 부모가 새 source로 rebuild/stage하고 Focus/pin 갱신 |

외부에서 임의 custom compiler command를 사용한다면 raw compile ABI 외에 이 JSON 모드도
지원해야 한다. 이전 ELF나 source-only wrapper는 export에서 실패한다. 자동으로 Python을
다시 실행하거나 성공 artifact로 위장하지 않는다.

## 부모의 정확 GHA 검증

로컬에서 아래 명령을 실행하지 않았다. 부모가 기존 자원 admission/SDK/source 봉인 아래
필요한 selector를 등록한다. 일반 native/CLI 검사는 작은 단위 입력이며, 실제 모델/컴파일
수치 검사는 기존 hosted/ARM-QEMU 경로를 유지한다.

```bash
cargo test --manifest-path rust/Cargo.toml --locked --no-default-features \
  --features native-skip-miri -p openpilot-model-runtime --test compiler_export_caller
cargo test --manifest-path rust/Cargo.toml --locked --no-default-features \
  --features native-skip-miri -p openpilot-model-runtime --test compiler_export_caller \
  maintained_python_export_calls_native_plan_without_fallback -- --ignored --exact --nocapture
cargo clippy --manifest-path rust/Cargo.toml --locked --no-default-features \
  --features native-skip-miri -p openpilot-model-runtime --lib \
  --bin offline-qcom-compile --test compiler_export_caller -- -D warnings
```

ignored 검사는 실제 Tinygrad의 unallocated QCOM descriptors/COPY UOp와 실제
`emit_schedule`→기존 CLI→새 Rust leaf→graph/weights writer를 통과한다. provider/GPU/model
실행은 없으며 GPU open을 거부한다. native argv 거부 시 다른 오류가 아닌 native 실패임을
확인하고 destination이 생기지 않아야 한다. 일반 검사는 세 native core 조합, invalid
initializer/alias/device/stage, lazy write-covered storage, 실제 CLI의 strict version/empty
error stdout을 검사한다. 합성 launch 입력은 genuine kernel compile 수용 증거가 아니다.
일반4번째 검사는 아래 explicit-local f64 CLI 회귀이며, 기존 ignored Python 호출은1개로
유지한다. 위 일반 selector가 새 회귀도 함께 실행한다.

기존 pinned ARM compiler command를 준비한 workspace에서 다음 실제 소비자 검사도 필요하다.

```bash
python -m pytest -q rust/tools/tests/test_offline_qcom_compiler.py \
  rust/tools/tests/test_offline_qcom_schedule.py
```

원래 compiler_launch/compiler_schedule/compiler_buffers의 ordinary/ignored reference gate를
유지한다. 사용자가 전달한 buffer179 비교의 `33923` CPU copyin readonly 실패와 부모의
bytearray2행 수정은 기존73batch 검증 중인 별도 증거다. native6/Clippy의 이전 성공은 이번
caller patch 성공으로 전용하지 않는다. 이 작업은 buffer 테스트를 수정하지 않았다.

## 2026-10-26 peer review의 f64 wire 회귀 추가

Hypatia가 지적한 `serde_json` 기본 float parser의 bit 변화는 strict Python JSON 비교에서
유효한 explicit-local 입력도 거부할 수 있다. 고정 회귀 값은 `51.248178375505404`다.
원본 `launch_dimensions`는 local이 명시되면 global/local의 값과 타입을 그대로 반환한다.
부모는 `model-runtime/Cargo.toml`의 `serde_json`에 `float_roundtrip` feature를 연결한다.
이 후속 작업은 테스트와 이 문서만 수정했고 Cargo/pins/WF는 건드리지 않았다.

새 일반 검사 `existing_cli_preserves_explicit_launch_float_bits_and_dimension_types`는
후속 지시에 따라 위 decimal을 explicit float local에 넣고 global은
`102.49635675101081`인 JSON 원문으로 고정한다. 실제 기존
`offline-qcom-compile --library /unused-export-plan-provider --export-plan`의 stdin/stdout을
통과한 응답을 해독하고, local의 f64 `to_bits()`를 독립 Rust 상수와 직접 비교한다.
해독 전 CLI 응답 원문에도 고정 decimal이 남는지 확인해 두 parser의 오차 상쇄로 잘못
통과하지 않게 한다.
global과 두 번째 local의 `1.0`도 float 타입과 bits를 확인하고 integer의 decimal string 및 boolean을 포함한
전체 global/local 응답을 대조한다. 같은 parser를 거친 `plan()` 결과만 기대값으로 삼지 않는다.
provider/GPU/compiler/model은 열거나 실행하지 않으며 새 Python 호출은 추가하지 않았다.

소스에서 확인한 계약 차이도 남긴다. 현재 `compiler_launch.rs:19`와 원본
`offline_schedule.py:90`은 explicit local이 있으면 나눗셈 전에 global/local을 그대로
반환한다. 원본 `//`는 local이 없는 auto-local 경로의114행에만 있다. 따라서 이 CLI
입력에서 현재 global 응답은 float `102.49635675101081`이며 `2.0`이 아니다.
테스트는 실제 원본 분기에 따라 global/local과 float/integer/boolean 타입 및 local의
정확한 f64 bits를 보존하는지 확인한다. launch core나 Python 계산은 변경하지 않는다.

일반 native/CLI 검사는3→4개, ignored genuine caller는1개, 합계5개다. 로컬 확인은
`rustfmt --edition 2021`/check, 소스·diff·개수 확인뿐이며 CLI/test/Cargo 기능 실행은0회다.
수정 전 실패·feature 적용 뒤 통과 모두 **GHA UNRUN**이며 부모의 기존4+1 batch에서 확인한다.
새 test block만 제거한 source 역투영 SHA256은 기존
`1d61909ae716e4ca47725ab67080de212fd214bf183719a612d5b8f3e75f3dd0`와 일치한다.
기존 일반3개와 embedded Python 원문/ignored caller는 byte 단위로 그대로다.

## 소스 확인과 남은 경계

로컬은 Python 파일·embedded caller의 AST parse, Rust rustfmt/check, diff 및 원문 읽기만
수행했다. original import/함수/compiler/model/helper/Cargo/build/test/Clippy 실행은0이다.
원래 Python `launch_dimensions`, `export_pipeline`, `export_model` 함수 전체와 기존
entrypoint/version AST 비교 구간은 기준73 source와 동일하다. `buffers.py`,
`offline_buffers.py`, native compiler_launch/compiler_buffers 및 부모 buffer 테스트도 동일하다.

부모는 새 leaf/test source를 포함해 기존 ELF를 build/stage하고 새 GHA selectors/Focus/pins를
등록해야 한다. native ABI 전체 동등성·실제 full model exporter 수용·256MiB/1GiB 제한 fit는
**UNRUN**이다. Python 계산/프런트엔드 제거와 source 완료 credit는 그 이후 별도 작업이다.

납품 source SHA256(기능 수용 hash가 아님):

| 파일 | SHA256 |
| --- | --- |
| `rust/crates/model-runtime/src/compiler_schedule.rs` | `0447c3a4213dc278b62505e4875ab24ce53fbf2c596aa712490a9e69c6aeafb3` |
| `rust/crates/model-runtime/src/compiler_schedule/export.rs` | `12aeeb60b209f22990bc518c155aed1f82743a867f64b0bcdf1874ba4453ed3c` |
| `rust/crates/model-runtime/src/bin/offline-qcom-compile.rs` | `efe6b3040bc822369fc86a159cff5f1d17f24b33045e442fadaefe6825183864` |
| `rust/crates/model-runtime/tests/compiler_export_caller.rs` | `a5f0a9288aaaf121cbeaabfcaa569ba3511301db43c290b918896bbd4c1e1c24` |
| `rust/tools/model_export/offline_schedule.py` | `bc1b7b572c8dd2f84f3ea0ca38b1744e4c7120155c33f3dabb633eabaf7f0144` |
