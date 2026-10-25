# Remaining functional closures for #296

The user's 2026-10-22 direction keeps strict vehicle/control/safety checks and
batches general development tools by functionality. Reuse successful evidence
unless relevant source, dependency, feature union, provider, environment or
target changes, or a new failure gives a reason to rerun it. Retain final
required integration checks. Do not run a heavyweight suite per removed file.

This ledger tracks functional closures, active dependencies and blockers.
The [routing map](rust-repository-python-retirement-296-families.tsv) covers
every remaining pending/partial baseline entry exactly once. Its counts classify scope;
they are not completed functionality. All families below still contain work.

| Family | Remaining dependencies | Completion conditions |
|---|---|---|
| Vehicle/control/safety | opendbc platforms and safety; Panda codecs/transports; Card; planner/MPC; radar; pose; control/monitoring original oracles | Preserve platform/case coverage, negative cases, tolerances, cadence and validity. Native implementation and actual callers, independently sourced comparisons, original-source absence, affected target checks. User vehicle acceptance stays separate. |
| Tensor/compiler/model export | tinygrad graph/schedule/lowering/backend APIs, autograd/training, optimizers, data/examples/tests; model export and offline compiler chain | Source-to-model compilation, kernel/provider provenance, metadata, numerics and failures for each supported backend/graph family. Frozen compiled-model consumption alone does not retire a compiler. |
| Runtime/model/media comparators | UI/cluster, camera/webcam, Xiaoge/OpenCV, model inputs, navigation, logging/telemetry, startup/services, sensor/GNSS/platform | Use existing job boundaries. Convert controller plus shared fixture/source adapters together; migrate all consumers and preserve success/failure/cleanup observations. Hardware/model validity retains strict affected checks. |
| Web/recovery/upload | HTTP/WebSocket/PTY/process peers; recovery controllers; shared upload/save/media/network and Params helpers; web terminal/configuration/profile/Bluetooth routes | Native wire/body/frame/error behavior, disconnect/retry/timeout and child ownership, followed by actual caller and shared-helper removal. One bounded gate per compatible route/controller group. |
| Replay/developer/docs | route/log reader/schema/cache; car-porting and notebooks; map tools; Android/Jetlink conversion; docs/wiki/renderers | Equivalent runnable workflows and failure effects. Full schema/service coverage and replay data semantics remain; do not activate inherited publication/deployment workflows to validate a port. |
| Build/generation/staging | Params/MsgQ/VisionIPC binding builders and consumers; generated vehicle/schema/pose tables | Native build/schema/dependency graph, exact relevant commands/manifests and early-failure cleanup. Reuse accepted JSON11/media/provider stages. Params still invokes Cython; its native JSON11 dependency alone does not close that builder. |
| CI/release/launch | remaining Python tests/controllers; AST syntax admission; release/sealing/source packaging; watchdog/bootstrap and embedded scripts | Preserve assertions, exact artifact/source binding and fail-closed release behavior. Native AST-equivalent admission or final Python removal; actual caller transition and required PR/postmerge gates. No publication is authorized merely by this ledger. |
| Other vendor infrastructure | MsgQ/VisionIPC APIs/tests, symbolic filter generation, teleoprtc and Jetlink transport | Preserve licenses and API behavior, migrate consumers and tool functionality. A Rust launcher around Python does not close the dependency. |
| Cython/SCons | root/module build graphs, extension declarations and consumers | Native build discovery, clean/incremental/error behavior and replacement APIs for every consumer before removing original bindings/build sources. |

Additional scope outside the baseline source inventory remains explicit:
`selfdrived_helpers_source.py`, `tests/test_selfdrived_source.py`,
`tests/test_params_native_stager.py` under `rust/tools`, six car-porting notebooks,
and the 39 currently recorded pending entries in the [execution-edge inventory](rust-repository-python-retirement-296-execution-edges.tsv).
The added hosted exporter bridge and publisher tests remain explicit pending Python edges.
Those include generated extensionless helpers, dynamic launch, embedded Python
and vendor release/build paths. The22October follow-up adds34 explicit sites
(30 missed paths, one generated-exporter subedge and three bootstrap retry sites).
This remains an observed inventory, not an exhaustive interpreter-free proof. Relocation and missing files do not close the
three partial baseline entries.

The ten uploader/route controllers are accepted at source12af083b2 by
[39796626003](https://github.com/bin9208/openpilot-rust/actions/runs/39796626003).
The baseline is now284replaced-local/3partially-replaced/3293pending; the routing
map retains10accepted rows and3296remaining pending/partial entries. This closes
two F03 routes and eight F04 helper roles, leaving1120/F03 and416/F04 entries.
Original runtime uploader/deleter/xattr/loggerd/Athena consumers remain live.

The four deleter development helpers are also accepted at source1d570b345 by
[39805553255](https://github.com/bin9208/openpilot-rust/actions/runs/39805553255).
The preceding baseline was288replaced-local/3partially-replaced/3289pending;
the routing map retains14accepted rows and3292remaining pending/partial entries.
This removes three F03 roles and one F07 role, leaving1117/F03 and49/F07.
The four retained runtime source dependencies remain live; embedded-edge counts
are unchanged. Native logging and syntax-check candidates receive no credit yet.

## 2026-10-24 Sanitizer 소스 수용

현재 baseline은 **289 replaced-local / 3 partially-replaced / 3288 pending = 3580**이다.
추가 routing map은15 accepted /3291 remaining pending·partial이다.
F05의 `openpilot/tools/lib/sanitizer.py` 한 파일만 갱신했다.
[a134 원본 비교](https://github.com/bin9208/openpilot-rust/actions/runs/40089943421/job/120860396114)의
원본5/native15/Clippy 성공 후, [fb1228 소스 부재 검증](https://github.com/bin9208/openpilot-rust/actions/runs/40092771424/job/120868686734)의
고정 원문5개 비교+재출현 음성통제1개/strict Clippy가 성공했다.
원문 fixture 전체SHA와 현재 native/IDL8개 pin, source/index/SDK 전후 seal을 확인했다.
tracked 전체·notebook 검색에서 기존 oracle 외 실사용 caller를 찾지 못했고 해당 파일을 제거했다.
raw trace는 frozen-test Cargo 범위이며 전체 Python 실행 부재 수용이 아니다.
나머지 auth/SSH/Bundle·vendor·tinygrad compiler 및 모델 수치/전달/실기 게이트는 미완료다.
자세한 근거는 [작업 기록](python-rust-523-route-sanitizer.md)을 참조한다.

## Current batches and blockers

### 2026-10-25 setup 원본1 삭제 후 수용

source `602ea095f7cd89769ffd9233a04a28983ca59d1f`의
[40137010381/job121000556049](https://github.com/bin9208/openpilot-rust/actions/runs/40137010381/job/121000556049)에서
`tools/scripts/setup_ssh_keys.py` 한 경로만 추가 수용했다. native 공개 entry와 controlled21,
거부3·재출현 guard6·Clippy, 원본 부재 HEAD/index·122held·source/SDK 전후·closure/upload/정리를
확인했다. artifact12577386813은10,462,974B/SHA256
`1ee86d2c9abfe5ff3141fec71ed582e373f5c27878552650a86013a67532361c`,172CRC다.
기존 raw21 원문과 metadata를 유지한다. 원문 Python·현재 expected 생성은0이다.

현재 baseline은 **293 replaced-local / 3 partially-replaced / 3284 pending = 3580**이다.
routing map은19 accepted /3287 pending·partial이다. `replaced-local`은 기존 원장 status이며
이번 기능 검증을 로컬에서 실행했다는 뜻이 아니다. public missing-argument1과 controlled21을
실제 외부 network21회로 확대하지 않는다. 전체 Python 부재·auth/SSH 원본·logging2·필수 PR/전달·
실기 수용은 계속 미완료이고 actual receipt의 `retirement_credit:false`도 그대로 보존한다.

1. Panda device/protocol/CAN-I/O source deletion and maintained fixtures are
   accepted in39640526320 and39647358174. The shared corpus is removed with both
   native callers. Complete independent observations and parser/failure effects
   are preserved. Final actual outer38-phase/two-root integration must cover
   the163-source/34-executable current composition; its14 mock controls alone
   do not replace that gate.
2. Driving/driver/logging faults: complete fixture/source-absence/finalization
   and optional-collector candidates are committed. Driving native collector
   and both private-root functional lanes passed. The cross-root metadata failure
   was isolated to one exact DEBUG camera-wait message; only that already-accounted
   diagnostic count is normalized. Four original controllers are now deleted and their maintained callers migrated;
   the final complete comparison and eight reappearance controls pass39663027128. The compact independent
   fixture preserves1649 original file pins in59,663,776 XZ bytes; native full
   regeneration/export passed39654500423. Its private fault-builder failure #473
   is now diagnosed and fixed:39661522311 passes8 units, strictClippy and actual
   byte-identical restoration with all locks/source seals unchanged. The final
   retained replay and full two-root traffic also pass39663027128. Driver's actual collector CLI data is
   independently auditable despite a wrapper row-count failure; do not repeat
   unchanged traffic solely to fix that assertion. The reused driver catalog failed warmup with confirmed SIGILL: AVX-512 objects
   were run on an AMD EPYC host without that ISA. CPU target compatibility is a
   real reuse boundary. Use current-host exports, preserve historical objects
   and source seals. The first fresh-target attempt39651208682 stopped before
   model traffic on a new shared stager Clippy lint;481ae6fe fixes it without
   changing runtime policy. Current-host gate39652828256 now passes
   no-collector/CLI/raw-alias comparisons; parent and reviewer audit all535 ZIP
   CRCs and both43-file original captures. Shared driver source/caller retirement
   remains pending.
3. Full f5c8 CI `39617311670` passed all runtime/ARM jobs except workspace:
   `desire_reference_cli` failed a source-output hash assertion. The serde feature
   ordering defect #463 is fixed and affected gate39639003843 is accepted. Preserve
   successful checks and retain required final integration.
   The historical aggregate remains failed. Navigation #457 is now closed after
   measured dev/test RSA optimization and published complete navigation119742946180
   plus Integration39754486834 acceptance; older uninstrumented stalls are not
   all attributed to that measured phase. Current full-model/workspace gates
   remain separate.
4. Recovery13 Python controllers/helpers are actually deleted and accepted by
   [39687004413](https://github.com/bin9208/openpilot-rust/actions/runs/39687004413),
   source57594f054, after all eight controllers in both absent roots, new ownership
   regressions, strict Clippy and13disk/6workflow policy cases. The parent audited
   7,346ZIP members,2,508trace seals/roster,4,795exec attempts (2,393success,
   2,402explicit failures; observed Python exec0),25unchanged presealed programs,
   96created bodies and both52-row owned-alias receipts. Source-authored tmux links
   now hold their own O_PATH inode and bind the presealed provider with two-phase
   receipts. Unknown execs remain rejected; raw argv is retained without claiming
   argv parity. Eight callers and the CWP selector are native, and old APIs are
   removed. server.py/repo_update.py remain live outside this closure. #474's
   historical PTRACE_LISTEN/EIO cause remains unexplained despite this pass.
5. Driver/startup/USB six-source candidate is actually absent and integrated into
   the required model-family-retirement job. Native preparation39685067872 passes
   eight regressions/strict Clippy. Full39686001854 passes separate4/5-model current
   catalog exports and genuine45-asset/three-license USB staging, then fails sealed
   fixture admission because Git text=auto normalized the NAS installed marker's
   30CRLFs (879→849bytes).1ccf8437c preserves original bytes; all114sealed staged
   Git blobs and both original manifest hashes are independently checked unchanged.
   Replay39689390187 and full119537355539 reproduce the first nonuniform driver
   frame mismatch (#476). A bounded original/direct model-run comparison passes
   on a different CPU, so same-host/catalog diagnosis remains required. Full
   six-source acceptance is not claimed. Compiler
   functionality remains Python and is a separate open family dependency.
6. Teleop's two embedded release computations are accepted in39670630661:
   independent34 computations and10 disposable Git scenarios match. The native
   CLI, maintained script caller and main CI policy integration are accepted
   in39674181956. This closes one execution edge without changing the baseline
   source count.

7. Native soundd source staging is accepted39688832018 at7219cf5e7: exact
   original-heredoc/native CLI20file outputs/173533B,21manifest pins and five
   controls plus strict Clippy. Existing seven audio modes in each root and
  22closed native execs/Python0 per runtime lane pass. This closes one embedded
   staging edge, preserves the full checker/finalization/source/ELF contract and
   does not retire remaining audio-job Python binding/policy consumers.

8. All four Kalman development commands are deleted and accepted: equation
   generators39697287904 and original-C++ corpus tools39698084823. Both native
   corpus CLIs retain full independent JSON bytes, RNG/function order, actual and
   empty-cwd execution, and symlink/hardlink output behavior. Original symbolic
   model definitions, C++/Cython builders and remaining consumers stay pending.

9. HardwareInfo3development tools are deleted97c33434c and accepted by full
   maintained hardware39710753791 plus corrected native policy39711275087. Both
   actual/private roots require all3absent, replay136cases/3110observations/
   70datagrams with same current native ELFs and independent captured expectations.
   All bootlog/control/amplifier/ASan boundaries pass. Parent verifies full12389
   ZIP CRCs, complete source/input receipts and selected3028HardwareInfo members.
   Exact failed-read partial ordering is limited to two verified failures in one
   case; general keyfile priority/value/wire contracts remain exact. Original
   HardwareInfo runtime Python and Params/Cython consumers remain separate work.

10. Hardwared3development tools are deletedfc0f017bd and accepted by complete
    pinned platform39717847363 with policy39716711789. Both absent roots retain
    12cases/891inputs/890observations and live IPC; all retained manager/modem/
    #398original-reference/native cleanup checks pass. Parent independently
    verifies full4165CRCs, source pins, held receipts and full CLI output pairs.
    Original hardwared runtime Python/shared binding dependencies remain pending.
    One previously missed embedded modem reference is explicitly recorded as
    closed from independent original capture; existing37pending sites remain.


11. Modem5development tools are deletedda3483724 and accepted by complete
    postdelete platform39727029012 plus policy39727105631. Actual root requires
    all5absent with runtime pinned; private root requires all6absent. Both run
    13cases/40rows/144ATcommands/93helpercalls per side, live reconnect/APN/SIM/
    serial-loss/PPP cleanup and3process boundaries;12CLI/6reappearance controls
    and4unchanged held ELFs pass. Full6623CRCs/23current Gitpins and cross-root
    equality are independently verified. Runtime modem.py remains pending for
    its live Python manager caller. Native summary's all-runtime retirement
    flag stays false; acceptance here covers the five development tools only.

12. Three manager QA Python controllers are deleted and accepted by full
    platform39742079076/sourceefd4350de plus13disk/78isolation cases.
    Both absent roots preserve seven original cases/four logs;77raw trace files
    contain61successful closed native execs. Parent and reviewer audit all8334
    platform CRCs and4policy CRCs, held providers and original case values.
    Car-name generator predelete39744327540/source285b12a62 also passes: four
    generator/thirteen manager units, fresh seven-brand original comparison,
    same held native ELF in checkout/private-absent roots and11single-exec
    admissions. Complete generator-postdelete platform/policy39746277084 at
    ac72102459752eec5936fb42b68aac2a05d6bef2 is accepted:8426platform/4policy
    CRCs,62manager execs in78traces and12generator invocations, all original
    results/JSON bodies and held seals agree. Credit includes the generator,
    bringing the inventory to274replaced-local/3partially-replaced/3303pending.
    Runtime manager/Params/swaglog remain live. Rust guarded-add/exporter
    integration and the two recording-upload QA controllers are current groups;
    their candidate source does not add retirement credit before hosted proof.

## Evidence reuse and delivery

Key reuse to relevant source/fixture/ABI hashes, compiler/provider/target and
features, original ZIP/manifest/member seals, normalization/error contract, and
actual caller/environment. Keep historical capture identities; do not relabel
old data as freshly captured or use native output as its own expectation.
Preserve exact required safety and final integration checks while grouping
ordinary tools. Earlier accepted updater, Panda outer, encoder, JSON11 and
other closures remain in the [evidence ledger](rust-repository-python-retirement-296-evidence.md).

Local work stays at syntax/format/diff and stored evidence inspection; execution
uses GitHub-hosted Actions. #296 stays isolated from dev-md/#338. The user will
pull and personally check normal boot, recording, upload and driving; no such
device acceptance is claimed here. Whole-repository conversion remains open.


## 2026-10-24 임시 진단의 interpreter 의존성

#523에서 추가한 원본 비교·증거 해석용 shell/workflow 11개 파일의 Python 의존성을
execution-edge 원장에 pending으로 명시했다. 파일당 대표 위치이며 모든 callsite를
열거하거나 새 baseline Python 파일로 세는 것은 아니다. 현재 원장 행 상태는
25 replaced-local / 50 pending이다.
원본 비교 성공이나 native 도구 한 개의 Python0 결과로 이 진단 경로까지 제거됐다고
주장하지 않는다. 전체 전환 완료 전 고정 데이터/native 처리 또는 진단 폐기가 필요하다.


같은 날 changed-files의 기존14proof data admission shell도 interpreter 의존성1개를
pending으로 추가했다. 현재 execution-edge 행은25 replaced-local/51 pending이며,
native-existing1/non-executing-text2 분류는 별도다. 새 native context builder·Bash wrapper는
Python 없이 metadata를 처리하지만 legacy proof admission이 제거된 것으로 표현하지 않는다.


fresh caller 뒤 provider 최종 seal을 수행하는 새 changed-files workflow의 metadata Python
대표 위치도 추가했다. 현재 행은25 replaced-local/52 pending/native-existing1/non-executing-text2다.
이는 발견한 행의 상태이며 exhaustive interpreter 부재 수용이 아니다.

## 2026-10-25 SSH 원본1 실제 부재 수용

source `01fcc36e09b39172f4c88954a702fa8353b2d1e8`의
[집중 검사40157685746/job121062463242](https://github.com/bin9208/openpilot-rust/actions/runs/40157685746/job/121062463242)는
GitHub 기본 X64 러너에서 최종 성공했다. artifact12585103692의41,120,449B와 SHA256
`ff144ca306527bc9d231beb83dcc1f60a7e7312e8e1a234ea1681261a8f6ade9`,343멤버 CRC를 확인했다.
원본 `tools/scripts/ssh.py`의 정확한 역사 SHA와 삭제 diff/HEAD/index 부재, 재출현 통제6,
127개 부재 기록과 명령9개의 전후18경계가 일치한다. frozen SSH39/setup21·importer7의
기존10회귀, Unicode7, 현재 native7/public1 및 entry4가 통과했다.
source/index·Rust/venv·held executable·고정 입력의 전후 봉인과 최종 closure/upload/소유 정리가 성공했다.
held provider의123개 기록은 두 차례 사후 검사 모두 원문과 동일하다.

baseline은 **294 replaced-local / 3 partially-replaced / 3283 pending = 3580**이며
routing map은20 accepted /3286 pending·partial이다. 이번 추가 수용은 원본1개에 한한다.
`replaced-local`은 기존 원장 이름이며 기능 검사는 Actions에서 실행했다.
실제 원격 SSH 연결, 전체 Python 부재, 완전한 build dependency closure 또는 전체 전환 완료를 뜻하지 않는다.
원시 receipt의 retirement_credit0/범위 제한은 원문 그대로 보존한다.


## 2026-10-26 Planner 제어·메모리 wrapper 삭제 후 수용

현재 baseline은 **299 replaced-local /3 partially-replaced /3278 pending =3580**,
routing map은25 replaced-local/3 partial/3278 pending이다. 정확 세 baseline 경로
`check_plannerd_ci.py`, `check_plannerd_native_memory.py`, `tests/test_plannerd_ci.py`를
갱신했다. 추가 memory wrapper unittest는 baseline에 넣지 않고 별도 실행 원장에 기록했다.
source425의 독립 원문5+5, source7a82 실제 host12/ASan3/ARM12 및 source100의 native14/
frozen5/guard6/memory15·strict Clippy 수용과 source/index/SDK 봉인을 결합했다.

추가 원장162행은30교체/128대기/1기존native/2비실행문구/1제거다. Rust 두 test의 live
Python 원본 실행과 Bash의 옛 직접 wrapper dispatch를 닫았으며 나머지 Python leaf/IPC/
Cython/컴파일러는 대기다. 세부 artifact SHA와 한계는
[삭제 후 수용 기록](python-rust-523-planner-retirement-acceptance.md)을 따른다.

## 2026-10-26 buffer 원본179 비교 실행경계의 후속 수용

source7a928의 buffers 일반7/ignored0, serde_json/preserve_order union1과 strict Clippy2 및
source/index/SDK 봉인이 실제 통과했다. [원문 감사와 후속 증거](python-rust-523-buffer-corpus-100-audit.md)를
근거로 추가 원장 한 행만 갱신했다. 최신 추가 원장162개는31교체/127대기/1기존native/2비실행/1제거다.
기준3580의299교체/3부분/3278대기와 production Python buffer/compiler 상태는 그대로다.


## 2026-10-26 stage73의 실제 native 수용

source `a62109fa706012b91b24206e7d8c875653746aec`의
[run40241338985/job121313583472](https://github.com/bin9208/openpilot-rust/actions/runs/40241338985/job/121313583472)은
launch9·stage8/0ignored, preserve_order union1, 원본launch4411 corpus1과 strict Clippy를 모두
성공했다. 새 Python subprocess/현재 원본 AST 조회가 없는 고정 reader다. 원본73 raw 입력·
기대값·SHA와 과거13 witness를 유지했으며 native 결과로 expected를 만들지 않았다.
artifact12616208518은1,583,513B/SHA256
`ece5ed6e6be36855b4180fe3400b97bbf9aed2d9bb55eb0a72a5b42c0018acc4`,
40 safe unique regular member/전체 CRC, expanded6,929,918B다. command4개 exit0,
source/index/index-stage와 whole SDK file/program/roster 전후 동일, SDK install0,
업로드 뒤 source/owned temp cleanup까지 실제 성공했다.

추가 실행 원장은 **162개 =32교체/126대기/1기존native/2비실행/1제거**다.
기준3580은 **299교체/3부분/3278대기**, production offline emitter와 전체 F02 수용은 그대로
미완료다. 이 좁은 비교 실행경계의 제거만 인정하며 source-file baseline credit은0이다.


## 2026-10-26 원본 launch unittest31 삭제 후 실제 수용

source `581c84520fc9d3c014ad155bf0eff13f96289db4`의
[run40242309674/job121316568836](https://github.com/bin9208/openpilot-rust/actions/runs/40242309674/job/121316568836)은
삭제된 `rust/tools/tests/test_offline_qcom_launch.py`의 working/index 부재를 검사 전후 모두
확인했다. 원래31개 parameterized case에 대응하는 native launch9, 독립 원본4411 corpus1,
stage8/ignored0·preserve_order union1·strict Clippy와 source/index-stage/wholeSDK 봉인·업로드/
소유 source cleanup이 모두 성공했다. 기존 Python file을 forwarding/skip으로 남기지 않았다.

artifact12618110662는1,586,311B/SHA256
`b05efdb316aaba463e68f47087c455e80268aefe81c26b8b27aac4af868bdf45`,
46 safe unique regular member/전체 CRC, expanded6,935,713B다. 명령4개 exit0,
source/index/index-stage·SDK file/program/roster6쌍 전후 byte 동일, SDK install0이다.

기준3580은 **300교체/3부분/3277대기**, routing3306은 **26교체/3부분/3277대기**다.
새 genuine compiler 비교 실행경계1개를 pending으로 추가했으므로 추가 원장163은
**32교체/127대기/1기존native/2비실행/1제거**다. production ordered emitter의 실제 genuine
검증은 별도 진행 중이며 이 테스트 파일1개 수용으로 전체 compiler/모델/Python 제거를
완료 처리하지 않는다.


## 2026-10-26 Planner native IPC 첫 통합 후보

baseline은 **300 replaced-local /3 partially-replaced /3277 pending =3580**, routing은
**26 accepted /3 partial /3277 pending =3306**을 유지한다. 이번 등록은 소스 제거 수용이 아니다.
기존 HOST12/ASan3/ARM12 뒤에 native reader4·strict Clippy·독립 원본401×4 공급·새 native IPC
네 run을 추가한다. 정확한 계약과 원본 artifact 출처는
[통합 기록](python-rust-523-planner-ipc-reader-next.md)에 남겼다. 첫 실제 새 feature 수용은 대기다.

새 Rust driver도 source 비교용 원래 Python daemon을 실행하므로 추가 실행 원장에 해당 경계1개를
pending으로 등록했다. 추가 원장 **164 =32 replaced-local /128 pending /1 native-existing
/2 non-executing-text /1 removed**이며 native transport 등록만으로 Python 실행 부재를 주장하지 않는다.
