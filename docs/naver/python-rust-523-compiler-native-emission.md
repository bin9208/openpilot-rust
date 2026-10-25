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
