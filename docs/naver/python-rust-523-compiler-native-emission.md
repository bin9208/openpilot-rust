# #523 F02 실제 offline emitter의 native bookkeeping 소비 전환

작성: 2026-10-26. 원본 기준 source: `7a928cefe22e3c81bcb941f6ed20eaa80eeabc59`.
상태: **순서 보존 stream 구현을 포함한 review 가능한 WIP, 새 production GHA UNRUN**.
로컬은 source/AST syntax/rustfmt/diff만 확인했다. Python 파일 전체 삭제 credit0,
전체 모델 wire fit/QCOM 실행 수용0이다. 부모는 stage frozen patch만 먼저 독립 publish했다.

[승인된 설계](python-rust-523-compiler-next-closure.md)와 후속 명시적 stream 승인에 따라
다음 네 파일을 변경했다. 이 문서 외의 docs, WF/Cargo/Focus/원장/pins/stage/commit/push는
변경하지 않았다. 공유 worktree의 다른 agent WIP와 stage reader/fixture는 수정하지 않는다.

- `rust/tools/model_export/offline_schedule.py`
- `rust/crates/model-runtime/src/compiler_schedule/export.rs`
- `rust/crates/model-runtime/src/bin/offline-qcom-compile.rs`
- `rust/crates/model-runtime/tests/compiler_export_caller.rs`

## 실제 소비와 원래 거부 순서

`export_model/export_pipeline → emit_schedule → 기존 compiler.command.argv + --export-stream`
경로다. 동일한 기존 ELF의 별도 내부 mode를 사용한다. 일반 source→genuine binary compile의
argv와 본문은 그대로며 stream에서는 provider library를 열지 않는다. 기존 `--export-plan`
v1/v2 final API도 그대로 남는다. version 오류나 stream 실패를 Python 계산으로 복구하지 않는다.

emitter의 Python `OfflineBufferTable` 생성과 view/read/write/weights 실행,
`launch_dimensions`, retained set/stage end/entrypoint 계산, native 대조용 Python expected
Manifest를 제거했다. 이를 다른 Python 파일이나 forwarding 함수로 옮기지 않았다.
Python은 Tensor/UOp lowering, constant 준비, memory planner, genuine rendering/to_program/
compiler, Kernel signature/dedup, metadata/event 기록, binding/call 조립과 writer를 소유한다.

처음 작성한 final-only v2는 lazy read/launch 실패를 뒤 compiler보다 늦게 관측하는 결함이
있었다. 그 patch는 수용하지 않았다. 현재 stream은 native state를 한 번 만들고 다음 각
경계를 동기적으로 ACK한 뒤에만 다음 원래 연산으로 진행한다. 초기화 전체나 growing event
prefix를 반복 재생하지 않는다.

| 경계 | 현재 순서 |
| --- | --- |
| initializer metadata | 기존 mapping의 descriptor를 취득하고 원래 실제 key/data를 한 번 전송. base 정규화·unused byte-length 전수 검사는 없음 |
| input | 각각 descriptor → view → initialized key가 있으면 read → write. 뒤 input/output descriptor보다 먼저 read 실패 |
| COPY | 모든 현재 argument descriptor → source read → destination write → source/destination view. 뒤 kernel compile 진입 전에 실패 |
| kernel | 현재 argument descriptor → genuine to_program/compile → 기존 info/device/source/binary 검사 → native launch → ins read → outs write → globals view |
| 마지막 경계 | 각각 output descriptor/read → native weights → raw stage/retained/prefix 및 version 판정 |

따라서 선행 input/COPY read 실패 시 compiler0, 현재 kernel의 launch/read 실패 시 해당
compiler까지만 진행하고 미래 compiler에 진입하지 않는 배치다. 이는 source 구현 결론이며,
새 독립 error-order GHA 통과를 아직 주장하지 않는다. unused/wrong-length initializer와
write-covered read는 기존 native `BufferTable::read_initialized`의 lazy 계약 그대로다.

`view`는 native index를 계산하지 않고 event 위치 token을 반환한다. input/output Binding,
COPY source/destination, kernel argument token을 final `event_views`로 해석한다. read/write
포함 encounter 순서, 함수/source/binary/argument identity, dedup 순서, 실제 call occurrence를
유지한다. 실제 storage/program 객체를 강하게 참조하여 숫자 id 재사용을 막는다.
initializer key는 `id(mapping_key)`이며 `id(buffer.base)`로 합치지 않는다.

`static_allocator_capability`, `plan_linear`, `schedule_outputs`, `_number`, `export_model`
본문과 destination 생성 이후 writer는 원본 기준과 byte 동일하다. kernel 파일명/순서,
weights/graph JSON의 indent/newline과 global_→global 처리를 바꾸지 않았다.
`export_pipeline`은 pruning 전 stage CALL tuple, pruning 후·memory planning 전 retained CALL
identity를 보존한다. pre/post planning의 위치·길이·operation kind와 실제 emitted prefix를
기록하며 membership/end 계산은 기존 native `pipeline_entrypoints`만 수행한다.

## 별도 stream protocol과 회수

한 caller가 소유한 Popen process와 private stdin/stdout pipe를 쓴다. 추가 Python worker,
임시 protocol file, 새 ELF, generic CLI 또는 새 GPU/host/device policy는 없다. QEMU/loader/
library argv는 기존 `compiler.command.argv`를 그대로 쓰고 `--export-stream`만 붙인다.

frame은 **4-byte big-endian 길이 + JSON**이다. envelope의 정확한 필드는 session,
sequence, command다. session은 32자리 소문자 hex nonce, sequence는0부터 정확히 증가한다.
Begin(version1/initialized) → Event 또는 Program/Launch → Finish 순서만 허용한다.
Program source/binary는 실제 program 객체별 한 번 전송하고 Launch는 call별로 보낸다.
unknown field/tag, malformed JSON/hex, 잘못된 owner/sequence/program, 재시작/완료 후 재사용,
부족한 EOF/truncated frame은 실패한다. native semantic 오류는 session을 latch하고 CLI는
즉시 nonzero 종료한다. Python 전송·응답 오류도 latch한 뒤 owned process group을 종료·회수한다.

checkpoint는 complete=false이고 Plan이 없다. Finish만 complete=true/Plan을 갖는다.
native는 Finish 뒤 **stdin EOF를 확인하기 전 final을 내보내지 않으며** trailing byte를 거부한다.
Python은 final 뒤 stdout EOF/추가 byte 부재, bounded stderr EOF/비어 있음, exit0와 wait/reap을
확인한 뒤에만 destination을 만든다. 불완전한 checkpoint를 final 수용으로 승격하지 않는다.

Python은 nonblocking selector로 partial write/read와 stderr를 함께 처리한다. 매 RPC timeout은
기존 `compiler.command.timeout_seconds`이며 작은 genuine 검사에서는60초다. native stdio의
frame/idle 상한은900초이고 부분 header/body의 read deadline을 갱신하지 않는다. 출력 write도
별도900초 상한이다. 원래 genuine compiler 호출 timeout과 argv는 변경하지 않았다.
900초는 전체 모델의 총 실행 예산이 아니다. native가 기다리는 동안 caller의 lowering/compile이
900초를 넘으면 stream은 거부할 수 있다. configured compiler timeout>900인 소비자의 전체
호환성은 아직 입증하지 않았다. Python timeout 시 owned unreaped PID의 process group에
SIGKILL 후 wait 최대5초로 회수한다. poll/reap으로 PID를 풀어놓고 그 뒤 killpg하지 않는다.

| 자원 | 상한/소유 |
| --- | --- |
| stream 입력·출력 | **각 누적256MiB**, 4-byte header/모든 checkpoint 포함 |
| event | 1,048,576 |
| initialized/program/launch roster | 각65,536 |
| 총 frame | 1,179,650 = events + programs + launches + Begin/Finish |
| raw stage/retained/prefix | stage65,536, retained65,536, prefix65,537, stage CALL occurrence 총1,048,576 |
| distinct storage / packed weights | 1GiB / 128MiB |
| retained initializer+program source/binary | native 합계256MiB |
| stderr | Python 누적64KiB, 정상 성공은 비어 있어야 함 |

입력 header 길이는 native allocation 전에 누적 cap과 대조한다. Python은 initializer hex의
합계 admission 후 hex를 만들고 JSON을 chunk별로 합산한다. 이 admission은 data와 base의
길이를 비교하는 semantic 검사와 다르다. 잘못된 길이라도 unused/write-covered이면 허용한다.
Protocol raw byte cap은 memory RSS cap과 같지 않다. JSON serializer의 한 string chunk,
hex, native table/packed/Plan, decoder와 Python metadata가 동시에 존재할 수 있다.
128MiB weights의 hex만256MiB이므로 metadata까지 더한 최댓값은 wire에 맞지 않는다.
실제 모델의 bytes/peak RSS/시간은 미측정이며 cap 증가나 fallback으로 fit를 선언하지 않는다.

## strict raw v2와 역사 v1 final API

기존 `--export-plan`의 공통 initialized/events/programs/launches/emitted_by_source 필드를
유지한다. v1은 stage_ends만 필수이고, v2는 stages의 원래 CALL ids와 retained_calls만 필수다.
누락/explicit null/교차 필드/unknown/malformed identity를 거부한다. Optional presence는 null을
absence로 흡수하지 않는 별도 deserialize를 쓴다. raw v2와 stream Finish는 같은 stage core를
사용한다. wire version과 Manifest version은 별개라 stage 없음은 v1, named empty stage는 v2다.

v1 fixture의 prepared boundary 의미와 final API는 보존한다. 그 API의 기존 stage-first
처리 순서를 ordered checkpoint로 가장하지 않는다. 실제 emitter는 새 stream을 사용한다.
launch 계산은 같은 `compiler_launch::launch_dimensions`를 공유한다. explicit locals의
원래 float/int/bool·rank를 유지하고 inferred local만 기존 strict core 규칙을 적용한다.
serde_json float_roundtrip은 부모가 기존 regression 때문에 이미 연결했다.

Python decoder는 정확한 중첩 field/tag/type/count, bool index 거부, event/view/extent 참조,
weights hex/digest, launch rank와 stage 이름/범위를 검사한다. allocation interning,
write-range subtraction, weight packing, launch regrouping, retained membership/entrypoint
계산을 다시 하지 않는다. 응답 duplicate key/nonfinite literal도 거부한다.

## 검사와 expected 독립성

`compiler_export_caller`는 **일반10 + ignored2 = 총12개**다. 기존 일반 native/CLI4 본문은
byte 동일하다. 추가 v2 일반3은 raw pruning/반복/expanded prefix/no-stage·empty-stage,
null/unknown/cross-version·잘못된 identity와 count, 실제 기존 final CLI를 검사한다.
65,536/65,537 stage와 occurrence cap을 유지한다.

추가 stream 일반3은 lazy wrong-length/unused/write-covered 및 failed/final latch, 고정 원본
weight/event/entry bytes, launch-before-read, 실제 CLI의 fragmented write/owner/sequence/
unknown/truncated header·payload/과대 길이/missing Finish/trailing input을 검사한다.
실제 CLI는 provider library가 없어도 이 mode를 처리해야 한다.

기존 ignored COPY selector는 `maintained_python_export_calls_native_plan_without_fallback`다.
실제 Tinygrad COPY → maintained emitter → 같은 native CLI → writer를 사용한다. Python table
생성자를 poison하고 launch 함수 부재와 고정 원본 weights/COPY/entrypoint를 확인한다.
3-byte partial write, 실제 ELF의 stalled Begin timeout/latch/reap/reuse 거부, 잘못된 argv와
응답 fault5의 destination 부재를 추가했다. 응답 오염은 거부 검사만이며 positive oracle가 아니다.
고정 decimal의 explicit-local f64 bits/int/bool decoder regression도 유지한다.

새 ignored selector는 `maintained_python_mixed_emission_matches_original_bookkeeping`다.
initialized input16×float32 → COPY → genuine QCOM kernel → 같은 kernel call 재사용과 named
empty stage를 비교한다. 실제 planner의 CALL rewrite를 요구하며 GPU open은 거부한다.
원본 expected는 별도 canonical file의 SHA
`bc1b7b572c8dd2f84f3ea0ca38b1744e4c7120155c33f3dabb633eabaf7f0144`에서 얻는다.
원래 launch 함수와 emitter의 첫 native assignment 앞 문장만 AST로 취하고 return만 붙인다.
원본 pass는 Python table/launch/stage/직렬화를 계산하며 native 호출0이다. source bytes를
복구하거나 current candidate Plan을 expected로 주입하지 않는다. graph JSON bytes,
weights bytes, genuine kernel source/binary를 original-first로 비교한다.

같은 pinned 원본 prefix와 candidate 각각을 다음7사례로 관측한다. compiler 오류를 가짜
성공으로 바꾸지 않는다. launch 음성 metadata는 **genuine compile 뒤**에만 변경한다.

- bad initialized input → compiler/lowering0.
- leading COPY의 uninitialized read → compiler/lowering0.
- 첫 kernel의 uninitialized read 뒤 다른 source의 future kernel → 현재 compiler/lowering1.
- 현재 launch와 read가 모두 잘못됨 → launch 오류 우선, 현재 compiler/lowering1.
- 마지막 output read와 bad empty-name stage → initialized 오류 우선, compiler/lowering0.
- unused wrong-length initializer → 원본/후보 graph·weights 성공, compiler/lowering0.
- write-covered wrong-length initializer → 원본/후보 graph·weights 성공, compiler/lowering0.

각 pass 전에 test 전용 to_program cache만 비우고 lowering 진입과 실제 provider compile 진입을
따로 센다. cache hit로 미래 compiler 호출을 숨기지 않는다. 음성 입력은 오류 범주·destination
부재를, lazy 양성은 독립 원본 graph/weights를 비교한다. 모두 새 GHA에서 확인할 검사이며
로컬에서 실행하지 않았다. 작은 혼합 입력은 전체 모델/constant pruning/GPU inference 수용이 아니다.

## 부모 GHA 연결 계약

일반 target의 기대치를 기존7에서 **10 passed/2 ignored**로 바꾼다. COPY와 mixed는 각각
정확 selector에 `-- --ignored --exact --nocapture`를 붙여 별도 실행한다. ignored 전체를
작은 native job에서 한꺼번에 실행하면 genuine compiler command가 없는 mixed까지 실행된다.
기존 package/features/locked 옵션과 Clippy의 lib/bin/caller-test 대상을 유지한다.

```bash
cargo test --manifest-path rust/Cargo.toml --locked --no-default-features \
  --features native-skip-miri -p openpilot-model-runtime --test compiler_export_caller
```

| env/proof | 계약 |
| --- | --- |
| `QCOM_COMPILER_COMMAND` | 같은 새 source ELF를 stage한 기존 genuine ARM/QEMU argv JSON array. raw compile와 새 --export-stream을 모두 지원해야 함 |
| `COMPILER_EMISSION_REFERENCE_SCHEDULE` | 위 고정 SHA의 원래 file을 별도 canonical absolute regular file로 제공. current emitter를 가리키면 실패 |
| `STAGE_REFERENCE_PYTHON` | 입증된 canonical absolute Python. mixed test는 PATH 탐색 없이 이 executable 직접 사용 |
| `CI_JOB_TEMP/proof/stage-oracle-python.sha256`, `.version` | executable full SHA/정확 path 및 actual child sys.version과 대조. 기존 admission 재사용, 새 version process 없음 |
| `COMPILER_EMISSION_EVIDENCE` | 부모 소유 canonical directory 아래 새 absolute directory. existing path 실패. proof upload 아래 배치 권장 |
| `GITHUB_ACTIONS`, `GITHUB_SHA` | 기존 trusted-source/source-index/SDK admission을 부모가 유지 |

mixed 관측은 record1MiB/총8MiB, directory0700, create_new/O_NOFOLLOW/0600이다. 원본 source,
Python proof, 입력, 원본 graph/weights/kernel, 별도 candidate decoder metadata/final Plan, error-order와 comparison
receipt를 남긴다. 선택 source/Python의 full SHA와 stat identity 및 proof2개는 전후 동일해야 한다.
`candidate-request.json`은 decoder가 참조하는 events/launches/raw-stage metadata이며 Begin/
Program까지 포함한 전체 전송 원문이라고 주장하지 않는다.
전체 import/stdlib/FD/DSO closure를 입증했다고 확대하지 않는다. 실패 전 기록한 원문은 남는다.

부모는 test_offline_qcom_launch.py의 직접 old-function 호출을 native launch9와 독립4411
reader로 전환하고 required source absence를 연결한다. 본 patch는 해당 파일을 수정하지 않는다.
기존 genuine `test_offline_qcom_schedule.py`의 export2와 raw compiler2 계약도 유지해야 한다.
source/SDK/pin/실제 worker 자원은 부모 WF 계약이며 이번 patch에서 완화하지 않았다.

부모가 갱신할 source pin SHA256:

| 파일 | SHA256 |
| --- | --- |
| `rust/tools/model_export/offline_schedule.py` | `2aba522b6af945a290be66203f4dfed9a278a4bc4cf7f7d7841e23a7b51a3a01` |
| `rust/crates/model-runtime/src/compiler_schedule/export.rs` | `b731f44bcb79f778654fdee9854ee484d01d5278cd6f47c8b28713ae007c5435` |
| `rust/crates/model-runtime/src/bin/offline-qcom-compile.rs` | `3687cc13749eed58b25bd204b246f77a1b0a4d6298221c35fc355b5d1d703aa0` |
| `rust/crates/model-runtime/tests/compiler_export_caller.rs` | `b8920e39c1d8f14ae7b68e49ba926d41a39ee7bcb438bee4393ea7c9479499dc` |

## 독립 stage 원문과 별도 publish

[actual source7a928/run40236805936/job121300012716](https://github.com/bin9208/openpilot-rust/actions/runs/40236805936/job/121300012716)은
SUCCESS다. artifact12615238889는2,069,028B, SHA256
`8dd0c45777c45b71ffc70012a966f4ef4402fdd746fab72b64066834a2b679ab`다.
부모가 받은 ZIP을 중복 다운로드 없이 inert 감사했다. 47 safe/unique/nosymlink member,
CRC 정상, expanded14,468,053B다. source/index·SDK 전후 봉인, exit3 label0,
source/SDK outcomes success 및 SDK install0을 확인했다. index/index-stage는 historical
Git tree19,571 entry와 일치하고 선택 source8개도 historical blob bytes/SHA와 같다.

request4,444,067B/stdout3,079,930B/stderr0, original73=68성공+5거부다.
13 witness 전후/canonical Python/proof2개/원본rc0/source SHA/16MiB·64MiB cap을 확인했다.
65,536 stage 수용/65,537 거부를 포함한 raw7의 합계는7,546,378B다.
별도 compiler_schedule.rs와 compiler-stage-original raw7+README patch는 original73 생성 및
native comparison을 byte 그대로 유지하고 SHA-bound frozen reader로 전환했다.
oracle.py→oracle.txt는 이름만 바꿨다. generated request는 JSON Value로 비교한다.

부모는 이 stage-only patch를 먼저 publish했고, 2026-10-26 인계에서
[sourcea621/run40241338985/job121313583472](https://github.com/bin9208/openpilot-rust/actions/runs/40241338985/job/121313583472)
normal model job SUCCESS를 알렸다. frozen8/union/launch4411/Clippy actual artifact 감사는
부모 담당이다. 본 stream 작업에서 stage test/fixture는 추가 수정하지 않았다.
이 stage 결과를 새 production stream이나 전체 모델 수용으로 투영하지 않는다.

## 검증 경계와 남은 일

로컬은 production·embedded Python AST, Rust rustfmt/check, diff와 불변 구간 byte 비교뿐이다.
Cargo/build/test/Clippy/project CLI/Python module import·원본 함수·model/compiler/helper/GHA
실행은0회다. 원본4/CLI raw+final 본문, planner/lowering의 위 불변 함수와 writer를 대조했다.

남은 것은 부모의 exact-source compile/Clippy/native10/COPY1/genuine mixed1와 seven-order
사례, 기존 genuine 소비자, 독립179/4411/73 연결 수용이다. 프로토콜 elapsed/peak RSS/실제
대표 모델 wire fit 및 QCOM 전체 수용은 별도 pending이다. offline_buffers.py의 descriptor,
공통 buffers.py와 CPU/live QCOM/AMD/constant/독립 original 소비자는 남아 있다.
파일 전체 퇴역 credit는0이며 이 source patch만으로 algorithm equivalence를 완료 처리하지 않는다.

## 2026-10-26 production kernel registry Rust 이관 — 새 source, GHA 미실행

부모가 승인한 다음 slice는 `offline_schedule.py`, `compiler_schedule/export.rs`,
`compiler_export_caller.rs` 세 파일이다. 기존 native stream의 명시적인 **Begin version2**에
`kernel_commit`/`kernel` reply/final `registry`를 추가했다. 새 CLI flag·helper·Cargo/lib 등록·
workflow는 만들지 않았다. 내부 stream version과 graph version은 별개이며 graph는 계속1/2다.

Python의 실제 binary SHA256·binary 길이·Kernel signature hash/dedup·index 할당·첫 대표
선택 코드를 삭제했다. Rust는 보유한 실제 Program binary에서 SHA/길이를 계산하고
`(name, binary_sha256, binary_bytes, 중첩 arguments)`로 interning한다. SOURCE와 PROGRAM
key는 signature에 넣지 않는다. stable registry Vec는 첫 commit의 `program_key`를 보관하고,
반복 call마다 index를 반환한다. Python은 강하게 보유한 UOp handle에 이 key를 연결하고
반환 metadata로 dataclass DTO를 복원한다. Python에서 signature를 재계산하지 않는다.
기존 `launch`의 `len(binary)`는 전송량 admission용으로 남으며 registry 길이 계산이 아니다.

commit 위치는 원래 Kernel 생성 위치다. genuine lowering/compile → info/aux·SOURCE/BINARY
검증 → native launch 검증 → 실제 input read/output write → kernel commit → global view/call
순서를 유지했다. Program 등록이나 launch에서 signature를 미리 확정하지 않는다. 누락·중복·
다른 key commit, commit 전 다음 launch, v1에서의 commit, unknown version은 거부하며
native 실패는 latch된다. original compiler/`compile_cached`/ASSERT_COMPILE 경로는 그대로다.

ImageArgument의 width/height/pitch/element_bytes는 원본 `ImageDType.shape: tuple[int, ...]`,
pitch와 itemsize가 생산하는 **정수 metadata 계약 범위**다. wire에서 10진 문자열로 전달해
Rust i128로 읽고, final graph에는 정수로 쓴다. Serde1.0.229의 internally tagged content가
i128을 직접 보관하지 못하는 제약을 피한다. runtime image allocator의 u32 admission을
앞당겨 적용하지 않는다. 임의 Python 정수 전체나 인위적인 float/bool image metadata까지
동등하다고 주장하지 않는다.

### 보존한 경계와 metadata 예산 보정

export-plan v1/v2의 `prepare` 본문은 final `registry: None` 추가 외에 바이트가 같다.
None은 serialization에서 생략되므로 기존 raw plan과 stream v1의 schema를 보존한다.
v1은 새 metadata 예산을 차감하지 않는다. Python의 기존 transport/deadline/EOF/reap/held
launch 함수와 planner/constant/lowering 진입·최종 call 복원·destination writer를 보존했다.
final은 기존 CLI의 stdin EOF 이후에만 나오며 Python은 stdout/stderr EOF·빈 stderr·exit0·
reap 확인을 마친 뒤 destination을 만든다. native 실패 시 Python fallback은 없다.

256MiB 누적 input/output wire cap, 동일 frame cap `1_048_576 + 2*65536 + 2`,
event1,048,576·Program/launch65,536·storage1GiB·packed weights128MiB·기존 deadline은
그대로다. 새 commit은 기존 frame/byte quota를 소비하므로 최대치들을 동시에 채우는 별도
여유를 추가하지 않았다. 기존 per-frame timeout과 부모 job deadline도 늘리지 않았다.

부모 review에서 `serialized signature 길이 * 2`가 실제 Argument enum payload를 작게
계상한다는 결함을 지적했고, 그 계산은 최종 source에서 제거했다. 작은 Buffer variant도
Image의 i128 네 필드를 담는 enum slot 전체를 점유한다. 최종 계상은 다음 payload를
checked product/sum으로 계산해 **wire budget과 별도로 기존 retained256MiB ledger에 합산**한다.

- 두 signature의 String capacity, arguments Vec capacity, 각 중첩 Vec의 capacity와
  `size_of::<Argument>()` payload. clone의 실제 heap capacity가 예상보다 크면 추가 차감한다.
- map entry와 registered entry의 최소 payload 및 registry/map container header.
- registry Vec와 launch-index Vec의 실제 추가 capacity. 기존 Vec의 amortized growth를 유지하고
  spare capacity가 있어도 최소 한 slot을 보수적으로 계상한다. 매 launch index도 차감한다.

곱셈·덧셈 overflow, metadata 합계 초과 또는 기존 held bytes와의 합계 초과는 거부한다.
allocator/BTreeMap node overhead·일시적인 decode/clone 할당을 포함한 전체 RSS 보장은 아니다.
실제 peak RSS/대표 모델 wire fit은 부모 GHA에서 확인해야 한다.

### 검사 구성과 독립 기대값

기존 normal native10 본문은 바이트 그대로 유지했다. 새 caller 검사4개를 추가해 target의
기대치는 **14 passed / 2 ignored**다.

| 새 normal selector | 검사 |
| --- | --- |
| `registry_v2_excludes_source_and_key_and_keeps_first_program_and_repeated_calls` | 다른 key90→2·다른 SOURCE·동일 signature의 첫 key90 유지, 같은 key 재호출과 launch3개 보존, 독립 b"abc" SHA/길이, raw plan/v1 stream registry 부재 |
| `registry_v2_distinguishes_every_argument_field_name_binary_and_nested_position` | name/binary, image 네 필드, buffer/image kind, outer/inner 순서·group 크기·빈 group의 총12개 구별 사례 |
| `registry_v2_rejects_cross_version_missing_duplicate_wrong_commit_and_read_failure` | v1/unknown version·누락/중복/다른 key·선행 launch/read 실패와 latch, caller SHA/길이/SOURCE 주입 및 불완전/unknown argument 거부 |
| `actual_registry_stream_cli_preserves_kernel_reply_and_final_eof_gate` | 기존 실제 CLI에서 kernel reply/index, commit 누락과 trailing input 거부, final 완료 신호 보존 |

private arithmetic helper를 검사하는 **기존 library target unit2개**도 추가했다. 새 test target이나
외부 public API는 없다. 필터는 `compiler_schedule::export::tests::`다.

- `registry_metadata_budget_rejects_product_sum_overflow_and_cap_without_allocation`:
  초대형 할당 없이 곱/합 overflow, cap exact/over, zero product, 기존 held+metadata 초과.
- `registry_metadata_counts_buffer_enum_capacity_and_both_owned_signatures`:
  작은 실제 Vec/String의 spare capacity, Buffer enum slot, 두 signature/entry payload가
  짧은 JSON 길이와 구분되어 계상되는 회귀.

기존 ignored COPY selector는 final registry schema/held key/count 오류 네 가지를 더 거부하고
destination 부재를 확인한다. 별도 새 helper는 없다. genuine mixed selector는 기존 pinned
original-first graph JSON/weights/kernel SOURCE+BINARY 비교를 유지하고 원본 PROGRAM의
LINEAR IR key와 후보가 받은 첫 commit의 실제 UOp 객체를 확인한다. 원본/후보 IR key·handle
기록 두 개만 기존 record1MiB/총8MiB 봉인 안에 추가했다.

genuine 원본 pass가 끝난 뒤 후보에서 `Kernel.__hash__`를 독으로 바꾸고, 현재 emitter의
binary SHA 호출도 거부한다. weight integrity SHA는 유지한다. 독이 실제로 활성화됐는지
negative control 후 후보를 실행한다. seven-order 사례는 원본을 먼저 실행하며 후보에서만
Kernel hash 독을 적용한다. 기존 compiler/lowering 횟수 `[0,0,1,1,0,0,0]`, 오류 범주,
실패 destination 부재·future compiler 호출0을 그대로 검사한다.

독립 원문 prefix SHA는 계속
`bc1b7b572c8dd2f84f3ea0ca38b1744e4c7120155c33f3dabb633eabaf7f0144`다.
expected를 candidate로 생성하거나 이전 관측값을 바꾸지 않았다. 현재 추가 검사는 모두
**GHA 미실행**이며 기존 sourceef172의 성공을 새 registry 수용으로 확대하지 않는다.

### 부모 통합과 로컬 검증

부모가 바꿀 기존 count literal은 두 곳이다. 해당 workflow는 본 patch에서 수정하지 않았다.

- `.github/workflows/python-rust-523-native-units.yml`의 `compiler-export-units.log`:
  `10 passed; 0 failed; 2 ignored;` → `14 passed; 0 failed; 2 ignored;`.
- `.github/workflows/python-rust-523-compiler-emission.yml`의 `native-v1-v2.stdout`: 같은 변경.

새 library unit2는 기존 package/library target에서 아래 filter로 실행해야 한다.
기존 caller native14/COPY1/genuine mixed1·Clippy selector와 canonical Python/provider/source·
SDK 전후 proof 및 최종 봉인은 유지한다. source pin은 부모가 기존 등록에 반영한다.

```bash
cargo test --manifest-path rust/Cargo.toml --locked --no-default-features \
  --features native-skip-miri -p openpilot-model-runtime --lib \
  compiler_schedule::export::tests:: -- --nocapture
```

| 수정 source | SHA256 |
| --- | --- |
| `rust/tools/model_export/offline_schedule.py` | `b4d51d33569eaf28e734b90c540e572d8e72abf88389e4217065a9fb0fdfe7f0` |
| `rust/crates/model-runtime/src/compiler_schedule/export.rs` | `0a1372cb24410f91fc22d3224ca62b9704c79a7bcd7f369b20f61da603438037` |
| `rust/crates/model-runtime/tests/compiler_export_caller.rs` | `275357ea69c62d471ae117847aa43abcc79a20a2319f06391249cf54beff74b4` |

로컬 실행은 Rust1.94 rustfmt/check, production/embedded Python AST parse, YAML inert parse,
diff/check 및 불변 구간 대조뿐이다. Rust 타입 검사·Cargo/build/test/Clippy/project CLI/
compiler/model/oracle 실행·GHA dispatch·stage/commit/push는0회다. Python binary hash/dedup
삭제, 기존 normal10·raw prepare·writer·cap/source pin 보존을 소스에서 확인했다.

새 source의 타입·Clippy·기능 수용은 부모 Actions가 담당한다. Python lowering/metadata
생산/writer와 genuine compiler/compile_cached 제어는 아직 남는다. Model D의 원본 compiler2
execution edge도 별도 pending이다. 이번 registry 이관은 전체 CPU/QCOM 모델 수용이나
Model D 원인 해결, Python 파일 퇴역 credit가 아니다.

## 2026-10-26 Git6c actual 감사와 StreamReply 최소 수정

판정 source는 `6c5d5a9be691d18ca850af6ef9b42e47b668d767`의 Git blob이다. 이후 worktree
상태를 이 actual 결과에 섞지 않았다.
[run40255341791/job121355876060](https://github.com/bin9208/openpilot-rust/actions/runs/40255341791/job/121355876060)은
completed FAILURE이며 최초 실패는 step18의 마지막 strict Clippy다.

| 같은 run_once block의 actual label | 결과 |
| --- | --- |
| `registry-budget` | exit0, library2 PASS, 57 filtered out |
| `native-v1-v2` | exit0, caller14 PASS / ignored2 |
| `actual-copy` | exit0, exact ignored COPY1 PASS |
| `actual-mixed` | exit0, exact ignored genuine mixed1 PASS, 61.12초 |
| `strict-clippy` | exit101, `clippy::large_enum_variant` |

Git6c WF의 명령을 읽어 새 library2가 기존 hosted `run_once` 안에서 실행됐음을 확인했다.
모든 기능 검사는 Clippy보다 앞에서 실행됐다. source/SDK/LLVM 후봉인 step20과 evidence upload,
owned source/alias/target cleanup step22·공통 temp cleanup step23은 실제 success다. 후봉인은
API step outcome과 Git6c의 strict cmp/SHA 명령으로 확인했다. 별도 private artifact12622218319는
metadata만 조회했고 ZIP은 다운로드하지 않았으므로 그 내부 raw before/after의 독립 재대조라고
확대하지 않는다.

functional artifact12621944150은18,529B, SHA256
`29c4d86c506c51e6d28bdcb63b47eee5b5560228691841f1a1910d938e2805f7`다.
API expected bytes/digest와 실제 ZIP을 대조했고, 1회 다운로드·26 safe unique/nosymlink member·
expanded44,505B·모든 member CRC PASS다. 기존 archive는 다시 받지 않았다.

원본 prefix16,463B SHA는 기존 `bc1b7b572c8dd2f84f3ea0ca38b1744e4c7120155c33f3dabb633eabaf7f0144`
그대로다. canonical Python은3.12.15이며 raw executable proof와 comparison의 source pin이 같다.
원본 QCOM compiler provider는 기존 `fb7e6390cc25700d6935b2eef3acad85a43f247206bdf84cb4d37bd48a60b093`,
actual staged ARM frontend는 `779267f94344c55f6edb3d2ed8ed69afd6063a446e3d342fc53660c63fbae21d`다.
기존 QEMU loader/DSO closure compatible이며 GPU 실행0이다.

artifact에서 독립적으로 다음 데이터를 대조했다.

- graph의 allocations/views/entrypoints·kernel signature가 candidate final Plan과 같다.
  calls3 중 kernel2의 native index는 `[0,0]`, global `[1,1,1]`/local `[4,1,1]`이다.
- weights64B raw가 candidate hex와 같고 SHA는
  `58dda328598e2f7fe472621bfc54935aaa354d1a6ebcaf9562cd743fd575eb19`다.
- original SOURCE396B SHA `4b68cdcb9c9f02dda5eb7ef71300444f3b0d050d4850d5d60f702a6a98c687f9`,
  BINARY2240B SHA `b2f550a3a3d46e3b3832494b46499427439fc5c5d18b4e09d400184c48912bd4`다.
  raw BINARY 길이/SHA가 원본 graph와 native registry의 값에 일치한다.
- 원본/candidate IR record는 bytes123·SHA
  `75022cbe658b7a6a55d2d55cc920f658a71663cc10092e0e57c32c002ff14285`로 동일하다.
  실제 대표 handle140414031945104가 registry의 첫 `program_key`와 같고 LINEAR IR key는
  `7110338e63766b5a4e7ea51a0ecdaf9ab75324cff0c76136e511aeb261dd27f1`이다.
- order7은 original-first, lowering/compiler 각각 `[0,0,1,1,0,0,0]`이다. 앞5 오류의 destination은
  양쪽 모두 없고 launch 오류가 read보다 먼저다. 뒤2 lazy 양성은 오류 없이 candidate destination을
  만들었다. future compiler0, Kernel hash/binary SHA 독과 활성화 음성 control도 PASS다.

전체 graph JSON bytes와 candidate SOURCE/BINARY bytes 동일성은 Git6c test의 실제 assertion PASS로
확인했다. candidate 파일 자체는 ZIP에 별도 보존되지 않았으므로 그것을 다운로드 후 다시 비교했다고
주장하지 않는다. 원본 graph2156B SHA는
`a6ef4f632e3f405effe4c0bac11ad9abcd48dc018e0766c19eccbb4aefc788d1`이다.
위 raw component/IR/receipt를 독립 대조한 기록은 private scratch의
`ci/f02-6c-genuine-audit.json`에 남겼다. expected/golden 생성·교체는0이다.

### 실제 Clippy defect와 승인된 두 줄 수정

Git6c `export.rs:492`의 `StreamReply`가 전체264B, Final variant 최소257B,
두 번째 Kernel variant41B라서 `-D warnings`의 `large_enum_variant`에 걸렸다. 새 registry가
Plan 크기를 늘린 실제 source defect다. metadata unit/type 오류나 poison/order7 실패가 아니다.

부모가 승인한 수정은 `export.rs`의 `Final.plan: Box<Plan>`과 생성자의
`plan: Box::new(plan)` 두 곳뿐이다. Git6c blob에서 이 두 replacement 외 모든 bytes가 같음을
확인했다. serde_core1.0.229 `ser/impls.rs:460,489`의 Box Serialize는
`(**self).serialize(serializer)`에 직접 위임한다. 기존 enum/field tag·Plan Serialize를 바꾸지
않으므로 v1/v2 JSON 내용·필드 순서·wire bytes 보존은 이 원문으로 확인했다. 실제 새 source
serialization/Clippy 수용은 다음 부모 GHA가 필요하다. allow/lint 완화는 추가하지 않았다.

수정 후 `export.rs` SHA256은
`e080c12280b8806f56ede77b297634d69ac9c0071dc1651ed22afc556e499f52`다.
caller14/ignored2·library2 및 모든 기존 selector/count는 그대로다. 부모는 기존 source pin의
export.rs SHA만 새 coherent source에 반영한다. 로컬은 rustfmt check·정확 두 줄 diff와 Serde
원문 대조뿐이며 Cargo/type/build/test/Clippy/compiler/model/oracle/CI 실행·stage/commit/push0이다.
이 실제 기능 성공을 strict Clippy까지 완료된 F02 전체 수용이나 전체 모델 수용으로 올리지 않는다.

## 2026-10-26 source57 최종 genuine PASS와 source69 cohort

[source57e1761/run40256938672/job121360705285](https://github.com/bin9208/openpilot-rust/actions/runs/40256938672/job/121360705285)은
completed **SUCCESS**다. 위 Git6c의 실패 이력은 보존하며, Box 수정 이후의 이 결과를 따로 기록한다.
library registry-budget2, caller14/ignored2, COPY1, genuine mixed1/order7, strict Clippy의
`exit-codes.txt` 값이 모두0이다. mixed는88.01초, Clippy는7.61초다. `-D warnings`를 유지하고
기존 selector/count·원본 expected를 수정하지 않은 실제 결과다.

F02 source SHA는 Python `b4d51d33569eaf28e734b90c540e572d8e72abf88389e4217065a9fb0fdfe7f0`,
export.rs `e080c12280b8806f56ede77b297634d69ac9c0071dc1651ed22afc556e499f52`,
caller `275357ea69c62d471ae117847aa43abcc79a20a2319f06391249cf54beff74b4`다.
Git57은 Git6c에서 승인된 Final Box 두 줄을 포함한다. `Box<T>`의 Serde 직접 위임 원문과
기존 v1/v2 deterministic CLI·stream/EOF·형식·원본 비교 검사 PASS를 함께 확인했다.
서로 다른 run의 session/PROGRAM handle 가변값을 포함한 frame 전체를 동일 bytes라고 주장하지 않는다.

새 ZIP 두 개를 API expected bytes/SHA admission 후 각각1회 받고 safe unique/nosymlink/전체 CRC를
확인했다. 과거 artifact는 다시 다운로드하지 않았다.

| artifact | ZIP bytes / members / expanded | ZIP SHA256 |
| --- | --- | --- |
| 12622657937 functional | 17,995 / 26 / 43,216B | `9039b91a14bdcfd3d33405ca8e648e54f28c12c545238afc358463dded4c32e8` |
| 12622309014 private source/SDK proof | 338,191 / 34 / 2,581,595B | `150f480e6264e014baec1ddb042d009602aa24d89ebc55dfb33216378df764bc` |

functional 원문에서 prefix SHA `bc1b7b572c8dd2f84f3ea0ca38b1744e4c7120155c33f3dabb633eabaf7f0144`,
Kernel/binarySHA poison 및 활성화 음성 control, 첫 PROGRAM IR 대표, 원본3call/1kernel과 반복
native index `[0,0]`를 확인했다. raw 원본 graph2156B SHA
`a6ef4f632e3f405effe4c0bac11ad9abcd48dc018e0766c19eccbb4aefc788d1`,
weights64B `58dda328598e2f7fe472621bfc54935aaa354d1a6ebcaf9562cd743fd575eb19`,
SOURCE396B `4b68cdcb9c9f02dda5eb7ef71300444f3b0d050d4850d5d60f702a6a98c687f9`,
BINARY2240B `b2f550a3a3d46e3b3832494b46499427439fc5c5d18b4e09d400184c48912bd4`다.
원본/candidate IR record는123B·SHA
`f57d7d8c126159f932db88a8887cfbc7334133c75e418a866ab5f6a7ba55fdc3`로 동일하고,
LINEAR IR key `7110338e63766b5a4e7ea51a0ecdaf9ab75324cff0c76136e511aeb261dd27f1` 및
native 첫 program_key와 결합된다. allocations/views/entrypoints·signature·weights hex·geometry를
원본 raw와 candidate Plan 사이에서 독립 대조했다. 전체 candidate graph/SOURCE/BINARY 동일성은
Git57 실제 test assertion PASS이며 candidate 파일 자체가 별도로 ZIP에 있는 것은 아니다.

order7 원문은 original-first 순서와 lowering/compiler 각각 `[0,0,1,1,0,0,0]`를 유지한다.
앞5 실패는 양쪽 destination 부재, launch-before-read 우선순위를 유지하고 뒤2 lazy 양성은
성공한다. canonical Python은3.12.15이며 proof가 functional/private ZIP 사이에서 동일하다.

이번에는 private raw proof도 독립 대조했다. source hash·index, SDK full-file SHA manifest와
stat/type/link roster의 before/after가 각각 동일하다. index19,623개가 Git57 tree 경로와 같고,
SDK file227개·roster252개를 포함한다. private/functional ZIP의 모든 검사 stdout/stderr·exit 원문,
원본 prefix와 Python proof도 서로 같다. Rust1.94.0/host x86_64와 LLVM provider 후검증을 확인했다.
API step20 source/SDK/LLVM after와 step22 source/alias/target cleanup, step23 공통 temp cleanup은
실제 success다. 후봉인 원문을 재대조하지 않았던 앞선 Git6c 감사와 구분한다.

source57e1761→`69d5489463e22a9ef4245cc941a5788f795a8b99` Git diff는 workflow3·기록2개뿐이다.
`rust`, `tools`, `tinygrad_repo` tree OID가 각각 같으며 F02 source3/CLI, model helper/D writer의
blob·SHA도 동일하다. genuine WF 전체 blob은 같고 current model WF의 jobs는 동일하며 push paths에
`compiler_schedule/**`, `diagnostic_482_writer.rs` 두 개만 추가됐다. 따라서 위 기능 결과는
동일 기능 소스 cohort의 증거이며, source69의 전체 source seal 자체를 source57의 seal로 대체하지 않는다.

bounded production offline registry slice의 위 기능·strict Clippy·후봉인 gate는 실제 PASS다.
Python compiler/compile_cached/lowering/metadata 취득/writer 제거, 전체 모델·device 수용과
파일 퇴역 credit는 별도다. current model source57 run0과 source69 신규 run의 상태는 다음
model 문서에 구분 기록했다. private scratch 기록은 `ci/f02-57-genuine-audit.json`,
`ci/f02-57-private-seals-audit.json`, `ci/f02-57-69-source-cohort.json`이다.
본 감사의 source 기능 편집·로컬 Cargo/build/test/compiler/model/oracle·CI dispatch/rerun/cancel·
stage/commit/push는0회다.
