# #523 F02 다음 offline bookkeeping 소비자 전환 제안

작성: 2026-10-26. 소스 읽기 기준 HEAD: `7a82b4e21283cf540f57c88689f58b2e8aca9f07`.
상태: **편집 전 설계 조사만 완료. production/test 변경0, 로컬 기능 실행0, 새 retirement credit0**.
이 문서는 기존 [native caller 연결 기록](python-rust-523-compiler-export-caller.md)과
[buffer 179건 collector 기록](python-rust-523-buffer-original-corpus.md)의 다음 경계를 제안한다.
실제 모델의 wire/resource fit, 전체 QCOM export/실행 수용은 계속 미완료다.

## 결론과 수용 단위

다음 구현은 기존 `offline_schedule.emit_schedule`에서 실제 storage/event와 call 참조만
기록하고, native의 allocations/views/weights/launches/entrypoints로 Manifest를 한 번 만드는
변경이다. Python `OfflineBufferTable` 인스턴스, alias/range/weights 계산,
`launch_dimensions`, entrypoint 투영과 native 대조용 Python 결과 계산을 제거할 수 있다.
알고리즘을 다른 Python 파일이나 forwarding 함수로 옮기지 않는다.

현재 v1 요청만으로도 table/launch와 emitter의 entrypoint 투영은 제거 가능하다. 그러나
`export_pipeline:328–333`의 `retained` 집합과 stage end 합산은 남는다. 따라서 **stage 계산까지
포함한 최종 최소 소비자 전환**에는 원래 stage CALL identity와 pruning 후 CALL identity를
넘기는 작은 wire 확장이 필요하다. v1만 소비하도록 바꾼 중간 상태를 stage 계산 퇴역이라고
등록하면 안 된다. Tensor/UOp lowering, constant 실행, memory planner 자체는 이 범위 밖이다.

파일 단위로는 두 원본 모두 남는다. `offline_buffers.py`의 `descriptor`는 constant 준비와
emitter에 필요하고, `buffers.py`는 CPU/live QCOM/AMD 및 독립 원본 관측 소비자가 사용한다.
검증 후 인정할 수 있는 것은 **offline table/launch/stage 알고리즘의 실제 caller 전환**이며,
두 파일 전체 삭제 credit는0이다. 현재 조사 자체의 기능 수용도0이다.

## 현재 실제 경로와 중복 계산

`build_offline_model_pipelines.py → offline_publish.emit → offline_pipeline.export_pipeline`
재수출 → `offline_schedule.export_pipeline → emit_schedule`가 실제 offline 경로다.
`export_model`은 같은 emitter를 쓰는 별도 모델/검사 진입점이다.

| 현재 경계 | 확인 위치와 동작 | 다음 구현의 소유자 |
| --- | --- | --- |
| table 생성/상태 | `offline_schedule.py:159–188, 200–206, 225–243`; Python view/read/write/weights와 event를 함께 수행 | 실제 metadata/event 취득은 Python, alias/range/packing은 기존 native core만 |
| launch 계산 | 같은 파일 `88–114, 224, 236–239`; 원래 ProgramInfo와 계산 결과를 모두 보유 | 원래 tagged 인자/source/binary만 전송, native 결과만 call에 사용 |
| stage retained/end | 같은 파일 `325–335`; pruning 후 set membership으로 stage별 end 합산 | raw source identity 전달 후 기존 `pipeline_entrypoints`가 계산 |
| entrypoint 투영 | 같은 파일 `244–256`; prefix를 이용한 Python entries/version/직렬화 | native entrypoints/version만 소비 |
| 이중 대조/실제 writer | 같은 파일 `264–312`; Python expected 대조 후 native 결과로 교체하여 파일 작성 | 독립 관측을 먼저 보존하고 이중 계산 제거, wire 검증 후 기존 writer 한 번 |
| native 조합 | `compiler_schedule/export.rs:167–288` | 기존 `compiler_buffers`, `compiler_launch`, `compiler_schedule` 재사용 |

native는 이미 실제 weights/manifest를 공급한다. 다음 변경의 목적은 그 앞 Python 알고리즘
실행을 없애는 것이다. 단순히 비교 조건만 삭제하고 Python table을 계속 실행하면 미완성이다.

## 최소 emitter 변경 계약

1. `view(buffer)`는 event를 append한 **위치 token**만 반환한다. input/output Binding,
   COPY source/destination, kernel argument는 token을 보관하고, 응답 후
   `native.event_views[token]`으로 최종 view index를 얻는다. token을 그대로 view index로
   쓰거나 Python view interning을 다시 만들면 안 된다. read/write도 event append만 한다.
   native는 모든 event에서 먼저 `table.view`를 수행하므로 반복 event와 view index는 다르다.
2. 실제 descriptor/initializer 객체를 native 응답과 writer 완료까지 강하게 참조한다.
   기존 Python table의 `bases`는 base 객체 수명을 유지한다. 이를 삭제하고 숫자 `id`만
   남기면 객체 수명에 따라 identity 재사용을 허용할 수 있다. 단순 reference 보관은 가능하나
   allocation index/range/zero image를 계산하는 두 번째 table은 만들지 않는다.
3. initializer는 기존 `{descriptor(item.buffer): item.data ...}`의 객체 key와 마지막 값
   의미를 유지한다. wire key는 `id(mapping_key)`이고 storage identity는 `id(buffer.base)`다.
   alias key를 base로 정규화하면 원래 lazy missing/wrong-identity 거부가 바뀐다.
   input의 `base in initialized`에 따른 read event 뒤 write event는 기존 metadata dispatch로
   유지할 수 있다. byte 길이/쓰기 구간/누락 initializer 판정은 native만 수행한다.
4. event 순서는 input view → 필요 시 initialized read → input write, output view,
   각 COPY의 source read → destination write → source/destination view,
   각 kernel의 `info.ins` read → `info.outs` write → `info.globals` 순서 view,
   마지막 output read를 그대로 유지한다. 먼저 모든 view를 수집하거나 event를 정렬하면
   allocation encounter order와 raw weights가 달라질 수 있다.
5. genuine `to_program`/render/compiler와 기존 `Kernel(function_name, binary SHA, byte 수,
   arguments)` 동등성에 의한 dedup을 유지한다. dedup된 source/binary/program roster의
   순서와 **모든 실제 kernel call마다 하나인** launch row를 구별한다. COPY도 emitted prefix에
   포함한다. 같은 kernel을 두 번 호출했다고 call/launch를 하나로 줄이면 안 된다.
6. pending call에는 kernel index와 argument token 또는 COPY token만 보관한다.
   native 응답 뒤 Binding/CopyCall/KernelCall/Allocation/View/Entrypoint와 Manifest를 만든다.
   `table.weights`, `view_checks`, Python launch 계산, `entries/start`, 최초 expected Manifest,
   `expected_launches/equal`을 없앤다. native 값으로 expected를 만들어 비교하는 대체도 금지한다.
7. `.command.argv + --export-plan`과 기존 timeout을 사용한다. QEMU/loader/library argv를
   다시 조립하지 않는다. 일반 source→genuine binary CLI 모드와 provider/stage는 유지한다.
   응답 검증 전 destination을 만들지 않으며, 실패 시 Python 계산/다른 compiler fallback은 없다.
   kernel 파일/weights/graph의 기존 이름·순서·`global_`→`global`·indent/newline 규칙도 유지한다.

현재 Python 응답 검사는 top-level key만 strict하고, 중첩 값의 의미는 Python 대조가 상당 부분
담당한다. 대조 제거와 함께 같은 기존 함수 안에서 bounded wire decoding을 보완해야 한다.
정확한 field/tag/type, 정수 index의 bool 배제, roster/event/launch 수, token/index 범위,
allocation/view/weight 범위, hex와 weights digest 일치를 확인한다. 이는 응답 내부 무결성
검사이며 독립 원본 parity 증거가 아니다. alias interning/range subtraction/packing이나
stage membership/launch regrouping을 Python validator에서 재계산하면 안 된다.

explicit local은 원본/native 모두 global/local을 계산 없이 반환한다. 일반 decoder에
무조건 `3개 양의 정수` 조건을 넣으면 기존 explicit int/float/bool 계약을 바꾼다.
integer decimal string, 유한 f64의 bits, boolean 태그를 구별하고 기존 `float_roundtrip`을
유지한다. inferred local의 정확한 int/range/register/source 판정은 native launch core가 맡는다.
device 문자열도 정규화하지 않는다. 원본의 allocated 거부는 정확히 `QCOM`일 때이며,
`QCOM:0` 등 suffix 처리를 이번 작업에서 바꾸지 않는다.

## stage 계산까지 닫는 wire 변경

현재 `export.rs:250–266`은 v1의 Python `stage_ends`를 이용해 synthetic `0..N` source identity와
stage slices를 재구성한다. 기존 native `compiler_schedule::pipeline_entrypoints`는 이미
`PipelineStage { name, calls }`, `retained_calls`, 실제 `emitted_by_source`를 받는다.
새 알고리즘 없이 이 함수에 genuine identity를 연결하는 것이 최소 확장이다.

- 제안하는 strict 요청 v2는 기존 initialized/events/programs/launches/prefix에 더해
  `stages: [{name, calls: [원래 stage의 CALL object id...]}]`와
  `retained_calls: [prepare_constants 결과의 CALL object id...]`를 받는다. `stage_ends`는
  새 버전에서 받지 않는다. Python의 set/count/end 합산과 Rust의 synthetic stage 복원을 제거한다.
- UOp는 현재 `@dataclass(eq=False)`이며 원래 membership은 객체 identity다.
  stage/prepared CALL 참조를 살아 있게 두고 `id(call)`을 그대로 보낸다. content hash,
  kernel id, dedup된 call 목록으로 대체하지 않는다. 중복 CALL occurrence와 빈 stage를 유지한다.
- retained identity는 **memory planning 전** `prepared.linear.src`에서 취득한다.
  `plan_linear`는 buffer를 SLICE로 substitute하므로 이후 CALL id가 달라질 수 있다.
  pre/post-planning tuple을 순서대로 짝지어 각 prepared occurrence의 실제 emitted prefix를
  기록하고 길이 불일치를 거부한다. substitution의 위치/순서 보존은 새 bridge 검사로 확인한다.
  rewritten CALL id로 원래 stage를 조회하면 정상 call까지 pruning된 것으로 오인할 수 있다.
- source/retained/stage roster와 모든 stage CALL occurrence의 **총량**도 기존 count/message
  admission 안에 제한한다. stage별 제한만 두어 곱셈으로 무제한이 되지 않게 한다.
  새 wire stage type에 `deny_unknown_fields`를 적용한 뒤 기존 core 타입으로 변환한다.
  현재 core의 `PipelineStage` deserialize 속성을 전체적으로 바꿀 필요는 없다.
- 요청 wire version과 응답의 manifest version을 구별한다. 요청이 새 버전이어도 stage 없음은
  graph v1/entrypoints 생략, 이름 있는 빈 stage를 포함한 staged graph는 v2다.
  65,536 stage 허용·65,537 거부와 전체 emitted partition 검사는 그대로다.
- 유지되는 일반4 검사의 v1 request fixture와 COPY1의 `Emission.stage_ends` 사용도 함께
  전환해야 한다. 동일 의미/거부 사례를 새 request로 옮기고 test 수나 assertions를 줄이지 않는다.
  확인한 production `--export-plan` caller는 이 emitter 한 곳이다. 새 caller/ELF를 함께 stage하고
  구버전 요청을 추측해서 허용하거나 버전 오류 시 Python으로 돌아가는 경로를 추가하지 않는다.

v1 소비 전환을 먼저 나누어 구현할 수 있으나, 위 raw stage 연결이 끝나기 전에는
`export_pipeline` 계산 퇴역을 수용하지 않는다. 두 단계 모두 기존 같은 Python/Rust 파일의
좁은 수정이면 충분하며 새 Python 알고리즘 wrapper나 별도 CLI는 필요 없다.

## 두 원본 파일의 전체 확인된 잔여 소비자

아래는 production/import, 검사, maintained 원본 관측 스크립트를 포함한 저장소 source 검색이다.
`.analysis`의 private 과거 산출물을 새 caller로 세지 않는다. 기존 frozen fixture의 path 문자열도
실행 caller와 구별한다.

| 파일/소비자 | 실제 사용과 잔여 범위 |
| --- | --- |
| `offline_schedule.py` | 유일한 production `OfflineBufferTable` 생성자. 다음 변경에서 제거 가능. `descriptor`는 유지 |
| `offline_constants.py:13, 41–46` | descriptor로 원래 storage/byte 길이를 검사하고 genuine CPU constant transform을 준비. 별도 남는 caller |
| `tests/test_offline_qcom_constants.py:9` | offline_schedule의 descriptor 재수출 사용. 제거/이동 없이 유지 가능 |
| `capture.py:12, 18–…` → `compile.py:18` | BufferTable/host_buffer로 live CPU JIT capture, actual host read/copyout, aliases/weights. cached/fresh `export_cpu` 모두 잔여 |
| `model_export/__init__.py`, `publish.py:61–72`, `build_model_pipelines.py` | CPU export 공개/발행 경로. 같은 publish의 qcom branch는 아래 live QCOM 사용 |
| `check_model_reference.py:49`, `check_pipeline_reference.py:76`, `check_policy_reference.py:37`, `check_input_pipeline.py:45` | 실제 `export_cpu` 호출. offline 전환으로 사라지지 않음 |
| `qcom.py:17, 82, 99` | captured/live QCOM export의 BufferTable, 실제 storage read와 COPY/SLICE/PROGRAM 경로. offline Manifest/call dataclass도 이 모듈에서 import |
| `amd.py:18, 54, 61` | captured AMD/NULL compile pipeline의 BufferTable. `local.py:89, 94, 97`/`build_usbgpu_local.py`가 AMD 및 QCOM warp를 소비 |
| `tools/ci/driver_retained_program_476.py:373, 420, 593, 610` | 원본 driver collection/replay의 BufferTable/host_buffer. 독립 원본 경계이므로 candidate 결과로 치환 금지 |
| `tools/ci/model_pr1344_replay_482.py:169, 174, 284, 317` | 원본 driving collection/replay의 BufferTable/host_buffer. 계속 잔여 |
| `tools/ci/model_current_triplet_482.py:310, 316, 408–420` | host_buffer 직접 사용 및 `export_cpu`를 통한 잔여 table. 자체 수집 코드도 이번 변경 범위 밖 |
| `tests/test_offline_qcom_schedule.py:18, 34` | 원래 CPU memory planner 비교에서 두 BufferTable 사용. offline emitter 제거 후에도 유지해야 함 |
| `tests/test_model_export.py`, `test_model_pipeline.py`, `test_pipeline_export.py`, `test_qcom_export.py` | CPU/live QCOM 공개 경로 검사. 전역 buffers 제거 근거가 될 수 없음 |
| `model-runtime/tests/compiler_buffers.rs` | 원본 두 파일의 method AST를 읽는 original179. 원문 고정/검사 전환 전 class 삭제 시 gate 파손 |
| `tests/test_offline_qcom_launch.py` | 삭제할 Python launch 함수를 직접 호출. 기존 원본 4,411 입력 reader로 독립 coverage를 보존하고 소비자 종료를 명시해야 함 |
| `model-runtime/tests/compiler_schedule.rs:116–158` | 삭제할 retained/end/entries/version AST 문장을 읽는 original73. 별도 원문 고정 선행 필요 |
| `dev-tools/src/current_catalog_export.rs:21–33`, `tests/driving_daemon_retirement.rs:197–208` | current CPU exporter source roster에 buffers/capture/compile/qcom 등이 포함. 부모 source pin/closure에 남음 |

`schema.entrypoint_version`은 `compile.py`, `qcom.py`, `amd.py`, `pipeline.py::compose`도 쓴다.
offline emitter에서만 사용을 끝내며 공통 함수를 삭제하지 않는다. package `__init__`와
qcom dataclass import 때문에 offline 실행에서도 buffers 모듈 자체는 transitive import될 수 있다.
다음 수용 검사는 **offline table 생성/메서드 호출0**이며, 공통 모듈 import 부재나 file absent가 아니다.

fixture의 driver dependency reference manifest, model-family original loaded-module/source manifest,
manager-qa의 repo-python before/after SHA 목록은 역사 증거다. 현재 사용 중인 원본 관측 경로와
이 역사 증거를 모두 offline-only native 출력으로 덮어쓰면 독립성이 사라진다.

## 기존 증거 재사용과 삭제 전 선행 조건

| 기존 증거 | 재사용 가능 범위 | 추가로 필요한 것/현재 한계 |
| --- | --- | --- |
| native4 (`compiler_export_caller`) | alias/weights/launch/stage core 조합, lazy 거부, 실제 CLI JSON/argv 거부, explicit f64/int/bool wire 회귀 | 새 request/consumer에 맞춘 동일4 의미 보존과 새 source 재검증. Python event→Binding/call 조립과 genuine kernel 경로 전체 증거는 아님 |
| COPY1 ignored | 실제 Tinygrad COPY UOp → maintained emitter → 실제 CLI → writer, GPU open 거부, native 실패 시 destination 부재 | 변경된 실제 emitter로 다시 실행. kernel/COPY 혼합, dedup된 kernel의 여러 launch, initialized input 경계는 미포함 |
| original179 = descriptor32 + recipe147 | 두 원본 table/descriptor의 독립 CPU 관측. lazy alias/초기화/구간/원시 bytes 의미에 재사용 | 부모 감사 완료한 정확 request/raw stdout/stderr/rc와 source/Python 전후 proof가 있어야 frozen reader로 전환 가능. emitted call/stage/모델 fit를 입증하지 않음 |
| `original-launch.json` 4,411건 | 원래 launch 405건 + Unicode 경계4,006건. 원본 함수/body, genuine buffer_add fixture를 사용한 독립 expected | frozen fixture/source/binary hash 보존. 실제 emitter가 native launch 결과를 쓰는 bridge 검사는 별도 |
| original stage73 ignored | pruning subset/중복 CALL/빈 stage/확장 prefix/Unicode/invalid partition/65,536·65,537 경계 | 현재 raw stdout 보존 없음. 삭제할 Python AST를 읽으므로 원문 관측 고정과 data-only reader 전환이 선행되어야 함 |

부모 최신 인계: source7a82의 native6와 Clippy compile은 통과했으나 original1은 owner exact
assert에서 실패했다. 실제 job/owner는 `40230515032:1:native-units`이고 생산자는 `printf %s`로
EOL 없이 쓴다. collector의 불필요한 `\n` expected는 부모가 수정한다. **원본 프로세스0회**이므로
현재 실행을 179 comparison 실패나 raw capture 성공으로 기록하지 않는다. test 파일은 부모 소유이며
본 조사에서는 수정하지 않았다. 수정 source의 GHA 및 artifact 감사가 끝날 때까지 179 고정은 pending이다.

launch fixture의 원본 전체 파일 SHA는
`033881478518ada0e88276f9d22c66cea40287f5474a19326df859b6b7e3815a`, genuine binary SHA는
`fe72d39f60f599c761e5f40697380f1426c7d6ca963ae80f0232336175f12512`다.
원본 SHA는 source73의 offline_schedule 전체 파일과 일치하며 현재 launch 함수 body도 같다.
현재 전체 파일 SHA로 과거 provenance를 다시 쓰지 않는다. 4,411건의 기존 reader는
`compiler_corpus::original_launch_captures_preserve_all_4411_inputs_without_python`다.
같은 test target에 있는 다른 full-model 검사까지 이번 이유만으로 반복할 필요는 없다.

stage73은 기존 한 번의 original 프로세스에 전달한 request와 raw stdout/stderr/rc,
schema/schedule·실행 snippet·native/test·canonical Python·GitHub SHA 전후 증거를 보존하고
현재73 입력/모든 assertions를 고정해야 한다. 65,536/65,537 stage 사례가 있으므로 buffer의
record당1MiB 상한을 그대로 복사하면 안 된다. 예를 들어 record16MiB/총64MiB의 별도 bounded
관측을 검토할 수 있으나 이는 제안값이며 현재 raw 크기를 실측/수용한 값이 아니다.

현재 graph.json은 이미 native 결과를 소비한다. 이를 그대로 저장한 뒤 새 native 출력과
비교하는 것만으로 독립 Python writer expected를 만들 수 없다. 필요한 원본 값은 native 교체
전의 genuine Python allocations/views/weights/launch/entrypoints 또는 고정된 pre-bridge 원본
실행에서 얻어야 한다. 새 기대값을 native 결과로 채우거나 AST를 native forwarder로 바꾸지 않는다.
가능하면 기존 작은 actual export 검사에 관측을 붙여 원본/model/compiler 호출을 중복하지 않는다.

## 새 구현에 필요한 좁은 GHA 검사

1. **독립 관측 소비**: original179와 stage73의 감사된 정확 request/raw result를 data-only
   reader로 읽어 기존 native assertions/거부 count를 유지한다. descriptor32/recipe147,
   native buffer6/original1과 stage73의 의미를 삭제/성공 skip으로 줄이지 않는다.
   launch 4,411건과 일반 native4/실제 COPY1을 기존 정확 selector로 재사용한다.
2. **실제 새 consumer**: Python BufferTable 생성/메서드를 poison한 actual emitter 경로에서
   COPY1과 작은 kernel 경로를 통과시킨다. input/output/COPY/kernel token이 반복되어도 올바른
   native view를 가리켜야 한다. GPU open 거부와 provider 없는 plan-only CLI 계약을 유지한다.
   malformed native response/추가·누락 launch/나쁜 token/argv 실패는 destination 생성 전에 거부한다.
3. **혼합 순서**: COPY와 kernel 혼합, 같은 kernel의 두 call/두 launch, 서로 다른 동일내용 base,
   alias offset과 initializer alias key, write-covered/unused/wronglen
   initializer, initialized input read→write, raw all-zero/negative-zero를 작은 실제 bridge 입력으로
   확인한다. core의179 전체를 새 테스트에 복제하지 않고 token/metadata 전달 차이를 검사한다.
   고정 원본 expected로 graph/weights 및 기존 writer 직렬화 규칙을 비교한다.
4. **stage raw identity**: pruning 전 stage와 pruning 후 retained, memory-plan rewrite로 바뀐
   CALL 객체, 반복 identity와 빈 stage, COPY/복수 kernel emitted prefix를 actual pipeline bridge에
   연결한다. core73 통과만으로 이 metadata 취득을 검증했다고 주장하지 않는다.
5. **기존 genuine compiler 소비자**: staged `QCOM_COMPILER_COMMAND`로
   `test_offline_qcom_schedule.py::test_export_preserves_real_initialized_weights_when_model_is_lazy`와
   `::test_compiled_original_ir_produces_real_values_when_executed_by_cpu_interpreter`를 유지한다.
   raw compiler mode의 `test_offline_qcom_compiler.py` 두 계약도 보존한다.
   CPU interpreter의 작은 kernel 수치 일치는 전체 모델/QCOM 장치 수용과 구별한다.

새 요청은 기존 wire256MiB, event1,048,576, roster65,536, prefix65,537, distinct storage1GiB,
packed weights128MiB와 timeout/resource admission을 유지한다. hex만으로 raw의 두 배이고,
128MiB weights는 JSON metadata를 더하면256MiB를 초과한다. Python table 제거로 복제 zero image는
줄일 수 있어도 JSON/source/binary/initializer/response capture와 QEMU/compiler 메모리는 남는다.
현재 `capture_output=True`는 반환 뒤 stdout 길이를 검사하므로 실행 중 pipe 메모리 제한을
입증하지도 않는다. 실제 대표 모델의 request/response bytes, distinct storage/weights,
peak RSS와 시간은 기존 GHA 자원 안에서 별도 관측해야 한다. cap 증가/fallback으로 fit를
선언하지 않으며 전체 모델 wire fit와 QCOM 전체 수용은 계속 pending이다.

## 구현 순서와 부모 인계

먼저 부모 owner fix source에서179 관측을 완료·감사하고, 삭제 대상 stage73 및 필요한 원래
writer 값을 고정한다. 그 다음 기존 native core를 유지한 raw-stage wire/strict decoder와
event-token consumer를 같은 좁은 변경으로 연결하고, live AST 의존 검사들을 독립 frozen
reader로 전환한다. 마지막으로 Python offline table/launch/stage 계산을 제거하고 위 actual
consumer 검사를 새 source에서 수용한다. 원본 관측을 확보하기 전에 먼저 삭제하지 않는다.

예상 구현 파일은 기존 `offline_schedule.py`, `offline_buffers.py`의 class/import 부분,
`compiler_schedule/export.rs`와 직접 영향 받는 caller/corpus 검사들이다. `offline_constants.py`,
공통 `buffers.py`, CPU/live export와 native core 알고리즘은 이 최소 구현에서 그대로 남길 수 있다.
새 Python wrapper/module은 필요 없다. 부모는 새 request를 아는 동일 source ELF를 build/stage하고,
실제 caller/관측 selectors 및 source/SDK/Focus pins를 갱신한다. WF/Cargo/원장/commit/push는 부모
소유이며 이 문서에서 변경하지 않았다. inventory의 두 파일은 계속 pending으로 유지한다.

조사 시 확인한 SHA256:

| 소스 | SHA256 |
| --- | --- |
| `rust/tools/model_export/offline_schedule.py` | `bc1b7b572c8dd2f84f3ea0ca38b1744e4c7120155c33f3dabb633eabaf7f0144` |
| `rust/tools/model_export/offline_buffers.py` | `9ab6c8c777b04d374750779fabfa75bb2b2af64fb6f8ce3ff8edd7a14138de4f` |
| `rust/tools/model_export/buffers.py` | `9429df49f408cda89f5b862fc0d51982668234cc18cb28ca0e83fd3d1a5bc7de` |
| `rust/tools/model_export/offline_constants.py` | `95c2be29ea9c40ffc3fb3a5ff46eefb36ba0d2ff0509b5c34f7d09fc7fe90233` |
| `rust/crates/model-runtime/src/compiler_schedule/export.rs` | `12aeeb60b209f22990bc518c155aed1f82743a867f64b0bcdf1874ba4453ed3c` |
| `rust/crates/model-runtime/tests/compiler_export_caller.rs` | `a5f0a9288aaaf121cbeaabfcaa569ba3511301db43c290b918896bbd4c1e1c24` |

공유 WIP의 부모 owner fix와 다른 agent 변경은 위 snapshot 이후 달라질 수 있다.
이 조사의 로컬 확인은 source 검색/읽기, inert AST·JSON/provenance 확인, 문서 diff뿐이다.
Cargo/build/test/Clippy/project CLI/Python oracle/model/compiler/GHA 실행과 artifact 중복 다운로드는0회다.
