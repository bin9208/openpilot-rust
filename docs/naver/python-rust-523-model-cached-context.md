# #482 fresh91 original BINARY context의 additive C 후보 — 2026-10-26

소유 범위는 `model_current_triplet_482.py`, 기존 `cpu/diagnostic_482_tests.rs`의 private module
등록, 새 `cpu/diagnostic_482_cached.rs` 및 이 문서다. #593 실제 완료 기록은 소유
`python-rust-523-model-482-425-vbmi-audit.md`에 추가했다. 부모가 FF publication/pins/CI/이슈를
통합하며 이 담당은 로컬 source/AST/rustfmt/diff와 기존 artifact inert read만 했다.

## 출발점과 이번 C가 묻는 것

source ee74의 currenttriplet40232519102는 **같은 GHA host에서 새로 생성한 PKL의 complete91**을
사용한다. 기존 B는 두 번째 frame model[100,101,1349]만 original -0/native +0이며,12개
output 중 다른11파일과 plain↔tap은 같다. 이를 단순히 historical frozen context 불일치라고
부르면 이 실제 same-host 차이를 설명하거나 해결하지 못한다.

C는 A에서 독립 인증한 full SOURCE/object/BINARY91을 기존 native issuer로 패키징하고,
명시 `CpuModel::load_cached`로 같은2frame을 실행해 **이미 관측/봉인된 B original12output**과
비교하는 후보이다. 새 golden, 원본 재실행, 새 tap 또는 compiler/guarded-add 수학 변경은 없다.
전체 B 실패는 C 성공 뒤에도 sticky다. C는 original BINARY context가 native ABI에서도 같은
출력을 내는지 묻는 진단이며 default runtime cached enable이나 수치 수용 변경이 아니다.

old PR1344의 frozen91 중 역사 image context가 다른20개에는 이 C 수용을 적용하지 않는다.
PR1344 helper의 run에는 새 C를 연결하지 않았고 frozen99/D1 gate도 수정하지 않았다.

## 기존 issuer API를 확인한 결과

`cached_code::Admission`은 candidate graph/certificate에서 읽은 기대 hash로 만들 수 없다.
`read_authenticated`도 launcher가 독립 인증한 admission file SHA를 요구한다. `IssuerRequest`는
source_root, independent original_manifest/model, inert ISA receipt, Admission과 모든 PROGRAM의
source/image/reconstruction_object를 받는다. 첫 package에는 graph/library가 없어야 하고,
최종 seal에만 둘 다 전달한다.

`cached_code::issue`는 original manifest/model SHA, 정확 guarded8/cached10 current source seal,
각 source/object/image의 전체 재구성과 ABI, 독립 receipt와 native decoder의 instruction count/
feature 일치 및 현재 host를 다시 검사한다. container/library의 정확 symbol roster/전체 image
bytes/정렬/비중첩과 graph call/kernel ABI도 확인한다. `CpuModel::load_cached`는 이 certificate와
독립 Admission을 비교한 뒤 dlopen한다. `CpuModel::load`는 cached graph를 기본 거부한다.

기존 `model_export.native.CachedExport.for_model`은 실제 PKL의 모든 PROGRAM을
SOURCE/image/globals/scalars/workers/core_id identity로 기대91개와 bijection 검증하고
`CachedModel`을 만든다. `export_cpu(..., cached=...)`는 copy wrapper를 유지하고 모든 PROGRAM을
기존 original image로 패키징해 preflight/package/final seal을 받는다. fresh 수학 kernel로
fallback하지 않으며 새 compiler target이나 feature override도 쓰지 않는다.

## 기대값의 독립 출처와 구현 흐름

1. 기존 ISA14/cached-source-object-ABI15, A whole91/ISA, B plain/tapped/whole-output gate는
   그대로 먼저 실행한다. 기존 `comparison.json` 내용도 바꾸지 않는다.
2. controller는 기존 `holds.verify()` 뒤 A request와 readonly receipt에서만 independent
   Admission을 만든다. fresh `triplet-request.json` 자체가 independent original manifest이며
   그 pinned SHA, 이미 생성한 PKL SHA, A original metadata91과 source/object/image paths를 쓴다.
   candidate graph/certificate에서 original 기대 metadata/hash를 복사하지 않는다.
3. A decoder 결과를 issuer schema의 inert ISA receipt로 옮긴다. 각 원문 source/image SHA와
   index/text/instruction/required를 결합하며 issuer가 동일 full bytes를 다시 decode/대조한다.
   현재 guarded/cached source roster는 A의 embedded source hash에서 가져온다. exporter Python
   roster 및 held helper ELF의 실제 path/size/SHA/FD identity는 source-sealed controller가
   candidate 생성 전에 기록한다. issuer ELF는 같은 private Cargo group의 기존 target으로
   선택·hold·DSO 확인하며 raw path의 임의 실행을 허용하지 않는다.
4. 새 `cached-context-export` worker는 같은 fresh PKL을 읽어 기존 `cached_export_session`과
   `CachedModel`을 사용한다. original stage execute/`.numpy()`/synchronize 호출은 없다.
   원본 참조 출력은 새로 만들지 않는다. export 뒤 old holds를 다시 검증해 바뀐 B raw를
   새 golden으로 재핀하지 않는다.
5. 새 ignored native selector는 A source/request/readonly 및 C-only source를 embedded source와
   결합하고, independent Admission을 따로 읽는다. A original metadata91의 exact equality,
   full source/object/image 재검증·전체91 ISA, actual issuer/guarded/helper ELF와 exporter roster
   SHA를 다시 확인한다. candidate 파일 hash는 immutability를 위한 controller pin이며
   original admission의 출처가 아니다.
6. C graph는 version/backend/library/cached descriptor 외에 B graph와 같아야 한다. allocations,
   views, inputs/outputs, weights SHA, kernels/calls/entrypoints와 모든 call의 worker/core/scalar/
   view binding을 보존한다. default cached 거부와 변조한 Admission 거부 통제 후에만 명시
   cached loader를 호출한다. 같은 B sequence/input bytes로 prepare/policy 두 번씩 실행해
   기존 original12output과 전체 bytes를 비교한다. 이12개 C actual output만64MiB 총합 안에
   보존해 독립 재비교할 수 있게 하며, 각 크기/SHA/동일 여부와 mismatch name도 보고한다.
7. C 오류는 별도 `cached-context-failure.json`과 command stderr에 보존한다. 그 뒤 원래 B의
   `need(not mismatches, ...)`를 그대로 실행하므로 B의 세 signed-zero 실패는 승격되지 않는다.
   B가 성공한 경우라도 C 실패는 후보 진단의 실패로 남는다. 기존 finalizer/후봉인/회수는 유지한다.

ee74 artifact12613144560의 metadata를 inert read해 실제91개 composite identity가 모두
유일함을 확인했다. 따라서 이 fresh91에서 `CachedExport.for_model`에 필요한 기대 roster가
없거나 candidate에서 기대값을 추출해야 하는 상태는 아니다. 실제 C 성공은 아직 미검증이다.

## 별도로 남은 정확한 issuer blocker — 이번 소유 범위 밖

현재 `cached_isa.rs`는 FP16/VBMI를 검사하지만 **production `cached_code.rs::host_has`는
avx512bw까지만 명시하고 avx512fp16/avx512vbmi를 거부한다.** `issue`의 독립 decode/host
대조와 `prepare`의 최종 host 검사 모두 그 함수를 사용한다.

ee74 current Intel fresh91은 PROGRAM18에서 AVX512_FP16을 실제 요구한다. 따라서 현 issuer를
그대로 쓰는 이 후보는 해당 host에서 native preflight의
`cached issuer independent complete decode/host proof differs`로 거부될 것으로 source상
예측된다. 이 예측을 실제 C 실행 결과로 쓰지 않는다. 이전425 EPYC fresh91의 VBMI도 같은
production vocabulary 경계에 걸린다. natural AMD에서 이 두 feature가 없는 fresh91은
그 이유로 거부되지 않지만, host를 강제해 이 경계를 피해 가는 설계는 하지 않는다.

부모가 Intel C 실행을 진행하려면 별도 소유의 `cached_code.rs::host_has`에서 **관측된 이 두
feature만** Rust1.94 OS-aware literal runtime detection과 대응시키고 기존 unknown/host-missing
회귀를 유지하는 별도 source admission이 필요하다. 이번 담당은 production issuer/ISA policy를
수정하거나 검사 함수를 주입/우회하지 않았다. 기존15/14 음성·양성 gate도 줄이지 않았다.

## bounded 추가 용량과 실행 한계

기존 전체40분/호출900·180·A90초/global reserve, build+registry8GiB/proof512MiB 및 끝512MiB
reserve를 늘리지 않았다. C exporter/native 각180초와4GiB AS 제한은 기존 runtime worker
수준으로 고정한다. 기존 issuer의90초 호출 및 CPU60초/AS4GiB/FSIZE64MiB도 그대로다.

추가 forecast는 `B weights bytes + 기존 original12output bytes + 4×full image bytes + 64MiB`이며
256MiB를 넘으면 시작하지 않는다. weights/image/기존 output 크기는 실제 A/B 자료에서만 읽는다.
ee74의 실제 metadata는 weights60410137B/image 합489743B/기존12output5138816B이며
forecast는134616789B다. 실제 모델 계산을 수행한 값이 아니라 보존 file/graph extent 합계다.
추가 용량과 기존 finish reserve를 statvfs로 확인하고, 시작 전 projected build+registry도
기존8GiB 안인지 확인한다. 실행 중에는 원래 scoped budget을 계속 검사한다.
완료한 C payload에 output64MiB/request4MiB를 더해256MiB 이하인 경우만 native 실행에 넘기고,
종료 뒤 실제 C namespace도256MiB 이하인지 확인한다. C 임시 issuer 디렉터리는 기존 mutable
Cargo target 안의 새 private child를 사용해, 원래 승인된 target ENOENT 재표본 경계 밖에
새 임시삭제 예외를 만들지 않는다. source/owner/target identity 조건은 바꾸지 않는다.

native C는 B 자료 materialization128MiB, 비교 output 총64MiB, request/Admission/ISA 각4MiB,
원문/각 image/object16MiB와 기존 full-image ISA 총합 한도를 지킨다. C weights/PKL은 artifact에
추가 업로드하지 않고 C actual12output과 input/issuer/certificate/source/raw error 및 SHA receipt를 기존
proof512MiB 안에 보존한다. proof quota 초과는 계속 실패다.

## selector·통합·검증 상태

기존 ISA14와 cached-code15 selector/count는 그대로다. 기존 A/B ignored selector의 함수 본문과
19-source request schema를 유지했다. 새 C-only module은 request의 별도 source pin과
`include_bytes!`로 결합하므로 PR1344의 기존19-source 소비자를 바꾸지 않는다.

새 selector는 다음1개 ignored test다.

```text
cpu::diagnostic_482_tests::cached_context::complete_fresh91_cached_context_matches_existing_original
```

helper가 같은 held libtest ELF의 `--ignored --exact --nocapture`로 C를 실행하도록 작성했다.
lib의 총 test roster는 ignored1개 증가하므로 부모는 외부의 고정 filtered-out count가 있으면
확인해야 한다. `cpu.rs`/lib/Cargo 등록은 수정하지 않았고 private 진단 module만 추가했다.
부모는 helper/WF/PR1344 BASE_SHA pins를 새 helper bytes에 맞춰 통합해야 한다.

로컬 Python AST와 Rust1.94 rustfmt 구문·서식, diff 검사만 수행했다. 모델/원본/Python oracle/
issuer/Cargo/build/test/Clippy/새 GHA 실행은0이다. production vocabulary blocker 수용 및
새 source의 issuer/native C 실제 결과, full128/full-model/기기/retirement 수용은 미완료다.

source-only 대조에서 기존 collect/two_frames/OriginalTap/원본 config의 AST, 전체 B 실행·whole
비교·sticky assert의 AST, finalizer/source seal/metadata 음성 통제와 ENOENT scope의 AST는
기준과 같았다. 기존 Rust 진단 파일도 새 module 등록2줄을 제거하면 전체 bytes가 복원된다.
따라서 기존19-source schema와 A/B test 본문은 그대로다. 이는 실행 성공 주장이 아니다.

| 구현 파일 | SHA256 |
| --- | --- |
| `tools/ci/model_current_triplet_482.py` | `52b1aae365f2a5d99c06bb08f116e9f41b5dcf77aa5c49c3466010846172795c` |
| `rust/crates/model-runtime/src/cpu/diagnostic_482_tests.rs` | `103f19b4590e316e669af4a4935441723ca7a07709dbe73f9ac657b0fc0ab188` |
| `rust/crates/model-runtime/src/cpu/diagnostic_482_cached.rs` | `d9c1e8c2ea095b7e197cfdcf190cec48330d6a9a59ad1d4c8cd0b56e76997eb4` |

## 부모 #596 후보 적용 뒤 ready 인계 — 2026-10-26, GHA UNRUN

위 production vocabulary blocker는 최초 C source 검토 시점의 기록으로 보존한다. 이후 부모가
[문제 #596](https://github.com/bin9208/openpilot-rust/issues/596)과
[별도 기록](python-rust-523-cached-issuer-596.md) 아래 `cached_code.rs::host_has`에
`avx512fp16`/`avx512vbmi` 두 arm을 Rust1.94 literal OS-aware detector로 연결했다.
검토 시 worktree 후보 SHA256은
`f6963fbf61b4463f80a88bd8d2d4a02f42b0c28cfa73fb9d66910b30022ddc16`이고 UNSTAGED 상태다.
이는 **candidate 적용, 새 exact-source GHA 미실행**이다. C preflight/native 수용을 실제
관측했다고 쓰거나 최초 blocker 기록을 삭제하지 않는다. 부모 소유 파일은 편집하지 않았다.

부모 hunk의 source-only peer 검토에서 구체적인 API/타입·cfg 오류는 발견하지 않았다.
두 runtime literal은 기존 decoder와 같고 `_ => false`는 유지된다. decoder host snapshot
대조는 cached-program-isa + Linux x64에서만 컴파일되므로 feature가 없는 build에서 존재하지
않는 `cached_isa` module을 참조하지 않는다. 잘못된 철자/VBMI2/future name 거부도 유지한다.
기존 inert metadata 양성에서 각 requirement 하나를 뺀 두 음성을 확인하며, 원래 빈 requirement
목록을 복원한 뒤 기존 unknown control을 실행한다. 원래 omitted/ABI/unknown 음성을 다른
실패가 가리는 변경이 없고 선언된 test 수는15 그대로다. 이 검토는 compile/test PASS가 아니다.

C 소스3개는 위 SHA 그대로이며 이번 후속에서는 바꾸지 않았다. 부모가 stage할 정확한 C
소스는 위 표의 helper, 기존 diagnostic 등록 파일, 새 C module이다. 이 C 문서도 새 파일이며
#593 실제 완료 기록은 별도 소유 VBMI 문서에 이미 반영됐다. 부모의 cached_code/#596 문서와
WF/BASE_SHA/pins/통합 commit은 부모가 함께 관리한다. 이 담당은 git add/commit을 하지 않았다.

진단 flag/metadata capsule은 다음 범위로 분리된다. C의 issuer/runtime true flag는 실제2frame
실행을 완료한 receipt에 기록한다. 수치 mismatch가 있어도 실행 사실은 true이며,
`existing_original_all12_equal=false`와 mismatch 목록 및 후속 strict 실패를 유지한다.
생성/검증 실패 시에는 C 실패 receipt와 원시 command stderr를 남긴다.

| capsule | 의미/보존값 |
| --- | --- |
| 기존 `comparison.json` | B 전용 `cached_issuer_invoked=false`, `cached_runtime_enabled=false`, 기존 strict mismatch와 `retirement_credit=0` 유지 |
| `cached-context.json` | `scope="explicit additive C only"`, `complete_roster=91`, `frames=2`, `cached_issuer_invoked=true`, `cached_runtime_enabled=true` |
| C 안전/분리 flag | `default_cached_disabled=true`, `new_original_executions=0`, `new_taps=0`, `b_failure_can_be_promoted=false` |
| C 실제 비교값 | `diagnostic_complete`, `existing_original_all12_equal`,12개 actual/original SHA·크기·equal 및 mismatch name; 값은 실제 native 비교 후에만 기록 |
| C 미수용 범위 | `full128_acceptance=false`, `full_model_runtime_acceptance=false`, `retirement_credit=0` |
| `cached-context-failure.json` | 실제 error 원문, 기존 B mismatch에 따른 `b_failure_sticky`, `numerical_acceptance=false` |
| `cached-context-capacity.json` | 동적 forecast/기존 output bytes, C256MiB·build8GiB·proof512MiB, 남은 기존 deadline, `old_caps_or_deadline_extended=false` |

cached exporter worker에만 `MODEL_CACHED_CODE_BINARY/SHA256/REQUEST/REQUEST_SHA256`를 전달하고,
C native selector에만 `MODEL482_CACHED_REQUEST/SHA256`를 전달한다. 기존 A/B 환경과 일반
`CpuModel::load`의 default cached-disabled 계약은 유지한다. C가12출력 전체를 맞혀도 원래
B의 strict assert를 먼저 유지하며 B 실패를 성공으로 바꾸지 않는다. ISA14/cached-code15
count는 바꾸지 않고 새 ignored C1개만 추가한다.

## actual972 감사 — B12 성공 / C issuer 성공 / C ELF hold 거부

2026-10-26. source `972468fab5279a9897d2d2bda736938c89a81a9a`,
[run40241573585/job121314300733](https://github.com/bin9208/openpilot-rust/actions/runs/40241573585/job/121314300733).
자연 host는 AMD EPYC7763/logical4다. 발행 helper52b1…/기존 진단103f…/새 C module d9c1…와
부모 #596 f696…의 SHA가 위 인계값과 일치했다. 처음 진행 중일 때 run/job 원문과 발행 source
hash를 private scratch에 먼저 보존했고, 완료 뒤 실제 API `structuredContent.content` 전체
로그와 최종 run/job JSON도 보존했다. source나 실행 중 job은 수정하지 않았다.

### 실제 수용과 실패의 분리

| 단계 | 실제 결과 |
| --- | --- |
| model build/libtest/strict Clippy | exit0; Clippy15.432906초 |
| native cached-code15 |15 PASS/0fail/0ignored; command2.925238초 |
| ISA14 |14 PASS/0fail/0ignored; command0.115630초 |
| A original capture/readonly | complete91/calls176/compile events104; capture62.798138초, readonly6.508970초/exit0 |
| A strict triplet/host | `strict_triplet_all=true`, `host_isa_all=true`, `isa_error=null`; AMD의 FP16/VBMI snapshot은 false |
| B plain/tapped | original/native 양쪽 모두 exit0; original15.664588초/native2.624926초/original-tap3.661170초/native-tap11.644610초 |
| B whole12 | `original_native_bytes_equal=true`, `plain_tapped_bytes_equal=true`, `mismatches=[]` |
| C export/issuer | export18.758619초/exit0; native preflight/package/final seal 모두 exit0, generated-code=false/libraries-loaded=false |
| C native |0.117669초/exit101; Rust line98의 held helper input에서 거부, model load/run 전 |
| step14/15/16 | source/index, SDK, 독립 immutable/provider/children 모두 success |
| final-seals/upload/cleanup | `pass=true`, `failures=[]`, `all_owned_children_reaped=true`; step18/19/20 success |

step13의 최종 오류는 `additive cached-context diagnostic failed; existing A/B retained`다.
step17은 그 후속 required-outcome 실패다. 이번 **B는 실제 PASS**였으므로
`cached-context-failure.json`의 `b_failure_sticky=false`가 맞다. 기존 B의 assert를 통과한 뒤
C 오류를 별도로 실패시켰으며, B 실패를 덮어 성공으로 바꾼 것이 아니다. 이전 Intel의
frame1 세 signed-zero가 해결됐다고 일반화하지 않는다. 이번에는 자연 AMD에서 원본과
native가 같은 전체12출력을 냈고, frozen PR1344/D1 또는 전체128frame을 비교한 run도 아니다.

독립 inert raw 재비교로12개 original/native/original-tap 파일, 경로당5138816B가 모두 같음을
확인했고 native-tap의12개 output SHA도 실제 native-plain bytes와 일치했다. C에서는
`cached-context.json`이 없고 C actual output file도0개다. 따라서 **C2frame/12output 및 cached
native ABI 수치 수용은 아직0**이다. 전체 모델/retirement 수용도0이다.

### C 최초 원인: 정상 Cargo ELF nlink2를 자료용 Held가 거부

`model482/cached-context-native.stderr`는 다음 원문을 기록한다.

```text
diagnostic_482_cached.rs:98:56
called `Result::unwrap()` on an `Err` value: "input regular/owner/size/link"
```

line98은 `capture.issuer`, `capture.guarded`, exporter roster 순서의 첫
`Held::open(input, 128*MIB, &job)`이다. C 전용 reader가 Cargo ELF에도 자료 파일의
`nlink()==1` 조건을 적용했다. 실제 최초 issuer와 다음 guarded ELF는 다음과 같다.

| 실제 input | bytes / UID / nlink | SHA256 |
| --- | --- | --- |
| `build/target/debug/offline-cached-code` |4573736 /1001 /2 | `d150420623396074b4ee983cfacc14538c4dc64fcd2dbb02069a3ce95056ed7d` |
| `build/target/debug/offline-llvm-guarded-add` |1445664 /1001 /2 | `674d665f01ecdbc12c2e20542004e5728fdde85c0117cd074fd514e2a568bdc6` |

mode는 두 파일 모두33261(regular executable)이며 before/after identity·SHA가 정확히 같다.
issuer의 size는 C request와 일치하고128MiB 미만, UID도 이미 통과한 C request/Admission과
같은1001이다. 따라서 이 복합 오류 중 실제 불일치는 **nlink2 대 reader의1**이다.
permission/size/SHA/ISA/메모리·공간·deadline 또는 model output 오류로 분류하지 않는다.

나중에 읽을 held libtest ELF는11053512B/nlink1이며, C candidate bundle input과 B traffic
input의 기록된 nlink도 전부1이다. 현재 증거에서 필요한 수정은 helper ELF 두 개에 한정된다.
첫 검사에서 멈췄으므로 다음 native model 수치 결과를 예측하지 않는다.

### issuer의 독립91 인증은 어디까지 실제 완료됐는가

preflight/package/seal status는 모두 exit0/generated-code=false/libraries-loaded=false다.
Admission의 manifest SHA는 A의 실제 triplet-request 전체 SHA와, model SHA는 새 PKL receipt
`b8d7bc413a4648536324f254432b7e60d1e8ef9cf497475dbe14a231cd93b21b`와 결합된다. PKL은 이번
artifact에 없으므로 로컬에서 unpickle/재해시/실행했다고 쓰지 않는다.

artifact의 독립 Admission, certificate, ISA receipt가 모두91개이며, 전체 index/kernel/original
metadata/current binding이 A와 일치한다. C `.cached.bin`91개를 A의 대응 full image와 전체
bytes/SHA로 재비교해 모두 같음을 확인했다. linked library/container의 SHA도 certificate와
같다. C graph도 backend/version/library/cached descriptor 외 모든 B 필드가 같다.

- certificate SHA256: `e28c28e8ee32891f4e3b75dd0c2b84937847537c9c094d807e9ce7a52bdc4ef7`
- library SHA256: `3b0a7ebb7b469904301c57aba8ba32ce28e4012159000f4c78e877ed4e262269`
- container SHA256: `15c0b6522d0ccdb0182dd6c200caaa449ffae66539859811d3aad21a73ffcd1a`

실제 ISA union은 avx/avx2/bmi1/cmov/fma/multibyte-nop/sse4a다. #596의15개 회귀와 AMD의
decoder/issuer snapshot 대응 및 이 host의 전체91 issuer는 성공했다. Intel FP16/VBMI positive
issuer 실행을 이번 AMD run에서 관측했다고 소급하지 않는다.

### 다음 최소 수정 제안 — 이번 audit에서는 미적용

공통 `Held`의 nlink1이나 production Admission/ISA 정책을 완화하지 않는다. C controller만
기존 source-selected Cargo ELF 두 개를 held SHA로 검증해 **새 C-private nlink1 실행 사본**을
만들고, mode0700·size/SHA/owner/canonical identity를 확인한 뒤 그 사본을 hold하는 경로가
가장 좁다. 원래 Cargo 경로·alias는 삭제/수정하지 않는다.

사본을 만들 경우 `CurrentBinding`의 두 helper path/FD identity binding과 C request의
issuer/guarded Input을 사본으로 함께 구성해야 한다. C worker에만 cached helper4env 및
`MODEL_LLVM_GUARDED_ADD_BINARY/SHA256`를 사본으로 맞춰 전달해야 native CachedExport의
binding hash도 일치한다. 기존 A/B 환경과 old B helper 경로는 그대로 둔다. 추가6019400B를
forecast와 기존 C256MiB/build8GiB/proof512MiB 한도 안에 계산하면 된다. 이는 다음 source
제안이며 이번에는 코드 수정·재실행하지 않았다.

### artifact·원시 증거 봉인

artifact12616924856은 API/download39828644B/SHA256
`8e1dadf02382e036c66bc5910ac8a553914dc128a77b470e6719e1b8224a9676`다.50MiB download cap,
API expected bytes/digest 확인 뒤1285개 safe unique canonical path/no symlink·special·encrypted
entry와 전체 CRC, expanded114475027B를 검사했다. 파일시스템에 풀거나 native image를
실행하지 않았다. retained 원문67295266B이며 기존 proof512MiB 안이다.

주요 receipt SHA256:

| 원문 | SHA256 |
| --- | --- |
| `comparison.json` | `c3404bd341f1115da21539424b1ea381ea91d8754b150f60440c1cd3e5543916` |
| `triplet-readonly.json` | `8831e918c142dc785a2db0770badf596247b80606141c99484b2b990c012129e` |
| `cached-context-failure.json` | `65693c75cb27b4e9386a323a8f719160df86f6a28580b50254c3db10969b056a` |
| `cached-context-native.stderr` | `74ad0defdb23ddc1aa8b51be4522c9310fd4eb71b7de06cbef98746f1dc9eea5` |
| `final-seals.json` | `59c4d6ac9864472bc2c328f6c0c5b70627375fdb6ef1b4af5ab1e0e803866fd0` |

완료 job log의 실제 structuredContent.content UTF-8 원문106633B/SHA256은
`2a6ced64a3b8433238b88cf1600ba5233b53da970e712836db263e97fc00715f`다. private
`scratch/ci/model972-current-{initial,final}-{run,job}.json`, 초기 source seal capsule,
`model972-current-job121314300733.log`, `12616924856.zip`에 보존했다. local functional/model/
oracle/Cargo/build/test/Clippy, CI dispatch/cancel, production/expected/math 수정은 모두0이며,
이번 tracked 변경은 이 감사 append뿐이다.


부모 통합은 current WF/helper의 새 SHA를 동일하게 고정하고 PR1344는 BASE SHA literal과
그 helper/WF pin만 갱신했다. 기존 PR1344 진단 알고리즘과19-source schema는 그대로다.
새 C-only source path를 current WF push 대상에 추가했다. 실제 Actions 결과가 나오기 전
#596/모델 수치·전체 기능 수용 및 Python source retirement credit은 계속0이다.

## 972의 nlink 거부 최소 수정 후보 — C 전용, GHA UNRUN

2026-10-26. 부모가 허용한 범위대로 `Controller.cached_context` 안에서만 source-selected
guarded/issuer 두 ELF의 private 실행 사본을 추가했다. native C module/공통 Held/production/
원본 Cargo 경로는 수정하지 않았다. 위 actual972 실패와 미적용 제안은 그 시점 기록으로 보존한다.

- C 시작의 기존 `holds.verify()` 뒤 original 두 ELF의 held identity/SHA를 별도로 잡는다.
  복사 직전 `regular`가 그 held seal과 정확히 같아야 하고 canonical source, current UID,
  executable 및64B..128MiB 크기를 확인한다.
- 새 `cached-context/helpers` directory는 canonical/current UID/mode0700이어야 한다.
  pinned 공통 `copy_new`는 source FD를 전후 identity/SHA 검증하며
  `O_CREAT|O_EXCL|O_NOFOLLOW`로 새 inode에 stream-copy한다. 복사 중 destination은 mode0400이며
  완료한 새 destination FD에만 `fchmod(0700)`을 적용한다. source의 mode/alias는 바꾸지 않는다.
- destination을 `O_NOFOLLOW|O_NONBLOCK|O_CLOEXEC`로 열어 regular/current UID/nlink1/원래 size,
  canonical path와 FD/path identity를 확인한다. mode0700과 held SHA, hash 전후 FD/path identity를
  재확인한 뒤 ELF로 hold한다. source도 복사 후 동일 held identity/SHA여야 한다.
- 두 사본 생성 뒤 기존 original aliases와 사본 모두 `holds.verify()`를 다시 통과해야 한다.
  `cached-context-helper-copies.json`에는 source before/after seal, copy seal, 실제 copied bytes를
  기록하고 그 receipt도 hold한다. 이것은 다음 GHA의 실제 기록이며 로컬에서 복사를 실행하지 않았다.
- C의 `CurrentBinding`, issuer request admission, native C의 issuer/guarded Input은 검증한 사본
  path/identity로 구성한다. C exporter worker에만 cached4env와 guarded2env를 사본에 맞춰
  전달한다. 기존 A/B의 `self.env`, 원래 helper 경로 및 source/ABI/수학·strict sticky gate는 그대로다.

추가 bytes는 source-selected held 두 size의 실제 합을 `helper_copy_bytes`로 forecast에 더한다.
actual972의4573736+1445664=6019400B를 대입하면 기존135249545B forecast는141268945B다.
사본 생성 전 이 합을 포함해 C256MiB 및 projected build+registry8GiB, 실제 filesystem finish
reserve를 다시 검사한다. 기존 actual C namespace256MiB/proof512MiB cap과 모든 timeout/
deadline은 유지한다. 기존 copy 루틴 및 source 선택/DSO 검증은 그대로다.

source-only 확인에서 **`Controller.cached_context`를 제외한 helper 전체 AST가 source972와
정확히 같다.** 따라서 기존 A/B 환경·원본 collect/tap·B 비교·15/14 gate·finalizer·budget 함수는
바뀌지 않았다. native C module d9c1…/기존 진단103f…/production cached_code f696…/공통 helper
8a5d…의 전체 SHA도 이전과 같다. Python AST와 diff만 확인했으며 copy/model/issuer/oracle/
Cargo/build/test/Clippy 및 CI dispatch/cancel은 로컬에서 실행하지 않았다.

수정 helper는98038B/SHA256
`379507cb2fc7b9d6c35c83f9851f9b541acad86a9d0085e8bb95cf71e03dfd24`다. 부모가 helper/WF/BASE
pins와 publication을 통합한다. git stage/commit/push는 하지 않았다. native C의 실제12출력
수용은 아직 미완료이며, 이후 native C 알고리즘 수정이 필요하면 먼저 실제 원인과 scope를
보고한다. 이 변경으로 full128/full-model 또는 retirement 수용을 주장하지 않는다.

## actual602 감사 — C copy 미도달, QCOM bin-test Clippy가 최초 실패

2026-10-26. source `602040b066e37487feb0595a09c8460023bf755e`,
[run40244158586/job121322096232](https://github.com/bin9208/openpilot-rust/actions/runs/40244158586/job/121322096232).
이미 완료된 failure를 원시 job API/log와 artifact로 감사했다. 실제 CPU는
Intel Xeon Platinum8573C/logical4다. CPU 이름만으로 아직 실행하지 않은 ISA/모델 수용을
추정하지 않는다. 발행 helper379507…/기존 진단103f…/새 C module d9c1…/production #596
f696…의 source SHA는 위 인계값과 같다.

### 실제 단계별 상태

| 범위 | 실제 상태 |
| --- | --- |
| current-bins | exit0/17.183590초; 기존3개 binary 선택·hold |
| current-libtest | exit0/6.166915초; lib test ELF build 성공, test 실행 아님 |
| current-clippy | exit101/14.798662초; 최초 실패 |
| native15 / ISA14 | command와 stdout 모두 없음, **미실행** |
| A whole91 / host ISA | request/readonly receipt 없음, **미실행** |
| B strict/plain/tap/output12 | comparison.json 없음, **미실행**. B sticky가 원인이 아니며 true/false를 판정할 값도 없음 |
| C copy/issuer/native/output12 | capacity/helper-copies/cached-context receipt와 C namespace 모두 없음, **미실행** |
| source/index·SDK 외부 후검사 | step14·15 success |
| 전체 독립 immutable 후검사 | step16 failure; 아래 registry baseline 미등록 때문에 fail-closed |
| 자식 회수 | `subreaper=true`, `live=[]`, `all_owned_children_reaped=true` |
| upload/cleanup | step18 success, 불완전 seal 때문에19·20 cleanup은 skipped |

step13의 원래 오류는 `current-clippy: exit101; preserve stderr`다. 그 뒤 finalizer가
`independent final seals failed`를 기록하며 required step17도 실패했다. 이번에는 이전972의
nlink 거부나 B 수치 실패까지 도달한 것이 아니다. C copy 수정의 실제 성공·실패를 이 결과로
판정하지 않고 full128/역사 실패/전체 모델·기기·retirement 수용은 계속0으로 둔다.

### 원시 Cargo JSON의 정확한 세 Clippy 오류

stderr의 마지막 `offline-qcom-compile test due to 3 previous errors`만으로 원인을 추측하지
않고, `model482/current-clippy.stdout`222551B의 `compiler-message` JSON을 읽었다.
세 오류 모두 `clippy::unused_unit`, message=`unneeded unit expression`이다.

| source602 위치 | 실제 코드/문맥 |
| --- | --- |
| `rust/crates/model-runtime/src/bin/offline-qcom-compile.rs:69` | read-exact stream의 Interrupted/WouldBlock match arm에 trailing `()` |
| 같은 파일93행 | write-all stream의 Interrupted/WouldBlock match arm에 trailing `()` |
| 같은 파일143행 | trailing-input 검사 Interrupted/WouldBlock match arm에 trailing `()` |

`cargo clippy --lib --tests ... -D warnings`가 bin test target도 검사하므로 C 진단 앞에서
이 target의 lint가 gate를 막았다. published QCOM file8195B/SHA256은
`3687cc13749eed58b25bd204b246f77a1b0a4d6298221c35fc355b5d1d703aa0`다.
이는 실제 compiler diagnostic이며 model/ISA/cap/deadline 실패가 아니다.

부모 쪽 최소 source 수정 후보는 해당 세 arm의 **trailing `()`만 제거**해 implicit unit
빈 block으로 두는 것이다. Interrupted/WouldBlock 재시도와 deadline/오류 분기는 그대로
유지할 수 있다. bin test target을 빼거나 lint를 허용/무시하는 우회는 제안하지 않는다.
이 파일은 담당 소유 범위 밖이며 이번에는 코드 수정하지 않았다.

### helper 불변성과 후봉인 실패의 인과관계

원래 Cargo issuer4578440B와 guarded1449872B는 UID1001/nlink2 그대로 hold돼 있고,
final held verification에 해당 오류는 없다. C-private nlink1 사본은 생성 단계 자체가
미실행이므로 사본의 immutability 성공을 주장하지 않는다.

`final-seals.json`은 `pass=false`, `failures=["baseline unavailable: registry"]`,
phase=`current-clippy`다. controller는 strict Clippy 성공 **뒤에** registry baseline을 등록한다.
이번 조기 종료에서는 그 등록문에 도달하지 않아 required baseline이 없었다. source/SDK/
Python/venv의 after snapshot은 보존됐고 child recovery도 성공했지만 전체 immutable guard는
불완전한 baseline을 성공으로 승격하지 않았다. registry 변조가 실제 관측됐다는 뜻은 아니다.
Clippy 원인을 먼저 고치면 원래 순서로 registry seal에 도달할 수 있으며 후검사를 완화할
이유는 없다.

### 원본 증거와 로컬 작업 범위

artifact12618012224는 API/download7222550B/SHA256
`63d113ce83ed53824c82e4e4bbd2c942fe33edb65914816bcb3de1e42b6924b6`다.20MiB download cap,
API expected bytes/digest 확인 후126개 safe unique canonical path/no symlink·special·encrypted
entry와 전체 CRC, expanded35501461B를 검증했다. 실제 archive에는 A/B/C receipt가 없다는
점도 확인했다. 압축 해제·모델/ELF 실행 없이 metadata와 Clippy 원문만 읽었다.

| 원문 | SHA256 |
| --- | --- |
| `current-clippy.stdout` | `daafa01a4b606441c75af155e89c70cc0269cb0a948ad6a03fff9cc047a06fac` |
| `current-clippy.stderr` | `e1c52503ab6040ca46d1ef4d3e3916e02578e83d4083f578d62aabf8ba5ddeab` |
| `commands.json` | `46e972f5cb4bbf5444235f6009e6ee74fffdecceac6265a5f7b1826a3f8b93ca` |
| `final-seals.json` | `c97aecc8f02232dc23bdcf8d52fc1307908cb4885e95c5d63e0cd6aeac2f6880` |

실제 API structuredContent.content 원시 log95239B/SHA256은
`d30b05ea5775d94945741c8c2af72bcb573ac9a19240d4a396eb377327b168c3`다. private scratch/ci에
`model602-current-final-{run,job}.json`, `model602-current-published-sources.json`,
`model602-current-job121322096232.log`, `12618012224.zip`으로 보존했다. 이번 tracked 변경은
이 audit append뿐이다. source/model/oracle/Cargo/build/test/Clippy 로컬 실행과 CI dispatch/
cancel, stage/push는0이다.

## actualef172 감사 — C12 실제 PASS, B strict 세 signed-zero로 전체 FAIL 유지

2026-10-26. source `ef17256300d8caf51d5b0a9b3a900fd2c9615d01`,
[run40245597205/job121326369244](https://github.com/bin9208/openpilot-rust/actions/runs/40245597205/job/121326369244).
원시 job log의 `structuredContent.content`, 완료 job API와 artifact12618762234를 읽었다.
실제 runner는 `GitHub Actions 1000029753`, CPU는 `INTEL(R) XEON(R) PLATINUM 8573C`,
x86_64/logical4다. **C는 미검증 상태를 벗어나 실제 원본12출력과 모두 일치했다.**
전체 FAIL은 C 완료 뒤에도 보존한 기존 B strict 비교의 세 signed-zero 차이다.

### 15/14/A/B/C 실제 진입과 완료 범위

| 범위 | 실제 결과 |
| --- | --- |
| current-bins / current-libtest | exit0; 각각13.271197초/4.859408초. 후자는 test ELF build이며 실행 수로 세지 않음 |
| current-clippy | exit0/12.394071초; 이전602의 세 `unused_unit` 오류 재발 없음 |
| cached-code native15 | 15 passed/0 failed, command exit0/1.844622초 |
| ISA14 | 14 passed/0 failed, command exit0/0.110725초 |
| A fresh collect / readonly | exit0/46.899505초 및3.925547초; complete91/calls176, strict_triplet_all=true/host_isa_all=true |
| B original/native plain 및 tapped | 네 command 모두 exit0. 각 경로 plain↔tap 전체12출력 동일, original↔fresh-native strict 비교는 아래 세 zero 차이로 FAIL |
| C 두 helper 사본 | 실제 생성·hold·사용 완료, copied_bytes6028312, originals_unchanged=true |
| C exporter / DSO 확인 | exit0/11.876003초 및0.110849초; issuer preflight/package/seal 각각 exit0 |
| C native selector | exit0/9.764772초; 1 passed/0 failed, `existing_original_all12_equal=true`, mismatches=[] |
| 외부 source/index·SDK·독립 immutable gate | steps14/15/16 모두 success |
| 전체 required gate | step17 failure; step13의 기존 B strict 오류 유지 |
| upload / source cleanup / job temporary cleanup | steps18/19/20 모두 success |

C selector는
`cpu::diagnostic_482_tests::cached_context::complete_fresh91_cached_context_matches_existing_original`다.
`cached-context.json`은 complete_roster91/frames2/diagnostic_complete=true,
cached_issuer_invoked=true/cached_runtime_enabled=true다. 새 original 실행과 tap은 각각0이며,
기존 B original12를 재사용했다. `cached-context-failure.json`은 없다.

A의 OS-aware host snapshot에는 `avx512fp16`, `avx512vbmi`가 모두 있다. 이번91개 image의
실제 decoder census는 AVX512_FP16을 요구하지만 AVX512_VBMI는 포함하지 않는다.
따라서 native15/ISA14와 이번 FP16 포함91개 실제 수용을 기록하며, VBMI 명령을 요구하는
전체 모델까지 이번 C에서 실행했다고 확대하지 않는다. A readonly 자체의
generated_code_executed=false/numeric_acceptance=false도 유지한다.

### A의 원본 자료 → 독립 admission → C 원본 image 연결을 별도 재검사

artifact의91개 A `compile_event`별 SOURCE `.ll`, reconstruction object `.o`, image `.bin`을
실행하지 않고 읽어 request의 size/SHA와 각각 대조했다.91개 모두 C certificate/독립 admission의
original metadata와 정확히 같으며, index/kernel/pointers/scalars/global_slots/workers/core_id,
image alignment/text bytes 연결도 같다. C `kernel-*.txt`의 source SHA와 `*.cached.bin`의 전체
bytes는 대응 A 자료와 같다. C ISA proof와 독립 ISA 자료가 같고, A의91개 required features/
instructions/text bytes와도 일치한다. 후보가 스스로 발급한 metadata만으로 원본임을 추정하지 않았다.

원본 request SHA는
`79de376fc35b75bfd0d5b46e2d8987f8980be458730d5134b374c7a099f72e98`,
original PKL recorded SHA는
`487ef8698775a593726c79ef0651940480f755f5ded57f672d0ee3a4bd8a624f`다.
두 값 모두 독립 admission/certificate와 같다. PKL 원본 자체는 archive에 없으므로 로컬에서
PKL bytes를 재검증·unpickle하지 않았다. C container/library의 실제 retained bytes SHA도
certificate와 같다. C/B graph는 허용된 `version/backend/library_sha256/cached_code`만 제외하면
정확히 같다. allocations334/views427/inputs9/outputs6/kernels93/calls176/entrypoints2와
weights SHA가 유지된다. issuer 세 status의 generated_code_executed/libraries_loaded=false는
그 issuer 단계의 속성이며, 뒤의 C native 실행까지 미실행이라는 뜻이 아니다.

### 12개 raw 독립 대조와 B sticky의 정확한 잔여 실패

C receipt만 믿지 않고 retained raw12개, 경로당 총5138816B를 직접 읽었다.
**C cached 출력 == 기존 B original plain == 기존 original tapped**가12파일 모두 byte exact다.
각 C size/SHA도 receipt와 같다. fresh-native raw SHA는 native tap의 output_sha256와 같으며,
native/original 각각의 plain↔tap 전체 동일성 receipt도 true다.

B original↔fresh-native 차이는 `model-1.bin`10304B 하나, 바이트 offsets403/407/5399뿐이다.
이는 float32 indices100/101/1349이며 세 원본 word는 모두 LE `00000080`(-0), fresh-native는
`00000000`(+0)다. 나머지11파일과 이 파일의 다른 bytes는 같다. C는 같은 세 word를 포함해
원본 전체 bytes와 같다.

| `model-1.bin` 경로 | 실제 SHA256 |
| --- | --- |
| B original / original tapped / C cached | `7fe2075ebcc4048be72666c5fc101238df672d438dfb849507251293d4c43242` |
| B fresh native | `bbebf82fb3e5c20efd6011d5c538b005464e4f9a7eaccc1f2cd1338f44f2732f` |

controller는 C 성공 뒤에도1308행에서
`strict original/native byte mismatch retained; diagnostic is not numerical acceptance`로 실패한다.
C의 b_failure_can_be_promoted=false/default_cached_disabled=true/full128_acceptance=false/
full_model_runtime_acceptance=false/retirement_credit0도 그대로다. 이번 결과는 같은 host의 fresh91
원본 BINARY를 명시적으로 admission한 native ABI 경로가 기존 original12를 재현한다는 증거다.
fresh guarded 재컴파일 경로의 세 zero 차이는 해결하지 않았다.91개 image를 함께 재사용한
관측이므로 특정 kernel·rewrite 한 곳의 원인까지 고립했다고 주장할 수 없다. C helper에 대한
다음 수정 근거는 없으며, 남은 B 원인은 fresh compilation/image context 쪽으로 좁혀 별도 검토한다.
old PR1344 frozen20/91의 생성 context, 과거 half5/sNaN, full128 및 전체 모델 수용과 구분한다.
수학·expected·tolerance·CPU·ISA 강제나 default cached 활성화의 근거로 사용하지 않는다.

### 두 사본 실제 사용·원본 불변성과 마지막 봉인

이번 실제 경로는 `/tmp/openpilot-ci.ZELxY1/build/target/debug/`의 두 원래 Cargo ELF와
`/tmp/openpilot-ci.ZELxY1/build/model482/cached-context/helpers/`의 두 C 사본이다.

| helper | bytes | 동일 source/copy SHA256 | 원본 inode → 사본 inode |
| --- | --- | --- | --- |
| offline-llvm-guarded-add | 1449872 | `270b1cc2abe7e881682b2dd7e84ae7c93b7f33ffe072576a01d900164fd756d6` | 9443675 → 9445441 |
| offline-cached-code | 4578440 | `9293fa09a64453993759fa97299d7f237aae6f5ef1026c006a8406a7647c7fcb` | 9443688 → 9445442 |

두 source/copy 쌍은 device66305/UID1001이다. 두 source는 regular0755/nlink2이며 source_before,
source_after와 holds-after의 전체 identity/SHA가 같다. 두 copy는 서로 다른 inode의
regular0700/nlink1이며 retained ELF bytes size/SHA와 마지막 held identity/SHA가 모두 같다.
원래 hardlink의 삭제·이동·변경 없이 이전972의 nlink 거부를 해소했다.

C forecast140645101B에는 실제 두 ELF6028312B가 포함돼 있고 기존256MiB cap 아래다.
기존 build+registry8GiB/proof512MiB cap과 deadline을 늘리지 않았다.
`final-seals.json`은 pass=true/failures=[]/phase=cached-context-native,
all_owned_children_reaped=true다. `children.json`도 subreaper=true/live=[]다. B의 수치 실패와
immutable 후검사 성공을 각각 기록하며 source/SDK/copy 변조나 자원 초과로 원인을 바꾸지 않는다.

### 증거 봉인과 변경 범위

artifact12618762234는 API/download42345579B, SHA256
`71209f8053826ef05fe0e2d2cb514649a73a65f52a884f2b5bcbcd63fbe69364`다.50MiB download cap과
API expected bytes/digest를 확인했다.1283개 safe unique canonical entry/no symlink·special·encrypted,
전체 CRC와 expanded124357921B를 검사했다. archive를 filesystem에 추출하거나 ELF를 실행하지 않았다.

| 원문 receipt | SHA256 |
| --- | --- |
| triplet-readonly.json | `7fd4bfc9d6fdddd1b3554865eb9bdf165f122b2d48acd8c614cd52a7d57f8967` |
| comparison.json | `991f0046bacc2d4e465d83e4590ac1d04748e6287cfa3ded2c2617c308074792` |
| cached-context-helper-copies.json | `2cbd5a120c52472a78c21bd29985716780bc04dada49b6535a391953ded55135` |
| cached-context.json | `14a51416524aa4a1c6345a2d3914eb1a3b307ce5664ab8c4c799b347703de511` |
| bundle/cached-code.json | `8de26749588e498b3dc80d5ebd9981b718b1a0d48c0c48d8554f8d8e28cd436c` |
| final-seals.json | `2048830bd67d58d8d2fc6e0897f85ac462b9a16027d32800e755d0d037fbf54b` |

원시 job log106492B/SHA256은
`eb3cd72472166bfcab90144bc555f67d9f04014526420a8857ed8dc3026b25e0`다. private scratch/ci의
`modelef172-current-final-{run,job,artifacts}.json`, `modelef172-current-published-sources.json`,
`modelef172-current-job121326369244.log`, `12618762234.zip`으로 보존했다.
published commit에서 current helper98038B/SHA379507…가602와 byte exact임을 확인했다.
기존 진단103f…/C module d9c1…/production #596 f696…/cached_isa a62a…도 위 source값 그대로다.
즉 기존 A/B 본문·source closure·guard/math 변경 없이 세 lint 수정 뒤 C 사본과 실제12출력까지 도달했다.

이번 tracked 변경은 이 문서 append뿐이다. 로컬은 Git 원문/JSON/ZIP/raw bytes의 inert 감사와
diff 확인만 수행했다. source 수정·model/oracle/Cargo/build/test/Clippy 실행, CI dispatch/cancel,
stage/commit/push는 모두0이다. 부모가 전체 수용 및 다음 qualification 범위를 결정한다.

## actualef172 후속 — 같은 host의 compiler 입력 차이와 빠진 fresh-unrewritten 대조군

2026-10-26. 같은 source/run/job/artifact12618762234를 재사용했다. **writer92의 native
fresh-unrewritten object는 이 artifact에 없다.** A의 원문/object/image, B의 renamed+guarded
IR/object/DSO와 C의 A image 재사용은 있지만, 함수명만 native symbol로 바꾸고 guard를 적용하지
않은 별도 compiler invocation/object/output은 보존되지 않았다. 따라서 이번 읽기로 확정하는 것은
실제 compiler 입력과 마지막 zero-arm 기계어의 차이이며, guard만 고립한 수치 인과관계나 ABI
전체에 대한 판정은 아니다. production/default/수학/expected/nsz 정책 수정은 하지 않았다.

### 실제 보존 원문과 없는 대조군

| 역할/정확한 artifact 상대경로 | bytes / SHA256 / 상태 |
| --- | --- |
| A `retained/triplets/compile-90.ll`, B `retained/traffic/bundle/kernel-92.txt` | 둘 다166049B/`e223a2c36c6bde7cf676bb92fd2529858a723f9ecfa91c811b2b941756a9bea0`, byte exact |
| native symbol로 rename한 중간 IR |166043B/`9781b747d6d436694b7d248a2b29b2fd95ddd78009405674e303a9fa405f6952`; receipt의 입력 hash이며, 원문 rename만으로 재대조 가능. 이 입력만 compile한 object는 없음 |
| B `retained/traffic/bundle/kernel-92.compiled.ll` |166727B/`ffc2206baa1f05f49a5c9d07b68de71da1a1437485da87d8e4a75d0692a31b52` |
| A `retained/triplets/compile-90.o` |15936B/`fbf737784835622e99706fc42357e9058bf9ce2bfbb648195453de9513d45ddd` |
| B `retained/traffic/bundle/kernel-92.o` |15944B/`a8fbf958e61e4424822a54512cd618e629308479ea16c05b3bd38896d47441da` |
| A `compile-90.bin`, C `retained/cached-context/bundle/kernel-92.cached.bin` | 둘 다14442B/`324697e4d28a400da3c521fea2f9140eea04d50f19e6771a96549f4b8a5a6767` |

B compilation-evidence는 renamed SHA와 **guarded** compiled SHA/object SHA를 연결한다.
renamed SHA만으로 fresh-unrewritten native object가 실제 compile됐다고 주장하지 않는다.
기존 로컬 archive12616924856도 writer92는 같은 종류의 A/B/C 자료뿐이고,12618012224는 Clippy
조기 실패로 writer 자료가 없다. 이 세 archive에서 별도 unrewritten/unguarded 후보와 `.bc`는
찾지 못했다. 보존 `.ll`은 compiler 입력이며 최적화 후 IR/bitcode/pass-by-pass trace가 아니다.
다른 archive 전체나 보존되지 않은 원래 runner filesystem까지 조사했다고 확대하지 않는다.

### 첫 외부 출력 차이와 실제 tap의 경계

이번 자료의 첫 다른 외부 출력은 zero-based frame1의 `model-1.bin`이다. serialized model
packet 파일은 없고 두 frame의 prepare/policy sequence·입력5종·출력6종 raw가 있다. frame0의
6출력은 모두 같고 frame1도 아래3word 외에는 같다. 따라서 packet 내부 필드나 최초 내부 SSA
분기까지 관측한 것으로 표현하지 않는다.

| frame1 output index | worker / local outer / global outer / lane | 기존 writer 진입 data0 LE | original 반환 / native 반환 LE | data1의 allocation16 내2B 시작 offset |
| --- | --- | --- | --- | --- |
|100 |0 /25 /25 /0 |`000030bb` |`00000080` / `00000000` |9672 |
|101 |0 /25 /25 /1 |`00e004bc` |`00000080` / `00000000` |9674 |
|1349 |1 /15 /337 /1 |`00a03c3b` |`00000080` / `00000000` |12170 |

이 before 값은 원본/native에서 실제로 같지만 **출력 data0의 이전 저장값**이다. 다른 인자의
입력값을 증명하지 않는다. request의 call175/PROGRAM90/kernel92는 data0=view9/allocation9/
offset0/10304B, data1=view365/allocation16/offset9472/3152B다. 원문의 출력 index는
`1288*worker + 4*local_outer + lane`, `%v182`는 `322*worker + local_outer < 394`다.
위 세 index는 모두 이 조건이 참이다. lane0은 `%v2647 → %v3059`, lane1은
`%v2648 → %v3130`을 거쳐 최종 vector store에 들어간다. data1의 해당 half load는 같은 index다.

그러나 이번 tap은 세 **data1 half bytes/변환 후 SSA**를 기록하지 않았다. 과거 DD의 input99
`0080` 관측을 이번100/101/1349의 실제 half 값으로 대입하지 않는다. view365의 producer는
call147/kernel67이고 call148/kernel68의 arg3을 지나 call175의 arg1에 연결된다는 것은 정적
provenance다. 이전 actor의 실제 출력이나 kernel92 진입의 모든 입력 동일성을 관측한 것은 아니다.

### source-only 접점의 정확한 차이

`model_export/native.py::compile_kernel`은 원문 함수명을
`r_322_2_4_256_32 → op_impl_92`로 바꾸고, 명시적으로 admission된 helper에 넘긴 뒤
`compiler.compile_to_obj(body)`를 호출한다. writer 원문 전체 diff에서 rename 외의 변경은
결과 `%v3059/%v3130/%v3202/%v3274` 네 곳뿐이다. 원문4231행의 첫 lane 예는 다음과 같다.

```llvm
%v3058 = select i1 %v182, float 0.0, float %v3057
%v3059 = fadd nsz arcp contract afn float %v2647, %v3058
```

guarded 입력은 다음 세 줄로 결과를 만든다. 다른 세 lane도 같은 형태다.

```llvm
%diagnostic_guarded_sum_0 = fadd arcp contract afn float %v2647, %v3057
%diagnostic_guarded_sum_0.zero = fadd float %v2647, 0.0
%v3059 = select i1 %v182, float %diagnostic_guarded_sum_0.zero, float %diagnostic_guarded_sum_0
```

두 IR의 인자 순서/타입/noalias/align32와 마지막 function attribute는 같다.
attribute는 `{ alwaysinline nounwind "no-builtins" "no-trapping-math"="true" }`이며
전역 no-signed-zeros/target-cpu/target-features attribute나 module target/source_filename header는 없다.
명시적인 차이는 마지막 add의 `nsz` 제거와 flags 없는 +0 add arm이다. 이 접점을 바꾸지는 않았다.

### 실제 LLVM context proof와 source로만 확인되는 부분

| 항목 | A original 실제 기록 | B native의 현재 증거/한계 |
| --- | --- | --- |
| provider | `/usr/lib/llvm-18/lib/libLLVM.so.1`, SHA`5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0`; package `1:18.1.3-1ubuntu1` | 같은 sealed provider 환경·autogen/compiler import와 sampled mapping. 별도 native TM getter receipt는 없음 |
| triple/CPU/features | `x86_64-none-unknown-elf` / `sapphirerapids`; features886B/SHA`886a41ba2fb2cc50445f17d23f2b7da6af8a54aec1f13c05ddc6d720e36afffa` | 같은 자연 host, `Device.default.renderer.compiler` 사용. native invocation의 실제 getter 결과를 따로 봉인하지 않음 |
| passes/options | `default<O2>`; LoopUnrolling/LoopVectorization/SLPVectorization/VerifyEach 모두 실제 setter value1 | 같은 `CPULLVMCompiler`/LLVMOPT1 source 경로. B의 실제 setter roster 별도 관측은 없음 |
| codegen/relocation/model | pinned compiler source는 `LLVMCodeGenLevelDefault/LLVMRelocPIC/LLVMCodeModelDefault` 호출 | 같은 함수 원문. 이는 source 증거이며 별도 런타임 getter 관측과 구분 |
| cache 및 reused context | 아래 실제104개 compile/cache/link journal과 선택91개 연결 | exporter는 `compile_cached`를 거치지 않고 `compile_to_obj` 직접 호출. 원래 B 실행은 A가 방금 만든 PKL의 BINARY 재사용 |

A 선택91개 chain의 config는 모두 같고 writer의 compile_event는90이다. 전체 journal은 보조
컴파일을 포함한104개 get/compile_enter/compile_link/put/compile_cached이며 cache hit는0이다.
104개를 모델 PROGRAM 수로 세지 않는다. collector는 PKL 각 SOURCE+image를 해당 실제 chain의
source/object/image와 연결하고 loaded address도 확인했다. 이번 B original이 과거 unknown cache나
old PR1344 image를 재생했다는 증거는 없으며, A의 **이번 same-host fresh image를 재사용**한다.

compiler source6679B/SHA`db106591197315ecf28f0e15de99d2841c9892e6e58290213c62c598e7014139`는
A/B actual import receipt와 published Git blob에서 같다. `LLVMParseIRInContext → VerifyModule →
LLVMRunPasses → EmitToMemoryBuffer`도 같은 Python/LLVM C API 원문이다. 이 모델 object를
Rust test ELF 빌드의 LLVM21이나 test profile opt-level이 생성했다고 혼동하지 않는다.
wrapper header는 stdint/string이고 link command는 `clang -shared -fPIC -O2 -ffreestanding
-fno-math-errno ... -lm`이다. kernel92 object는 그 전에 이미 emit됐다. 별도 fast-math/LTO 옵션을
발견하지 못했으며 아래 실제 object→DSO의 store-region 동일성도 확인했다.

### 보존 object가 보여 주는 실제 zero-arm lowering

GNU `/usr/bin/objdump -dr -Mintel --no-show-raw-insn`으로 anonymous memfd의 두 ELF object를
정적으로 읽었다. compiler/LLVM 도구나 모델을 실행하지 않았다. `.text` 상대 offset은 다음과 같다.

```text
A original: 26de cmp [rsp+0x140],0x18a; 26f4 cmovl eax,r11d; 26f8 kmovd k1,eax
            26fc vaddps xmm0,xmm2,xmm0
            2700 vmovaps xmm0{k1},xmm2
            2713 vmovaps [rax+rcx*4],xmm0
B guarded: 26de cmp [rsp+0x140],0x18a; 26f4 cmovl eax,r11d; 26f8 kmovd k0,eax
            26fc knotw k1,k0
            2700 vmovaps xmm0{k1}{z},xmm0
            2706 vaddps xmm0,xmm0,xmm2
            2717 vmovaps [rax+rcx*4],xmm0
```

조건이 참이면 A는 입력xmm2를 마지막에 선택하고, B는 가산항을+0으로 만들어 실제 덧셈을 남긴다.
B DSO의 `op_impl_92` symbol은0x73840/14260B이며 object 상대0x26de..0x271b의 bytes와 같다.
따라서 이 차이는 sidecar뿐 아니라 로드 대상 DSO에도 남는다. 함수의 첫 byte 차이는0x348이지만,
그 위치를 최초 수치 divergence나 원인 instruction으로 간주하지 않는다.

[LLVM18.1.3 LangRef](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.3/llvm/docs/LangRef.rst#L3242-L3264)는
`nsz`에서 zero 부호를 중요하지 않게 최적화할 수 있다고 정한다.
[동일 tag InstructionSimplify.cpp](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.3/llvm/lib/Analysis/InstructionSimplify.cpp#L5179-L5183)는
+0 add를 입력으로 접는 조건에 noSignedZeros 또는 negative-zero 불가능성 증명을 둔다.
[APFloat.cpp](https://github.com/llvm/llvm-project/blob/llvmorg-18.1.3/llvm/lib/Support/APFloat.cpp#L1873-L1878)의
zero 합 규칙도 기본 round-to-nearest에서 반대 부호 zero의 합이 +0임을 설명한다. 이는 관측한 두
lowering의 의미 차이에 부합한다. 해당 함수/특정 pass가 이번 변환을 실제 수행했다는 trace나
현재 세 half 입력이 같았다는 증거로 확대하지 않는다. 원본 `nsz`를 strict raw 비교 면제로 삼지 않는다.

### 결론의 제한과 필요한 최소 후속 후보

전체91개의 B 원문 SHA는 A와 같지만, B canonicalizer는25개 kernel에서 총385곳을 rewrite했다.
C는91개 image를 한꺼번에 원본 것으로 바꾼 경로이므로, 성공만으로 ABI 일반론이나 writer92
한 곳의 인과관계를 고립하지 못한다. 이번에 확인한 source-only 차이를 근거로 guard 제거/nsz
복구/zero 후처리/수학 patch를 적용하지 않는다. same-IR+same-config의 **fresh-unrewritten native
대 guarded native 동시 대조**가 빠져 있으므로 그런 수정안을 확정할 수 없다.

추후 부모가 원인 고립 진단을 허용한다면, 기존 A/B 과정의 같은 process/자연 LLVM provider·TM·
pass-options에서 **동일하게 rename한 writer92 IR 하나**를 두 후보로 직접 fresh compile하는 범위가
가장 좁다. 한 후보는 rename만, 다른 후보는 현재 helper의 네 rewrite를 그대로 사용한다. 양쪽
실제 getter/setter context·SOURCE/object SHA를 기록하고 cache를 사용하지 않는다. 다른90개
object와 graph/weights/입력/초기 상태는 동일하게 유지한 두 native 후보를 별도 namespace에 두어,
기존2frame의 원본12출력과 비교하면 writer 접점만 분리할 수 있다. 새로운 original 실행/golden/
tap/blanket SSA dump 또는 production 기본 경로 변경을 추가할 이유는 없다. 기존 B failure와
15/14/A/후봉인·회수·cap/deadline은 유지해야 한다. 이는 **필요성 제안뿐이며 구현·dispatch하지 않았다**.
현재 archive는 weights/PKL을 업로드하지 않으므로 이 제안을 artifact만으로 로컬 실행할 수도 없다.

private inert 계산 근거는 `modelef172-first-difference-audit.json`4297B/SHA256
`992fffc164bb2e78eea431da0264fac586aa1c03d51979e0e1bcacc4d2ffc31e`로 보존했다. tracked 쓰기는
본 문서 append뿐이고 기존 기록은 유지했다. local model/oracle/compiler/LLVM/Cargo/build/test/
Clippy 실행 및 GHA dispatch/cancel은0이다. B2frame 결과를 full128/과거 half/전체 모델 수용으로
확대하지 않았다.

## D writer92 두 후보 구현 — source-only 완료, GHA UNRUN

2026-10-26. 부모가 #523 범위에서 승인한 대상은 helper, 새 `cpu/diagnostic_482_writer.rs`, 등록용 기존
`cpu/diagnostic_482_tests.rs`와 본 문서뿐이다. production ABI/issuer/admission/guarded-add,
공유 lib/Cargo/WF/Focus/pin은 수정하지 않았다. 아래는 **새 source의 설계·정적 검토 결과**다.
actualef172의 C12 PASS를 새 D의 실행 성공으로 옮겨 적지 않는다.

### 기존 private API로 가능한 경로와 원래90개 image의 고정

새 test는 기존 C `native-request.json`과 독립 A request/readonly/Admission을 다시 읽는다.
원본91개 SOURCE/object/image/ABI 및 전체 ISA를 재검증하고, production의 기존
`cached_code::prepare`/`CpuModel::load_cached`를 그대로 호출한다. 잘못된 Admission의 거부와
default `CpuModel::load`의 cached 거부도 유지한다. C certificate/current binding을 바꾸거나 새
writer의 hash를 OriginalProgram/issuer metadata에 끼워 넣지 않는다.

CPU의 기존 `Invocation.function`은 private이지만 같은 CPU test의 descendant에서 접근할 수 있다.
이를 이용해 **strict C를 정상 load한 뒤에만** PROGRAM90/kernel92의 유일한 call175 함수 포인터를
test 전용 adapter로 교체한다. 동일한 C bundle을 후보마다 새로 load해 weights/초기 상태를
같게 시작하며 graph/입력/entrypoint/worker/scalar/view binding도 그대로다. 다른 호출 포인터는
변경 전후 동일함을 검사하고 writer 포인터도 실행 뒤 복구한다. 따라서 다른90개 PROGRAM은
원래 C의 인증된 image다. 앞선 제안의 B 나머지90개보다 이번 부모 지시의 **C 원본90개**를
고정하는 쪽으로 구현했다.

adapter는 이미 검증한10pointer+i32를 동일 ABI로 전달하며 Tensor/Buffer의 값을 관측하거나
lazy allocation을 유도하는 tap이 아니다. TLS dispatch는 library보다 먼저 해제한다. 이 private
교체를 C certificate가 새 writer까지 인증했다는 주장으로 사용하지 않는다. production 경로와
새 public API는 없으며 receipt의 candidate_original_certificate_claim은 false다.

### 두 fresh compile의 실제 context receipt

새 내부 worker `writer-pair-compile`은 A의 고정 원문에서 함수명만 바꾼 `rename-only`와 현재 B와
byte exact인 `guarded` 입력을 만든다. 기존 admitted C helper 사본을 사용하며, Rust에서도 기존
`guarded_add::canonicalize` 결과/네 rewrite receipt를 B sidecar와 독립 대조한다. canonicalizer의
구현·nsz 정책·zero 처리·수학은 그대로다.

한 `CPULLVMCompiler(['x86_64','native'])` 인스턴스/context/TM/PBO로 두 입력을 직접 compile한다.
두 compile 전후의 실제 TM triple/CPU/features, provider path/SHA, compiler source SHA,
`default<O2>`, 네 실제 pass-option setter value를 기록하고 A의 실제 config와 같아야 한다.
`LLVMCreateTargetMachine`의 실제 codegen/relocation/code-model 인자도 관측한다. pinned autogen의
기존 Default/PIC/Default 값2/2/0을 검사하며 다른 값으로 호출을 고치거나 CPU를 강제하지 않는다.
동일 context/TM/PBO identity도 두 후보에서 같아야 한다. `Compiler.compile_cached`는 worker 동안
호출 즉시 거부하므로 성공 receipt의 cache0은 허용된 cache 재사용 경로가 아니다.

여기서 원본 모델 실행0과 원본 compiler 실행0은 같은 뜻이 아니다. 새 worker는 원본 Python
`CPULLVMCompiler.compile_to_obj`를 두 번 호출한다. `compiler-receipt.json`과 최종
`writer-pair.json`은 `original_model_executions=0` / `original_compiler_invocations=2`를
명시하고 native가 compiler receipt의 두 값을 검사한 뒤 최종 receipt로 전달한다. 기존
`original_executions=0` / `new_original_executions=0`은 **추가 원본 모델 실행0**이라는 의미로만
유지하며, 원본 compiler 호출까지0이라는 뜻으로 사용하지 않는다. 추가 원본 compiler2회는
부모가 별도 등록할 **pending execution edge**다. 현재 GHA UNRUN이며 모델 실행0 표기로
이 compiler 의존성이나 실제 호출을 은폐·완료 처리하지 않는다.

### 실행 전 image/ISA/DSO 검사와 두 native 실행

첫 private selector는 아직 생성 code를 실행하지 않는다. 두 object를 기존
`cached_elf::reconstruct`로 독립 재구성하고 같은 strict `cached_isa::admit_all`로 전체 text를
검사한다. 각 실제 필수 feature를 하나씩 뺀 host snapshot은 거부되어야 한다. image SHA,
text bytes/instructions/required/실제 host feature도 기록한다. 기존 mechanical `cached_elf::package`로
한 image만 넣은 container를 만들고, helper가 `clang -shared -nostdlib --no-undefined`로 link한다.

둘째 selector는 DSO의 단일 `op_impl_92` 전체 bytes가 재구성 image와 같고64B 정렬인지 확인한다.
dependency/init/fini/interpreter/TLS/RWX/textrel 경로를 거부하는 **private test-only** 검사를 거친다.
PT_INTERP mutation도 dlopen 전 거부한다. 기존 cached ELF parser의 relocation/whole-image 검사도
적용한다. held FD로만 load하고 실제 mapped symbol bytes를 다시 비교한 뒤 adapter에 연결한다.
이 검사는 새 DSO의 실행 전 경계이며 production validation을 대체하거나 완화하지 않는다.

후보마다 기존 prepare/policy2frame을 실행하여 **기존 B original12출력**과 전체 bytes를 비교한다.
새 원본 모델 실행·golden·tap은0이다. 원본 compiler 호출은2회다. 후보별 raw12파일/실제 SHA/
원본 SHA/equal/mismatch를 남기고
controller가 raw를 다시 읽어 비교한다. 진단 완료와 두 후보의 all12_equal은 별개다. 어느 결과도
수학 수용·B 승격·full128/역사 half/전체 모델 수용으로 간주하지 않는다.

### 기존 gate·자원·회수 경계

- A/B run 본문은 기존 C 진입 전까지 byte exact이고 `Controller.cached_context`의 AST도 동일하다.
  기존 collect/plain/tapped/config/OriginalTap 및 공통 Held/finalizer/retention 메서드는 동일하다.
- 기존 C12가 실제 성공한 경우에만 D에 진입한다. C 실패면 D skipped receipt를 남긴다.
  D 실패는 별도 failure receipt에 보존한다. 그 뒤 원래 `need(not mismatches, ...)`가 그대로
  실행되므로 B sticky는 D 성공/실패 뒤에도 전체 FAIL이다.
- D는 C namespace 아래의 새0700 `writer-pair/`에만 쓴다. **C+D 합계256MiB**를 사전/중간/최종
  검사한다. 별도256MiB를 더 부여하지 않는다. 기존 build+registry8GiB/proof512MiB와 finish
  reserve/전역40분 deadline을 유지한다. D 전체에도180초 deadline을 두고 각 command가 남은
  시간만 사용하게 했다. AS4GiB는 기존 모델 command와 같다.
- 최초 D forecast는 `2*기존12출력 bytes + 4*원문 bytes + 8*원본 image bytes + 32MiB`다.
  ef의 실제 size를 역투영하면44611796B이며 새 실행의 실측값은 아니다. object 생성 뒤 실제
  두 size로 image/container/DSO/output 증가량을 다시 admission하고, 실행 전 output 예산도
  재검사한다. 기존 C helper 두 사본·원래 Cargo hardlink·A/B 원문은 모두 계속 hold한다.
- D files는 기존 C subtree retention에 포함되고 weights/PKL은 추가 업로드하지 않는다.
  기존 source/SDK/provider 전후 봉인과 child 회수는 그대로 적용한다.

### 부모 통합용 selector·파일과 로컬 검증

기존 cached-code **15**와 ISA **14** unit selector/count는 바꾸지 않았다. 다음 ignored private
selector **2개**를 추가했고 부모 private 파일에는 module 등록2행만 넣었다.

```text
cpu::diagnostic_482_tests::writer_pair::fresh_writer_pair_is_bound_without_execution
cpu::diagnostic_482_tests::writer_pair::fresh_writer_pair_reuses_admitted_original90
```

| source | bytes / SHA256 |
| --- | --- |
| `tools/ci/model_current_triplet_482.py` |111582B / `de8c8946d13e0e555f542f0939a32474efa655d8bd5a16dfc19ad240fa16cfc2` |
| `rust/crates/model-runtime/src/cpu/diagnostic_482_writer.rs` |30472B / `51047852eb0c81fa1d804764f6a28198190775d796c86fc28d542ea2c195c9a7` |
| `rust/crates/model-runtime/src/cpu/diagnostic_482_tests.rs` |36066B / `03876312dc0aba59401585613182c567894aba300a3d7ef54efdf57b048564a5` |

주요 새 원문은 `compiler-receipt.json`, `writer-pair-preflight.json`, `writer-pair.json`, 각 후보의
`.ll/.o/.bin/.container.o/.so`와 `outputs/{rename-only,guarded}/`다. 실패/skip/capacity/import receipt도
별도로 보존한다. 부모가 helper/source pin·path trigger·WF publication을 통합한다.

로컬 검증은 Python `ast.parse`, Rust1.94 rustfmt의 구문/서식 확인, source AST/byte 비교와
`git diff --check`뿐이다. 기존 C module/production cpu/cached_code/cached_isa/guarded_add의
전체 SHA가 작업 시작 시점과 같음을 확인했다. Cargo/type-check/build/test/Clippy/compiler/model/
oracle 및 GHA dispatch/cancel/stage/commit/push는 모두0이다. 새 D의 compile/Clippy/실행·수치
결과는 **GHA UNRUN**이며 빈 결과나 source 검토를 실제 성공으로 대체하지 않는다.

## actual0283 — AMD에서 typed/15/14/A/B/C/D 완료, WF16의 이전 helper pin으로 FAIL

2026-10-26. source `0283e6345fc8c8e60cd3537424f1c2df383d9088`,
[run40250301884/job121340512193](https://github.com/bin9208/openpilot-rust/actions/runs/40250301884/job/121340512193).
job은00:46:40 UTC에 완료됐다. 완료 전에는 상태 metadata만 확인했고 running log/artifact는
받지 않았다. 완료 뒤 원시 log `structuredContent.content`와 artifact를 각각 한 번 받았다.

**모델 collect13은 성공이다. 이번 B에는 sticky를 발생시킬 mismatch가 없고 C/D도 성공했다.**
전체 FAIL의 최초 원인은 별도 WF16의 오래된 helper SHA literal이다. 내부 final-seals는
pass=true/failures=[]이므로 모델 수치·D API/type·SDK 변조 실패로 원인을 바꾸지 않는다.

### 실제 host와 단계별 결과

runner는 `GitHub Actions 1000029787`, CPU는 AMD EPYC7763/logical4/x86_64다. 자연 LLVM target은
`znver3`이며 std host snapshot에는 AVX/AVX2/FMA 등이 있고 AVX512는 없다. ISA를 강제하지 않았다.

| 범위 | 실제 결과 |
| --- | --- |
| current-bins | exit0/16.970171초; typed Cargo terminal build-finished/success=true, error diagnostic0 |
| current-libtest | exit0/6.267234초; 새 D를 포함한 test ELF typed build 성공. `--no-run`이므로 전체 test 실행으로 세지 않음 |
| current-clippy | exit0/15.778943초; terminal success=true/error diagnostic0 |
| cached-code15 / ISA14 | 각각15 passed/14 passed, failed0; commands2.930671초/0.117025초 |
| A fresh collect / readonly | exit0/63.717938초 및6.526973초; complete91/calls176/strict_triplet_all=true/host_isa_all=true |
| B original/native plain 및 tapped | 네 command exit0; plain↔tap 동일, original↔native 전체12출력 동일, mismatches=[] |
| C | exporter exit0/18.794240초, native exit0/15.657555초; 기존 original12와 모두 동일 |
| D compile | exit0/2.047835초; 원본 Python compiler 실제2회, compile_cached0/원본 모델 실행0 |
| D inert selector | 1 passed/failed0, command25.225775초; whole-image/ISA/package 검사 완료 |
| D link 두 개 | 각각 exit0/0.116999초 및0.115936초 |
| D native selector | 1 passed/failed0, command47.164490초; 두 후보의 original12 비교 완료 |
| source/index·SDK | 외부 steps14/15 모두 success |
| 독립 후봉인/회수 | 내부 final-seals pass=true/failures=[]/error=null, subreaper=true/live=[] |
| WF16 / required17 | failure; 아래 helper pin 불일치가 최초 원인 |
| upload / cleanup | step18 success; fail-closed로19/20 cleanup은 skipped |

0283의 `cached_isa.rs`는 `pub struct IsaProof`와 `pub use ... IsaProof`를 모두 가진다.
실제 typed compile/Clippy 및 D 두 selector도 통과했으므로 이 타입에 대한 API 수정 근거는 없다.
15/14 stdout은 각각63/64 filtered out이고 D selector는77 filtered out이다. 새 ignored2개를
포함한 실제 등록이 확인되지만 filtered test를 실행 PASS 수에 더하지 않는다.

### D의 실제 동일 context와 서로 같은 object/image

두 compiler 입력은 여전히 rename-only SHA`9781b747d6d436694b7d248a2b29b2fd95ddd78009405674e303a9fa405f6952`,
guarded SHA`ffc2206baa1f05f49a5c9d07b68de71da1a1437485da87d8e4a75d0692a31b52`로 다르다.
`compiler-receipt.json`의 각 compile 전후 config는 서로 같고 **이번 A의 실제 writer chain config와도
정확히 같다.** provider는 `/usr/lib/llvm-18/lib/libLLVM.so.1`, SHA
`5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0`,
triple=`x86_64-none-unknown-elf`, CPU=`znver3`, passes=`default<O2>`다. 실제 LoopUnrolling/
LoopVectorization/SLPVectorization/VerifyEach setter는 모두1, constructor는 codegen2/relocation2/
code-model0이다. target features886B의 SHA는
`6f19f7ff208d429cf27d8e758c295a33bb84253507f368cdfd2896df2f5b69e1`다.

실제 LLVM context94233412818416/TM94233416175536/PBO94233413981872가 두 후보에서 같다.
최종 D와 compiler receipt 모두 original_model_executions=0/original_compiler_invocations=2다.
기존 original_executions/new_original_executions의0은 원본 **모델** 실행0이라는 의미로 유지됐다.
이 추가 원본 compiler edge는 이번 AMD 실행에서 실제 관측됐으며 부모가 별도 원장에 반영한다.

| 두 후보 공통 결과 | 실제 size/SHA256 |
| --- | --- |
| emitted object |21104B / `757b66b27ddbc1f7a90772ced2c722bb6796cdddd58c88c60d4ab3a100dcd762` |
| complete image |19248B, text19042B/instructions3252 / `bd5c40e153a7ef23d17142a5cc660c21be5126d9f5c67f3736f42313410d13a8` |
| packaged container |SHA `17aa3514c1295ca2191780d2f38e1297fae28344a49b9e2f0ecc4d406fa2be44` |
| linked DSO |29568B / `c8d9bbc0cb31931438e491b87f7a17abce8b6a33aaebd5e7c87a690efa1825a1` |

object 두 파일은 서로 다른 inode9438263/9438265, DSO도9438272/9438273이며 nlink1이다.
직접 bytes를 읽어 두 object와 image의 동일성을 확인했다. image는 이번 A original writer image와도
같다. 두 receipt의 writer_address는 같지만 library를 순서대로 load/drop하는 source와 구별해
기록한다. 주소가 같다는 사실을 한 후보 누락이나 다른 compiler 결과로 재해석하지 않는다.
새 image의 실제 ISA requirements는 avx/avx2/cmov/fma/multibyte-nop뿐이다.

### raw12와 원래90개 인증·후봉인의 독립 재대조

각 원본 출력 경로당12파일/5138816B를 읽어 B original plain == original tapped == native == C ==
D rename-only == D guarded를 byte exact로 대조했다. 각 receipt의 size/SHA/equal도 같다.
`model-1.bin` SHA는 `bbebf82fb3e5c20efd6011d5c538b005464e4f9a7eaccc1f2cd1338f44f2732f`이고,
indices100/101/1349의 실제 word는 모두 LE`00000000`(+0)다. B의 target_context_reproduced=false,
original_native_bytes_equal=true, mismatches=[]를 그대로 기록한다. Intel ef의 원본-0/native+0
세 lane 조건을 이번 AMD에서 재현했다고 주장하지 않는다.

A91개 SOURCE/object/image의 size/SHA, 독립 Admission 및 C certificate의 original metadata,
C cached image bytes와 container/library SHA를 다시 대조했다. 새 D는 같은 C에서 원래90개를
고정하고 call175만 교체했으며 production_admission_changed=false/candidate_original_certificate_claim=false다.
두 C helper 사본의 retained bytes/SHA와 UID1001/mode0700/nlink1, 원래 Cargo helper의 before/after
전체 identity/SHA 및 마지막 holds-after도 같다. children live=[]/모든 소유 자식 회수를 확인했다.

D forecast는44650244B, 기존 C namespace79837906B이며 aggregate cap256MiB/전체 D deadline180초를
늘리지 않았다. retained C+D files30235543B와 봉인된 미업로드 weights60410137B를 합하면
90645680B다. 기존 proof512MiB/build+registry8GiB 경계도 유지됐고 해당 자원 실패는 없다.

### 최초 실패: 부모 소유 WF의 단일 stale helper SHA

완료 log의 Python traceback은 `<stdin>:20`이다. published YAML을 parse하여 step16의 heredoc
line20이 다음 검사임을 확인했다.

```python
assert r['helper_sha256']=='379507cb2fc7b9d6c35c83f9851f9b541acad86a9d0085e8bb95cf71e03dfd24'
```

실제 `build-metadata-controls.json`의 helper SHA와0283 Git blob SHA는 모두
`de8c8946d13e0e555f542f0939a32474efa655d8bd5a16dfc19ad240fa16cfc2`다. metadata controls 자체는
positive2/negative12/pass=true다. line20 앞의 final seal/child 회수/regular UID/mode/link/schema
검사는 통과했고 이 pin에서 멈췄다. 뒤의 검사를 실행 성공으로 추정하지 않는다. step17도
SEALS_OUTCOME=failure 때문에 실패했다. 이번에는 B sticky가 전체 FAIL의 원인이 아니다.

최소 수정 후보는 부모 소유 `.github/workflows/python-rust-523-model-current-triplet.yml`의 해당
literal을 실제 발행 helper SHA로 동기화하는 것이다. 현재 worktree의 같은 parsed step에도
379507…가 남아 있음을 읽기로 확인했다. pin/assertion/후봉인을 삭제하거나 성공 처리할 이유가 없다.
WF와 source/math는 수정하지 않았다. AMD에서 두 후보가 같은 사실은 그대로 보존하고, 필요하면
기존 D를 자연 지원 Intel 문맥에서 qualification하는 판단은 부모에게 남긴다. 추가 tap/산술 수정이나
CPU forcing·동일 실패 재dispatch는 제안·실행하지 않았다.

### 보존 증거와 수용 범위

artifact12620264731은 API/download46033247B/SHA256
`90a86e948a01117a7e1afca3ce0b90e1db02ea33a7b54a3014ff8c82ef29fa14`다.60MiB 상한과 실제
가용 공간을 확인하고1411개 safe unique canonical/no-symlink·special·encrypted entry, 전체 CRC,
expanded136702288B를 검사했다. filesystem 추출·원본/compiler/model/ELF 실행은 하지 않았다.

| 실제 receipt | SHA256 |
| --- | --- |
| comparison.json | `c3404bd341f1115da21539424b1ea381ea91d8754b150f60440c1cd3e5543916` |
| cached-context.json | `16ba5c7c5eaa778906d288451e121abccf5f71d55897ea26315cdc564a551a75` |
| writer-pair-preflight.json | `c56a92e2c11ef8ee912f028219524b24b8a11b4a14adb29c32ce4b2f192fda1c` |
| writer-pair/compiler-receipt.json | `aebc3f9300e9218585fbb5b9afda6674923f27da4642b606281e91ed41163160` |
| writer-pair.json | `1c68020af088cf82df3b711261a95185bf5972d30f91c57340184ccb3c090a52` |
| build-metadata-controls.json | `3493325c3cec54e47409279aa10e46877172b4d6621a21eed52b149b71cd61ec` |
| final-seals.json | `54bbcb2e8809ef21daad0410904dabdeb3f1efe469a14dd61923ca076418b6b1` |

exact structuredContent.content log90877B/SHA256은
`c9733b1cf7601695b22ddabb87fcffbbd546506fffbaadd9d2a8eda484725cd9`다. private scratch/ci에
`model0283-current-final-{run,job,artifacts}.json`, `model0283-current-job121340512193.log`,
`model0283-actual-audit.json`, `12620264731.zip`으로 보존했다. actual audit JSON3415B/SHA는
`ba198fc09ce23ac49727c49ffafc8485e4d04efcc3f6f05b9c5865b6b3b09a0d`다.

이 append만 tracked 변경했다. local은 inert metadata/원문/raw 비교와 문서 diff 확인뿐이며,
Cargo/build/test/Clippy/LLVM/compiler/model/oracle 실행 및 CI dispatch/cancel은0이다. 새 source의
typed/Clippy/15/14/A/B/C/D **AMD 실제 성공 범위**는 확인했지만, 전체 job PASS·Intel 숫자 실패
해결·B2frame 밖 full128·과거 half·전체 모델·retirement 수용으로 확대하지 않는다.

## 다음 F02 독립 후보 — offline kernel registry의 production Python 계산 제거 제안

2026-10-26, 읽기 기준 HEAD `ad1e35cf6813dd352f700907489f2edc31e1482e`.
0283 actual 감사는 완료됐고 stale WF pin 교정은 부모 소유다. 이 절은 모델 D 원인 해결이나
Intel 강제 재현이 아닌 **별도 F02 구현1개 제안**이다. 현재 문서 외 source는 수정하지 않았다.

우선 [compiler-emission actual 기록](python-rust-523-compiler-emission-actions.md),
[native emitter 구현 기록](python-rust-523-compiler-native-emission.md),
[앞선 소비자 경계 설계](python-rust-523-compiler-next-closure.md)와 실제 source를 읽었다.
기존 sourceef172/run40245597111/attempt2/job121329475106의 문서화된 수용 범위는 native10/
COPY1/genuine mixed1/order7/strict Clippy와 source·SDK 후봉인·회수다. original-first graph
calls3/kernels1/native launches2, weights64B 및 오류별 lowering/compiler 횟수
`[0,0,1,1,0,0,0]`가 수용됐다. 이 역사 artifact를 새로 받거나 실행하지 않았으며, 해당 기록을
전체 model export/원본 compiler 제거 수용으로 확대하지 않는다.

### 선택한 실제 잔여 계산과 소비자

`build_offline_model_pipelines.py → offline_publish.publish/emit → offline_pipeline.export_pipeline`
재수출 → `offline_schedule.export_pipeline → emit_schedule`가 production 경로다.
`export_model`도 같은 emitter를 쓴다. 현재 `emit_schedule`513–522행은 Python에서 binary SHA/
길이와 `Kernel(name, hash, bytes, arguments)` signature를 만들고, `kernel_ids` 사전으로 index와
첫 대표 PROGRAM/source/binary를 결정한다. 이 계산을 **기존 native stream의 kernel registry**로
옮기고 Python의 해당 hash/dedup/index 결정을 삭제하는 한 slice를 권한다. 별도 helper/process/
collector나 새 compiler를 추가하지 않는다.

`OfflineQcomCompiler`는 `Compiler(None)`을 사용하므로 이 offline 경로의 `compile_cached`는
disk-cache get/put을 하지 않는다. 다만 tinygrad `codegen.do_compile → compile_cached`의
`ASSERT_COMPILE` 거부가 `.compile`과 subprocess 진입보다 먼저다. 이를 cache 포팅 명목으로
child 안으로 옮기면 원래 호출/오류 순서가 바뀐다. 이 slice는 그 guard, 원래 compile process,
render/to_program/lowering, provider와 timeout/error mapping을 그대로 둔다. 모델 D의 원본
Python compiler2회를 이 제안으로 제거했다고 등록하지 않는다.

실제 반환 소비자도 유지해야 한다. `OfflineCompilation.programs`는 Python interpreter 수치
검사에서 원래 emitted IR을 읽고, `offline_publish.emit`은 graph/descriptor digest를 이용한다.
따라서 native는 index뿐 아니라 **첫 대표의 실제 program key**를 반환하고, Python은 이미
강하게 보유한 PROGRAM에서 원래 handle/source/binary를 선택하는 bridge만 담당한다. live
`qcom.py::export_qcom`와 공용 Kernel dataclass는 별도 소비자가 있으므로 삭제 대상이 아니다.

### 정확한 순서·key와 기존 stream의 재사용

현재 순서는 descriptor → genuine to_program/compile → info/aux/device 및 source/binary 검사 →
native launch → ins read → outs write → **Kernel signature/dedup** → globals view/call이다.
새 registry commit은 바로 이 굵은 위치에서 ACK받아야 한다. 초기 Program 등록이나 Launch로
dedup을 앞당기지 않는다. raw arguments를 취득하는 기존 Python 위치도 보존해 aux 오류를
뒤로 미루지 않는다. 모든 read/launch/compile 오류는 미래 compiler 및 destination 생성보다
앞서 거부되어야 한다.

key는 원래 `(function_name, SHA256(binary), len(binary), arguments)`와 같아야 한다.
**SOURCE와 PROGRAM 객체 identity는 signature에 추가하지 않는다.** 서로 다른 PROGRAM/key나
SOURCE여도 같은 signature면 첫 대표를 유지하고, 같은 PROGRAM의 반복 call은 제거하지 않는다.
signature의 name/binary/argument 종류·width/height/pitch/element_bytes 중 하나가 다르면 별도
kernel이다. native가 actual Program binary에서 SHA/length를 만들며 caller가 supplied hash를
expected로 넘기지 않는다.

기존 `--export-stream`에 명시적인 새 wire version과 kernel-commit/reply를 추가하는 방법이 좁다.
기존 v1 stream과 `--export-plan` v1/v2는 그대로 유지한다. `compiler_schedule/export.rs`의
Program SOURCE/binary 보관을 재사용하고 final에 kernel table/첫 program-key 순서를 반환한다.
Python은 wire shape/reference/count만 검증하고 Kernel/hash/dedup의 expected를 재계산하지 않는다.
기존 Manifest DTO와 파일 writer는 반환 native 순서를 소비한다. EOF/빈 stderr/exit0/reap 이후
destination 생성이라는 순서는 그대로이며 native child가 먼저 파일을 쓰게 바꾸지 않는다.

같은 process·owner/sequence/latch/EOF·256MiB 입출력/retained cap·기존 총 frame cap을 유지한다.
새 commit frame과 registry metadata도 기존 예산을 소비하며 cap을 늘리지 않는다. unique registry는
launch 한도65536 안에서 제한한다. metadata memory accounting과 실제 작은 mixed의 wire fit도
검증해야 한다. 전체 모델/128MiB weights의 wire fit과 상위 CLI의 실제 자원 admission은 별도 미수용이다.

### 구현 전 제안 write scope와 수용 조건

제안하는 source 범위는 다음3개뿐이며 **아직 편집하지 않았다**.

- `rust/tools/model_export/offline_schedule.py`: 기존 Python kernel_ids/hash/index 계산 제거,
  native registry 소비와 엄격 decoder/PROGRAM handle bridge.
- `rust/crates/model-runtime/src/compiler_schedule/export.rs`: ordered kernel registry/명시적 새
  stream 계약. provider compile·수학과 runtime QCOM graph API는 건드리지 않음.
- `rust/crates/model-runtime/tests/compiler_export_caller.rs`: 기존 target 안의 직접 core/CLI 경계와
  기존 genuine mixed 검사를 확장. 새 진단 helper/새 standalone WF는 만들지 않음.

수용은 native signature의 첫 대표/같은 key·서로 다른 key/다른 SOURCE·동일 signature/변경된
각 signature field/반복 call 보존, unknown key·누락/중복 commit·failed latch·cap·EOF와 구버전
원문 보존을 포함해야 한다. 기존 SHA 고정 원본 prefix를 original-first로 실행해 graph JSON bytes,
weights, 파일명·순서와 source/binary 전체 bytes, 대표 PROGRAM IR handle을 비교한다. 기존
order7의 오류 우선순위·compile/lowering 횟수와 destination 부재도 같아야 한다. candidate에서
Python Kernel hash를 poison해도 실제 export가 성공하도록 검사하면 남은 Python dedup 소비를
찾을 수 있다. expected를 candidate registry에서 만들지 않는다.

변경 source에 대한 기존 native10/COPY/mixed/order7와 strict Clippy·source/SDK/회수 gate를
부모가 다음 coherent source에 연결한다. 이미 성공한 불변 source를 다시 dispatch하지 않는다.
소비자 비교 통과 뒤 인정할 것은 **production offline kernel registry 계산의 Rust 전환**이다.
파일 전체 삭제 credit은0이며 `offline_compiler.py`/generic compile_cached, UOp lowering,
metadata 취득과 최종 Python writer는 계속 pending이다. 현재 제안 자체의 기능 수용도0이다.

이 절 작성 중 로컬 compiler/oracle/model/Cargo/test/Clippy 실행·CI dispatch/cancel은0이다.
부모의 구현 범위 확정 전에는 위 source나 공유 CLI/lib/Cargo/WF/pin/원장을 수정하지 않는다.

## 2026-10-26 Git6c current actual — 공통 Clippy 전단계 실패

새 판정은 `6c5d5a9be691d18ca850af6ef9b42e47b668d767` Git blob과
[run40255341774/job121355876329](https://github.com/bin9208/openpilot-rust/actions/runs/40255341774/job/121355876329)에만
귀속한다. completed FAILURE다. 과거 source0283의 AMD A/B/C/D exact 결과를 이번 run의 결과로
재사용하지 않았다. 이번 자연 host는 AuthenticAMD / AMD EPYC7763 / vCPU4다. fresh original
compiler가 시작되지 않아 실제 새 LLVM target/options 또는 model numeric context receipt는 없다.

| 실제 도달 label | 결과 |
| --- | --- |
| `source-head`, `source-index` | exit0, source Git6c |
| `current-bins` | exit0, model-run/guarded-add/cached issuer typed build |
| `current-libtest` | exit0, library test ELF의 `--no-run` 컴파일 |
| `current-clippy` | exit101, lib/lib-test 모두 `clippy::large_enum_variant` |
| ISA14 / cached-code15 / A whole91 / B12 / C12 / D pair | **전부 미진입** |

최초 diagnostic은 genuine compiler run과 같은 `compiler_schedule/export.rs:492`의
StreamReply다. JSON stdout에 보존된 두 error의 code와 Git6c source span을 대조했다.
Final variant257B, Kernel41B, enum264B이며 `-D warnings`가 이를 거부했다. model 출력·ISA
admission 실패가 아니다. cached/default/guarded math/expected/tolerance/CPU 정책은 바꾸지 않는다.

artifact12622631825는7,226,422B, SHA256
`0a23161430f151f417f7d3711227f88131498b2eb671c136c79ffb629210f1a2`다.
1회 다운로드 전 API 크기/digest·저장 growth를 admission했고 124 safe unique/nosymlink member,
expanded35,457,153B와 모든 member CRC를 확인했다. 모델 raw 출력은 생성되지 않았으며
새 original/model/compiler 실행이나 과거 archive 다운로드를 하지 않았다.

baselines의 source·SDK·Python·Python-base 네 개는 각각 after JSON과 동일하다. source/index 및
prepared SDK file/program/roster/rejection 원문 쌍도 동일하다. source roster19,623 entry 중
F02 source3·현재 helper·D writer의 SHA를 Git6c blob과 결합해 확인했다. held 검증 실패는 없고
owned children은 전부 회수됐다. `children.live=[]`, subreaper=true다. 반면 최종 봉인은
`pass=false`, phase=`current-clippy`, failures=`["baseline unavailable: registry"]`다.

이 registry baseline은 Git6c helper1297에서 Clippy 성공 뒤에만 설정하므로 최초 실패의 후속
결과다. 이를 optional 처리하거나 gate를 건너뛰는 수정은 필요하지 않다. API source/SDK after
step14/15는 success, independent-required step16과 complete-required step17은 failure,
cleanup step19/20은 skipped다. 회수 성공과 cleanup success를 혼동하지 않는다.

이전 stale post-helper literal은 Git6c WF에서 실제 helper SHA
`de8c8946d13e0e555f542f0939a32474efa655d8bd5a16dfc19ad240fa16cfc2`로 교정돼 있다.
helper 시작 전 SHA check는 실제 PASS다. metadata control2 positive/12 negative도 actual PASS이며
동일 source/helper SHA를 보존한다. 다만 post step16은 앞선 `final-seals.pass` assertion에서
실패했으므로 뒤의 helper literal assertion까지 실행됐다고 주장하지 않는다.

최소 다음 수정은 부모 승인에 따라 별도 compiler-emission slice에서 적용한 StreamReply의
Final `Box<Plan>`화 두 줄이다. wire/content/selector/count와 strict lint는 유지한다. 수정 source의
GHA는 아직 실행하지 않았으며, 그 뒤의 자연 host A/B/C/D 결과는 pending이다.
독립 감사는 private scratch `ci/model6c-actual-audit.json`에 남겼다. 이 기록은 numeric 실패 해결,
새 D12 수용, full128/역사 모델 수용 또는 모델 승격의 근거가 아니다.

## 2026-10-26 source57/69 cohort와 자동 current model 재진입

source `57e1761c5cac9ddee282550d8c079083e6fa9a5a`의 genuine compiler
[run40256938672/job121360705285](https://github.com/bin9208/openpilot-rust/actions/runs/40256938672/job/121360705285)은
최종 SUCCESS다. Box 수정 뒤 library2·caller14/ignored2·COPY1·mixed/order7·strict Clippy와
source/SDK 후봉인·cleanup이 실제 통과했다. 기능/원문/두 ZIP 감사는 compiler-native-emission
문서의 source57 절에 남겼다. 이를 current model의 cached-ISA feature Clippy 또는 A/B/C/D 수치
결과로 확대하지 않는다.

source57의 current model API 조회는 total_count0이다. Git57 WF는 push만 허용하고
workflow_dispatch가 없으며 `compiler_schedule/export.rs`를 paths에 포함하지 않았다.
부모가 보고한 manual422를 로컬에서 재현하거나 dispatch하지 않았다. 해당 source의 current
model actual 결과는0이며, 앞선 source6c의 중단 결과나 source0283의 성공으로 대신하지 않는다.

부모의 source `69d5489463e22a9ef4245cc941a5788f795a8b99`는 current model paths에
`rust/crates/model-runtime/src/compiler_schedule/**`와
`rust/crates/model-runtime/src/cpu/diagnostic_482_writer.rs` 두 개를 추가했다.
Git57↔69의 모든 차이는 workflow3·기록2개로 한정되고 `rust`/`tools`/`tinygrad_repo` tree OID는
각각 같다. genuine WF 전체 blob과 current model jobs도 같다. F02 source3·CLI·현재 helper
`de8c8946d13e0e555f542f0939a32474efa655d8bd5a16dfc19ad240fa16cfc2`·D writer
`51047852eb0c81fa1d804764f6a28198190775d796c86fc28d542ea2c195c9a7`의 blob/SHA를 대조했다.
따라서 source57의 genuine 수용과 source69의 앞으로 실행할 모델을 동일 기능 source cohort로
연결할 수 있다. source seal과 실제 CPU/provider/config·A/B/C/D 결과는 run별로 유지한다.

API에서 source69의 새 push
[run40257284043/job121361745784](https://github.com/bin9208/openpilot-rust/actions/runs/40257284043/job/121361745784)이
생성됐음을 확인했다. 감사 시점 step1~12 success, step13 in_progress, 후봉인·required·cleanup은
pending이다. 실행 중 log/artifact는 다운로드하지 않았다. 아직 cached15/ISA14/A91/B/C12/D pair의
성공·실패나 자연 CPU/LLVM context를 판정하지 않는다. 새 관측·tap·수학/expected 변경·모델 승격0이며
CI 재실행·dispatch/cancel 및 로컬 model/compiler/oracle 실행도0이다.

## 2026-10-26 source69 actual 완료 — Intel에서 writer92 guarded 차이 재현

[source69d5489/run40257284043/job121361745784](https://github.com/bin9208/openpilot-rust/actions/runs/40257284043/job/121361745784)은
completed FAILURE다. 판정은 해당 Git69 blob에 결합하며 부모의 후속 policy 기대 count 수정 WIP와
분리했다. step13 최초 오류는 helper1411의
`strict original/native byte mismatch retained; diagnostic is not numerical acceptance`다.
C/D가 완료된 뒤에도 B mismatch를 sticky로 유지한 결과다. 모든 child command는 exit0이며
이번 실패는 앞선 source6c의 Clippy 중단이나 후봉인 실패가 아니다.

| actual 범위 | 결과 |
| --- | --- |
| current binary/libtest compile, cached-ISA feature strict Clippy | PASS |
| ISA unit14 / cached-code unit15 | PASS / PASS |
| A fresh complete roster91 / calls176 | source/object/image·ABI·host ISA admission PASS |
| B original/native plain↔tap, 각 two-frame output12 | tap 영향 없음. original/native는 model-1.bin의 3 zero sign 불일치 |
| C strict explicit cached original BINARY91 | 기존 B original output12 전체 exact |
| D rename-only writer92 + retained original90 | 기존 original output12 전체 exact |
| D guarded writer92 + retained original90 | model-1.bin 불일치, output12 전체가 B native와 같음 |
| source/SDK/independent after, child recovery, cleanup | step14/15/16/19/20 success |
| complete numerical-required | step17 failure, B strict 기준 유지 |

### 자연 host와 실제 compiler context

host는 GenuineIntel / `INTEL(R) XEON(R) PLATINUM 8573C` / vCPU4다. 원본의 실제 LLVM18
provider `/usr/lib/llvm-18/lib/libLLVM.so.1` SHA는
`5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0`다.
원본 A compile-event90과 D 양쪽 before/after config를 raw JSON에서 대조해 동일함을 확인했다.

- CPU `sapphirerapids`, triple `x86_64-none-unknown-elf`, passes `default<O2>`.
- LoopUnrolling/LoopVectorization/SLPVectorization/VerifyEach가 모두1.
- constructor codegen_level2, relocation_mode2, code_model0.
- 원본 compiler_cpu.py6679B SHA
  `db106591197315ecf28f0e15de99d2841c9892e6e58290213c62c598e7014139`가 Git69와 같다.
  helper558의 `CPULLVMCompiler(['x86_64','native'])`와 원본 compiler_cpu.py99~100의
  LLVM host CPU/features 조회 경로를 읽어 확인했다. 임의 CPU forcing을 추가하지 않았다.

D 두 compile은 같은 worker의 LLVM context/target-machine/pass-options identity를 사용했다.
원본 모델 추가 실행0·원본 compiler 실제 호출2·compile_cached0·새 tap0을 compiler receipt와
최종 D receipt에서 각각 확인했다. A/B의 기존 원본 실행까지0이라는 의미는 아니다.

### B/C/D raw 독립 대조

새 artifact12623466484는45,351,509B, SHA256
`9c4c58a54721d0e8ac0425c38caa2e074adc881ceae6f765ac3c6dd7edc1a65e`다.
API expected bytes/digest와 저장 growth admission 뒤1회 다운로드했다. 1,387 safe unique/
nosymlink member·expanded135,298,652B·모든 CRC PASS다. 과거 archive는 다시 받지 않았다.
로컬은 JSON/IR/ELF section metadata·raw bytes의 inert read만 했으며 실행하지 않았다.

선택된91개의 SOURCE/object/BINARY 각각을 ZIP raw 길이/SHA와 대조했다. C12·D 두 후보12·
B original/plain/tapped/native raw를 이름별 전체 bytes와 receipt SHA에 결합했다. 원본 tap과 C,
D rename-only는 원본12와 모두 같고, D guarded12는 B native12와 모두 같다.

B 유일한 차이는 `model-1.bin`10,304B의 byte offset `[403,407,5399]`, f32 index
`[100,101,1349]`다. 세 값 모두 원본 `00000080`(-0), native/guarded `00000000`(+0)다.
다른11개 출력과 model-1의 다른 bytes는 같다.

| raw model-1 결과 | SHA256 |
| --- | --- |
| original/plain/tapped, C, D rename-only | `7fe2075ebcc4048be72666c5fc101238df672d438dfb849507251293d4c43242` |
| B native, D guarded | `bbebf82fb3e5c20efd6011d5c538b005464e4f9a7eaccc1f2cd1338f44f2732f` |

D의 교체 call은175, selected roster index90/kernel92다. 원본 SOURCE166,049B SHA는
`e223a2c36c6bde7cf676bb92fd2529858a723f9ecfa91c811b2b941756a9bea0`다. 원본 함수명
`r_322_2_4_256_32`를 `op_impl_92`로 바꾼 것만 D rename-only SOURCE와 다름을 전체 문자열로
확인했다. 동일 원본 compiler context에서 아래 실제 object/image를 얻었다.

| writer | SOURCE SHA256 | object SHA256 | image SHA256 |
| --- | --- | --- | --- |
| rename-only | `9781b747d6d436694b7d248a2b29b2fd95ddd78009405674e303a9fa405f6952` | `bf56a30c37f0128497b626329df7239dd7a1c4b5dd9ec728d501f2c11deebea1` | `324697e4d28a400da3c521fea2f9140eea04d50f19e6771a96549f4b8a5a6767` |
| guarded | `ffc2206baa1f05f49a5c9d07b68de71da1a1437485da87d8e4a75d0692a31b52` | `a8fbf958e61e4424822a54512cd618e629308479ea16c05b3bd38896d47441da` | `fcec802c0d092d54ceab436f6cd095152cdce1ddf6f8e0ebf0cf2e7a00902b09` |

원본 A image도 rename-only image SHA와 같다. ELF64 section header만 읽어 object의 `.text`를
독립 추출했고 원본 A와 rename-only는14,244B·SHA
`d378323b17a16fb771353d05773eee890c44898f22b048450b22634233e0f7c9`로 동일하다.
guarded는14,260B·SHA `4f2bf877082d043f87752fa33d2edb1c41ebca26c262745b7b943786a5c7ba42`다.
기존 ISA receipt의 instruction count는 각각2410/2411이며 양쪽 모두 같은 실제 host 요구를 통과했다.
로컬 disassembler/LLVM/compiler 실행은0이다.

### 좁혀진 원인과 다음 검토 범위

두 D IR의 전체 차이는 네 군데다. source line4231/4302/4374/4446 부근의
`fadd nsz arcp contract afn a, (select cond, +0, b)`를 guarded가
nonzero `fadd arcp contract afn a,b`, zero `fadd a,+0`, 후속 select로 펼친다.
현재 artifact의 정확한 IR delta이며 수학이나 expected를 새로 계산한 결과가 아니다.

C 성공만으로 ABI 일반화를 하지 않는다. 이번에는 다른90 PROGRAM 고정·동일 원본 compiler
context의 D pair가 rename-only exact / guarded=B mismatch를 재현했다. 따라서 이 two-frame의
strict byte parity 차이는 writer92의 해당 guarded rewrite로 좁혀진다. 원본/fresh context라는
이름만 바꾸거나 CPU/provider/issuer admission 기준을 조정해서 해결할 근거는 없다.

다음 최소 source 검토 후보는 `guarded_add.rs`의 이 `nsz` zero-arm canonicalization 계약이다.
원래 select/fadd의 부호 선택과 기존 보호 목적을 함께 보존하는 구체안을 먼저 검토해야 한다.
현재 자료는 세 output index가 네 rewrite 중 어느 SSA/lane·기계 instruction에 대응하는지까지
확정하지 않는다. 새 tap·추가 original 실행·재dispatch 없이 보존된 두 IR/object부터 검토할 수 있다.
이번 감사에서 production/math/expected/threshold/CPU/ISA 소스 변경은0이다.

### 후봉인과 수용 한계

raw source/SDK/Python/Python-base before/after snapshot 및 source/index·prepared SDK manifests/
roster/rejection 쌍이 동일하다. held 검증 실패가 없고 C helper copy 원본 불변도 확인했다.
registry의 before는 helper 메모리 baseline에서 after와 대조됐으며 final `pass=true`, failures=[],
all_owned_children_reaped=true, children.live=[]다. source/SDK required step16 success이므로
이전 stale helper pin assertion도 이번에는 실제 통과했다. B mismatch가 있어도 후봉인·cleanup이
성공한 사실과 최종 numerical gate failure를 분리한다.

독립 대조 결과는 private scratch `ci/model69-actual-audit.json`에 보존했다. 같은 source cohort의
F02 Clippy 수정은 cached-ISA feature에서도 PASS다. D compiler2의 Python 실행 edge는 별도
pending이며 production cached default-disabled·strict admission·B sticky를 유지한다.
full128·역사 half/sNaN5·oldPR1344/frozen context·전체 모델·기기 수용 및 퇴역 credit는0이다.
로컬 model/oracle/compiler/Cargo/test/Clippy 실행·GHA dispatch/rerun/cancel·stage/commit/push0이다.

## 2026-10-26 #482 original-parity caller policy repair — GHA 미실행

부모가 승인한6개 source만 수정했다. source69 D에서 확인한 writer92 guarded 차이를 근거로,
native fresh capture의 호출 정책을 원본 flags 보존으로 분리한다. standalone canonicalizer의
엄격한 zero/sNaN 의미를 바꾸거나 특정 kernel/hash/CPU를 예외 처리하지 않는다.

### 실제 호출 정책과 증거

`native.py::compile_kernel(..., preserve_original_flags=False)`의 기본값은 기존 strict 경로다.
`capture.py::capture_cpu`의 실제 fresh compile 호출만 `True`를 명시한다. 일반 fresh와 cached
모델의 fresh 보조 kernel 모두 같은 호출을 지나며, 기존 `fresh_in_cached` symbol namespace도
보존한다. 새 사용자 CLI·환경변수 toggle은 없다. `True`이면 기존 단일 함수명 rename 뒤의
SOURCE를 fast-math flags/select/fadd 변경 없이 실제 `compile_to_obj`에 전달한다.

기존 paired helper가 설정돼 있으면 parity에서도 helper ELF/FD/SHA/identity를 compile 전후
검증하고 동일16MiB source admission을 적용한다. `_guarded_add` subprocess는 실행하지 않는다.
기본 False는 기존 helper 실행/receipt/failure/cleanup 경로를 그대로 사용한다. 두 helper env가
모두 없는 legacy 경로의 rename-only/no-compilation-evidence 동작도 보존했다. 실제 #482 경로는
admitted helper가 설정돼 있으므로 아래 parity 증거가 반드시 생성되고 D에서도 검증된다.

`compile.py::export_cpu`는 실제 compiler 입력을 기존 `.compiled.ll`에 남긴다. parity는 별도
`kernel-N.source-policy.json`을 쓰며 binding의 `source_policy=preserve-original-flags`,
`source_policy_receipt_sha256`로 묶는다. receipt는 원본/rename/compiled SHA와 길이,
symbol_rename_only=true, math_rewrite_applied=false, helper_executed=false를 기록한다.
canonicalizer를 실행하지 않은 것을 `unchanged/rewrite_count=0` strict receipt로 위장하지 않는다.
strict 경로의 `.guarded-add.json`과 기존 receipt 형식은 유지하며 unknown policy는 거부한다.
기존 `CompilationEvidence` DTO와 compilation-evidence index version1을 유지하고 per-kernel
binding으로 정책을 구분한다. graph/ABI/cached certificate 형식은 바꾸지 않았다.

### 기존 D 두 후보 유지

`Controller.writer_pair`의 compile request는 인증된 A original·B parity `.compiled.ll`·B
source-policy receipt·A compiler config를 전달한다. `writer_pair_compile`은 B 입력이 정확한
A 함수명 rename과 canonical parity receipt인지 먼저 결합해 확인한다. 같은 기존 held strict
helper에서 guarded 후보를 생성하고, 그 실제 receipt를 D namespace의 `guarded-add.json`으로
보존한다. D compiler receipt는 이 파일의 path/bytes/SHA를 추가로 봉인한다.

Rust `diagnostic_482_writer.rs::load`는 B가 rename-only인지, original/compiled/object/helper SHA와
policy receipt가 맞는지 확인한다. D guarded SOURCE/receipt는 A에서 계산한 기존 strict canonicalizer
결과와 대조한다. 따라서 예전의 'D guarded가 B guarded와 같아야 함' 의존을 제거하면서
strict 후보의 원문 검증을 유지한다. A 원본91 Admission·C baseline·다른90 PROGRAM·call175
교체 경계·같은 자연 compiler context 검사는 그대로다. 새 E/F stage나 tap은 없다.

### 회귀 검사·실행 edge와 수용 gate

기존 `catalog` target은 **normal4 / ignored3**, selector 이름도 그대로다. hosted bridge 안에
아래 사례를 추가하거나 실제 새 정책의 증거 파일 이름에 맞춰 검증을 연결했다.

- 두 zero-arm·두 operand-order의 네 조합을 일반/fresh-cached namespace에서 검사한다.
  compiler에 전달된 SOURCE·남은 flags·policy/hash를 원본 rename과 대조한다.
- `_guarded_add`를 poison해 parity가 strict rewrite를 호출하지 못하게 한다. 원본에 이미 있는
  diagnostic namespace도 parity에서는 보존한다. strict 기본 경로의 collision 거부는 유지한다.
- parity source cap, paired helper SHA/path·native ELF admission, compile 후 helper identity 변조
  거부를 검사한다. 기존 strict helper 오류/descendant/timeout/overrun 검사는 유지한다.
- 실제 TinyJit capture/export의 모든 fresh 호출에서 True 전달·COPY/evidence 정렬·source-policy
  파일/실제 object hash를 확인한다. helper 없는 legacy output roster도 유지한다.
- publisher의 전체 immutable file roster/reuse 거부를 유지하고 source-policy 증거를 확인한다.
  combined caller proof는 guarded_receipts=0, source_policy_receipts>0,
  guarded_helper_executions=0을 실제 새 정책으로 기록한다.

**새 process entrypoint/실행 edge는0개**다. production parity는 기존 kernel당 strict-helper
subprocess를 생략하며 compiler 호출은 그대로다. D의 Python worker·strict helper1회·원본
compiler2회/model0·compile_cached0·native prepare/execute 두 selector·linker 호출 수는 그대로다.
새 D 파일은 strict receipt 하나이며 기존4MiB receipt admission·C+D256MiB 실제 합산과 forecast,
180초 D/global deadline·artifact512MiB retention에 포함된다. cap/deadline은 늘리지 않았다.

hosted bridge의 기존 Python process 안에서 추가되는 LLVM `compile_to_obj` 호출은10회다.
그 중8회는 네 조합×두 namespace,1회는 원본 namespace 보존,1회는 compile 후 helper 변조
거부다. 생성 object는 실행하지 않는다. 추가 original model 실행이나 추가 test process/helper
subprocess는 없다. 이10회도 로컬 실행0이며 부모 GHA에서만 수행한다.

standalone `guarded_add.rs`, CLI, 일반13·CLI4·256행 strict zero/sNaN 검사와 `cached_code.rs`
issuer/Admission은 바이트 그대로다. A/B/C orchestration·B 전체12 exact 비교·plain/tap parity·
sticky failure·후봉인·회수/caps/deadline 함수 AST도 보존했다. 다음 actual에서 Clippy·ISA14·
cached15·A91·B12 exact·C12·기존 D 두 후보 완결과 source/SDK/child gate를 확인해야 한다.
어떤 B 차이도 계속 실패하며 D strict 후보의 차이는 숨기거나 B 수용으로 대체하지 않는다.
source69 D의 writer 단일 교체 성공을 이번 전체 fresh91 parity 성공으로 미리 올리지 않는다.

### 부모 통합용 source와 로컬 확인

| 변경 파일 | SHA256 |
| --- | --- |
| `rust/tools/model_export/native.py` | `f83f5541a832a316c6b2ceb1b10578cfd9da99632f24ec593aea0728497d2a6c` |
| `rust/tools/model_export/capture.py` | `a44a5a0d911b9f7dded1dbdfb4eeb454baf53cce6738ae9cd2e0e88212b1fece` |
| `rust/tools/model_export/compile.py` | `1aba5d607faff99fc19fe7230c5d36ce87ce584cc14814b18d6908ae1f938237` |
| `tools/ci/model_current_triplet_482.py` | `81d2e8ee3ad1a4d905d014da17425c5446d0377ef5bbdf9c3cbafe0928b3776d` |
| `rust/crates/model-runtime/src/cpu/diagnostic_482_writer.rs` | `bed89b27feee9b0a05f7a8ba34a9d696bf3639af52374d63c55d697091f293ce` |
| `rust/crates/model-runtime/tests/catalog.rs` | `c6f45c288e8144da8889ee7cfbf793afbebce205893917f123c4f66d7b83d3dc` |

부모가 갱신할 기존 literal은 current-triplet WF의 MODEL482_HELPER_SHA256과 post helper assertion,
그리고 python-rust-523.yml의 helper/D writer/catalog source pin이다. catalogue count/selector 및
standalone strict count 변경은 없다. exporter source-roster는 기존 전체 source 봉인을 계속 사용한다.
Focus/ledger/pins/WF/Cargo/lib/외부 registry는 수정하지 않았다.

로컬은 production4 Python·embedded3 script AST, Rust1.94 rustfmt/check, diff/check와 불변
함수·strict source/test/issuer bytes 대조뿐이다. Cargo/type/build/test/Clippy/LLVM/compiler/model/
oracle·CI dispatch/rerun/cancel·stage/commit/push0이다. source 구현 완료와 actual 수용을 구분하며
전체 모델/full128·historical half/sNaN5·oldPR1344/frozen·기기 수용 및 퇴역 credit는 여전히0이다.

## 2026-10-26 catalog 정책 회귀의 current branch 실행 연결 — GHA 미실행

정적 선택 경로를 확인한 결과 새 정책 회귀가 들어간 `catalog` ignored3은 `rust.yml`의
model-pipelines step20에서 ELF를 만들고 step22/26에서 실행하도록 연결돼 있었다. 그러나 그
workflow는 dev/main push·PR·manual 경로이며, 이번 작업 branch의 current helper는 lib ELF의
ISA14/cached15만 실행했다. `Clippy --tests`는 타입검사일 뿐 이3개의 실제 실행 증거가 아니다.
native-units의 수동 `run_once` 목록에도 catalog가 없었다. 부모가 이 gap을 확인하고 기존
controller를 통한 catalog no-run1회와 기존 exact selector3회의 추가를 승인했다.

### 선택·cache·증거 경계

`Controller.run`에 `cargo test ... --test catalog --no-run --message-format=json`을 추가한다.
`common_args`의 현재 SDK/locked/no-default-features/cached-program-isa를 그대로 쓰며 기존
`select`가 Cargo package/source/features/profile과 유일한 executable을 검사한다. 같은 JSONL에서
`catalog` kind=test/profile.test=true와 실제 `publish-model-catalog` bin/profile.test=false를
고른다. 두 ELF는 기존 hold/DSO 검사에 추가하며 third selector에는 이 publisher path/SHA를
명시한다. helper를 추측한 경로나 과거 ELF로 대체하지 않는다.

각 selector는 `$CI_JOB_TEMP/build/target/catalog-policy/{0,1,2}`의 서로 다른0700 private root,
빈 `xdg`·`tmp`, 처음에 없는 `cache.db`·`proof`를 받는다. command별 env copy에만 `CACHEDB`,
`XDG_CACHE_HOME`, `TMPDIR`, `MODEL_BRIDGE_PYTHON`, `MODEL_PUBLISHER_POLICY_PROOF`를 넣는다.
controller의 `self.env.CACHEDB=$CI_JOB_TEMP/build/model482/original-cache.db`는 바꾸지 않는다.
각 selector 전후에 그 A cache가 absent/!symlink인지 검사하므로 catalog가 A worker의 fresh-cache
조건을 오염시키면 즉시 실패한다. 임시 파일 삭제는 이미 허용된 mutable target child 안에서만
발생하며 build scanner의 예외 범위·3회·root identity·cap·기한을 확장하지 않는다.

first selector의 기존 C peer는 executable maps에 표본 관측될 수 있다. 종전에는 그 peer 자체도
`TemporaryDirectory` 종료 시 삭제돼 원래 경로까지 끝에 확인하는 `Holds.verify`와 충돌했다.
테스트의 peer 배치 한 줄만 `TMPDIR/catalog-peer-*/peer`의 retained directory로 옮겼다.
실제 C 원문/clang 호출/5개 success·failure 모드/descendant 검사는 그대로이며 pid·receipt
임시는 기존대로 삭제한다. 이 작은 ELF는 job 정리까지 남아 기존 FD/path/SHA 최종 hold를
통과해야 한다. maps/command/공통 Holds/최종 후봉인에 삭제 허용이나 skip을 넣지 않았다.

command stdout/stderr와 Cargo JSONL은 기존32MiB 로그 경계로 보존한다. selector별
`catalog-policy-inputs-N.json`은 source·cache/proof 경로와 catalog/publisher/Python/guarded
실제 path·길이·SHA를 기록한다. selector3/3을 확인한 뒤 `catalog-policy.json`을 쓰며 third의
실제 `complete.json`은4MiB 이내로 `catalog-policy-publisher.json`에 복사한다. 이 파일도 기존
proof512MiB budget에 들어간다. selector마다64MiB 추가 growth를 기존8GiB build/registry cap
안에서 admission하고, 기존 no-run900초·selector180초·global40분 및 finalizer reserve를 따른다.

### 추가 실행 수와 변하지 않는 gate

추가 test 최상위 command는 정확히4개다.

1. `current-catalog-test`: catalog no-run1회.
2. `catalog-policy-0`: `guarded_add_bridge_retains_original_and_compiled_source_evidence` exact1/0/0.
3. `catalog-policy-1`: `guarded_add_publish_reuse_requires_exact_evidence_roster` exact1/0/0.
4. `catalog-policy-2`: `native_publisher_with_guarded_emitter_preserves_binding_policy` exact1/0/0.

별도로 새 두 ELF의 기존 DSO closure 검사를 위해 `provider-dso-9/10` 두 command가 추가된다.
위4개에 이2개의 provider 검사까지 합하면 controller command 증가는6개다. 새 Python callsite는
0개이며, 이미 있던 catalog의 Python3 callsite를 branch에서 각각1회 활성화한다. 그 안의 기존
clang native-peer compile, strict helper/오류 통제, linker·publisher·negative CLI도 함께 수행한다.
상단 caller-policy 구현 기록의 process-edge0은 이 branch 실행 연결 전 source 변경만의 범위다.

실행 대상은 원본 driving model이 아닌 기존64원소 COPY/산술 toy와 두16원소 publisher toy다.
publish/reuse 검사는 기존 반복 횟수 그대로이며 third의 tiny emission은 기존1회다. first의 신규
원본 flags object compile10회도 위에서 기록한 그대로다. full model/original model 실행 추가0,
A91·B output12 exact·C12·D compiler2/model0·ISA14·cached15·standalone strict13/CLI4/256은
유지한다. catalog는 normal4/ignored3, D는2 selector로 count·이름 변화가 없다. E/F 단계,
새 CLI/env policy toggle, Rust NativeRunner bridge나 CI framework를 추가하지 않았다.

최종 source pin은 다음 두 파일에서 갱신된다. 나머지4 source SHA는 바로 위 표 그대로다.

| 파일 | bytes / SHA256 |
| --- | --- |
| `tools/ci/model_current_triplet_482.py` |116096 / `dafe96d44bb819480427ecb30c7f726a910a0650648ff9abfdaa974ea85afda4` |
| `rust/crates/model-runtime/tests/catalog.rs` |33133 / `cc765f59a2150cae27db0c63c561a9ae19f54813c2ff9602060ae9a3fed7d37b` |

부모의 WF helper2곳+focused helper1곳과 catalog1곳 pin이 이 최종 SHA를 사용해야 한다.
D writer pin은 위의 `bed89b27...` 그대로다. 로컬은 AST·rustfmt/check·diff/check·불변 source 대조만
했으며 Cargo/test/Clippy/LLVM/compiler/model/oracle 실행0이다. 이 연결의 실제3/3, A cache 부재,
provider/finalizer 후봉인과 B 전체12 exact는 다음 coherent source의 GHA에서 확인해야 한다.

## 2026-10-26 d666 actual 감사 — receipt 경로 오류와 #476 수치 실패 분리

대상 source는 `d6667936845ba90b2b897c1b75f9af173131e5c1`이다. helper/catalog와 관련11개
소스·WF를 이 Git blob에 묶어 대조했다. 이후 다른 WIP를 actual 근거로 사용하지 않았다.

### current-model: 수치 비교 전 receipt 작성 실패

[run40264806443/job121384756130](https://github.com/bin9208/openpilot-rust/actions/runs/40264806443/job/121384756130)은
step13과 required step17에서 실패했다. 실제 최초 예외는 source1374의 catalog 입력 receipt
작성 중 `spec_file(self.m, python, 512*MIB)`에서 발생했다.

`RuntimeError: bounded regular file: /tmp/openpilot-ci.SFXKfS/venv/bin/python`

venv의 Python alias는 이미 정상 admission된 symlink다. `holds-after.json`은 그 alias가
`/opt/hostedtoolcache/Python/3.12.15/x64/bin/python3.12`로 resolve되고 실제 provider SHA가
`53dab24bf9e0e9c490088a53d71770b6f027b057678b313648e75f7fc87a675e`임을 보존한다.
새 receipt 코드만 이 alias를 `lstat` regular-file 경계에 직접 넘긴 통합 오류다. Python 자체의
admission 실패, cache 오염, cap 초과나 원본/native 수치 실패로 분류하지 않는다.

| 실제 완료 단계 | 결과 |
| --- | --- |
| current-bins / libtest / catalog no-run | PASS; catalog no-run1.193초 |
| strict Clippy --lib --tests | PASS; command16.766초 |
| catalog/publisher DSO provider-dso-9/10 | 둘 다 PASS |
| cached ISA unit | 14 passed/0 failed; filtered66 |
| cached_code unit | 15 passed/0 failed; filtered65 |
| catalog exact3 | **0회**; first inputs JSON 작성 전에 예외 |
| A whole91 / B12 / C12 / D2 | **미진입** |
| source/SDK/Python/child 후봉인 | PASS; failures=[]/all_owned_children_reaped=true |
| artifact upload / source·job cleanup | step18–20 모두 PASS |

`final-seals.phase`에는 직전 성공 command의 `cached-code-controls`가 남아 있다. 이 문자열만
보고 해당15개 unit이 실패했다고 판단하면 안 된다. `commands.json`의43개 command는 전부
exit0/회수 성공이며, stack trace가 다음 command 전 receipt 작성에서 난 첫 오류를 명시한다.
`catalog-policy-0.stdout`, `triplet-request.json`, `comparison.json`, C/D receipt는 없다.
A collect 전이므로 이 run의 자연 CPU 모델·LLVM natural target/context receipt도 생성되지 않았다.
runner 이름은 `GitHub Actions 1000029894`일 뿐 Intel/AMD나 whole91 ISA 수용으로 추정하지 않는다.

source-before/after digest는 둘 다
`db8476bfd7314c1d50eb38fe1fb4e641499044443a9cc01c1c1424c39b7d9cf9`다. artifact의
source/SDK/Python/Python-base baseline도 각각 after와 독립 대조해 동일함을 확인했다.
별도 registry seal과 모든 held provider/alias 검사는 실제 final-seals.pass=true로 완료됐다.

artifact12625154282는7,483,487B, SHA256
`fdc7bd036bc797a11e5bbd78d93bcec8f58a85c7152a68348e6c159bf98b9e5f`다. API expected
bytes/digest/source를 먼저 확인하고 ZIP을1회만 받았다. 285개 safe unique/nosymlink·암호화 없음,
총36,986,179B·CRC285/285를 확인했다. raw model blanket dump나 원본 실행은 하지 않았다.
완료 log105,308B의 SHA는 `b4c2d03f73109b00bb0a3b577162fc8a4613ce988662df3a72c24ba6117fd2bc`다.

### 부모 승인 후 최소 source 수정 — 아직 GHA 미실행

부모가 실제 원인 보고 후6파일 범위의 단순 integration fix를 승인했다. helper의 위 한 식만
`spec_file(self.m, python.resolve(strict=True), 512*MIB)`로 바꿨다. 실행용 Python alias,
MODEL_BRIDGE_PYTHON·기존 alias/FD/SHA 최종 hold와 paired helper 검증은 그대로다. `regular`
검사 자체·source/SDK·cache 부재·기한/cap·CPU/ISA·수치 비교는 완화하지 않았다. 새 실행 edge0,
catalog no-run1+exact3와 DSO2 및 기존 모델/D 실행 수도 변하지 않는다.

수정 후 helper SHA256은
`ddf48345a1d358b80ce7296ffd42293e3992b7194808e476750914aea593f19d`다. 다른5개 source는
d666 blob과 동일하다. 부모 통합 때 current WF admission/post helper2곳과 focused helper1곳의
기존 literal이 이 SHA를 사용해야 한다. catalog pin은 `cc765f59...` 그대로다. 로컬 AST와
diff check, d666 대비 정확히 한 식 변경인 것만 확인했다. 로컬 model/compiler/Cargo/test/Clippy,
CI dispatch/rerun/cancel·stage/commit/push0이며, 이 수정으로 actual catalog3/B12 PASS를 주장하지 않는다.

### 새 model-family: old84와 같은 #476의 큰 driver 수치 차이

[run40264806375/job121384755741](https://github.com/bin9208/openpilot-rust/actions/runs/40264806375/job/121384755741)은
step19의 `same_elf_replays_all_three_native_clis_in_both_absent_roots`에서 실패했다. 실제 첫 오류는
`model_family_retirement_support.rs:253`가 child의 exit1 stderr를 전달한 것이다.

`driverStateV2.driverStateV2.leftDriverData.faceOrientation[0]: original driver float changed`

| 항목 | 실제 값 |
| --- | --- |
| expected | -0.08113645017147064 |
| observed | -0.08065624535083771 |
| 기존 tolerance | 0.000001 |
| absolute_difference | 0.00048020482063293457 |

이는 부모가 정정한 old84 수치와 동일하다. signed-zero bit 차이나 #482의3 lane으로 축소하지
않으며 새 raw-flags 정책이 #476을 고쳤다고 주장하지 않는다. exact-bits assert라는 이전
분류도 적용하지 않는다. step14 grouped compile/regression/Clippy, step15–18 prerequisite/
helper binding/catalog export/USB staging은 PASS였다. step20–27의 raw sidecar, 최종 provider/
source guard, 업로드와 cleanup도 전부 PASS다. 전체60 retirement 수용은 실패 상태다.

이 분류에는 완료 log219,054B만1회 받아 사용했다. SHA는
`2e394483884c583bbb4ceaa8589af68a75cc0fa709aa6cc9c57b8f7a106d4125`다. old84 log/ZIP를
재다운로드하지 않았다. 새 family ZIP도 받지 않았다. 주 data artifact162,738,726B는 이번
100MiB archive admission 범위 밖이며 최초 수치 실패 분류에는 필요하지 않다. 새로운 driver
수학/기준/expected나 추가 관측 hook은 변경하지 않았다.

### genuine compiler: 같은 d666 source의 별도 성공 범위

[run40264806390/job121384755486](https://github.com/bin9208/openpilot-rust/actions/runs/40264806390/job/121384755486)은
SUCCESS다. library metadata2, caller14/ignored2, actual COPY1, genuine mixed1+order7,
strict Clippy가 모두 exit0이다. comparison의 Python kernel hash/binary SHA poison과 첫 IR 대표
대조는 true이며 원본 native 호출0이다. 원본 prefix SHA
`bc1b7b572c8dd2f84f3ea0ca38b1744e4c7120155c33f3dabb633eabaf7f0144`를 유지한다. artifact에서
original/candidate program IR123B의 전체 bytes 동일성과 order7의 lowering/compiler counts
동일·실패 시 destination 부재를 별도로 재검사했다. 이 성공은 model current B12나 family60의
성공 증거로 대체하지 않는다.

주 artifact12626256266은18,011B, SHA256
`6e23d063a68a9146bdd565ece03d65fa4a12a33260c28281b42c25d8b98a594d`다. expected bytes/
digest/source를 admission한 뒤1회 다운로드했고 safe unique/nosymlink26개, 총43,223B,
CRC26/26을 확인했다. main artifact 외 private-workspace ZIP은 받지 않았다. 실제 step20은 source
index/hash와 전체 SDK file SHA/identity roster before/after 및 LLVM pin을 비교해 PASS했고,
step21–23 upload/cleanup도 성공했다. 완료 log191,046B의 SHA는
`83dad81f1bd06c9a6ee1c4b425cf90a3a8f368e60c77aaf3271053f00c6e8971`이다.

compact 독립 감사는 private `ci/model-d666-actual-audit.json`, `ci/model-family-d666-actual-audit.json`,
`ci/genuine-d666-actual-audit.json` 및 각 ZIP admission JSON에 남겼다. 이 기록 외 공유 WF/Focus/
ledger/pins/registry는 수정하지 않았다. 전체 모델/full128·historical half·기기 수용 및 퇴역 credit0을 유지한다.

## 2026-10-26 source495 actual — catalog3와 B12 원문 bytes 수용

source `495860d144770603c1d5a1df0f4d0a7ffc26a83f`의
[run40266155898/job121388936320](https://github.com/bin9208/openpilot-rust/actions/runs/40266155898/job/121388936320)은
SUCCESS다. helper SHA는 `ddf48345a1d358b80ce7296ffd42293e3992b7194808e476750914aea593f19d`이며,
다른5개 모델 source는 d666 blob과 바이트 동일함을 먼저 확인했다. 이 감사 중 모델 source는
수정하지 않았다. d666 genuine PASS는 재사용했고 해당 로그/ZIP를 다시 받거나 실행하지 않았다.

### 완료 artifact의 독립 admission

artifact12626760941은45,685,277B, SHA256
`5b0ecec82f6f86667bd818de66c1d5011ba07d3485e2c8c58ef005e813cf43ad`다. API의 expected bytes,
digest, run/source/expired 상태와100MiB download/512MiB uncompressed admission을 확인한 뒤
1회 다운로드했다. safe unique/nosymlink·암호화 없음1410개, 총136,643,272B, CRC1410/1410을
통과했다. `retained-index.json`의960개 파일88,911,692B도 길이와 SHA를 모두 직접 대조했다.
model PKL/full weights를 받아 실행하거나 raw model 데이터를 blanket dump하지 않았다.

완료 log105,284B도1회만 받았다. SHA256은
`9d067368809a57111a85ff5dcddd37d13ea628fc57daa23373c7b58c93fdefe0`이다. 실행 중 로그는 받지
않았다. compact 감사·raw output별 전체 SHA는 private `ci/model495-actual-audit.json`, source
결합은 `ci/model495-source-cohort.json`, ZIP admission은 `ci/12626760941-495-zip-admission.json`에 있다.

### catalog3·선행 gate·cache 격리

| 실제 command | 결과 / controller elapsed |
| --- | --- |
| catalog no-run | PASS /0.954초 |
| strict Clippy --lib --tests | PASS /11.402초 |
| ISA unit14 | 14/0/0 /0.112초 |
| cached issuer unit15 | 15/0/0 /1.651초 |
| catalog-policy-0 기존 bridge selector | exact1/0/0 /32.162초 |
| catalog-policy-1 기존 publish/reuse selector | exact1/0/0 /1.785초 |
| catalog-policy-2 기존 combined caller selector | exact1/0/0 /2.209초 |

catalog target normal4/ignored3 중 이번에는 기존 ignored3만 각각1회 실행했다. metadata normal4가
실행됐다고 합산하지 않는다. 서로 다른 target/catalog-policy/{0,1,2}의 cache.db/xdg/tmp/proof를
받았고, `catalog-policy.json`의 selectors_passed=3/a_cache_absent=true를 확인했다. 실제 A collect도
cache absent guard를 통과했다. third receipt는 absent model root 전에 bad publisher를 거부했고,
negative CLI exit1·tiny emission1·source-policy receipt1·strict helper executions0을 기록한다.

입력 receipt의 Python은 canonical provider
`/opt/hostedtoolcache/Python/3.12.15/x64/bin/python3.12`, SHA `53dab24b...`로 일관되게 기록됐다.
실행 alias와 기존 hold는 유지됐으며 d666의 receipt 경로 오류가 이번 실제에서 해소됐다.
first native peer1개도 retained 경로와 SHA가 `holds-after.json`에 남아 있다. 삭제된 provider를
허용하거나 maps/최종 hold를 skip한 성공이 아니다. 기존4 test command+2 DSO 검사만 추가됐고
새 Python callsite·full model 실행·D compiler 횟수 추가는0이다.

### 자연 AMD host와 현재 compiler context

`hosted-cpu.json`의 실제 host는 **AMD EPYC 9V45 96-Core Processor**, AuthenticAMD,
family26/model2, x86_64, vCPU4다. original compile chain과 D 두 후보 before/after가 모두 동일한
자연 context를 기록한다. CPU명을 강제한 실행은 아니다. 원문 `CPULLVMCompiler(['x86_64','native'])`는
기존 `LLVMGetHostCPUName/Features`를 사용하며 이번 LLVM receipt의 CPU 값은 `generic`이다.

- target `x86_64-none-unknown-elf`, passes `default<O2>`, LLVM18 provider SHA
  `5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0`.
- 전체 자연 feature-string886B의 SHA는
  `5c6ca0d75d5d499353a630a8750daa1450a2310954f1da9a267ee6bd7532c3cb`다.
- LoopUnrolling/LoopVectorization/SLPVectorization/VerifyEach는 기존4개 모두1이다.
- D constructor options는 code_model0/codegen_level2/relocation_mode2이며 두 compile이 같은
  context/target-machine/pass-options 주소를 사용했다. 이 값이나 CPU/ISA를 변경하지 않았다.

A receipt는 strict_triplet_all=true/host_isa_all=true/complete_roster91이며 call176개를 검증했다.
current host feature에는 AVX512F/BW/DQ/VL/VBMI가 포함된다. 허용되지 않는 extension이나 없는
host feature를 일반 허용한 근거로 쓰지 않는다.

### B 전체12 raw exact와 D guarded 반례

B는2frame×6output **12개 총5,138,816B**에서 original/plain-native/original-tapped 파일의 전체
bytes가 동일하다. 원본 output을 변환하거나 zero 부호를 지우지 않고 ZIP의 raw bytes를 직접
비교했다. native tap의12개 output SHA도 이 raw SHA와 모두 일치한다. original/native의 기존
plain↔tap whole-output 동일성과 worker event4개씩도 그대로 PASS다.

B bundle의 source-policy91개는 모두 original-flags/rename-only/helper_executed=false다.
각 `kernel-N.txt`와 `.compiled.ll`의 차이가 정확히 단일 함수명 `@op_impl_N` rename뿐인지
직접 확인했다. source/compiled/object SHA와 policy receipt SHA도 각각 binding과 대조했다.
기대 output·허용오차·zero 후처리·원본 arithmetic AST를 바꾼 성공이 아니다.

C도 이미 관측한 B original12개와 raw bytes가 모두 같다. 기존 D의90개 original image 고정과
writer92/call175 한 곳 교체를 유지한 두 후보 결과는 다음과 같다.

| D 후보 | existing original12와 직접 비교 | writer text bytes |
| --- | --- | --- |
| rename-only | 12/12 exact |14166 |
| guarded | 다른11개 exact, model-1.bin의3byte만 다름 |14380 |

guarded의 차이는 byte403/407/5399, f32 index100/101/1349뿐이다. original/B/rename-only는 모두
little-endian `00000080`(-0), D guarded만 `00000000`(+0)다. 따라서 단순히 원본도 +0인 host에서
우연히 통과한 범위가 아니다. 같은 자연 AMD context에서 기존 strict rewrite의 차이는 D에
그대로 남고, 실제 parity caller의 B는 원본 -0를 보존했다. standalone strict canonicalizer의
zero/sNaN 규칙을 변경하거나 guarded 결과를 새 expected로 쓰지 않았다.

D의 원본 compiler2회/model0회·compile_cached0·new taps0, 두 selector와 기본 cached-disabled를
유지한다. guarded의 진단 mismatch는 B 실패를 승격시키지 않으며 B의 실제12 exact가 필수인
최종 gate가 통과했다. 두 helper copy의 총6,037,936B, mode0700/UID1001/nlink1과 SHA를 확인했다.
각 original Cargo ELF는 nlink2를 유지하고 before/after identity·SHA가 같으며 originals_unchanged=true다.

### 후봉인과 수용 한계

실제 command61개는 전부 exit0·all_owned_children_reaped=true다. final-seals는
pass=true/failures=[]/error=null이며 phase=writer-pair-native다. source index/whole source digest,
준비 SDK file/program/identity roster before/after를 artifact에서 독립 대조해 모두 같았다.
source/SDK/Python/Python-base baseline4개도 after와 동일하다. 최종 provider/registry/held alias
guard와 업로드·source/job cleanup(step14–20)은 모두 PASS다.

현재 source의 catalog3·fresh91 admission·B2frame12raw exact·C12·D 두 후보 분리는 실제 수용된
범위다. full128·모델 전체/runtime·historical half/sNaN5·oldPR1344/frozen·기기 수용·퇴역 credit는
여전히0이다. source495의 model 성공으로 d666 family의 #476 driver float 불일치를 닫지 않는다.
genuine d666 PASS는 그대로 재사용하며 이 감사에서 추가 CI dispatch/rerun/cancel·로컬 model/
compiler/Cargo/test/Clippy 실행은0이다. 변경은 이 문서 append뿐이다.

d666 CPU 기록도 정정한다. 앞선 감사에서 A/compiler context가 미생성인 것은 맞지만,
`hosted-cpu.json` 자체는 ZIP에 이미 있었다. 같은 cached ZIP의 해당 metadata만 재확인한 결과
host는 AMD EPYC 7763 64-Core Processor/vCPU4였다. 이전의 CPU 모델 자료도 없다는 서술은
이 항목으로 정정한다. d666의 A/B 미진입 및 수치 수용0은 변하지 않으며 재다운로드는 하지 않았다.

## 2026-10-26 Frozen36b actual — #482 원래1344 첫 raw 계약 수용 범위

source `36b9fef61127e6175e3179efe7bee7f8729c97ca`의
[run40270227599/job121401247650](https://github.com/bin9208/openpilot-rust/actions/runs/40270227599/job/121401247650)은
SUCCESS다. replay helper `e8271fec4397769a705513c4aad1164ff418a505c74d7b68f478a2dced1c240e`,
base `ddf48345a1d358b80ce7296ffd42293e3992b7194808e476750914aea593f19d`, WF
`9d0b19f465f4c0fad63271c95b470ecafeca1b72b1506a25a8dbba9adbb8aa82`를 exact Git source와
대조했다. 세 pin만 동기화한 기존 push-only replay이며 새 model/diagnostic stage는 없다.

### 단일 ZIP와 raw 독립 감사

artifact12628675929는80,788,416B, SHA256
`c5f95b64c1859c09e7cd8b8df83c438f5cd2d588a186f582d14aa0638db9a5db`다. API expected
bytes/digest/run/source/expired와100MiB download/512MiB uncompressed admission 뒤1회만 받았다.
safe unique/nosymlink·암호화 없음2192개, 총171,570,186B와 CRC2192/2192를 확인했다.
retained-index의1704개 파일127,225,117B도 길이·SHA를 전부 직접 재검사했다. parent는 이
artifact를 중복 다운로드하지 않았다. 완료 log100,602B도1회만 받았고 SHA256은
`21047e62a4ef7d5eb388091a439e3a79ffed9e5678bab06c279abf83bcf61576`다.

### D1은 실제12/33 완료이며 수집용 test exit0만의 판정이 아니다

`d1.json`은 strict_pass=true/strict_failure_sticky=false/error=null/differences=[]이며
later_recursive_fields_qualified=true다. `retained/d1/report.json`도 실제 runs1/frames12/
messages33/compared_fields21153을 기록한다. discrete는 exact, raw_comparison은 exact bytes이며
기존 float tolerance1e-6/polynomial2e-5는 그대로다. 12개 trace 중 prepare-only1개를 제외한
ModelV2 packet은11개다. D1 stage28개도 receipt-write-after까지18.14965366초 안에 완료했다.
controller의 dual-first-packet command는18.563초/exit0/전원 회수다. 기존180초를 늘리지 않았다.

첫 frame_id3의 ModelV2 packet19152B SHA256은
`0fa266bf098e65f56bd99a007e766b325bcb0c2ad04a261251be5cf779e5d3fa`다. receipt의 packet
길이/SHA를 실제 bytes와 대조했다. 추가로 프로젝트 parser/모델을 실행하지 않고 Cap'n Proto
framing과 byte-list pointer를 inert 파싱해 segment1/word0이 packet byte8216의10304B payload를
가리킴을 확인했다. 이 payload는 D2 native raw 및 frozen raw와 전체 bytes가 같다.

### D2 prepare→prepare→첫 policy1: 전체6출력과 half99

기존 sequence는 prepare2회 후 policy1회 그대로다. model/warped/img_q/big_img_q/feat_q/
desire_q 총6출력 **2,569,408B**에서 original plain/original tapped/native raw가 전부 같다.
native plain/tapped receipt의6개 길이·SHA도 이 실제 raw에 각각 일치한다. model10304B 전체
2576개 float32 word의 frozen/current-original/current-native 차이는0이다.

writer92/call175/arg1/view365/alloc16의 실제 half99 입력은 양쪽 모두 little-endian `0080`
(half bits0x8000=-0)다. view offset9472/3152B, allocation 내 element byte9670, 실제 element
주소=buffer 주소+198을 검증했다. 원본 dtype=dtypes.half이며 native는 untyped buffer에 held LLVM
i16 저장 계약을 결합한다. operand_file=null인 실제 호출 버퍼를 읽은 기존 tap이며 새 할당이나
다른 kernel 실행을 유도한 관측이 아니다.

원본/native worker0의 output99는 모두 before/after `00000080`(-0)다. worker0/1의 기존2개
callback과 whole-output plain↔tap 동일성도 유지한다. 실제 identity는 다음과 같다.

| 근거 | SHA256 |
| --- | --- |
| original writer SOURCE | `e223a2c36c6bde7cf676bb92fd2529858a723f9ecfa91c811b2b941756a9bea0` |
| A fresh original object | `ecac5435f64713ba6f2b310c9747f2299000f4754fd04bf11616d788da542828` |
| A fresh original image | `b3558b20e727ce303d10a22a26d005b66ba5d7bcce45b8f84ae22ca0909f8043` |
| native current library | `d4cc85a788b89542aa94c7ff30198189a2aba173703aa8eebb9fb2fca424a445` |

native half receipt의 object/image 항목은 결합한 A original triplet이며 native object 자체와
동일하다고 주장하지 않는다. 별도로 current1344 bundle의91개 source-policy·compiled.ll을
검사해 원문에서 함수명만 rename했고 수학/flags는 보존했음을 확인했다. 각 policy/object SHA도
binding과 일치한다. 입력/출력 zero masking이나 frozen expected 교체는 없다.

### source939a 원래 실패와의 연결·자연 context

[#482](https://github.com/bin9208/openpilot-rust/issues/482)의 원래 actual receipt source는
`939a190a11fc991a4eaff958d514dc2fca5c07e2`, host EPYC7763이었다. 당시1344-dual 첫 frame3의
raw10304B는 byte399/float99에서만 native+0/frozen-0로 달랐다. 이번 D1 packet payload/D2
original/native/frozen 모두 원래 expected 전체 SHA
`9afb2defff4ff29e5bf70e54db32d0d531d5e2b44507c8de652ee2c0dfcb67fe`와 일치한다. float99만
골라 통과시킨 결과가 아니라 원래 raw 계약 전체의 회복이다.

fixture provenance의 source `e25a14386582858dae25c6daca5ced3bfb4bacab`와 원래 실패 source939a는
같은 식별자가 아니다. 기존 fixture/observation manifest SHA `afe1c795...`/`ed174bb4...`, PKL
72,045,323B SHA `487ef869...`, calibration source SHA `b8ba3f01...` 및 선택한 실제 입력의
원문 SHA를 그대로 유지한다. input-provenance는 expected_regenerated=false/
new_capture_replaces_expected=false다. 새 캡처를 original golden으로 만든 것이 아니다.

이번 실제 host는 **AMD EPYC 9V45 96-Core Processor/vCPU4**, LLVM18의 자연 CPU getter 값은
`generic`, triple=x86_64-none-unknown-elf, passes=default<O2>다. 기존4개 pass option은 모두1,
provider SHA는 `5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0`다.
91개 original compile chain은 같은 config를 기록하며 A whole91 triplet/hostISA와 기존 ISA14,
model/retirement strict Clippy가 PASS다. CPU/ISA forcing은 없다.

D2 original은 frozen PKL의 SOURCE91을 이 자연 host에서 fresh compile한 image를 실행한다.
old BINARY는 identity 결합에만 쓰며 실행 fallback하지 않았다. 따라서 이 성공을 과거
EPYC7763의 retained BINARY/완전한 historical compiler context 복원이라고 쓰지 않는다.

### 독립 후봉인과 #482 남은 완료 조건

commands56개는 전부 exit0/error·cleanup_error=null/all_owned_children_reaped=true다.
final-seals는 pass=true/failures=[]/error=null, phase=native-tapped다. whole source/index와
SDK file/program/identity roster 전후 bytes, source/SDK/Python/Python-base baseline4개를
artifact에서 독립 대조해 모두 동일함을 확인했다. source digest는 전후 모두
`5cceadc6756122d346f3aa2263e577b34b23f8be8218b3d0865b7545444bc91e`다. fixture/catalog 및
held ELF/provider 후검증, 실제 upload와 source/job cleanup(step13–19)도 모두 PASS다.

원래1344-dual 첫 raw regression과 현재 D1 12/33·D2 whole6·writer half/output 범위는 이번
exact source에서 실제 통과했다. #482 본문이 요구한 **현재 PR343 소스의 재현·회귀·원본 비교와
전체 필수 검증/통합 수용**은 이 focused run으로 대체하지 않는다. 원래 EPYC7763 context의
동일 실행으로도 확대하지 않는다. full128/historical_half_snan5_resolved/historical_provider_proven/
ssa_first_producer_observed/full_model_runtime_acceptance는 여전히 false, retirement_credit0이다.
#482와 별도인 #476 driver float 실패도 이 결과로 닫지 않는다.

private `ci/pr1344-36b-actual-audit.json`에 output별 전체 SHA, actual report, packet pointer 연결,
half/worker identity, source/compiler config와 독립 후봉인 결과를 남겼다. 이 감사의 source 수정,
원본/model/compiler/Cargo/test/Clippy 로컬 실행, CI dispatch/rerun/cancel 및 issue closure는0이다.


### 부모의 최종 소스 등록 — 실행 전

변경한 exporter 원문 `capture.py`·`compile.py`·`native.py` 세 경로를 Focus의 명시 변경 목록과
SHA 검사에 추가하여 고정 source367개로 등록했다. current-model workflow의 helper admission과
후봉인 literal 두 곳을 모두 `dafe96d44bb819480427ecb30c7f726a910a0650648ff9abfdaa974ea85afda4`로
동기화했다. model-family caller는 해당 exporter 변경에도 기존 전체 job을 실행한다. 비교
selector·수치 허용치·원본 compiler context·standalone strict zero/sNaN 경계는 유지한다.

새 catalog 세 selector는 별도 cache/proof를 사용해 A의 `original-cache.db` 부재 조건을
보존한다. 그 tiny fixture 실행과 full-model A/B/C 및 D compiler2를 구분한다. 부모의
로컬 확인은 indexed SHA·YAML/변경 Bash/독립·embedded Python 구문·Rustfmt·diff뿐이다.
**이 정책 수정의 모델 수치 및 새 lifecycle 통합 actual은 아직 UNRUN이며 퇴역 credit0이다.**


## 기존 frozen1344 replay의 source pin 재연결 — 실행 전

source495의 current-case 성공 뒤에도 #482는 원래 frozen1344/float99와 PR343 검증을
별도로 요구한다. 기존 replay helper는 BASE SHA가 오래되어 새 코드 실행 전에 거부된다.
BASE·workflow helper·사후 BASE 검사 세 literal만 현재 `ddf48345…3f19d` base로 동기화한다.
새 helper SHA는 `e8271fec4397769a705513c4aad1164ff418a505c74d7b68f478a2dced1c240e`,
workflow SHA는 `9d0b19f465f4c0fad63271c95b470ecafeca1b72b1506a25a8dbba9adbb8aa82`다.

기존 SOURCE91 fresh compile/전체 triplet·ISA, current catalog, D1의12frame/33publication,
D2의 prepare 두 번→첫 policy 한 번/6출력/2576word/half99 검사를 그대로 사용한다.
subclass의 기존 run 경로이므로 current-model의 catalog3/C/D를 추가 실행하지 않는다.
새 E/F·전체60 진단·CPU 강제·허용치/기대값 변경·old BINARY fallback은 없다.
proof512MiB·14일 보존과 실패 업로드·독립 후봉인/cleanup 조건도 유지한다.

이 변경은 원래939a/EPYC7763의 고정 기준과 새 자연 host를 구분해 수용한다. 전체128 및
historical half5 수용을 뜻하지 않는다. 실제 replay는 발행 후 확인하며 현재 UNRUN이다.
