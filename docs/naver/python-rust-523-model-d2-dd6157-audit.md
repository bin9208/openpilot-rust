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

## 실제 half8000 수용과 zero 분기 차이 — source561f20d9f, 2026-10-26

위 후보의 UNRUN은 작성 당시 상태다. 이후 source `561f20d9f2a0d2986b7bf271fabab237b7b3d0e6`의 [run40216927059](https://github.com/bin9208/openpilot-rust/actions/runs/40216927059)에서 새2B 관측을 포함한 D2가 완료됐고, 마지막 실패는 다시 `D1 strict frozen failure remains sticky after diagnostic D2`다. SDK의 별도 후속 실패와 구분한다. 이 절에서는 모델 소스를 모두 읽기 전용으로 취급하고 이 문서에만 추가한다.

artifact12608182890은27434534B/SHA256 `b7a9337a893a1addaa97804d3bc25ca8da7dc23d6f066478a053f2dffb425780`이며 보존 경로는 `.analysis/scratch/2026-10-25-523-completion/ci/12608182890.zip`이다. 작성자는 ZIP bytes/SHA·central member2103개와 아래 작은 receipt/held 원문을 읽었다. safe unique path·nosymlink·전체 CRC 통과는 부모의 선행 독립 감사 결과를 전달받았으며 이번에 중복 실행하지 않았다. 전체 result JSON·tensor를 출력하지 않았다.

### 실제 호출 입력과 관측 영향

| 항목 | original-tapped | native-tapped |
| --- | --- | --- |
| 실제 half[99] raw2B, little endian | `0080` | `0080` |
| half bit 해석 | `0x8000`, -0 | `0x8000`, -0 |
| binding | call175/kernel92/arg1/worker0/view365 | 동일 |
| allocation/offset/nbytes | allocation16/9472/3152 | 동일 |
| element의 allocation byte 구간 | `[9670,9672)` | 동일 |
| 실제 buffer → element 주소 | `139797391213824` → `139797391214022` | `140098751268096` → `140098751268294` |
| 실제 dtype 기록 | `dtypes.half` | null, held LLVM `i16*`/half 변환 원문 근거를 별도 기록 |
| 기존 output99 전후4B, little endian | `00000080` → `00000000` | 동일 |

주소는 서로 다른 프로세스의 가상 주소이므로 양쪽 숫자 자체가 같다는 주장이 아니다. 각 호출 안에서 이미 존재하던 actual arg1과 live base+view offset/전체 nbytes를 대조했고 element는 정확히 arg1+198이다. 양쪽 `operand_file=null`, phase=writer-before/status=observed이며 실제 입력을 파일이나 frozen 값으로 대체하지 않았다. 실제 dtype가 없는 native Buffer에 동적 dtype 검증을 수행했다고 쓰지 않는다. 기존 events는 worker0/1 각1회, 합2개이며 새 읽기는 worker0 진입2B뿐이다.

공통 held identity는 SOURCE166049B/SHA256 `e223a2c36c6bde7cf676bb92fd2529858a723f9ecfa91c811b2b941756a9bea0`, original object21112B/SHA256 `b1833aa2ff3b84f90d6126091bddd870542c17743aa8ebc5c8870d3d70a8ad7a`, original image19248B/SHA256 `bd5c40e153a7ef23d17142a5cc660c21be5126d9f5c67f3736f42313410d13a8`다. native receipt는 이 original image의 대응과 별도로 실제 native library SHA256 `82b885f8e9036565f266846c4a3a2d4e43d386a096fdfaf6886668bf7e9e0cd7` 및 function address를 기록한다. native가 original machine image를 실행했다는 뜻이 아니다.

`model482/comparison.json`968B/SHA256 `0308a37fd017e07d524546e4e89194636d175339d043491a5909426e5ee05f33`은 `half99_input_equal=true`, `all_outputs_original_native_equal=true`, `plain_tapped_bytes_equal=true`, `d1_packet_equals_direct_native=true`, `d1_strict_failure_sticky=true`를 기록한다. 기존3stage·전체6output bytes 비교와 raw2576 검사가 유지됐으며 frozen와의 차이는 index99의 frozen `00000080` 대 current original/native `00000000`다. 이는 current port 전체6출력 동등성과 tap 영향 거부의 실제 성공이다. frozen raw 기준 통과를 뜻하지 않는다.

작은 receipt 자체의 SHA256은 original-tapped60428B `6a70133572fd0605d47796474fa8cc83a33e1d9c34f2f30607513f0307316688`, native-tapped3038B `85225f89070352c8f37f9f618742df6f09b8db6c858ad4fad2341a6ca414e784`, final-seals1190B `ad06557d81ea6318f7c3a7e610e658334de44f3c1d6450f7b7114e8869af5c6c`다. final-seals의 source는 위561f이며 phase=native-tapped/pass=true/failures=[]/all_owned_children_reaped=true다. 새 관측 수용·현재 original/native 동등성·D1 frozen 실패를 분리한다.

### 이2B로 확정되는 lane3 산술 경로

기존 정적 연결은 call147/kernel67의 data0/view365 → call148/kernel68의 입력slot3 → 마지막 call175/kernel92의 입력slot1이다. kernel68은 이 입력에 store하지 않는다. 이 producer 연결은 계속 정적 provenance이며 kernel67 실제 출력이나 모든 중간 SSA를 관측했다고 승격하지 않는다. 이제 소비자 진입의 실제2B가 있으므로 이 lane의 입력 부호를 알기 위해 kernel67 hook을 더할 필요는 없다.

kernel92의 worker0/outer24/lane3은 input index99 및 model output99다. held 원문은 half 입력 `%v189`의 magnitude/sign을 각각 `32767`/`32768`로 분리하고 float 비트로 복원한다. `0x8000`은 이 경로에서 float `0x80000000`(-0)이 된다. `%v182 = icmp slt ... 394`는 해당 row24에서 true이고 `%v3273`은 명시적 +0 arm을 선택한다. lane3의 마지막 원문 식은 다음과 같다.

```llvm
%v3273 = select i1 %v182, float 0.0, float %v3272
%v3274 = fadd nsz arcp contract afn float %v2650, %v3273
```

현재 native compiled sidecar SHA256 `ffc2206baa1f05f49a5c9d07b68de71da1a1437485da87d8e4a75d0692a31b52`의 rewrite4개 중 lane3은 다음과 같다. 기존 guard receipt SHA256 `26aa536b60e78568081d43f9095c1a90634f4ea31e1ee6c24246620e9e4e5530` 및 원래 복구/forward 경로를 그대로 읽었다.

```llvm
%diagnostic_guarded_sum_3 = fadd arcp contract afn float %v2650, %v3272
%diagnostic_guarded_sum_3.zero = fadd float %v2650, 0.0
%v3274 = select i1 %v182, float %diagnostic_guarded_sum_3.zero, float %diagnostic_guarded_sum_3
```

zero arm에는 `nsz`가 없고 함수에 전역 no-signed-zero attribute도 없다. 기본 round-to-nearest 환경에서 -0와 +0의 합은 +0다. 실제 양쪽 output-after +0는 이 식과 일치한다. 기존 output-before -0는 data0의 저장값이지 data1의 입력값을 대신하는 증거가 아니며, 이번2B가 입력 -0를 별도로 확정한다. 이 결과는 입력 copyin 오류나 양쪽의 다른 half 입력으로 설명할 필요가 없다.

### frozen BINARY와 일치하는 기존 image의 정적 명령 비교

새 artifact의 `model482/retained/fresh-runtime.json`에서 writer PROGRAM90/kernel92의 old BINARY SHA256은 `324697e4d28a400da3c521fea2f9140eea04d50f19e6771a96549f4b8a5a6767`이다. 이미 보존된 source `0e521792e8473be142d6c8bbc7aaaa9457e32a56`의 [run40207816680/job121212441629](https://github.com/bin9208/openpilot-rust/actions/runs/40207816680/job/121212441629), artifact12605181275의 `model482/retained/triplets/compile-90.bin`14442B가 **그 SHA와 정확히 같다**. 그 `.ll` SOURCE도 e223a…로 같고 대응 `.o`15936B의 SHA256은 `fbf737784835622e99706fc42357e9058bf9ce2bfbb648195453de9513d45ddd`다. 과거 archive는26780256B/SHA256 `bc00acb0b8a38fb2a3d4a5c692aa7ae5417f8d339cad0cc02f0e42af27a565b4`다.

따라서 비교 대상은 막연한 과거 CPU 후보가 아니라 현재 PKL의 해당 old BINARY와 전체 image bytes가 같은 실제 보존 image다. 다만 이는 frozen fixture 최초 생성 시의 compiler/provider/실행 입력 이력까지 증명하지 않는다. prior0e 실행은 NPY 쓰기에서 나중에 실패했으므로 그 실행을 original 3stage 성공으로 사용하지도 않는다.

두 ELF object를 `/usr/bin/objdump -dr -Mintel --no-show-raw-insn`으로 정적으로 읽었다. ZIP에서 읽은 object bytes를 anonymous memfd로 전달했으며 dlopen·machine image·모델·compiler 실행은 없다. 아래 주소는 ELF `.text` offset이다.

frozen BINARY와 일치하는 쪽은 `0x2996..0x29aa`의 half load에서 lane3을 구성하고, `0x248b..0x24d1`에서 magnitude/sign mask와 unpack/or로 변환한다. relocation이 가리키는 상수는 half magnitude `0x7fff`, sign `0x8000`이다. 마지막에는 다음 명령이 있다.

```text
26de cmp DWORD PTR [rsp+0x140],0x18a
26f4 cmovl eax,r11d
26f8 kmovd k1,eax
26fc vaddps xmm0,xmm2,xmm0
2700 vmovaps xmm0{k1},xmm2
2713 vmovaps [rax+rcx*4],xmm0
```

row24<394이면 mask가 최종 store 전에 변환된 입력 xmm2를 선택하므로 zero arm은 -0 비트를 복사한다. 현재 fresh original object b1833…는 `0x2dd6 vpandn`으로 선택된 가산항을 zero로 만든 뒤 `0x2dda vaddps xmm0,xmm0,xmm1`, `0x2dde vmovaps [rax+rcx*4],xmm0`를 수행하며 그 뒤 입력을 덮어 복사하는 선택이 없다. 즉 같은 SOURCE의 **zero arm을 입력 복사로 낮춘 경우와 실제 +0 덧셈으로 낮춘 경우**의 차이가 실제 held object에 있다. 동일한 실제 -0 입력에 대해 전자는 frozen -0를 설명하고 후자는 이번 +0 관측을 설명한다. 과거 image에 이번 tensor를 넣어 실행했다는 주장은 하지 않는다.

이 두 fresh reconstruction의 compiler provider는 모두 `/usr/lib/llvm-18/lib/libLLVM.so.1`/SHA256 `5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0`, triple=`x86_64-none-unknown-elf`, passes=`default<O2>`이며 LoopUnrolling/LoopVectorization/SLPVectorization/VerifyEach는1이다. prior0e의 자연 host는 Intel Xeon Platinum8573C/target `sapphirerapids`, 이번은 AMD EPYC7763/target `znver3`다. target features도 다르다. 어느 단일 feature나 pass가 이 lowering을 유발했는지까지는 이 두 결과만으로 확정하지 않는다. CPU forcing이나 현재 AMD에서 old AVX512 image 실행을 제안하지 않는다.

전체 roster에서는 새 PKL의 old BINARY와 prior0e의 reconstruction image가91개 중71개만 같다. 나머지 PROGRAM indices는 `8,18,21,25,27,31,33,39,45,47,49,51,52,55,56,57,58,60,62,65`다. PROGRAM key/SOURCE roster와 PKL model SHA 대응은 같지만 이20개 image의 일치는 없다. 이번 fresh image는 old BINARY와0/91 일치한다. kernel92의 정확한 image 일치를 전체 frozen cache의 입증이나 재사용 허가로 확대하지 않는다.

### LLVM 의미와 수용 기준의 경계

[LLVM18.1.8 LangRef의 fast-math flags](https://releases.llvm.org/18.1.8/docs/LangRef.html#fast-math-flags)는 `nsz`가 있는 연산에서 optimizer가 zero 부호를 중요하지 않게 취급할 수 있다고 정의한다. -0가 존재하지 않거나 poison이라는 뜻은 아니다. 같은 버전 [InstructionSimplify.cpp L5630–5640](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.8/llvm/lib/Analysis/InstructionSimplify.cpp#L5630-L5640)는 +0 가산항을 입력으로 접는 조건에 `FMF.noSignedZeros()`를 명시한다. 이 허용 조건은 위 입력 복사 lowering과 부합하지만 이 함수가 실제로 선택된 유일 pass였다는 trace 증거는 아니다.

[LangRef 기본 floating-point 환경](https://releases.llvm.org/18.1.8/docs/LangRef.html#floating-point-environment)은 round-to-nearest를 전제한다. 같은 버전 [APFloat.cpp L2025–2033](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.8/llvm/lib/Support/APFloat.cpp#L2025-L2033)의 zero 합 처리도 반대 부호 zero의 결과를 이 환경에서 +0로 정한다. 이는 공식 원문 읽기이며 APFloat나 별도 산술 oracle를 실행한 결과가 아니다.

따라서 원본 SOURCE의 `nsz`는 두 lowering의 부호 차이를 의미상 허용하지만, 프로젝트의 고정 raw bit reference는 계속 -0를 요구한다. 현재 port가 current fresh original의 전체6출력과 같다는 수용과 frozen bit reference 수용은 서로 다른 조건이다. 이번 입력/환경에서 current original +0와 frozen -0를 한 native 출력이 동시에 만족할 수 없다. 이 충돌을 expected 교체·zero masking·기준 완화로 해소하지 않는다.

현재 guard의 flag 없는 +0 덧셈을 입력 복사로 바꾸면 이 lane의 frozen 비트만 맞을 수 있지만 이번 original/native parity와 기존 strict-add negative-zero 회귀 조건을 깨므로 근거 있는 최소 수정이 아니다. 역사 half5 중 negative-zero/subnormal 및 ±sNaN 자료는 별도의 synthetic 결과다. 이번 실제 입력은 정확히8000 하나이고, 역사 signaling NaN·다른 half 사례나 full128/full-model 수용까지 해결됐다고 기록하지 않는다. guard 삭제·이전 copy arm 복원·원본 PROGRAM/수학/expected/tolerance/ISA 변경은 하지 않는다.

### 다음에 필요한 증거와 최소 수정 판단

이 lane의 직접 차이는 **실제 같은 -0 입력 이후 zero arm의 codegen 선택**까지 좁혀졌다. 추가 producer hook·worker-after 읽기·SSA dump·MXCSR probe나 같은 fresh 경로 CI 반복은 현재 결론에 필요하지 않다. 새2B와 기존 source/object/image를 재사용한다.

다음 필요한 작업은 frozen fixture를 만든 원본 cache/compiler context의 귀속 확인이다. 기존 retained request·`fresh-runtime.json`·`compile-events.json`과 보존 SOURCE/object/image에서 위20개 PROGRAM 및 원래 fixture의 입력/기대 raw seal을 대조해, 어떤 봉인된 실행 context가 frozen 전체 결과를 정의했는지 확인해야 한다. writer 한 개 image의 일치만으로 역사 provider나 전체 image roster를 고정했다고 간주하지 않는다.

그 재료가 확보된 뒤에도 실제 실행 증거가 필요하면, 원래 reference context를 ISA가 자연 지원되는 host에서 한 번 확인하는 별도 GHA reference audit가 다음 실험이다. 기존 A의 전체91개 SOURCE/object/image·ISA와 D1 raw2576·D2의3stage/전체6output plain↔tap/후봉인·회수 조건은 그대로 둔다. 기존 test에 frozen BINARY fallback을 추가하거나 CPU를 강제하는 수정은 제안하지 않는다. 지금은 그 실험을 등록·실행하지 않았다.

현재 증거가 정당화하는 산술 code 수정은0개다. 후속 최소 fix의 검토 경계는 완전히 식별된 reference/compile artifact의 생성·선택 일관성이며, 아직 정확한 코드 hunk를 확정할 근거는 없다. 새 guard나 수학식으로 차이를 감추지 않는다. 부모가 reference provenance·등록·CI·이슈를 통합하며 device acceptance/full-model frozen acceptance는 계속 미완료다.

이번 추가분은 지정 worktree의 이 D2 문서에만 썼다. 기존19088B/SHA256 `453e7a4af3f2b1509b6c7b881b5f97077f3d137ead0ae816ac24fa805b40496b` prefix를 보존하고 append 여부·diff 공백을 확인한다. 모델/driver/SDK/helper/테스트·shared WF/Focus/ledger/pin을 수정하지 않았고, 로컬 Cargo/build/test/Clippy/model/oracle/프로젝트 CLI 실행0, GHA 재실행0이다.

범위 보충: 이번 실제8000 두 값은 current fresh original/native의 소비자 입력이다. frozen fixture 최초 실행 때의 data1[99]도8000이었다는 실측은 없다. 따라서 보존 image의 입력 복사 명령은 **같은 -0를 넣었을 때 frozen -0와 부합하는 직접 codegen 설명**이며, 역사 frozen 실행 전체의 유일 원인을 확정한 결과는 아니다. 이 구분 때문에 추가 tap보다 먼저 기존 reference 생성 context의 귀속 확인이 필요하다.

## frozen reference 생성 context 귀속 — 2026-10-26, 읽기 전용 후속

이번 조사의 결론은 **reference의 입력/PKL/실행 원문/expected raw 생성 경로는 연결되지만, PKL 내부 BINARY의 최초 compiler/provider/CPU/cache 이력은 완전히 연결되지 않는다**는 것이다. 미확인20개는 PKL에서 사라진 image가 아니라 prior0e 재컴파일 image와 bytes가 다른20개다. frozen expected를 교체하지 않았으며 새 tap·산술 수정·CI 실행을 추가하지 않았다.

### 생성 run → 원본 capture → fixture 봉인

원래 [run39635596188/job119345061762](https://github.com/bin9208/openpilot-rust/actions/runs/39635596188/job/119345061762)은 workflow commit `8e77eed65b9c68ada39f0415f7a67c8164c34f26`, checked-out source `e25a14386582858dae25c6daca5ced3bfb4bacab`다. API의 실제 `structuredContent.content` 로그와 step summary를 읽었다. step17 `Compile the original grouped driving and driver artifacts`, step19 `Capture exact independent original outputs for this compiled model`, step21 `Archive and seal complete same-PKL catalog and independent original fixture`는 각각 success다. run 전체 failure를 전체 수용으로 바꾸지 않는다.

[원래 workflow](https://github.com/bin9208/openpilot-rust/blob/8e77eed65b9c68ada39f0415f7a67c8164c34f26/.github/workflows/driving-native-296-once.yml#L279)는 다음 순서를 고정한다.

1. source e25a…의 `compile_modeld.py`를 model-size512x256/camera-resolutions1928x1208·1344x760/frame-skip4로 실행한다. 원문 `compile_jit`은 seed42 capture/replay와 pickle round-trip을 검증하고, 마지막에 `dump_oob(out, f)`로 저장한다. 실제 로그의 PKL 저장 시각은 `2026-10-21T16:37:31.9931238Z`다. 이는 reference capture용 seed20260930과 다른 생성 단계다.
2. 이전 artifact12327097515의 `original_only.py`를 SHA256 `bfe55536e9bf47cda093d9986247cf338f978c3c82b0cc6e1000731008697206`으로 확인해 이번 PKL에 적용한다. 이 script는 [이전 workflow source](https://github.com/bin9208/openpilot-rust/blob/97adeffc4e2c942586d5e5870446612e1ff02d56/.github/workflows/driving-original-only-296-once.yml)의 base64 literal에도 존재한다. 실행하지 않고 decode/AST 읽기만 했으며17413B/SHA가 정확히 일치했다. 따라서 script를 얻기 위해291464175B artifact를 새로 다운로드하지 않았다.
3. `fresh-original-observation/manifest.json`의 source/script SHA·1069개 file seal·6개 scenario·PKL SHA를 검증한 뒤, `seal-driving-daemon-fixture`가 models/catalog/observations를 복사해 상위 fixture manifest를 만든다. 원래 source의 `driving_daemon_reference/fixture.rs::capture`는 각 복사 전후 source/destination fingerprint를 확인하고 inventory한다. expected를 다시 계산하는 함수가 아니다.

로컬 보존 `rust/crates/dev-tools/tests/fixtures/driving-daemon-original/observations.tar.xz`는59663776B/SHA256 `3ddfb1213cccbbf0d4d0ef7854b79cef5637acdfd7b59d883b13063d6968b4fe`다. tar1564 member/expanded247086724B를 파일시스템에 풀지 않고 읽었다. 그 안과 tracked 원문의 fixture manifest310291B/SHA256 `afe1c795d7f609801eb4cd5a5a7748c3c996929403a3c0fa102f232f2a8d248d`는 observation manifest279981B/SHA256 `ed174bb4610ee55108b3f2502d7ed284cb66abb3cd153777ca546bf8fe0d3fca`와 model72045323B/SHA256 `487ef8698775a593726c79ef0651940480f755f5ded57f672d0ee3a4bd8a624f`를 함께 봉인한다. 이번에는 PKL 전체도 stream hash해 그 크기/SHA를 직접 확인했다. 모델을 역직렬화하거나 실행하지 않았다.

### 첫 frozen raw의 원본 입력과 현재 D2의 연결

observation manifest의1344-dual scenario는 warmup frame2/prepare-only=true와 첫 frame3/index0/prepare-only=false를 지정한다. 초기 state의 live_calib_seen=false/run_count0, warmup 뒤 live_calib_seen=false/run_count1/last_vipc_frame_id2도 봉인돼 있다. 원래 capture script는 scenario마다 새 Oracle과 seed20260930을 만들고 warmup zero image 후 첫 main/extra random image를 저장한다. packed 입력은 원본 `write_tensor`에 전달되기 **전** 한 번 관측해 SHA를 확인하므로 이후 recurrent output으로 바뀐 npy view를 input으로 오인하지 않는다.

| 원래1344-dual file | bytes | 봉인 SHA256 |
| --- | --- | --- |
| `warmup.bin` | 2428928 | `c93662152aaf108e3a0cadfebadfba4fd1d0e951fed4ac0168ff2cc66b0141aa` |
| `images/main-0.bin` | 2428928 | `06edc0ba7b1ed1d7a83e616c943a615a9ae2b88a578dd8a48b16e761643797c4` |
| `images/extra-0.bin` | 2428928 | `4a96b3fb724c1b72eb3e83db64d15edc6f0d25ffaecfb005e112c466decfcea6` |
| `inputs/main-transform-0.bin`·`extra-transform-0.bin` | 각36 | `6db65fd59fd356f6729140571b5bcd6bb3b83492a16e1bf0a3884442fc3c8a0e` |
| `inputs/packed-warmup.bin`·`packed-0.bin` | 각2096 | `9351907dd34c5363a5199130b08ab4b6de2d06f47ee001c1d9314ba504c34b21` |
| `outputs/raw-pred-0.bin` | 10304 | `9afb2defff4ff29e5bf70e54db32d0d531d5e2b44507c8de652ee2c0dfcb67fe` |

현재 artifact12608182890의 retained traffic에 있는2회 prepare용10개 실제 input file을 각각 위 해당 seal과 대조해 크기/SHA가 전부 같음을 확인했다. warmup용 두 transform은 별도 warmup 파일의 직접 관측이 아니라, 원래 Oracle의 zero 초기값·초기/warmup의 calibration-unseen 상태·첫 frame에서 liveCalibration 부재와 봉인된 첫 transform zero bytes를 결합한 기존 source 근거다. 이 시간적 귀속을 개별 warmup transform capture가 있다고 표현하지 않는다.

compact tar는 원래 image86개를 생략한다. 기존 GHA stager가 원래 seed/소비 순서로 만들고 각 파일의 고정 크기/SHA와 정확히 비교하는 코드도 읽었다. 이번에 작성자가 image를 생성하거나 RNG/모델을 실행한 것은 아니며, **이미 GHA에 보존된 실제 bytes**를 봉인과 비교했다.

tar의 원래 raw와 retained `frozen-model.bin`은 전체 bytes가 같고 index99 LE는 `00000080`다. current original의10304B/SHA256 `069099e1d4d8081ed9de155494cd18dd00cf2d8be3e12cfe04be08d2e6857f87`와 직접 byte 비교하면 차이는 offset399 한 byte뿐이다. current `input-provenance.json`은 위 두 manifest SHA/source e25a…/current source561f… 및 expected_regenerated=false/new_capture_replaces_expected=false를 기록한다. 따라서 다른 fixture·다른 입력 파일·expected 재생성으로 이번 차이를 설명할 근거는 없다. 같은 외부 입력/PKL이라도 서로 다른20개 PROGRAM이 중간 half를 동일하게 만들었다는 실측으로 확대하지 않는다.

원래 capture는 raw model/publication/state를 저장하며, policy의 전체6output을 각각 frozen 파일로 저장한 자료는 아니다. 따라서 위 직접 frozen byte 비교의 범위는10304B raw model이고, 이번 current original/native의 전체6출력 수용과 구분한다. 후속 audit 설계에서도 없는 frozen6output seal을 새로 입증했다고 쓰지 않는다.

### 원본 expected를 실제로 만든 실행 경로와 provider

봉인된 script L171–256의 `case`는 원본 `driving_daemon_reference.Oracle`만 만들고, `prepare_with_input_snapshot`을 거쳐 `oracle.scope['model_output']['raw_pred'].tobytes()`를 raw 파일로 쓴다. [source e25a…의 Oracle](https://github.com/bin9208/openpilot-rust/blob/e25a14386582858dae25c6daca5ced3bfb4bacab/rust/tools/driving_daemon_reference.py#L73)은 captured prepare 뒤 prepare-only면 반환하고, 첫 policy가 완료되면 output model tensor를 numpy로 복사해 `raw_pred`에 넣는다. Parser에는 별도 slice copies를 주며 raw 값에 zero 정규화·tolerance·native 출력을 적용하지 않는다. publication용 시간 sentinel은 raw 저장 후 사용된다.

원본 실행의 연결은 다음과 같다.

```text
동일 PKL/load_oob
  → model_export.original.driving: captured warp/policy 복원
  → check_pipeline_reference.execute: stage.jit.captured(inputs, variables)
  → engine.realize.get_runtime: PROGRAM.src[4].arg를 runtime에 전달
  → ops_cpu.CPUProgram: 저장 BINARY를 mmap에 복사해 실행
  → Oracle의 실제 model output copy → raw_pred → Capture.write/SHA → observation manifest
```

이 도식은 위 SHA로 확인한 동일 PKL의 source 호출 연결이며 작성자가 이 경로를 로컬에서 실행한 것이 아니다. 완성된5-src PROGRAM의 BINARY는 `to_program`의 SOURCE-only compile pattern에 다시 해당하지 않는다. 원래 capture가 이미 저장된 PROGRAM을 재생한 것과 `compile_modeld`가 그 BINARY를 처음 만든 것은 구분해야 한다. source `CPUProgram`의 CPULLVM 경로는 lib bytes를 그대로 매핑하며 LVP만의 별도 ELF loader 분기와도 구분된다.

observation manifest는 imported repo source113개 before/147개 after와 변경 없음 검사를 기록한다. 이 중 Oracle/check_pipeline_reference/model_export original·pipeline/realize/ops_cpu/compiler_cpu/support.c의8개 source SHA를 exact e25a… Git blob과 직접 대조했다. `ops_cpu.py` SHA는 `012f3d932abba9e14f82aedefe0ca52243eba1728f847c38cae54ae05b171a16`, `compiler_cpu.py`는 `db106591197315ecf28f0e15de99d2841c9892e6e58290213c62c598e7014139`다.

또한 e25a…와 실제 current source561f… 사이에서 original.py/pipeline.py/realize.py/ops_cpu.py/compiler_cpu.py/support.c/modeld.helpers.py의7개 전체 bytes가 같고, check_pipeline_reference의 execute/write_tensor 두 함수 AST도 같다. 현재 helper의 NPY copyin 분기는 원래 buffer 쓰기 API를 복구한 별도 관측이고, 이 원본 capture 경로를 native 산술로 교체한 것이 아니다.

기록된 실행 context는 ubuntu-24.04, DEV=CPU:LLVM/JIT1/JIT_BATCH_SIZE0/CPU_COUNT2/OPENPILOT_HACKS1, Python3.12.15/NumPy2.5.3/pycapnp2.1.0이다. LLVM_PATH는 `/usr/lib/x86_64-linux-gnu/libLLVM-18.so`이고 실제 로그는 libllvm18 `1:18.1.3-1ubuntu1` 설치 상태를 기록한다. `support.c::DLL.findlib`가 이 env 경로를 우선하는 원문도 확인했다. 이는 source/runtime 계층과 설정/패키지의 연결이며, 당시 resolve된 DSO 전체 bytes/SHA·mapping·kernel별 최초 compiler invocation receipt를 대신하지 않는다.

### 20/91 image 차이의 실제 범위와 남은 생성 이력

PKL의 OOB header 뒤 opcode metadata는6987550B/SHA256 `8d86fabaa6e1d89f2f69028a1cf158be512cace51183f13c719d0434d9056026`다. 표준 `pickletools.genops`로 literal bytes 위치/SHA만 읽었으며 GLOBAL/REDUCE 실행·pickle.load/load_oob·tinygrad import를 하지 않았다. current fresh-runtime receipt가 지정한 old BINARY SHA **91개 모두**가 이 고정 metadata의 실제 byte literal과 대응한다. 기존 helper는 이 old SHA를 실제 PROGRAM.src[4]와 결합해 검증한다. 따라서 나머지20개도 exact frozen bytes 자체와 그 PKL 귀속은 확보돼 있다.

그91개 literal을 prior0e의 fresh reconstruction91개와 비교하면71개는 전체 bytes가 같고20개는 다르다. 아래 changed는 길이가 다를 때 늘어난 구간도 세는 단순 byte 위치 차이이며 수치 error 크기가 아니다.

| PROGRAM index | frozen bytes | prior0e fresh bytes | changed bytes |
| --- | --- | --- | --- |
| 8 | 1126 | 1126 | 729 |
| 18 | 10810 | 10810 | 58 |
| 21 | 3200 | 3200 | 1095 |
| 25 | 2904 | 2904 | 185 |
| 27 | 3200 | 3200 | 1091 |
| 31 | 2872 | 2872 | 172 |
| 33 | 3200 | 3200 | 1091 |
| 39 | 2454 | 2454 | 1443 |
| 45 | 894 | 894 | 664 |
| 47 | 127840 | 128864 | 123832 |
| 49 | 910 | 910 | 667 |
| 51 | 1006 | 1006 | 728 |
| 52 | 926 | 926 | 686 |
| 55 | 1094 | 1094 | 771 |
| 56 | 878 | 894 | 721 |
| 57 | 878 | 894 | 721 |
| 58 | 878 | 878 | 659 |
| 60 | 974 | 990 | 796 |
| 62 | 878 | 894 | 721 |
| 65 | 19398 | 19398 | 7291 |

PROGRAM8의 frozen image SHA256은 `2bd7faf4d120456cc18f72304c33e2958410c9bdd0b2aa506711bf469a8fbd45`, prior0e는 `9cd73dfa840d5dc8e82b579d90328c1f1f605562a1b21dc59d3bfc321f2e04e5`다. 이 한 image의 code 구간도 anonymous memfd/objdump로 정적으로 읽어 word load·vector 구성 명령 배치가 다름을 확인했다. image20개가 다르다는 사실을 단순 누락, blanket ISA 허용, 한 특정 LLVM pass/CPU feature의 원인 확정으로 바꾸지 않는다. 이 표가 old BINARY의 실행 admission을 제공하는 것도 아니다.

최초 생성 경계의 원문은 `codegen.do_compile → compiler.compile_cached(source)`다. cache miss 때만 `LLVMCompiler.compile → compile_to_obj → jit_loader`가 실행된다. 자연 target은 `LLVMGetHostCPUName/Features`로 정하고 cache key는 processor/features/jit/opt를 담지만 LLVM DSO SHA를 담지 않는다. 확인한 생성 workflow/로그/manifest에는91개 각각에 대해 cache key·실제 hit/miss·최초 object·그 object와 BINARY의 연결·최초 loaded LLVM SHA·TM CPU/features를 함께 봉인한 기록이 없다. 로그에도 actual CPU census/LLVM init/key 관측은 없다. cache hit나 cache 결함이 실제 발생했다고 단정하지 않는다.

따라서 source/compiler Python code의 정확한 귀속, 고정 PKL의 모든 image bytes, frozen raw의 생성 실행은 연결됐지만 **91개 BINARY의 최초 생성 provider context 전체를 복구한 상태는 아니다**. prior0e가 kernel92와71개 image를 byte-identical하게 재현했다는 사실로 나머지20개 또는 최초 host를 역추정하지 않는다. 이 조사에서 대형 원래 artifact12365522894 전체를 새로 다운로드하지 않았다.

### port 판정과 필요한 경우의 별도 reference audit 설계

현재3stage에서 native와 **같은 fresh codegen context의 original**은 전체6출력 bytes가 완전히 같다. 이 lane의 +0를 Rust만의 잘못된 산술이라고 판정할 근거는 없고, 원문 `nsz`가 허용하는 zero 부호의 context 차이와 frozen의 엄격 raw 계약을 구분해야 한다. frozen -0는 실제 원본 capture에서 나온 유효한 고정 reference이며, 현재 original +0와 같지 않다는 사실도 그대로 남긴다. reference의 context 간 bit 재현성 문제를 기대값 변경이나 산술 patch로 숨기지 않는다. full-model/frozen/device 수용은 여전히 미완료다.

추가 tap이나 같은 fresh 경로 CI 반복은 필요하지 않다. 향후 부모가 별도 GHA reference audit를 검토한다면 다음 경계를 먼저 충족해야 한다.

1. 위 exact PKL/metadata·old91개 image·두 manifest·original capture script/source를 고정하고, warmup→첫 prepare→policy의 동일 입력/초기 state와 frozen10304B를 사용한다. SOURCE를 자연 host에서 새로 컴파일한 뒤 그것을 역사 BINARY와 같은 실행이라고 부르지 않는다.
2. **old91개 각각**의 실제 코드 범위/ABI/ISA와 source/object/image 연결이 기존 strict 기준으로 입증되고, 실제 host가 그 ISA와 OS state를 자연 지원하는지 먼저 확인한다. fresh triplet A의 성공은20개가 다른 old image의 admission을 대신하지 않는다. 원래 object/경계 증거가 없으면 실행 단계에 들어가지 않으며 기존 A/guard를 완화하지 않는다. 현재 자료만으로 실행 준비 완료라고 보고하지 않는다.
3. 실행이 정당화될 때에만 original reference context에서 고정 raw와 전체 출력 및 source/provider 전후 봉인·회수를 한 번 확인한다. 기존 D1 strict/raw2576·현재 D2의 전체6출력 plain↔tap 동등성은 별도 조건으로 유지한다. CPU forcing·unsupported old image 실행·expected 재생성·새 수학 code·관측 확대를 포함하지 않는다.

이 설계는 다음 실행 요청이나 workflow/helper 구현이 아니다. 지금 확인한 자료에서 필요한 것은 추가 숫자 probe가 아니라 빠진 최초 생성 object/provider/context의 기록이다. 그런 기록이 남아 있지 않으면 그 한계를 명시한 reference-context 의사결정이 부모에게 필요하며, 새 compile probe로 과거 provenance를 소급 확정할 수는 없다.

이번 쓰기는 이 문서 append뿐이다. 이전34137B/SHA256 `3a8575f5c3711bacfabcf880d45e44962e5b9ee6097e4d3540bce56b56a01222` prefix를 보존한다. 기존 artifact·tar·원문/Git blob·작은 로그/step metadata·AST·static disassembly만 읽었고, 원본 script/PKL/old BINARY/compiler·모델/oracle/프로젝트 CLI·Cargo/build/test/Clippy 실행0, 새 CI/새 helper/추가 tap/산술 수정0이다. shared WF/Focus/ledger/pin 및 부모의 #476 prestate/candidate 작업은 수정하지 않았다.

## frozen capturer의 실행 모드 확정 — retained BINARY와 D2 fresh SOURCE의 구분

**소스와 봉인 자료로 확정되는 답:** frozen expected를 만든 original capturer는 PKL에 저장된 모델 PROGRAM의 **retained BINARY를 실행**한다. capture 때 각 모델 SOURCE를 현재 host에서 재컴파일하는 모드가 아니다. 반면 source561f/run40216927059의 D2 original은 같은 PKL의 SOURCE에서 새로 만든 image91개로 runtime을 명시적으로 교체한다. 따라서 두 경로의 모드 차이는 가능성이 아니라 원문에 있는 실제 차이다. BINARY 최초 생성 시의 cache/compiler 이력 미확인과 이 실행 모드 판정은 별개다.

### frozen capture가 SOURCE compile을 건너뛰는 정확한 분기

원래 사용 script17413B/SHA256 `bfe55536e9bf47cda093d9986247cf338f978c3c82b0cc6e1000731008697206`의 전체 AST/원문을 다시 읽었다. `case`는 `Oracle(models, resolution, mode)`를 만들고 원본 `prepare`를 호출한다. 교체하는 함수는 packed 입력을 관측하기 위한 `source.write_tensor`뿐이며 PROGRAM/BINARY/get_runtime/runtime_cache/Compiler를 fresh 모델로 바꾸는 경로는 없다. raw는 실제 Oracle output에서 저장한다.

다음은 모두 observation의 checked-out source e25a…에 대한 원문 위치다.

| 연결 | 정확한 source | 결정되는 동작 |
| --- | --- | --- |
| PKL 선택 | `rust/tools/driving_daemon_reference.py:47`, `model_export/original.py:70–87` | 같은 PKL의 captured warp/policy를 load_oob로 복원 |
| stage 실행 | `driving_daemon_reference.py:85–89`, `check_pipeline_reference.py:27–29` | stage의 `jit.captured(inputs, variables)` 호출 |
| JIT 실행 모드 | `tinygrad/engine/jit.py:210–214` | **`run_linear(..., jit=True)`** 호출 |
| compile 분기 | `tinygrad/engine/realize.py:272–275` | **`if not jit: compile_linear(...)`**이므로 위 경로에서는 건너뜀 |
| kernel runtime 선택 | `engine/realize.py:176–185`, `114–119` | runtime cache miss면 `Device[device].runtime(..., ast.src[4].arg, ...)` 호출; cache hit면 같은 PROGRAM key의 기존 runtime 사용 |
| 실제 code bytes | `runtime/ops_cpu.py:80–114` | CPU:LLVM 경로는 넘겨받은 lib bytes를 mmap에 쓰고 그 시작 주소로 실행 함수 구성 |
| raw 저장 | `driving_daemon_reference.py:90–94`, capturer L231–233 | 실제 model output copy를 raw_pred에 넣어 원래 bytes/SHA로 저장 |

핵심 두 원문은 [CapturedJit.__call__](https://github.com/bin9208/openpilot-rust/blob/e25a14386582858dae25c6daca5ced3bfb4bacab/tinygrad_repo/tinygrad/engine/jit.py#L210)와 [run_linear](https://github.com/bin9208/openpilot-rust/blob/e25a14386582858dae25c6daca5ced3bfb4bacab/tinygrad_repo/tinygrad/engine/realize.py#L272)다.

```python
# CapturedJit.__call__
run_linear(self.linear, var_vals, input_uops=concrete, jit=True)
# run_linear
if not jit: linear = compile_linear(linear, validate=VALIDATE_WITH_CPU)
```

frozen manifest의 imported source before/after에 있는 `tinygrad_repo/tinygrad/engine/jit.py` SHA256 `61bd488758f233710d86a1a49803fe28139ae3e6771b2aef125e741f8150558b`를 exact e25a… Git blob과 직접 대조했다. 이 파일은 current source561f…와도 전체 bytes가 같다. 위 분기는 다른 버전의 일반 JIT 설명이 아니라 실제 capturer가 import한 source에 있다.

보조 확인으로 `codegen.pm_to_program`의 compile pattern은 SOURCE까지의4-src PROGRAM에 해당하며, `UPat`의 기본 allow_any_len=false/strict_length 검사 때문에 BINARY를 가진5-src PROGRAM과 일치하지 않는다. 다만 이번 판정의 직접 근거는 그보다 앞선 captured `jit=True` 실행 분기다. 모델 PROGRAM의 재컴파일 부재를 설명하는 것이며, 프로세스의 초기 Tensor 복원 등 모든 보조 연산에서 어떤 compiler 함수도 절대 호출되지 않았다는 역사 trace 주장으로 확대하지 않는다.

원래 workflow의 `Compile the original…` step에서 PKL을 **앞서 생성**한 것과, 다음 `Capture exact independent original outputs…` step에서 그 PKL을 **재생**한 것은 다른 단계다. workflow 이름의 `fresh-original-observation`은 새 capture 파일을 만든다는 뜻이며 그 이름만으로 capture가 SOURCE를 새로 compile한다고 해석할 수 없다. DEV=CPU:LLVM/LLVM_PATH 설정 또한 저장 BINARY를 SOURCE 재컴파일로 바꾸는 스위치가 아니다.

### 우리 D2는 왜 같은 PKL에서도 fresh image를 실행하는가

source561f…의 helper57130B/SHA256 `f91b77dbc23b4d7fa6c12a13ff2be0390130944ae4f80d0c850e7d73be104855`는 현재 파일과 전체 bytes가 같다. 다음 교체 지점을 exact helper에서 확인했다.

1. A collector는 `source = op.src[3].arg.encode()`와 `old_image = op.src[4].arg`를 구분하고, `CPULLVMCompiler(['x86_64','native']).compile_cached(source.decode())`의 새 SOURCE→object→image chain을 보존한다. actual12608182890의 compile-events는 get91/hit0/compile_enter91/compile_link91/put91/compile_cached91이며 chain91개다. 이는 이번 A가 실제 fresh compile이었다는 증거이고 역사 compile/cache의 소급 증거는 아니다.
2. `original_replay`는 PKL의 old SOURCE/name/BINARY SHA를 먼저 인증한 뒤, A가 입증한 **새 `row['image']`**를 읽어 `Device[device].runtime(..., image, ...)`를 만든다. 이후 `realize.runtime_cache[(op.key, device)] = program`으로 원래 PROGRAM key의 실행 runtime을 새 image로 채운다. old BINARY SHA 검사는 원본 identity binding이며 old BINARY 실행을 뜻하지 않는다.
3. 교체된 `get_runtime`은 모든 실제 호출이 그 cache의 동일 runtime인지 확인한다. key 누락·cache bypass·old fallback은 진단 실패다. tapped receipt의 fresh_runtime_not_frozen_binary=true 및 매핑된 image SHA들은 이 모드를 기록한다. plain도 같은 fresh runtime 구성이고 tapped만 별도 writer 관측을 켠다.

즉 D2가 captured JIT 실행 API를 그대로 쓴다고 해서 frozen capture와 machine bytes까지 같은 것은 아니다. 원래 captured API가 lookup하는 runtime cache를 D2가 의도적으로 fresh image로 먼저 구성한다. 이것이 두 모드를 가르는 정확한 지점이다.

| 비교 대상 | 모델 code 선택 | 이번 lane99 결과와 검증 의미 |
| --- | --- | --- |
| frozen original capture/source e25a… | PKL의 retained PROGRAM BINARY; capture 시 SOURCE 재컴파일 안 함 | 봉인 raw의 -0; 역사 serialized-program reference |
| D2 current original/source561f… | 같은 PKL/SOURCE에서 A로 fresh compile한91개 image를 runtime cache에 설치 | +0; 현재 host/codegen의 original 실행 |
| D2 current native/source561f… | 현재 catalog의 native 코드, 기존 guarded-add 유지 | +0; 위 current original과 전체6output bytes 동일 |

### 부모의 validation 정의 판단에 필요한 구분

D1은 **역사 retained-BINARY 원본의 frozen raw bit 재현성**을 요구한다. D2는 **현재 host에서 fresh SOURCE로 만든 original과 native의 동등성** 및 관측 무영향을 확인한다. 두 gate가 같은 input/PKL/SOURCE를 사용한다는 사실은 이미 봉인돼 있지만, 같은 실행 image나 같은 codegen context를 사용한다는 뜻은 아니다. 특히 원문의 `nsz`는 zero 부호를 codegen 사이에서 유지한다고 약속하지 않는다.

따라서 현재 성공은 current3stage original/native 전체6출력 동등성이고, 남은 실패는 retained-BINARY reference와 current fresh codegen 사이의 raw bit 계약이다. 이 lane의 차이를 곧바로 Rust 산술 port bug로 분류하거나, 반대로 D1 expected를 무효화할 근거로 삼지 않는다. 같은 실제 half8000에 대한 zero-arm lowering 차이는 위 정적 object 분석과 부합하며, 역사 frozen 내부 half가8000이었다는 새 실측을 추가한 것은 아니다.

이 절은 어떤 validation 정의를 채택하거나 변경하지 않는다. 부모가 판단해야 할 사실은 **현재 D1과 D2가 서로 다른 실행 모드의 명시적 계약을 각각 검사한다**는 점이다. retained image와 같은 reference context의 추가 수용이 필요한지, SOURCE 동등성과 역사 bit 재현성을 어떻게 함께 충족할지는 이 구분을 유지해 결정해야 한다. 현재 strict sticky/raw2576·ISA/guard·expected는 모두 그대로다.

이번 후속은 같은 cohort의 기존 manifest/compile-events/receipt와 exact Git/capturer 원문만 읽고 본 문서에 append했다. 이전50695B/SHA256 `d1f15c6323651738d5e3bac52813e6efd4807a0645223f2197df0995fb4ad44d` prefix를 보존한다. 새 artifact/capture·새 tap·helper/수학 code 수정·로컬 기능/old BINARY/compiler 실행·GHA 실행은0이다. #476/Bacon의 issuer audit 및 다른 소유 코드/공유 등록은 수정하지 않았다.
