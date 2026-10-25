# #523 Planner IPC 실제 capture reader와 외부 감사 입력 계약

2026-10-26. 작업 루트는 `/home/fma1/.codex/worktrees/python-rust-523-completion/openpilot-rust`다.
아래 단계별 감사 기록은 원래 Python collector 두 파일과 기존 IPC 동작을 보존한 채 작성했다.

## 현재 통합 상태: 첫 HOST/ARM qualification 대기

`planner-ipc-native` feature, `check-plannerd-ci ipc`, 원래 C++ msgq source를 직접 링크하는
peer와 native reader를 등록했다. `observed_tests`는 별도 integration target 대신 lib의
`planner_ci_controller::planner_ipc_native::observed_tests::` 아래 일반4개로 연결했다.
아래 초기 기록의 미등록·부모 대기 표기는 해당 단계의 상태이며 현재 등록 상태는 이 절을 따른다.
실제 새 feature의 컴파일·4개 실행·Clippy·전체 IPC 성공은 아직 관측하지 않았다.

기존 standalone HOST12/ASan3와 ARM12를 유지하고 그 뒤에 정확한 세 단계를 추가한다.
첫 단계에서 lib4개와 strict Clippy를 확인하고, 둘째 단계에서 고정 Bash stager가 독립 원본을
공급하며, 셋째 단계에서 source/rust×epoch0/1의 네 native IPC run을 비교한다.
runtime, memory, IPC는 처음에 빌드한 같은 controller ELF와 source/feature/SHA 봉인을 사용한다.
새 scheduler 증거160행은 기존 Python IPC의160행과 별도로 보존한다.
main `.github/workflows/rust.yml`, private alias79/ARM24, 원래 Python scheduler13개는 유지한다.

부모가 실제 HOST/ARM ZIP 네 개와 receipt를 감사한 결과로
`tests/fixtures/planner-ipc-original-audit/{x86_64,aarch64}.json`을 작성했다.
고정 SHA는 각각 `384bcef1bafc962019557d7b89ef5dac4c85ee2d1683906c1942458b04118e54`,
`7dd3abad75e472a7241d16143b155dd3ad6e8bb826359bd0afedca1360689efc`다.
stager는 원래 run/source/artifact ID/bytes/digest를 API에 대조하고 private capture와
정확한 summary receipt만 안전 추출한다. 원문 ZIP도 그대로 보존한다. native reader는
외부 audit→ZIP→index→네 manifest→모든 raw 순서로 다시 인증한다.
stager의128 MiB 디스크 admission은 RAM 측정이나 메모리 admission의 증거가 아니다.

새 IPC controller는 기존 owned session launcher를 사용한다. 종료 시 직접 자식을 wait하고
확인된 소유 그룹의 live process 부재를 검사한다. 모든 입양 자손의 reap을 주장하지 않는다.
Rust daemon/peer의 오류 경로는 KILL 뒤 최대3초 동안 직접 child의 reap을 기다린다.
새 native IPC를 실제로 통과하기 전 Python collector/daemon/oracle 삭제 credit은0이다.

## 실제 writer에 맞춘 reader/caller 변경

단일 manifest를 source/rust×epoch에 재사용하는 경로를 없앴다. `Capture::open`이 실제 root
index를 인증하고 source-0/source-1/rust-0/rust-1의 네 manifest를 각각 읽는다.
caller는 매 run에 `capture.select(mode, epoch)`를 호출해 해당 Params/CarParams/401 event만
전달한다. mode/epoch를 바꾸거나 같은 manifest를 다른 run에 연결하면 거부한다.
header나 raw bytes가 같다고 가정하지 않으며 input을 생성하는 fallback은 없다.

index는 실제 schema_version1/kind=`independent-original-planner-ipc-run-index`, COMPLETE,
events1604/frames160/CarParams4를 요구한다. 네 entry는 원래 순서, events401,
`<mode>-<epoch>/manifest.json`, 실제 Params 파일 수를 요구한다. 각 manifest의 source commit,
source9 SHA, origin run/attempt/job/host와 mode/epoch를 index·외부 audit에 대조한다.
Python executable/canonical path/SHA/version, NumPy version, prefix, daemon argv와 prelaunch
snapshot 표기도 유지한다. source/rust argv는 원래 형식과 역사 binding/solver 경로에 대조한다.

각 run은 원래 initial live1 +40×(non-model8→modelV2→liveTracks)의401개 경로/순서를 요구한다.
Params의 `params/<key>`와 실제 `CarParams.pending.bin`을 별도로 읽는다. JSON과 raw의 path/size/SHA,
순서, snapshot184개 등은 원본 관측에서 온 값이다. 입력의 예상 수치/bytes를 Rust로 계산하지 않는다.
native driver의 source/Rust daemon 실행과 기존 publication 비교·valid/trigger/overlay 검사는 유지한다.

packet/CarParams1 MiB, Params64 KiB/key·2048개·4 MiB/run, manifest/index 각각1 MiB,
run64 MiB·capture 전체256 MiB를 writer와 같이 적용한다. JSON도 합계에 포함한다.
외부 audit/원본 summary receipt는 각각1 MiB, 각 보존 ZIP은1 GiB를 상한으로 둔다.
ZIP은64 KiB buffer로 hash만 읽고 메모리에 전체 파일을 올리지 않는다.
canonical path/regular file/O_NOFOLLOW 및 FD/path의 dev/ino/mode/size/mtime/ctime 전후 동일성을
검사한다. 경로 이탈, symlink, 변조, incomplete index, 원래 receipt의 실패는 모두 거부한다.

## 업로드 후 공급하는 엄격한 외부 audit 인터페이스

capture 시점에는 artifact ID를 알 수 없다. 원본 manifest에 `origin_artifact_id`를 넣거나
나중에 원문을 고쳐 채우는 경로는 없다. `Audit` JSON은 **실제 업로드 후 원문을 감사한 부모가**
작성하고 그 파일의 SHA를 별도로 고정한다. 원본 index/manifest/receipt bytes는 그대로 둔다.

현재 `Options`에서 이전 `corpus_manifest_sha256` 대신 필요한 입력은 아래 셋이다.

| 필드 | 의미 |
| --- | --- |
| `corpus` | 보존 ZIP 두 개와 각 안전 추출 디렉터리가 있는 absolute canonical audit bundle root |
| `corpus_audit` | 부모가 작성·검토한 외부 audit JSON의 absolute canonical 파일 경로 |
| `corpus_audit_sha256` | 부모가 감사 결과로 고정한64자리 SHA256. 실행 시 미검토 JSON에서 스스로 계산해 신뢰하는 용도가 아님 |

`Audit`의 정확한 필드는 다음과 같다. 실제 고정 JSON은 위 현재 통합 절의 두 파일이다.

| 필드 | 요구 사항 |
| --- | --- |
| `schema_version`, `kind` | 1, `audited-original-planner-ipc-capture` |
| `source_commit`, `source_sha256` | 실제 origin40자리 commit과 index/manifest의 정확한 source9 path→SHA map |
| `origin_run_id`, `origin_run_attempt`, `origin_job`, `host_architecture` | 원래 capture의 값. run/attempt는 양수 decimal string; host는 x86_64 또는 aarch64 |
| `artifacts` | 정확히 `capture`, `receipt` 두 role. 실제 private/functional artifact를 별도로 선언 |
| 각 artifact의 `id`, `bytes`, `sha256` | 실제 업로드 artifact ID, 내려받은 ZIP bytes/SHA. 두 ID는 달라야 함 |
| 각 artifact의 `archive`, `extracted` | bundle root에 상대적인 보존 ZIP 경로/안전 추출 디렉터리. 서로 별도 경로 |
| `index` | capture 추출 root 기준 실제 ZIP member의 `{path,bytes,sha256}` |
| `receipt` | receipt 추출 root 기준 실제 원본 summary receipt member의 `{path,bytes,sha256}`. index의 actual_output_receipt_sha256과 일치 |
| `identities` | 정확히 binary/params_binding/cpp_binding/solver_manifest 네 role |
| 각 identity의 `captured_path`, `sha256` | 원래 receipt identities의 절대 경로 key와 SHA. 네 key/hash 전체가 receipt 원문과 일치해야 함 |

모든 구조는 unknown field를 거부한다. root index와 각 manifest에도 같은 원칙을 적용한다.
현재 private ZIP은 `planner-ipc-original-inputs/index.json`을 최상위에 포함한다. 임의의
`tmp/.../proof/` prefix를 추정하지 말고 **실제 ZIP member 이름**을 `index.path`에 선언한다.
functional ZIP의 receipt member 이름은 그 ZIP을 감사한 뒤 선언한다.

인증 순서는 고정 audit SHA→두 ZIP bytes/SHA→index SHA→네 manifest SHA→각 raw SHA다.
원본 receipt는 별도 artifact에서 읽고 index와 동일 SHA인지, 네 returncode=-15,
비교3개 differences empty, valid3종/trigger2종/overlay가 양수인지 확인한다.
capture source commit/binding/receipt는 이 역사 기록으로 유지한다. 현재 GITHUB_SHA나 새 빌드
경로/hash로 원문 identity key를 치환하지 않는다.

현재 실행물의 binary/params binding/원래 cpp binding/solver manifest는 기존 native receipt의
현재 경로별 identities로 따로 봉인하고 성공·실패 반환 전에 재검사한다. 역사 빌드 SHA를
새 빌드 SHA로 덮어쓰거나 두 빌드가 항상 바이트 동일하다고 가정하지 않는다.
현재 host와 Cereal log/custom/car schema·services는 capture domain과 일치해야 한다.
역사 producer Python 파일을 현재 checkout에서 다시 실행하거나, 미래 삭제 뒤 존재를 요구하지 않는다.
기존 원본 Python daemon/fixture/oracle 및 actual source/Rust 출력 비교는 계속 필요하다.

`native-peer-provenance.json`에는 외부 audit와 그 고정 SHA, 네 선택 manifest를 기록한다.
`run`은 인증이 끝나기 전 output/peer build/daemon을 만들지 않는다. 작업 결과를 보존한 뒤
성공·실패 모두 capture/audit/ZIP/receipt/current schema/current identities를 반환 전에 재확인한다.

reader는 ZIP을 해제하거나 GitHub API에 접속하지 않는다. **ID와 실제 Actions source/run의 연결,
ZIP 전체 safe-path/CRC/member 감사, 추출물과 해당 ZIP의 연결은 부모의 선행 감사 책임**이다.
reader가 직접 확인하는 것은 그 외부 감사의 고정 SHA와 선언된 ZIP/member bytes의 정합성이다.
따라서 audit SHA를 검토 없이 후보 JSON에서 계산해 넘기는 것은 qualification이 아니다.

## aa79 HOST 실제 private capture의 읽기 전용 감사

사용자가 제공한 실제 cohort는 source `aa79c7cd21a4b9bbb6bfb4c68ded9e857511f1cf`,
run40237263524/job121301376514다. cached private ZIP
`.analysis/scratch/2026-10-25-523-completion/ci/12615278417.zip`을 원문으로 읽었다.
부모가 이미 완료한 전체2378 safe-member/CRC 감사는 반복하지 않고 새 input 자료만 검사했다.

| 관측 | 확인 결과 |
| --- | --- |
| private ZIP | 1,241,625 B, SHA `3df5894ffe4a0ba003a670dee9978981d57656644dc16f83fb3da953db823a8d` |
| root index | 2467 B, SHA `41fbf416e5593eb8ca408b230e0822fb6f2b58852c0db9283e2854956f6d9416`, COMPLETE |
| source-0 manifest | 135349 B, SHA `7ea42a9b5331de3aed91c6589d3095fdeccc683af45fdf63ae46a88073921da7` |
| source-1 manifest | 135349 B, SHA `2359f49887663492b8508ae98dd5c2b5a41afa4d4204d905f5cf8870eed5749d` |
| rust-0 manifest | 135266 B, SHA `adca249f510fed929c29463ce4c4c8ea71ae54e3b3ace210ab5bd8a4b0d1d564` |
| rust-1 manifest | 135266 B, SHA `8042cdb352e86ab6965b12948660cf1ec4b381597dec1abf72ff7e6ec92b8d5b` |
| raw files | events1604 + Params736 + pending CarParams4 =2344개, 모두 실제 size/SHA/경로·순서 일치 |
| capture 전체 | index/manifest 포함2,022,221 B. source 각504980 B, rust 각504897 B |
| source9 | 네 manifest/index와 현재 파일9개의 SHA가 전부 일치 |
| 초기 Params | 각184개. 현재 params_keys.h 선언의 literal 값과 owner fixture/run의 명시 override를 소스로 읽어 대조해 전부 일치. 원본 module 실행0 |
| pending CarParams | 각각280 B, 각각 SHA `74de9e672b5d93d21bfeeb57ec6f45677bec4dce85afeffcee027b11c15763df` |
| origin | run40237263524/attempt1/planner-runtime/x86_64, 실제 Python3.12.14/NumPy2.5.3, provenance15개 |
| 실제 Python 실행물 SHA | `bef88f140b625959f8af25c7b75cce2cd5d4b29cc2f2b079befd7f68eda4dba0`이라는 원본 provenance 값 확인. 로컬에서 원격 실행물을 재검증한 의미는 아님 |

네 run의401개 packet SHA는 각각 읽어 대조한 결과 현재 cohort에서 모두 같았다.
이 결과를 reader의 dedup/동일-header 전제로 사용하지 않았고 네 manifest/snapshot을 계속 선택한다.
source argv의 params binding은 `/tmp/openpilot-ci.0oi4Ta/planner-original/params/params_pyx.cpython-312-x86_64-linux-gnu.so`,
source-native는 `/tmp/openpilot-ci.0oi4Ta/planner-source`다. rust argv의 solver는
`/tmp/openpilot-ci.0oi4Ta/planner-artifact`다. argv는 origin 관측이며 파일 hash의 대체 증거가 아니다.

원본 summary receipt의 SHA는 index에
`6cc4835f7a026ba77cc3e21a32d1544454ff6f0343bf836d342e1b9bd78e33f6`으로 기록되어 있다.
**private ZIP에는 이 receipt와 네 origin identity hash 원문이 없다.** 작업 중 부모가 cached
functional artifact12615802090을 준비해 선택 receipt를 추가로 읽었다. ZIP은55,187,534 B/SHA
`c583db165f1b5c0226dc25d2a25fb1cbba7a1fad0e282d25506f327f151cf828`과 일치했다.
선택 member는 `tmp/openpilot-ci.0oi4Ta/planner-comparison/ipc/receipt.json`, 2033 B이며
실제 SHA가 index의 `6cc483...`와 일치했다. 네 run 모두 returncode=-15/messages121,
valid driverAssistance39/lateralPlan39/longitudinalPlan40, trigger liveTracks35/modelV2 6,
fast_overlays34이며 비교3개의 differences는 empty다.

원래 receipt에서 읽은 origin identities는 다음과 같다. 경로 key는 앞서 관측한 실제 argv와
맞으며 새 빌드 경로로 바꾸지 않았다.

| role | 원본 identity SHA256 | 추가 확인 범위 |
| --- | --- | --- |
| binary | `dec820a673025300238df7fee030833c96d97217b0320afa9fb41ee8f4a549fd` | ZIP의 실제 openpilot-plannerd17,085,888 B를 hash해 일치 확인 |
| params_binding | `cc09298c46680c6b345f089556fdea3dc8c4f4c433fc1d32e1b0fb9e688ca2e2` | 원본 receipt에 기록된 hash. 해당 .so payload는 ZIP에 없어 직접 재hash하지 않음 |
| cpp_binding | `b5b8f4feb14bff5aaa0ca0861bbc6b4bb77cd31460d6cf5a3300c28c9a4ffe5c` | 원본 receipt에 기록된 hash. 해당 .so payload는 ZIP에 없어 직접 재hash하지 않음 |
| solver_manifest | `5d722312729fcaf17350f796ec09f61c5d66bed62dd59f23b413c2b4a11c1328` | ZIP의 실제 planner-artifact/manifest.json1128 B를 hash해 일치 확인 |

binding 경로 key는 각각 `/tmp/openpilot-ci.0oi4Ta/planner-original/params/params_pyx.cpython-312-x86_64-linux-gnu.so`,
`/tmp/openpilot-ci.0oi4Ta/planner-original/ipc/msgq/ipc_pyx.cpython-312-x86_64-linux-gnu.so`다.
binary key는 원본 argv의 private source 아래 `rust/target/debug/openpilot-plannerd`,
solver manifest key는 `/tmp/openpilot-ci.0oi4Ta/planner-artifact/manifest.json`이다.
없는 binding payload나 예상 hash를 argv/native 계산에서 생성하지 않았다.
이번 선택 감사는 부모의 functional ZIP 전체 CRC/actual leaf/source/index seal 감사와 구분한다.
최종 qualification 및 audit JSON/SHA 고정은 계속 부모 소유이며 이 문서에서 대행하지 않는다.

## 회귀 소스와 남은 수용

기존 `observed_tests.rs`의 source425 decoder1/missing capture1을 유지하며 reader 경계2개를
추가했다. 향후 부모가 등록할 `planner_ipc_observations`는 일반4/ignored0이다.

- `source425_original_cpp_packets_match_original_python_policy_fields`
- `missing_independent_model_input_corpus_is_rejected_without_generating_inputs`
- `writer_schema_selects_each_mode_epoch_without_reusing_or_generating_packets`
- `capture_auth_rejects_unpinned_swapped_incomplete_and_reappeared_data`

새 private fixture의 raw는 구별 가능한 opaque unit token이다. Cap'n Proto message, 원본 corpus,
native 예상치, 실제 artifact ZIP 또는 retirement 근거로 쓰지 않는다. 이 fixture로 mode/epoch
선택, audit pin·index·원래 receipt 실패·binding 치환·symlink·archive 변조 거부만 검사한다.
실제 capture를 native reader/driver로 실행한 증거는 아직 없다.

앞선 reader 단계에서는 `scheduler_client.rs`, scheduler parser, `frame_loop.rs`, `decode.rs`,
`peer.rs`, C++ peer, source425 raw fixture를 변경하지 않았다. 이전 미등록 client test의2칸 oracle 끝 marker 수정과
원래 `a04e889c725715395baae751a5b3526c0c686b0075d67ab95b95fb94bd987be1` pin도 유지한다.
이후 실제 parser35 수령에 따른 후속 변경은 아래에 기록한다. 전체 adapter 수용의 실행 증거와 구분한다.

부모가 두 artifact와 원본 receipt를 감사한 뒤 audit JSON/SHA를 고정하고, 공유 등록을 승인한
후에만 필요한 GHA 검사를 배정한다. 예정 target은 기존 feature `planner-ipc-native` 아래
`planner_ipc_observations`4개와 strict Clippy다. 실제 CLI mapping은 위 Options3개를 전달하며,
기존 SDK/jobs1/timeout/growth와 원본 source/Rust 실행·수용 조건을 유지해야 한다.
이 시점의 reader 변경은 retirement credit0이며 ARM/parser 후속 증거와 전체 native 수용은 별도다.

공유 등록 승인 후 부모 GHA의 reader 회귀/서식 외 compiler 검사 명령은 다음과 같다. 로컬에서는 실행하지 않았다.

```bash
CARGO_BUILD_JOBS=1 cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --features ci-receipt-metadata-native,planner-ipc-native --lib planner_ci_controller::planner_ipc_native::observed_tests:: -- --test-threads=1 --nocapture
CARGO_BUILD_JOBS=1 cargo clippy --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --features ci-receipt-metadata-native,planner-ipc-native --lib --bin check-plannerd-ci -- -D warnings
```

로컬은 Rustfmt2021 syntax/format/diff와 ZIP/JSON/source bytes의 inert 감사만 수행했다.
Cargo/build/Clippy/test/원본 Python/project CLI/C++ compile0, WF/GHA/stage/commit/push0이다.

현재 미등록 변경 source SHA256은 다음과 같다.

```text
e051a50ba2b93fcc01c47367f508fefa411bdaf001e67e85a84f723c70e2b8bd  rust/crates/dev-tools/src/planner_ci_controller/planner_ipc_native/corpus.rs
31e4365621dd4aa9a90c8271dd3cae60bf2bceef7777f98442e7f3b554904096  rust/crates/dev-tools/src/planner_ci_controller/planner_ipc_native/mod.rs
522c0e5e4151687376911be7719c531d0be80a687ada823ad127177d46ea5437  rust/crates/dev-tools/src/planner_ci_controller/planner_ipc_native/observed_tests.rs
```

## 후속: aa79 실제 parser35를 수령한 client 문법 수정

사용자가 original sourceaa79 run40237263418/job121303387188 SUCCESS와 cached artifact12615269988을
제공했다. 선택 원문 `planner-scheduler-original-parse/observations.json`의9160 B/SHA
`afe5ea47497bf9ea1af07231400055f348c3c8e555020c1f747c97eb51168acd`를 직접 확인하고
`rust/crates/dev-tools/tests/fixtures/planner-scheduler-original/observations.json`에 바이트 그대로 복사했다.
origin/source/class/AST/Python 실행물·version 및 ZIP provenance는 같은 디렉터리 README에 기록했다.
원문은 finite17/RuntimeError7/ValueError9/UnicodeDecodeError2의35개 실제 관측이다.
raw JSON을 다시 만들거나 native 결과에서 expected를 채우지 않았다.

기존 `parse_canonical_response`를 `parse_response`로 바꿔 실제 public ASCII domain을 받도록 했다.
ASCII decode 거부를 먼저 적용하고, 바깥 whitespace는 `09/0A/0B/0C/0D/20`을 명시한다.
특히 원본 VT(`0x0B`)/FF가 통과하도록 Rust trim_ascii/is_ascii_whitespace에 의존하지 않는다.
sign/decimal/dot/exponent와 digit 사이 underscore를 검사한 뒤 수치 변환만 std f64 parser에
맡긴다. nonfinite spellings와 overflow는 NonFinite, 문법 오류는 InvalidFloat,
nonASCII는 NonAscii로 거부한다. signed zero/underflow bits는 원문과 비교하도록 작성했다.
NonFinite 진단은 원본 RuntimeError 메시지를 유지한다. 다른 Python repr/codec 메시지의
바이트 복제까지 주장하지 않으며 원래 메시지는 고정 JSON에 보존했다.

client의 framing/256-byte limit/2초 공유 deadline/latch/3초 close·TERM·KILL·reap/Drop 코드는
parser dispatch 한 줄을 제외하고 수정 전과 바이트가 같다. 원래13개 중12개 test body는
수정 전과 바이트가 같고 real-native4×40=160의 입력/expected bits/호출 수 역시 그대로다.
남은 invalid-response test는 원래 여섯 실패 peer를 유지하면서 잘못 거부하던 whitespace/
underscore를 새 finite 수용 회귀로 옮겼다.

추가 일반 회귀는 아래2개이며 target `planner_scheduler_client`는 이제 일반15/ignored0이다.

- `frozen_original_ascii_parser_stage_matches_native_bits_and_error_classes`: 원문35개 bits/오류 종류 비교.
- `persistent_client_accepts_frozen_original_ascii_responses_in_one_session`: 원문 finite17개를
  한 shell peer session의 실제 응답 bytes로 제공하고 같은 public client 경로/회수 검사.

이 shell peer는 원본 결과를 생성하지 않는다. parse35 자체는 AST 단계 관측이며 전체 Python
client의35 session 실행 증거로 확대하지 않는다. canonical 생성자 이름은 기존 공식 schedule
endpoint 연결 API로 유지했고 응답의 canonical-only 제한은 제거했다.

부모 등록 후 필요한 GHA 명령은 아래와 같다. 현재 세션에서 실행하지 않았다.

```bash
CARGO_BUILD_JOBS=1 cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --test planner_scheduler_client
CARGO_BUILD_JOBS=1 cargo clippy --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --bin check-plannerd-ci --test planner_scheduler_client -- -D warnings
```

두 artifact를 받는 외부 audit/reader 인터페이스와 reader 일반4개는 앞선 준비본 그대로다.
사용자가 추가 보고한 ARM run40237263421/job121301376184 SUCCESS와 privateartifact12615287431은
부모 증거이며 이 세션에서 추가 다운로드/감사하지 않았다. Python 원본·기존 발행 IPC 문서,
shared registration/CLI/Cargo/WF/pin은 이번에도 수정하지 않았다. 로컬은 구문/서식/diff와 inert
원문 복사·비교뿐이며 새 Rust parser/stdio 회귀/actual native IPC 성공은 부모 Actions에서 확인해야 한다.

후속 parser 변경의 source SHA256:

```text
a15de8f4509c395850052084447575b48d6ed5db60df09a6799e0943af653d6b  rust/crates/dev-tools/src/planner_ci_controller/scheduler_client.rs
4f9ec7a00b8ec00dfcb9ed518f10f995b5c0420c74271cca78d34d39b96dfd66  rust/crates/dev-tools/tests/planner_scheduler_client.rs
afe5ea47497bf9ea1af07231400055f348c3c8e555020c1f747c97eb51168acd  rust/crates/dev-tools/tests/fixtures/planner-scheduler-original/observations.json
12a824ce752fa30efad8af0e00a7fb9466474708b61b9e5dad5824980a4fb926  rust/crates/dev-tools/tests/fixtures/planner-scheduler-original/README.md
```

## 후속: aa79 ARM 원문 capture/receipt의 data-only 감사

부모 검토가 끝난 최소5파일(client/frame_loop/test/parser35 JSON/README)은 동결했다.
이 후속 작업은 해당 파일과 reader/Cargo/CLI/WF를 수정하지 않고 이 문서와 ignored scratch의
원문 ZIP·데이터 관측 기록만 추가한다. 전체 IPC 등록과 새 native client CI는 부모 소유다.

GitHub artifact metadata를 조회해 아래 두 ID가 모두 run40237263421,
source `aa79c7cd21a4b9bbb6bfb4c68ded9e857511f1cf` 소속임을 확인했다.
job121301376184의 원래 ARM cohort이며 artifact의 origin job은 `planner-runtime`이다.
조회/다운로드만 수행했고 workflow 실행·재실행은 하지 않았다.

| 실제 ARM artifact | ZIP bytes / SHA256 | 데이터 감사 |
| --- | --- | --- |
| private12615287431 (`private-rust-planner-runtime-0-40237263421-1`) | 1,239,760 / `efa5a289799a4e1fbcd63cc8baae5f3379df20240f6c48ecad6dc68b15c5682b` | 2371 unique safe members, symlink/경로 이탈/암호화 없음, 전체 CRC 정상, expanded4,379,337 B |
| functional12614614983 (`rust-planner-hosted-arm64-evidence`) | 44,843,072 / `2726bee5baa231c56c1856cea7bc0096b549bd6bb600cd262e04836551311798` | 29679 unique safe members, symlink/경로 이탈/암호화 없음, 전체 CRC 정상, expanded141,299,042 B |

ZIP은 `.analysis/scratch/2026-10-25-523-completion/ci/12615287431.zip`과
같은 디렉터리의 `12614614983.zip`에 보존했다. 증가 예상 약46 MB에 대해 다운로드 전 가용
74,909,171,712 B를 확인했다. 압축을 파일시스템에 풀거나 업로드 binary를 실행하지 않았다.
member당512 MiB/expanded 합계1 GiB 상한 아래64 KiB 단위로 읽어 CRC를 검사했다.

ARM root index는 private ZIP의 `planner-ipc-original-inputs/index.json`, 2467 B,
SHA `2983ef06918d9cdc667cdae75f061d2a97c4fbd875819d4f2ffc15b961765c4e`다.
COMPLETE/events1604/frames160/CarParams4와 source/rust×epoch의 원래 순서를 확인했다.

| ARM run | manifest bytes / SHA256 | raw 관측 |
| --- | --- | --- |
| source-0 | 135353 / `44c7e47475e63ffcb5e115e6ecbe1e2f9adb783ac83899a907fc12efbfb7217f` | events401, Params184, run 전체504984 B |
| source-1 | 135353 / `7e1935ebaf2e19088c2647e173a80c1cb90c350e6fcb8d04aa47e15f0c475b66` | events401, Params184, run 전체504984 B |
| rust-0 | 135269 / `f1b39c4447c24dd203b58071e63b386d0439903f91a68b177d743994367d6428` | events401, Params184, run 전체504900 B |
| rust-1 | 135269 / `ebd3da7d08be63177c5ffc8e64d192511550a694beaf3f7484b243c0186ac228` | events401, Params184, run 전체504900 B |

네 manifest 경로는 각각 `<mode>-<epoch>/manifest.json`이다. manifest 선언과 실제 member의
bytes/SHA, initial live1→40×(non-model8→modelV2→liveTracks)의 순서/파일 번호를 전부 대조했다.
events1604+Params736+pending CarParams4의2344개 raw 파일은 중복 경로 없이 일치했다.
index/manifest까지 포함한 ARM capture는2,022,235 B다. 모든 packet/Params/run/전체 상한 이내다.

Params는 네 run 각각184개이며 현재 params_keys.h의 선언된 literal default와 fixture/run의
명시 override를 **소스로만** 읽어 byte 값을 대조했다. 전부 일치했다. 원본 parameters 함수의
import/호출은0이다. 실제 pending CarParams는 네 개 각각280 B,
SHA `74de9e672b5d93d21bfeeb57ec6f45677bec4dce85afeffcee027b11c15763df`다.

아래 source9 SHA는 ARM index/네 manifest 및 현재 파일9개와 전부 일치했고 HOST와도 같다.

| source9 경로 | SHA256 |
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

각 run의15개 provenance에서 origin run=`40237263421`, attempt=`1`, job=`planner-runtime`,
host=`aarch64`, input_transport=`actual-original-cpp-pubmaster`, params_snapshot=`before-daemon-launch`를
확인했다. Python은 `/tmp/openpilot-ci.hHf287/venv/bin/python`, canonical 실행물은
`/opt/hostedtoolcache/Python/3.12.14/arm64/bin/python3.12`, 기록된 SHA는
`ce6ebce3a3a4b6216475acb8090d1a6b58dbaa73b0843e03e2b4027b5d729e7d`다.
version은 `3.12.14 (main, Aug 13 2026, 02:48:06) [GCC 13.3.0]`, NumPy는2.5.3이다.
이 실행물 SHA는 원래 capture의 기록이며 로컬에서 원격 Python을 실행/재hash한 의미가 아니다.
source prefix는 `planner_70jlhgs5`, rust prefix는 `planner_pgbvvwhv`로 각각 epoch0/1에서 보존됐다.

ARM의 원본 summary receipt member는 functional ZIP의
`tmp/openpilot-ci.hHf287/planner-comparison/ipc/receipt.json`, 2035 B,
SHA `3628d937d6df5db8f1ac3cb9664b0ff9ccad0c354138e3710a94ab6a0f3810b7`다.
private index의 actual_output_receipt_sha256과 일치했다. 네 run 모두 returncode=-15,
messages121, valid driverAssistance39/lateralPlan39/longitudinalPlan40,
trigger liveTracks35/modelV2 6, fast_overlays34이며 comparisons3개의 differences는 empty다.

| ARM origin role | 원래 captured_path | SHA256 / 확인 범위 |
| --- | --- | --- |
| binary | `/home/runner/work/openpilot-rust/openpilot-rust/.ci296-source-40237263421-1.gsAqQC/source/rust/target/debug/openpilot-plannerd` | `e287eb82a5be2be2be0a832798a725ce02adebf7bfab97e0f79a92b6152ed41e`; 업로드된16,810,608 B를 hash해 일치, ELF64 little-endian/EM_AARCH64(183) 확인 |
| params_binding | `/tmp/openpilot-ci.hHf287/planner-original/params/params_pyx.cpython-312-aarch64-linux-gnu.so` | `31f9c777dfff8230f4462724e82b4347404485fe798fb60edbf12e53329264e5`; 원래 receipt 기록, .so payload는 ZIP에 없어 직접 재hash하지 않음 |
| cpp_binding | `/tmp/openpilot-ci.hHf287/planner-original/ipc/msgq/ipc_pyx.cpython-312-aarch64-linux-gnu.so` | `606c78f15402a5af42dfd4681b131dd0c9bd9a07c26ea1ab84ec0e2cb5d6f84a`; 원래 receipt 기록, .so payload는 ZIP에 없어 직접 재hash하지 않음 |
| solver_manifest | `/tmp/openpilot-ci.hHf287/planner-artifact/manifest.json` | `7f75765aaba92a6e22587941c82e7e5d5352facc5d34e187895b6410739d2c83`; 업로드된1129 B를 hash해 일치 확인 |

source/rust daemon argv가 위 실제 binding/binary/solver 경로를 쓰는 것도 대조했다.
source-native는 `/tmp/openpilot-ci.hHf287/planner-source`다.
ARM 네 run의401개 packet SHA를 HOST의 해당 run과 각각 비교한 결과 모두401/401이 일치했다.
네 run 및 host/ARM의 manifest·archive·Python/binding/provenance는 계속 별도로 보존한다.

위 표의 실제 값을 사용하면 ARM 외부 audit의 source_commit/source_sha256/origin4필드,
두 artifact ID/bytes/SHA, index/receipt의 실제 member/bytes/SHA와 네 identity를 채울 수 있다.
현재 cache를 bundle root로 선택하면 archive 파일명은 `12615287431.zip`, `12614614983.zip`이다.
**extracted 디렉터리는 아직 만들지 않았다.** 부모가 qualification 후 안전 추출한 실제 상대경로를
각 artifact의 extracted에 선언하고 외부 audit JSON/SHA를 고정해야 한다. 미생성 경로나 placeholder를
실제 관측처럼 기록하지 않았고, capture 원문에 artifact ID를 추가하지 않았다.

ignored scratch의 `planner-arm-aa79-private-audit.json`, `planner-arm-aa79-functional-audit.json`은
이번 원문 데이터 관측 기록이다. reader가 신뢰할 qualification Audit JSON을 발행한 것이 아니다.
전체 IPC/feature/CLI/WF 등록은 계속 부모 대기이며, 새 native client/CPP driver의 actual 성공도
이번 원문 감사로 대신하지 않는다. 로컬 프로젝트 CLI/원본/테스트/빌드 실행0, CI 조작0이다.
