# #523 Planner HOST IPC capture aa79 원시 artifact 감사

작성: 2026-10-26. 실제 source는 `aa79c7cd21a4b9bbb6bfb4c68ded9e857511f1cf`다.
[run40237263524](https://github.com/bin9208/openpilot-rust/actions/runs/40237263524) /
[job121301376514](https://github.com/bin9208/openpilot-rust/actions/runs/40237263524/job/121301376514)은
completed/SUCCESS이며 attempt1이다. 실행 WF는 `.github/workflows/python-rust-523-planner-caller.yml`,
job 이름은 `rust planner source comparison (local-x64)`다.

**full12 원본 비교·ASan3·finalOutput receipt 연결·source/index 후봉인 및 cleanup의 실제 성공을
확인했다.** HOST Rust whole-SDK 전후 봉인이나 실제 기기 수용으로 확대하지 않는다.
raw401×4·Params별 SHA·source corpus reader의 독립 감사는 Ohm 소유다. 이 문서는 그 검사를
반복하거나 대신 승인하지 않는다. Focus의 별도 실패도 부모 소유로 유지한다.

## 읽은 자료와 안전한 다운로드

API의 run/head repository는 `bin9208/openpilot-rust`이고 두 artifact의 workflow_run.id/head_sha는
위 run/source와 일치한다. 실제 runner는 `GitHub Actions 1000029643`, github-hosted/Linux/X64다.
job은 `2026-10-25T21:49:13Z`부터 `21:55:02Z`까지 실행됐다.

| artifact | 원문 bytes / SHA256 | ZIP 안전성 / 보존 |
| --- | --- | --- |
| private `12615278417`, `private-rust-planner-runtime-0-40237263524-1` | 1241625 / `3df5894ffe4a0ba003a670dee9978981d57656644dc16f83fb3da953db823a8d` | 2378멤버 / 4380147B expanded; 부모의 전체 CRC 검사를 재실행하지 않음. 이번에 전체 ZIP SHA·길이와 전체 경로/형식을 재확인. API expired=false, expires `2026-11-08T21:54:46Z` |
| functional `12615802090`, `rust-planner-local-x64-evidence` | 55187534 / `c583db165f1b5c0226dc25d2a25fb1cbba7a1fad0e282d25506f327f151cf828` | 34282멤버 / 170392398B expanded, 전체 CRC 성공. API expired=false, expires `2027-01-23T21:49:10Z` |

다운로드 전에 API size/digest/source를 확인했다. functional ZIP은 승인된64MiB 상한 안에서
고정 ID의 인증된 API redirect를 통해 받았다. credential과 signed URL은 출력/기록하지 않았다.
cache는 `.analysis/scratch/2026-10-25-523-completion/ci/12615802090.zip`이며 기존 private cache도
같은 디렉터리에 있다. 두 ZIP 모두 중복·절대 경로·빈 component·`.`/`..`·backslash·NUL 및
symlink/special-file entry를 거부해 검사했다. 추출/ELF 실행/원본 실행은 하지 않았다.

functional의 job prefix는 `tmp/openpilot-ci.0oi4Ta/`다. 아래 `planner-*` 경로는 이 prefix 아래이고,
private의 proof 경로는 ZIP root 기준이다. raw 전체나 ZIP 전체 내용을 출력하지 않고 지정한
receipt/manifest/log/source와 저장된 JSON 출력만 읽었다.

## full12의 실제 원본 비교

`planner-comparison/commands.json`은 정확히 아래12개 순서이며 각 returncode가0이다.
`result.json`은 status=PASS, architecture=x86_64, source_commit=aa79 전체 SHA와 같은12개 checks를
기록했다. private `runtime-outcome.txt`도 mode=runtime/source=aa79/exit_code=0이다.

| leaf | 원시 log/receipt에서 확인한 실제 범위 |
| --- | --- |
| abi | lateral/longitudinal 원본 Cython 대 native의 총12 solve, `bit_exact_solves=12`. 저장된 source/native JSON 값도 이번 inert 읽기에서 동일 |
| policy | departure17/coasting322/lead_tau15/geometry30, 합384 frame. source/native JSON 값 동일 |
| radar | 원본 pytest39 성공, actions311/instances36, mismatches=[] |
| gap | 원본 pytest42 성공, actions857/owners18/positive_plans187/positive_credit9, mismatches0 |
| owner | 원본 LongitudinalPlanner/LateralPlanner/CarrotPlanner의360 frame, mismatch_count0 |
| main | 원래360 입력 frame에 대응한712 poll frame/1080 messages, modelV2 trigger427/liveTracks285, mismatch_count0 |
| fault | `--faults --frames 80`,160 poll frame/240 messages, trigger99/61, mismatch_count0 |
| volkswagen | `--brand volkswagen`,712 poll frame/1080 messages, trigger712/0, mismatch_count0 |
| stock-longitudinal | `--stock-longitudinal`,712 poll frame/1080 messages, trigger427/285, mismatch_count0 |
| radar-off | `--radar-mode 0`,712 poll frame/1080 messages, trigger712/0, mismatch_count0 |
| ipc | source0/source1/rust0/rust1 네 daemon run, 각40 timing row/121 output messages. 비교3개 모두 differences=[] |
| lifecycle | 13 case×source/rust=26행, 전부 ok=true이며 실제 returncode가 각 expected_returncode와 일치 |

radar/gap log의 pytest 경고3개는 기존 config의 `asyncio_default_fixture_loop_scope`, `cpp_files`,
`cpp_harness` 미인식이다. leaf 실패/생략은 없다. fault의 `SQP_RTI: QP solver returned error status 3`
출력은 수집돼 있으며 fault receipt의 mismatch0와 구별한다. 이를 sanitizer 오류가 없다는 근거로
지우거나 숨기지 않았다.

원본 독립성은 aa79 source를 정적으로 대조했다. `plannerd_acados_source.py:108-112`는 원본
Cython trace로 expected를 먼저 쓰고, `plannerd_policy_source.py:182-188`은 source_modules에서
expected를 만든다. `plannerd_owner_source.py:95-104`와 `plannerd_runtime_source.py:184-195`도
원본 planner/SourceRun을 실행해 expected.json을 저장한 뒤 native를 호출한다. 이번 감사가
native 출력으로 expected를 새로 만들거나 원본을 로컬 실행한 것은 아니다.

비교 의미도 그대로다. ABI/policy는 JSON 값의 정확한 동등성을 요구한다. owner/gap/runtime/IPC의
공유 `difference`는 float를 `struct.pack('<d', ...)`로 비교해 signed zero까지 구별한다.
이 Planner 비교에 새 `1e-6` 허용오차를 적용한 것이 아니다. runtime의 canonical 출력은 원래대로
solverExecutionTime을 제외하고, IPC의 `policy`는 processingDelay/solverExecutionTime/
plannerExecutionTime/fastRadarExecutionTime을 제외한다. 원래 comparator의 timing 제외 범위를
전체 raw packet byte 일치로 확대해서 표현하지 않는다.

lifecycle은 wait-missing/directory/empty의 SIGINT/SIGTERM, ready SIGINT/SIGTERM,
recover-directory/settings-directories, fatal-integer/fatal-float/malformed-CarParams를 모두 포함한다.
대기/SIGINT의 raw Python -2 대 Rust0, 잘못된 숫자의 Python abort -6 대 typed Rust1은 원래
명시된 경계로 남아 있다. 모든 case의 exit가 서로 같다는 판정이 아니다.

## actual capturedOutput와 finalOutput 결합

private `planner-ipc-original-inputs/index.json`은2467B,
SHA256 `41fbf416e5593eb8ca408b230e0822fb6f2b58852c0db9283e2854956f6d9416`이며
schema_version1/kind=`independent-original-planner-ipc-run-index`/status=COMPLETE다.
그 `actual_output_receipt_sha256`은 다음 실제 functional member와 정확히 일치한다.

```text
tmp/openpilot-ci.0oi4Ta/planner-comparison/ipc/receipt.json
2033B
6cc4835f7a026ba77cc3e21a32d1544454ff6f0343bf836d342e1b9bd78e33f6
```

receipt의 네 run 모두 output service별 longitudinalPlan41/lateralPlan40/driverAssistance40이다.
valid count는40/39/39, longitudinal planningTrigger는 modelV2=6/liveTracks=35,
fast_overlays=34다. 각 daemon의 returncode=-15는 원래 수집 종료 SIGTERM과 `wait(timeout=3.0)`
assertion에 맞는다. 이를 비교 leaf의 exit0와 혼동하지 않는다.

| 저장된 output run receipt | bytes | SHA256 |
| --- | ---: | --- |
| `ipc/source-0/receipt.json` | 452513 | `0a4b1725d2f3a6a4971816dd728161cc068573ded7098c651767ea5a07bc1696` |
| `ipc/source-1/receipt.json` | 452517 | `ba219f2c0402b170856c5b250ffbcbfc0d140c1e50f3e9456891539b22fe404d` |
| `ipc/rust-0/receipt.json` | 452187 | `1eccd5f0646012a2fb58544a68a70dabef0135c04759bf186e2a7f6959b28cce` |
| `ipc/rust-1/receipt.json` | 452169 | `0d77543e809572cbead13cb0a3f940f742d40098bdf97f909617497a08d89c90` |

이번 inert 감사에서 저장된 네 output receipt의 records를 읽고, frozen source `comparable`와 같은
service/valid/data projection을 대조했다. float는 f64 little-endian bytes로 비교했으며 네121행이
동일하다. timestamp/path/timing은 원래 comparator와 같이 제외했다. raw401×4 입력의 의미 비교나
Cap'n Proto reader 실행을 대신한 결과가 아니다.

최종 index가 가리키는 네 입력 manifest의 실제 길이/SHA와 source_commit/source9 map도 확인했다.

| capture manifest | bytes | SHA256 |
| --- | ---: | --- |
| `source-0/manifest.json` | 135349 | `7ea42a9b5331de3aed91c6589d3095fdeccc683af45fdf63ae46a88073921da7` |
| `source-1/manifest.json` | 135349 | `2359f49887663492b8508ae98dd5c2b5a41afa4d4204d905f5cf8870eed5749d` |
| `rust-0/manifest.json` | 135266 | `adca249f510fed929c29463ce4c4c8ea71ae54e3b3ace210ab5bd8a4b0d1d564` |
| `rust-1/manifest.json` | 135266 | `8042cdb352e86ab6965b12948660cf1ec4b381597dec1abf72ff7e6ec92b8d5b` |

index가 선언한 events1604/frames160/CarParams4 및 각 run의 events401/Params184는 여기서는
**manifest 선언값**으로 기록한다. 실제 raw input 파일별 hash/순서/Params 내용과 reader qualification은
Ohm의 독립 결과를 따로 결합해야 한다.

네 manifest의 actual Python은 `/tmp/openpilot-ci.0oi4Ta/venv/bin/python`, canonical 실행물은
`/opt/hostedtoolcache/Python/3.12.14/x64/bin/python3.12`, 기록 SHA는
`bef88f140b625959f8af25c7b75cce2cd5d4b29cc2f2b079befd7f68eda4dba0`다.
Python3.12.14/GCC13.3.0/NumPy2.5.3/host x86_64와 같은 run/attempt/job가 기록돼 있다.
이 Python executable 원문은 artifact에 없으므로 SHA 기록을 로컬 bytes 검증으로 표현하지 않는다.
source daemon argv는 기존 `plannerd_daemon_source.py --binding ... --source-native ...`이고,
rust daemon은 실제 `openpilot-plannerd --solver ...`다.

aa79 source `InputCapture.finish`는 네 run 완료와 source9 재확인 뒤 실제 output receipt를 해시한다.
main은 비교 differences·valid steady state·두 trigger·overlay assertion을 통과한 뒤에만 finish를
호출한다. 따라서 COMPLETE와 위6cc4835f SHA는 candidate metadata 생성만의 통과 표시가 아니다.

## source9와 전후 source/index proof

네 capture manifest와 최종 index의 source9 map은 동일하다. 각 항목을 **aa79의 Git blob bytes**로
읽어 SHA를 대조했으며9/9 일치했다. 현재 다른 agent의 WIP 파일은 원본으로 사용하지 않았다.

| source | SHA256 |
| --- | --- |
| `rust/tools/plannerd_ipc.py` | `a4aec1991a6751af60f5616698e75e0861b0a1f8b1c478e5382889681598c8dc` |
| `rust/tools/plannerd_owner_fixtures.py` | `03b22ad4d2d0fb87270338e0f485a9936f7c4c8f48613bb9a44887add19da98c` |
| `openpilot/common/params_keys.h` | `b8422e8ad2cd74ec3c5750c3e919f6546519d84fabb5f6ec868b5372713b4d7f` |
| `openpilot/cereal/log.capnp` | `fdecc98871c0ebc11417a558a600de6ff96cc51c2ce620122d39b2cee55f1620` |
| `openpilot/cereal/custom.capnp` | `102d0aeb45effe3d15abb02a8d930834f8355f9817bf6e135a98435d9a832693` |
| `opendbc_repo/opendbc/car/car.capnp` | `6a4d28799933e7728a0a3d4d5a4d47c64fb11e9a6d75e3d4494d2344aefebfda` |
| `openpilot/cereal/services.py` | `a0745e2ef00036f0eef08b453a4798b4281549ea9ac6dc91c8b348ef76131edb` |
| `openpilot/selfdrive/modeld/constants.py` | `5d597d287df626d08b234a17db05e884302fead646ff4e7f0153079547c8010c` |
| `openpilot/cereal/messaging/__init__.py` | `1938c80c71dba0855700015bc03371786a711c67bfe3a0c5e073ed790d5e10a9` |

private `SOURCE_COMMIT`은 aa79 전체 SHA다. `index-before.bin`과 `index-after.bin`은1175705B,
NUL 구분19573경로로 bytes가 같다. 공통 SHA는
`5bd40abe60a4a6ea242b5aab2fda4eec27fa7be6ab3af379eac84a292dcb6b1b`이고,
로컬에서 읽은 `git ls-tree -rz --name-only aa79...`의 전체 roster와도 byte 동일하다.
source-before/source-after 파일이 기록한 normalized tracked tar SHA는 모두
`4b5359aa1c8a7825dfdb7e03344b38b4a2f782c5f4fb46e881442b9ae1e1f5e5`다.
이 tar 원문은 artifact에 없으므로 이번에 다시 봉인/재생성했다고 주장하지 않는다.

실제 job의 `Seal private source and index after maintained checks` step22가 SUCCESS다.
aa79 WF 본문은 HEAD exact 일치, index cmp, normalized tar hash cmp,
`git diff --exit-code`와 cached diff를 모두 요구한다. functional upload는 step20,
후봉인은22, private proof upload는23 순서라 후봉인 증거는 private artifact에서 읽었다.

`ownership.txt`은 owner=`40237263524:1:planner-runtime`, job=`/tmp/openpilot-ci.0oi4Ta`,
source owner=`/home/runner/work/openpilot-rust/openpilot-rust/.ci296-source-40237263524-1.VDTO9X`,
source_identity=`66305:9198252`, alias_identity=`66305:9198260`과 같은 workflow SHA를 기록한다.
actual step24의 source/alias/target cleanup과 step25의 common owned temp cleanup도 SUCCESS다.
cleanup source는 owner/canonical/dev-inode/alias와 두 upload의 성공·source seal을 요구한다.
별도 orphan census나 모든 process 회수 raw proof가 존재한다고 확대하지 않는다.

## native/원본 binding과 sanitizer 경계

full12 final receipt의9개 SHA 중 실제 ZIP member가 있는 native 실행물7개+manifest1개는8/8 재해시
일치했다. `openpilot-plannerd` SHA는
`dec820a673025300238df7fee030833c96d97217b0320afa9fb41ee8f4a549fd`다.
나머지 Params DSO는 functional upload가 `params/*.json`만 포함해 실제 bytes가 없다.
기록 SHA `cc09298c46680c6b345f089556fdea3dc8c4f4c433fc1d32e1b0fb9e688ca2e2`는
final/IPC/Params provenance 간 일치한다. msgq DSO의 기록 SHA
`b5b8f4feb14bff5aaa0ca0861bbc6b4bb77cd31460d6cf5a3300c28c9a4ffe5c`도 IPC identity에 있으나
해당 DSO bytes는 없다. 둘을 독립 whole-DSO before/after 검사로 세지 않는다.

ordinary와 sanitized의 각 manifest7개 library bytes는 모두 SHA와 일치한다. ABI는
`ocp-double-i32`, architecture=x86_64, acados=`0.2.2.post103`, wheel SHA는
`3b451852e83d62815cead999ab31073db9be60307f772650336cdd4534f12b9e`다.
두 provenance의 source272항목 중 동봉된270개 bytes를 검증했고, 나머지 원본 lat_mpc.py/
long_mpc.py 두 개는 aa79 Git blob과 일치했다. license5개도 각 bytes 일치다.
generator_files304개의 **기록 map**은 ordinary/sanitized/source binding receipt 간 같고,
그 전체 외부 generator 설치 bytes를 재다운로드/재실행한 것은 아니다.
source Cython lateral/longitudinal binding2개의 동봉된 `.so`는 해당 receipt의 binary SHA와 맞는다.
owner의 원본 source4와 radar 원본 test2의 기록 SHA도 aa79 Git blob과 일치했다.

ASan은 `planner-native-memory/comparison/commands.json`에 abi/owner/fault 정확히3행,
returncode0으로 남아 있다. 각각12 bit-exact solves/360 owner frame/160 fault poll frame·240 messages가
성공했고 `planner-native-memory/result.json` status=PASS와 private memory exit_code=0이 일치한다.
세 leaf의 native stderr는 모두0B이며 저장된 memory log8개9541B에서 AddressSanitizer/
LeakSanitizer error·sanitizer summary·UB runtime error 패턴을 발견하지 못했다.

동봉된 세 wrapper는 그대로 읽기만 했다. 기존 native solver/owner/runtime trace에
`LD_PRELOAD=/usr/lib/x86_64-linux-gnu/libasan.so.8.0.0`,
`ASAN_OPTIONS=detect_leaks=1:abort_on_error=1:strict_string_checks=1`,
`UBSAN_OPTIONS=halt_on_error=1:print_stacktrace=1`을 주고 argv를 그대로 exec한다.
owner ASan leaf receipt의 binary SHA는 wrapper 자체 SHA
`336f7317703d46b3f86a1e67ba94af524a00464e49e482b4766718a0a2427b90`이고,
final memory receipt는 underlying native trace SHA를 별도로 기록한다. 두 종류를 혼동하지 않는다.
ASan library의 기록 SHA는 `8d5845bdf056b5db4cfb9fff40ad5eeea8f6da05a8ca6f5935a7d1c138ed2558`이며
libasan 자체 bytes는 artifact에 없어 독립 재해시는 하지 못했다.

sanitized provenance는 sanitizers=true이고 ordinary는 false다. 생성된 solver library 두 개의
SHA는 달라지고 pinned acados/blasfeo/hpipm/qpoases library는 ordinary와 같다.
final scope 그대로 **generated solver C 계측·allocator leak/ownership 검사**를 수용한다.
Rust와 pinned external solver library 전체가 ASan/UBSan 계측됐다는 판정은 하지 않는다.

## 자원·상한과 SDK의 정확한 증거 범위

aa79의 실제 caller는 hosted ubuntu-24.04/local-x64 단일 matrix row와 job45분을 유지한다.
IPC 실행 직전에512MiB growth admission을 하고 `PLANNER_IPC_GROWTH_MIB=512`를 준다.
private runtime-operations는 abi/policy/radar/gap 각128MiB, owner/main/fault/volkswagen/
stock-longitudinal/radar-off 각1024MiB, ipc512MiB, lifecycle256MiB 및 각2700초를 기록한다.
memory-operations는128/1024/1024MiB·2700초다. commands의 free_before는 모두 해당 growth보다 크다.
controller build admission은2147483648B, sanitizer wrapper admission은134217728B다.
이 값들은 기존 작업별 예상 증가량과 실제 admission 기록이며 peak memory나 전체 합산 측정이 아니다.

같은 `/dev/root`의 job/source disk-before available=91719843840B,
disk-after=86413840384B가 남아 있다. source checkout 전2GiB 계획도 같은 FS의 proof에 있다.
이를 source/target/registry/venv/solver/proof 각각의 독립 peak 증명이나 새 일반 자원 정책으로
해석하지 않는다. 이번 감사는 source와 저장된 proof를 읽었으며 resource/cap 실행을 추가하지 않았다.

aa79 InputCapture의 기존 상한은 packet/CarParams 각1MiB, Params 파일64KiB·2048개·합계4MiB,
run64MiB, 전체256MiB, manifest/index 각1MiB다. `O_EXCL|O_NOFOLLOW`/0600 파일,
0700 소유 directory, canonical path/uid/dev-inode/길이/hash 검사와 실패 sticky 경계를 정적으로
확인했다. 이번 감사에서 상한/권한 negative를 실행했다고 주장하지 않는다. 실제 raw 크기·파일별
SHA 및 원본 reader 검증은 Ohm 범위로 남긴다.

private controller-source는 aa79:`x86_64-unknown-linux-gnu`, rustc.txt는 Rust1.94.0
`4a4ef493e3a1488c6e321570238084b38948f6db`/LLVM21.1.8이다. helper는 실제 private SDK의
cargo/rustc/rustdoc 절대 경로와 `RUSTUP_AUTO_INSTALL=0`을 요구한다.
**HOST whole-SDK before/after manifest는 private/functional artifact에 없다.** ARM530의 별도
whole-SDK 검증을 이 HOST 결과에 가져오지 않는다. controller SHA도 private에 기록돼 있지만
check-plannerd-ci 자체 ELF는 functional upload roster에 없어 그 bytes를 재해시하지 못했다.

## 추적용 핵심 SHA와 인계

| 자료 | SHA256 |
| --- | --- |
| `planner-comparison/result.json` | `0882cc0f4b33fea60e174872a82deaeb555c8ec20cedf52e087211f0fc6d9514` |
| `planner-comparison/commands.json` | `eb9b617a0a9882a83e054b6f8f16c01ec8f6f4b4a7ec87e9c9afa622763c3bc9` |
| `planner-native-memory/result.json` | `f90057c08d57d7bb464bc081f76b4dc52689d02678a03514eed93aa0be9fdf9d` |
| `planner-native-memory/comparison/commands.json` | `3db46523525ddb2d1bc9972eabbdafa0d740950a1e0df8e28ea8a3f719c5d4c1` |
| ordinary manifest / provenance | `5d722312729fcaf17350f796ec09f61c5d66bed62dd59f23b413c2b4a11c1328` / `b40cbf9f176736e3276d145922a135e23e0bf80c91e27e4c5b4eaefc0626b628` |
| sanitized manifest / provenance | `782b86bbfe720add5779233dad01157fd7e3dea358bfafec968c124009923b2c` / `834d15629e777883cfc4bb100c5efff84647cc2ae14574baffb1e20111242e8e` |
| original source binding receipt | `826497dc04d0e9e6a332eb8b63b78d61d0e2bb3ef807b40015d37bbdcb017f8b` |
| actual caller WF | `928efb63ef62fc055e9597f8a4b297ae9fbf4fdc57b0d9bf7bcffa26f10f86ba` |
| reviewed main WF499336B | `b51def7342359305c818f15ec59e9002d6b7c1ee73752dc08e91442444cfface` |
| actual helper7197B | `32cfc4a9b57010c57bf3bdd112c4339ee22ea2ecd5618c39f6b1aa441de5ea63` |

부모가 연결할 범위는 aa79 HOST runtime12/ASan3의 실제 수용, output receipt6cc4835f와 private
capture index/manifest/source9/post-source proof의 결합이다. 독립 raw401×4/Params/corpus-reader
수용은 Ohm 결과를 기다린다. 향후 native scheduler/IPC peer 전환, Python 삭제, ARM raw capture,
whole-SDK 수용 또는 기기 acceptance를 이 결과만으로 승인하지 않는다.

이번 작업의 tracked 쓰기는 이 새 문서 하나뿐이다. 별도 승인된 retention brief는
`python-rust-523-evidence-retention.md`에 기록했다. source/WF/expected/ledger/pins를 수정하지 않았고
stage/commit/push/GHA dispatch·취소·로컬 기능/model/oracle/Cargo/test 실행은0이다.
