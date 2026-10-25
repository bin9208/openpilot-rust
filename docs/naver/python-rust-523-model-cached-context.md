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


부모 통합은 current WF/helper의 새 SHA를 동일하게 고정하고 PR1344는 BASE SHA literal과
그 helper/WF pin만 갱신했다. 기존 PR1344 진단 알고리즘과19-source schema는 그대로다.
새 C-only source path를 current WF push 대상에 추가했다. 실제 Actions 결과가 나오기 전
#596/모델 수치·전체 기능 수용 및 Python source retirement credit은 계속0이다.
