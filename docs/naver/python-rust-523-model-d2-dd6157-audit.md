# #591 NPY 쓰기 수용 및 DD D1/D2 signed-zero 조사 — 2026-10-26

## 정확한 실행과 조사 경계

- source: `dd6157c3984a73a59fb808b551cdab73f04dba35`.
- 실행: [run40212410219/job121226282705](https://github.com/bin9208/openpilot-rust/actions/runs/40212410219/job/121226282705), GitHub-hosted Linux x86_64, Microsoft hypervisor, AMD EPYC7763, logical CPU4개.
- artifact: `model-pr1344-replay-482-40212410219-1`, ID12606905172,27425271B/SHA256 `71d370b2638e719eb2973274c4f16622e2358930bcfdb4e18e8d6ab585b7bf85`. job 로그의 실제 upload ID/크기/digest와 받은 ZIP을 대조했다.
- private 보존본: `.analysis/scratch/2026-10-25-523-completion/ci/12606905172.zip`.
- 전체2098 member의 중복 이름·절대 경로/상위 경로/역슬래시·symlink 거부와 CRC 검사를 통과했다. expanded118335440B다. PKL/모델/원본 helper/개별 machine image/프로젝트 CLI를 실행하거나 decoder·oracle로 재계산하지 않고 ZIP/JSON/원문 bytes/소스만 읽었다.

job의 실패 원문은 `RuntimeError: D1 strict frozen failure remains sticky after diagnostic D2`다. source0e521의 NPY `NotImplementedError: need copyin`과 구분한다. 실제 로그 API의 `structuredContent.content` 본문을 확인했다. 같은 DD source로 CI를 재실행하지 않았고, 이후 SDK 새 실패/공유 CI scope는 부모 담당이다. 이번 기록은 코드·shared WF/Focus/pin/원장을 변경하지 않는다.

## #591에 전달할 실제 수용 증거

해당 실행 helper pin은51842B/SHA256 `2e8627ab5b71a86efaf815a35d64e443f38f8f97ec1d8d93b593790a519405ab`다. 이는 NPY만 `as_memoryview(force_zero_copy=True)[:] = raw`로 쓰는 수정이다. 원래 길이·입력·plain/tap 비교 및 strict 조건을 유지한 실제 source다. [직전 실패·최소 수정 기록](python-rust-523-model-npy-copyin.md)의 GHA UNRUN 상태를 이 실행의 아래 범위로 갱신할 증거를 확보했다. 이전 문서 원문은 수정하지 않았다.

| command | exit | 실제 elapsed 초 | 자식 회수 / cleanup error |
| --- | --- | --- | --- |
| current-isa-feature-tests | 0 | 0.117633 | true / null |
| triplet-readonly | 0 | 0.533351 | true / null |
| dual-first-packet | 0 | 12.512199 | true / null |
| original-plain | 0 | 4.014628 | true / null |
| native-plain | 0 | 0.844432 | true / null |
| original-tapped | 0 | 4.021886 | true / null |
| native-tapped | 0 | 0.844015 | true / null |

ISA unit11 passed/0 failed/0 ignored 및 A 전체91개 strict SOURCE/object/image·host ISA가 통과했다. 자연 AMD host의 census는 AVX/AVX2/BMI1/CMOV/FMA/SSE4A 등을 기록하며 AVX512_FP16은 없다. 따라서 다른 자연 Intel host의 FP16 실제 image 재수용으로 확대하지 않는다.

D2는 기존 prepare→prepare→policy의3stage, 동일 봉인 원문5입력과 전체6output을 사용했다. sequence798B/SHA256 `06a76950a9f1ec45c9212f869515d04556856cd46320ef871152aa24a8b082fd`다. original은 fresh runtime91개를 실제 매핑했고, original/native의 초기334 allocation bytes/SHA가 대응했다. 원본의 frozen BINARY fallback을 사용한 수용이 아니다. original/native의 plain/tapped 네 receipt가 아래 전체6개 output의 동일 크기/SHA를 기록한다.

| output | bytes | 네 mode의 동일 SHA256 |
| --- | --- | --- |
| big_img_q | 983040 | `ca02793076b8845454d3cc4d949bda5bc49fe16d063e310331f86318ed644ba7` |
| desire_q | 3200 | `5a312281df4bd8dfbb4d4a94ad0bf44d01bb8cfced1206b90e21b4ca0568cdb1` |
| feat_q | 196608 | `3381de4ca9f3a477f25989dfc8b744e7916046b7aa369f61a9a2f7dc0963ec9e` |
| img_q | 983040 | `ca02793076b8845454d3cc4d949bda5bc49fe16d063e310331f86318ed644ba7` |
| model | 10304 | `069099e1d4d8081ed9de155494cd18dd00cf2d8be3e12cfe04be08d2e6857f87` |
| warped | 393216 | `a6619f482fee91a315f76cdcd8705d39b6ce11077c435ccc696142e130c27762` |

한 mode의 전체 출력 합은2569408B다. artifact에 별도 보존된 original/plain·original/tapped·native/plain의18파일을 모두 읽어 raw equality·크기·SHA를 확인했다. native/tapped는 별도 raw6파일을 쓰지 않으며, 실행 중 기존 native/plain 원문6개와 직접 byte 비교하는 private test의 실제 성공 및 receipt6개를 확인했다. 따라서 보존 raw24파일이 있다고 쓰지 않는다. 두 tapped 모드 모두 worker0/1 각1회, 동일 writer 함수를 한 번씩 forward했고 backend별 plain/tapped 전체 bytes도 같다.

`comparison.json`868B/SHA256 `89e027f14d7911ab9dd65a0d4fdece0cbdd94a5411ebe59e828c62a863de5800`는 diagnostic_complete=true/all_outputs_original_native_equal=true/output_mismatches=[]/plain_tapped_bytes_equal=true/d1_packet_equals_direct_native=true를 기록한다. `original-tapped.json`59629B/SHA256 `dc45e51aacab765dca7b3c5886f4ffce2ae4fca55c70ebd95fb1f6c5491879f8`, `native-tapped.json`1986B/SHA256 `379c8c1e7612892551db491ce14ecd856f0b60699fb48e3da48e3c75dc96819d`다.

### source/SDK 전후 봉인과 회수

아래6쌍은 ZIP에서 직접 읽어 전체 bytes equality를 확인했다. source-before/after는 동일 SHA 목록을 담은68B 파일이며, 아래 SHA는 그 파일 자체의 digest다.

| 전후 증거 쌍 | 각 bytes | 동일 파일 SHA256 |
| --- | --- | --- |
| source-before.sha256 / source-after.sha256 | 68 | `e20aa45155831bbd9c26e935cf6becad16f1221f5e1cfe33823246d5a894ad85` |
| index-before.bin / index-after.bin | 1172219 | `bb5c420f3b483ff499f7037aee2fe2f9db35cbc073eccb34e7dc110954b6ef16` |
| prepared-rust-files.before.sha256 / prepared-rust-files.after.sha256 | 32244 | `bd6ca4a2d7dd1ff9cbb0cc27a55c0e124ce934a23e21e715a85983d0fee6d8c4` |
| prepared-rust-programs.sha256 / prepared-rust-programs.after.sha256 | 634 | `13a808bdfff4aa69af27142bee4e664549a4c17e625c5b45fc3a5d72589cc812` |
| prepared-rust-roster.before.bin / prepared-rust-roster.after.bin | 37509 | `196ffbcd86547f57b43ea8ae5a3ae1f226ea75c717e4b1098992885820d3cf1c` |
| prepared-rust-rejected.before.bin / prepared-rust-rejected.after.bin | 0 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |

job step13 source/index、step14 whole private Rust SDK、step15 independent immutable source/SDK/providers/child recovery는 모두 success다. internal after snapshot은 source19514/SDK188/venv2037/Python-base9398/registry17031/fixture1794/current-catalog1128/held1592 entry이며 final verifier가 실패0을 기록했다. 이를 whole PID/모든 DSO·FD 폐쇄 증거로 확대하지 않는다.

`final-seals.json`1190B/SHA256 `df3e5972793cd6a9d80894a0e649e1aaf4bc0939fe90330d1212fae470b9b985`는 source DD, phase=native-tapped, pass=true/failures=[]/all_owned_children_reaped=true다. retained-index는1558 record/74115670B, model_pkl_uploaded=false/full_weights_uploaded=false다. 실제 artifact upload 및 조건부 source/temp cleanup도 성공했다. 최종 qualification step16은 D1 sticky 때문에 failure를 유지했다.

따라서 #591의 한정 완료 조건인 NPY 입력 쓰기 오류 해소·네 mode 전체6출력 비교·source/SDK 봉인·회수·CRC 증거는 확보했다. 이슈 갱신/종료는 부모 담당이다. 이 판정은 frozen expected 수치 해결이나 전체 모델/장치 수용이 아니다.

## D1 별도 실패와 실제 writer 전후 bits

D1 receipt822B/SHA256 `56fcf1c1ece305b397474caf84c88231ea4d842f03b09706c15c18dd5f138a91`는 frame3/전체2576raw 중 index99 한 개의 actual bits `00000000`(+0)/frozen bits `80000000`(-0), strict_pass=false/strict_failure_sticky=true를 기록한다. D1 packet19152B/SHA256 `b967780145aa5fa8a924e1905dff4ddc442051ffe972d58927ad5d4e8994bc51`이며 decoded raw SHA는 위 model 출력과 같다. frozen raw SHA는 `9afb2defff4ff29e5bf70e54db32d0d531d5e2b44507c8de652ee2c0dfcb67fe`다. 첫 raw 거부 뒤 recursive fields 및 정상12frame/33publication 완료를 주장하지 않는다.

보존 raw를 단순 LE byte 비교한 결과도 frozen과 세 current raw의 차이는 index99 하나뿐이다. native-tapped는 실제 실행 중 같은 native/plain bytes와의 비교를 통과했다.

| 관측 위치 | index99 LE bytes | 의미 |
| --- | --- | --- |
| frozen-model.bin | `00000080` | frozen -0 |
| current original/plain 최종 | `00000000` | fresh original +0 |
| current original/tapped 최종 | `00000000` | tapped도 +0 |
| current native/plain 및 tapped 검증 | `00000000` | native도 +0 |
| original/tapped worker0 writer 진입 / 반환 | `00000080` → `00000000` | 출력 storage의 -0가 writer 뒤 +0로 덮임 |
| native/tapped worker0 writer 진입 / 반환 | `00000080` → `00000000` | 같은 writer 경계 전후값 |

worker1의 index/before/after는 null이며 worker0 원소를 관측했다고 쓰지 않는다. 원본 writer의 fresh image SHA는 `bd5c40e153a7ef23d17142a5cc660c21be5126d9f5c67f3736f42313410d13a8`이고 실제 source/function/mapped bytes·ABI에 결합된다. 이 호스트의 image를 앞선 Intel image와 같은 것으로 취급하지 않는다.

중요한 구분은 **writer 진입의 -0는 data0 출력 버퍼 값이며 data1 half[99]가 아니라는 점**이다. native graph는 model allocation9를 참조하는 다른 call이 없음을 검증한다. initial allocation9의 SHA는 `873f43e01bf29b069f4c87b7d524dba021a6f6c517e4182d944efc6d66f96153`으로 frozen 전체 raw SHA와 다르다. 따라서 before 값은 직전 half producer 산술 결과로 읽을 수 없다. 두 tap event에는 half/operand/SSA 값 필드가 없고 pointer 주소만 있으며, 종료된 프로세스의 주소는 내용 증거가 아니다.

## kernel67 half → kernel92 lane3의 정적 연결과 남은 관측

같은 DD graph의 마지막 call175는 kernel92, views `[9,365,417,421,422,423,420,424,425,426]`,10pointer/1scalar/2worker/core_id0이다. data0=model view9/allocation9/offset0/10304B, data1=view365/allocation16/offset9472/3152B다. worker0·outer24·lane3의 정적 index 역투영은 data1[99]의 half load `%v189` → bit 처리 `%v682` → `%v694` lane3 → `%v2650` → `%v3274` → model[99] store다. `%v182`의 index 조건은24<394이다.

- kernel92 원문166049B/SHA256 `e223a2c36c6bde7cf676bb92fd2529858a723f9ecfa91c811b2b941756a9bea0`: 해당 원문 마지막 식은 `fadd nsz arcp contract afn float %v2650, %v3273`이고 해당 select arm은 명시적+0이다.
- native compiled sidecar166727B/SHA256 `ffc2206baa1f05f49a5c9d07b68de71da1a1437485da87d8e4a75d0692a31b52`: 기존 rewrite4개 중 lane3은 `%diagnostic_guarded_sum_3.zero = fadd float %v2650, 0.0`를 조건 선택한다. receipt1390B/SHA256 `26aa536b60e78568081d43f9095c1a90634f4ea31e1ee6c24246620e9e4e5530`다.
- data1[99]의 allocation16 byte 구간은9670..9672다. 마지막 선행 producer 후보는 call147/kernel67의 data0/view365다. call148/kernel68은 이 view를 slot3 입력으로 읽고 다른 data0에만 store한다. kernel67 SOURCE217245B/SHA256 `2d46595564d66783b4846a87bf02247e200ba3dec4a6ea03b02204103064e948`, kernel68 SOURCE9990B/SHA256 `79e46f7d94d2bba9d04e995d8e43ba7511ed9f60e1c60fcdcf6efb210fa247f2`다.

이 연결은 소스에서 읽은 경로이며 이번 DD의 실제 half[99] bit나 `%v2650` 값을 읽은 것이 아니다. artifact에는 initial allocation의 크기/SHA와 최종6output·writer 출력 전후값이 있지만 runtime allocation16 또는 위 half2B dump가 없다. retained triplet `.bin`은 실행 image이며 tensor operand로 해석하지 않는다. 양쪽 current fresh original/native가 같은 +0를 내므로 현재 증거는 Rust만의 산술 불일치를 보여주지 않는다. 그렇다고 frozen의 역사 provider/compile/state 원인을 확정하거나 정상 expected를 current +0로 교체할 근거도 아니다.

**현 증거만으로 정당화되는 산술 수정은 없다.** 필요할 경우 다음 새 관측의 최소 제안 scope는 `tools/ci/model_pr1344_replay_482.py::original_replay`의 private tapped writer callback과 `rust/crates/model-runtime/src/cpu/diagnostic_482_pr1344.rs`의 private writer99/state다. 두 쪽 모두 authenticated view365/길이/실제 arg1 pointer를 검증한 뒤 writer 진입의 data1[99] raw2B만 기록하면, 같은 입력 half를 소비했는지와 부호를 먼저 구분할 수 있다. 현 snapshot/전체6output plain-tap parity·bounds/후검증·strict sticky를 그대로 요구해야 한다. 이는 다음 관측 범위 제안이며 구현·등록·CI를 수행하지 않았다. kernel67 내부 최초 producer까지 필요하면 그 실제2B 관측을 확인한 뒤 범위를 다시 정해야 하며 이번에 instrumentation을 확대하지 않는다.

kernel92를 copy로 바꾸기, guarded-add 제거, zero 정규화, expected/tolerance/CPU/ISA/timeout 변경은 하지 않는다. historical half/sNaN5와 실제 DD 입력을 합치지 않는다. full128/full-model runtime/historical provider/device acceptance 및 retirement credit은 여전히 false/0이며, source DD의 diagnostic_complete=true를 전체 PASS로 승격하지 않는다.

## 이번 작업의 쓰기와 검증

지정 completion worktree에서 이 새 문서만 작성했다. 기존 NPY 문서·모델/테스트/helper 코드·shared WF/Focus/ledger·SDK 파일은 수정하지 않았다. 로컬은 inert ZIP CRC/원문 byte 비교·소스 읽기·문서 diff 검사만 수행했으며 model/oracle/compiler/Cargo/test/Clippy/decoder 실행0, CI 재실행0이다. 작성자의 관측을 별도 독립 peer 검토로 표기하지 않는다. 부모가 #591 증거 반영과 후속 관측의 scope/이슈/등록을 관리한다.

## kernel92 writer 입력 half[99] 최소 관측 후보 — 2026-10-26, GHA UNRUN

후속 구현은 `tools/ci/model_pr1344_replay_482.py::original_replay`와 private `rust/crates/model-runtime/src/cpu/diagnostic_482_pr1344.rs::writer99`의 기존 tap에만 적용했다. 실제 읽기는 **기존 worker0의 kernel92 진입에서 data1[99]의2B 한 번**이다. 기존 data0 output99의 진입/반환4B와 worker0/1 관측·실함수 단일 forward·복구를 유지한다. kernel67→view365 연결은 위 정적 provenance로 남기며 kernel67 hook/worker-after 읽기/추가 barrier를 만들지 않았다.

원본 tap은 이미 구성된 기존 PROGRAM 호출의 Buffer 참조만 보관한다. A로 인증된 단일 writer SOURCE/object/image·launch/global arg roster를 재결합하고, 실제 Buffer의 dtype.base=float16/itemsize2/nbytes3152, view offset9472와 전체 backing allocation 크기를 확인한다. 호출 시에는 각 arg pointer를 **이미 존재하는** 실행 device의 base `_bufs` 항목 주소+해당 offset과 대조하고 범위를 검사한다. 새 `ensure_allocated`/`get_buf`/`allocate`/map 요청이나 동기화는 없다. 별도 view가 lazy 상태여도 강제로 만들지 않으며 live base mapping이 없거나 포인터/범위가 다르면 `half99 unknown` 진단 실패를 기존 stderr에 남긴다. 실제2B는 `args[1] + 99*2`에서만 읽는다.

native는 이미 로드된 model Buffer의 `view_ptr` 범위 검사와 실제 Invocation의10pointer를 전부 대조한다. 이 accessor는 기존 allocation의 주소를 반환하며 새 storage를 만들지 않는다. State에는 검증된10pointer와 `Option<[u8;2]>`만 추가했다. callback에서 실제10pointer·worker를 다시 확인하고 worker0일 때만2B를 읽는다. 기존 null 인자 거부 probe는 읽기 전에 거부되며 기존 복구·실행 횟수는 바뀌지 않는다. 미관측·반복 worker0·잘못된 binding은 unknown 진단 실패이며 추정값을 채우지 않는다.

native Buffer 자체에는 runtime tensor dtype가 없다. 그래서 native receipt의 `actual_dtype`는 null이고, 별도 `dtype_evidence`에 held original LLVM `i16*` 인자와 고정 half 변환 SOURCE 근거를 명시한다. 원본 receipt는 실제 tinygrad dtype 문자열을 기록한다. 두 쪽 모두 `storage_dtype=i16`, element_bytes2, 실제 view/offset/nbytes와 source/object identity를 확인하며 native에 동적 dtype 탐지가 있다고 주장하지 않는다. original의 image SHA는 실제 fresh mapped image이고, native는 A의 `original_image_sha256`와 실제 로드한 `native_library_sha256`·function address를 구분해 기록한다. 관련 held source/object/image 및 기존 library의 사후 검증은 유지한다.

새 `half_observation` 필드는 tapped receipt에만 추가한다. worker0/phase=writer-before/index99, arg_slot1, graph call/kernel/view/allocation/offset/bytes, actual_nbytes, allocation_byte_offset, 실제 buffer/element 주소 및 위 identity 근거를 담는다. 원본 값은 `input_le_hex`4자리, native는 `input_le_bytes`2개다. `operand_file=null`은 이 관측이 live 호출 버퍼를 읽고 operand file을 만들거나 fallback으로 읽지 않는다는 뜻이며 파일시스템 전체 부재 스캔을 뜻하지 않는다. 기존 output99 전후값과 pointer_args는 원래 events에 그대로 남는다.

controller는 두 tap의 상태·graph/범위/source/object/A image 대응을 확인한 뒤 comparison에 `half99_original_le_hex`, `half99_native_le_hex`, `half99_input_equal`만 추가한다. 값의 동일 여부는 raw2B 비교 결과이며 **어떤 half bits도 기대값으로 고정하지 않는다**. `half99_input_equal=false`도 관측 사실로 기록한다. D1의 frozen -0 expected와 strict sticky는 그대로이며, current original/native가 같은 값을 냈다는 이유로 frozen expected를 교체·무효화하지 않는다. kernel67의 실제 출력/최초 산술 producer를 관측했다고 쓰지 않고 `ssa_first_producer_observed=false`도 유지한다.

plain 모드의 원본/네이티브 receipt에는 새 필드를 넣지 않는다. 원래 prepare→prepare→policy3stage·전체6output plain/tap byte 비교·raw2576 비교·strict 실패 순서·180초/global/기존 caps·CLI/상수·수학·원본 PROGRAM·native C++/allocator는 보존한다. 관측 code를 제외하면 helper 기존51842B/SHA256 `2e8627ab5b71a86efaf815a35d64e443f38f8f97ec1d8d93b593790a519405ab`와 native leaf14684B/SHA256 `132bbfb8ef3e2881cbe217002512d687ce5c04fbc5e65ba670087b6f82eb673c`의 전체 bytes가 정확히 복원됨을 source-only 역투영으로 확인했다. Python AST도 기존 전체로 복원되며 execute/previous_exec/ensure_allocated 및 native 실함수/run_entry 호출 수·외부 callback 수는 동일하다. Python AST 구문·rustfmt1.94·diff 공백 확인을 수행했다.

현재 candidate helper는57130B/SHA256 `f91b77dbc23b4d7fa6c12a13ff2be0390130944ae4f80d0c850e7d73be104855`, native leaf는18813B/SHA256 `3ac762795a793e29f613aec53993058032efc92c86df09c1116722ccf78cd283`다. 기능·컴파일·Clippy·새 실제 half bits·관측 전후 parity는 **GHA UNRUN**이다. 로컬 functional/model/oracle/decoder/프로젝트 CLI 실행 및 CI 재실행은0이다. 부모가 helper pin/Focus/공유 등록·SDK/data wiring·Actions를 관리하며 이 작업자는 두 소스와 이 D2 문서 외 파일을 쓰지 않았다. #591 종료와 이 새 관측의 수용을 구분하고 산술 수정은 새 실제 half 증거를 받은 뒤 원인/scope를 먼저 제시한다.
