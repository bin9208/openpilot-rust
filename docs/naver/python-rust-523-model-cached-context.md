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

2026-10-26. 사용자가 허용한 범위대로 `Controller.cached_context` 안에서만 source-selected
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
