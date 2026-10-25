# #523 F02 compiler buffers: 기존 storage core 재사용

작성: 2026-10-26. 추적: [#523](https://github.com/bin9208/openpilot-rust/issues/523).
읽기 기준 HEAD: `126fdbea36598566d1040be0a35a365c909be4e4`.
작업 루트: `/home/fma1/.codex/worktrees/python-rust-523-completion/openpilot-rust`.
상태: 일반6/strict Clippy GHA 통과(부모 보고), 원본 비교는 import 실패.
실제 checkout root 추가 수정 후 원본 비교 GHA 재수용 대기.
**baseline 삭제 credit 0**. 기존 Python caller/원본 삭제·전환 수용을 뜻하지 않는다.

## 기존 구현 조사와 실제 추가 범위

`rust/tools/model_export/{buffers,offline_buffers}.py`,
`model-runtime/src/{export,buffer}.rs`, `compiler_onnx/{memory,storage}.rs`,
`compiler_backend_cpu.rs`의 호출부 및 기존 feature 구성을 읽었다.

- `buffer.rs`는 실행 시 native 할당·포인터 수명 API다. compiler의 alias identity와
  초기화 영역 계산을 맡기는 API가 아니므로 재사용 대상으로 삼지 않았다.
- `compiler_onnx/memory.rs`는 호출 수명 기반 arena 계획이며, `storage.rs`는 모델
  표현식의 storage alias/fusion admission이다. exporter byte capture와 구분했다.
- **`export::BufferTable`에 이미 alias/view 등록, write interval 차감, host view
  snapshot read, `read_initialized`, raw weights packing이 구현되어 있다.**
  CPU backend도 이 core를 사용한다. 같은 알고리즘을 새로 복제하지 않았다.

추가한 `compiler_buffers.rs`는 이 core의 `Buffer`, `BufferTable`, `Allocation`,
`View`를 재노출하고, 원본 host/offline descriptor admission 및 초기화 map을
보유하는 `OfflineBufferTable`을 제공한다. 새 allocator·UOp scheduler·장치 연결·
파일 방출·모델 실행은 추가하지 않았다. 수정 파일은 새 production 모듈, 새 test,
이 문서 세 개뿐이다. 기존 source/Cargo/mod/workflow/원장은 부모 소유다.

## production 호출 계약

부모가 `rust/crates/model-runtime/src/lib.rs`에 다음을 등록해야 외부 caller가
`openpilot_model_runtime::compiler_buffers`를 사용할 수 있다.

```rust
#[cfg(feature = "native-skip-miri")]
pub mod compiler_buffers;
```

이 feature는 이미 존재하는 `export`의 경계다. 순수 범위 알고리즘을 복제하거나
기존 export 파일을 수정하지 않기 위해 동일 경계를 사용했다. 새 dependency나
Cargo feature는 필요 없다. **`--no-default-features`만 주면 새 test는 cfg로
제외되므로 통과 증거로 쓰면 안 된다.** 아래 명령처럼 feature를 명시한다.
feature 활성화 자체가 모델·QCOM·LLVM·외부 numerics 실행을 뜻하지는 않는다.

호출 인터페이스는 다음과 같다.

```text
Descriptor::Single { buffer: Buffer, device: &str, allocated: bool }
Descriptor::NonSingle
host_buffer(Descriptor) -> Result<Buffer, String>
descriptor(Descriptor) -> Result<Buffer, String>
BufferTable::{view, read(view_bytes), write, weights}  // 기존 host core 재노출
OfflineBufferTable::new(&BTreeMap<u64, Vec<u8>>)
OfflineBufferTable::{view, read, write, views, weights}
```

`Buffer`는 `base: u64`, `allocation_bytes`, `offset`, `bytes`를 갖는다. lowering은
원본의 **base object identity**에 대응하는 안정적인 ID를 전달해야 한다. 내용이
같은 두 base를 합치거나, alias object마다 다른 base ID를 부여하면 안 된다.
동일 base의 크기·device 메타데이터는 불변이어야 한다. view ID는 첫 등장 순서로
부여하며 `(allocation, offset, bytes)`가 같으면 동일 ID를 재사용한다.

`host_buffer`는 single `CPU/NPY/PYTHON` prefix만 받고, `descriptor`는 여기에
`QCOM` prefix를 추가한다. prefix는 원본처럼 `:` 앞 문자열이며 대소문자를 바꾸지
않는다. `NonSingle`은 MultiBuffer/비 Buffer를 거절한다. 원본의 allocated 검사도
그대로 유지한다. **완전한 문자열이 `QCOM`인 allocated descriptor만 거절**하며
`QCOM:0`/`QCOM:7`을 임의로 canonicalize해서 이 조건을 확대하지 않았다.

caller는 UOp/자체 native IR을 해석한 후 이 metadata admission을 호출하고 반환
`Buffer`를 table에 전달한다. host `read`의 바이트는 **해당 view의 실제 snapshot**
이며 길이는 view bytes다. 실제 capture/장치 읽기는 caller 책임이다. offline은
실제 initialized bytes를 map에 넣고 **base ID**로 조회한다. 데이터는 full-base
길이여야 하며 view offset으로 그 안을 자른다. map은 schedule 동안 immutable하게
빌린다. 원본 실행을 숨겨 호출하거나 0으로 initializer를 대체하지 않는다.

read/write 호출은 원본 schedule 순서를 유지한다. kernel input read 뒤 output
write, copy source read 뒤 destination write를 호출한다. write는 앞서 읽어 저장한
초기 바이트를 지우지 않는다. read도 written으로 취급하지 않아, 이후 read는 아직
written이 아닌 범위를 새 snapshot으로 갱신할 수 있다. 완전히 written인 read는
missing/wrong-length initializer를 검사하지 않는다. 실패한 read에서 이미 생성된
view도 원본처럼 남는다.

`weights()`는 allocation 전체가 raw byte 0일 때만 `weight_offset=None`을 쓴다.
하나라도 nonzero이면 **allocation 전체 바이트**를 첫 등장 순서로 연결한다.
alignment padding·float 변환·NaN/음수 0 정규화·내용 기반 중복 제거는 하지 않는다.
예를 들어 f32 음수 0의 raw bytes `[0,0,0,128]`은 반드시 packed에 들어간다.

## 독립 원본 비교

`tests/compiler_buffers.rs`는 부모가 등록한 실제 library의
`openpilot_model_runtime::compiler_buffers`를 직접 호출한다. 별도 path module이나
테스트용 storage core는 사용하지 않는다.
일반 경계 test 6개와 GHA 전용 ignored test
`original_python_buffer_tables_match_native`를 추가했다.

ignored test는 합성 입력 recipe만 Python process에 보낸다. expected 결과나
native output은 보내지 않는다. GHA에서 다음을 실제 실행한다.

1. 원본 `schema.py`의 ExportError/Allocation/View 및 두 buffer 모듈의 원래
   함수·class AST를 변경 없이 추출한다. model_export 패키지의 무관한 import
   side effect만 피하며, range 계산/weights 계산의 대체 구현은 만들지 않는다.
2. 현재 checkout의 실제 tinygrad Buffer/MultiBuffer를 import한다. host trace는
   실제 CPU byte allocation·copyin·alias copyout에 원본 table method를 호출한다.
   offline trace는 실제 unallocated Buffer object에 원본 method를 호출한다.
   Python 객체 `id(base)`에 의한 원본 identity 계산을 그대로 사용한다.
3. 각 호출 직후 view 목록, allocation 목록, packed bytes, 반환 view ID 또는
   거절 여부를 native 전체 trace와 정확히 비교한다. source/provider SHA256도
   출력한다. CPU device가 실제 열렸음을 확인하고 다른 device open을 거절한다.

**32개 descriptor 입력**은 host/QCOM 및 suffix·유사 prefix·대소문자·비 single·
할당 상태를 비교한다. allocated descriptor control만은 Buffer 내부 allocation
flag에 합성 marker를 잠깐 설정하고 즉시 제거한다. 이 control은 실제 GPU 할당이나
CPU provider 동작 증거가 아니다. descriptor/table 원본 함수는 실제 호출한다.

**147개 table recipe**에는 host/offline 각각의 내부 6-byte write mask 64개,
부분·중첩·전체 write, read→write→read, 같은 view 반복·같은 내용의 별도 base,
base 첫 등장 순서, raw zero/음수 0/nonzero packing과 padding 없는 offset,
missing/짧은/긴 initializer 및 partial/full-write 우회, 잘못된 initializer
identity와 같은 base의 alias object를 initializer key로 잘못 쓴 경우,
offline host/QCOM suffix, zero-byte lazy validation과 host recapture가
들어 있다. 매 단계 snapshot을 비교하여 마지막 weights만 맞는 오류도 찾는다.

compiler entry point는 oracle에서 거절하도록 했고, kernel·Tensor graph·모델을
컴파일하거나 실행하지 않는다. 실제 CPU allocator와 import provider는 사용하는
작은 buffer 경계 검사다. 모델 수치 parity·전체 exporter 수용으로 해석하지 않는다.

## 부모 Actions 실행 명령

아래를 부모가 관리하는 소유 source/SDK/자원 admission 및 전후 봉인이 있는
Actions job에서 실행한다. 현재 checkout의 Python 3와 tinygrad source가 필요하며,
새 pip 설치·모델 다운로드·GPU는 필요하지 않다. 로컬에서는 실행하지 않는다.

```sh
cargo test --manifest-path rust/Cargo.toml --locked --no-default-features --features native-skip-miri -p openpilot-model-runtime --test compiler_buffers
cargo test --manifest-path rust/Cargo.toml --locked --no-default-features --features native-skip-miri -p openpilot-model-runtime --test compiler_buffers original_python_buffer_tables_match_native -- --ignored --exact --nocapture
cargo clippy --manifest-path rust/Cargo.toml --locked --no-default-features --features native-skip-miri -p openpilot-model-runtime --lib --test compiler_buffers -- -D warnings
```

두 번째 명령은 `GITHUB_ACTIONS=true`를 요구하며 정확히 **1 test**를 실행해야 한다.
부모는 해당 test 안의 32/147개 입력 수 검증과 전체 원본/native trace 일치를
수용해야 한다. 기존 no-default-feature units job에 단순히 target만 추가하면
cfg 때문에 0 tests가 될 수 있으므로 명시 feature가 포함된 집중 명령을 등록한다.
Clippy도 이 경계를 포함하되 기존 성공 검사를 불필요하게 반복하지 않는다.

## 소스 기준과 남은 수용

| 읽은 기존 source | SHA256 |
| --- | --- |
| `rust/tools/model_export/buffers.py` | `9429df49f408cda89f5b862fc0d51982668234cc18cb28ca0e83fd3d1a5bc7de` |
| `rust/tools/model_export/offline_buffers.py` | `9ab6c8c777b04d374750779fabfa75bb2b2af64fb6f8ce3ff8edd7a14138de4f` |
| `rust/crates/model-runtime/src/export.rs` | `7dc0993039db5247aa0e71d7c122c46b393e30a8e02724f8367a3b26ec6f6e8c` |

로컬에서 새 Rust 두 파일의 rustfmt 구문·서식과 내장 Python의 `ast.parse` 구문이
통과했다. source/diff를 검토했으며 새 파일의 diff whitespace 오류 출력은 없었다.
`git diff --no-index --check /dev/null <path>`의 종료코드 1은 새 파일 차이다.
Cargo/build/test/Clippy/원본 호출/모델 실행은 하지 않았다.
초기 납품 당시 기능 GHA는 **UNRUN**이었다. 이 sidecar의 commit/push/dispatch는0회다.

남은 작업은 부모의 gated mod 등록, 실제 compiler caller 연결, Focus/CI/원장
등록과 exact-SHA Actions 결과 기록이다. 기존 Python caller가 남아 있고 native
caller의 전체 exporter 원본 대체도 미수용이므로 **baseline 삭제 credit는 0**이다.
이 묶음으로 기존 Python 원본을 삭제하거나 F02/tinygrad 전체 전환 완료를 선언하지
않는다. source/원본 삭제는 실제 수용 뒤 부모가 담당한다.

부모는 production mod를 기존 `native-skip-miri` 경계에 등록하고 test import를 정식
library API로 연결했다. 등록 local Actions의 `buffers` role에 일반6/원본1(32 descriptor
+147 recipe)/strictClippy와 정확 실행수 확인을 추가했다. 최초 실행 결과는 아래에 기록한다.
집중 workflow는 변경 시 전체 role를 검증하고, 이후 source 변경은 필요한 role만
명시 dispatch한다. 기존 PR/전체 integration 요구는 유지한다.

## source DD 원본 비교 import 실패와 최소 수정 — 2026-10-26

사용자/부모가 전달한
[native units40212410286/buffers job121226283307](https://github.com/bin9208/openpilot-rust/actions/runs/40212410286/job/121226283307)
결과에서 일반 native6개와 strict Clippy는 통과했다. 그러나 ignored
`original_python_buffer_tables_match_native`는 tinygrad CPU provider import 도중
`openpilot.common.usbgpu_bus_lock` 의존성을 찾지 못해
**`ModuleNotFoundError: No module named 'openpilot'`**로 실패했다.
이를 원본/native 결과 불일치나 32/147개 입력 비교의 수용으로 기록하지 않는다.
이 sidecar는 해당 GHA를 조작하거나 재실행하지 않았다.

원인은 `python3 -I` snippet이 `sys.path`에 `tinygrad_repo`만 추가하여 실제 checkout의
`openpilot` package를 검색하지 못한 것이다. 입력으로 이미 받은 **실제 checkout root**를
한 줄 추가했다. 삽입 순서는 checkout root 다음 tinygrad root이므로 최종 검색 우선순위는
`tinygrad_repo`, checkout root 순이다. `-I/-B` 및 원래 환경·device/compile 금지는 유지한다.

stub·fallback·예외 무시·새 package 설치를 추가하지 않았다. 원본 Python 정의/AST 추출·
실제 CPU byte-copy·native 수학·일반6개 테스트·32 descriptor/147 recipe·expected trace
비교 로직은 그대로다. 현재 checkout에 있는 genuine dependency를 import하도록 경로만
연결했다. 공통 launcher/prepare helper·WF/Focus/원장은 수정하지 않았다.

로컬 Rust 서식·내장 Python 구문 검사가 통과했고, 추가한 `sys.path` 한 줄을 제거하면
수정 전 test 전체 bytes와 동일함을 역투영/diff로 확인했다. 실제 import·원본 함수·
Cargo/test/Clippy·모델은 실행하지 않았다. 수정 후 원본 비교는 **GHA UNRUN**이다.
부모가 같은 ignored exact selector를 실제1개 실행하여 32/147개 원본 비교를 확인해야 한다.
앞선 일반6/Clippy 성공은 이 원본 비교의 성공을 대신하지 않으며 baseline 삭제 credit는0이다.
수정 test SHA256: `2c391995d28a4cd64ad25cf10ec2dc2bb8625226c8bf493c7bc34905269eda64`.
