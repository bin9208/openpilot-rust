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


## source47fb qualification 뒤 두 Python 경로 퇴역 준비 — 읽기 전용 계획

2026-10-26. 부모는 source47fb의 HOST run40246606213/job121329384086과
ARM run40246606219/job121329384161의 실제 API SUCCESS를 확인했다.
이 긍정 상태는 재사용한다. **새 C++ peer의 actual121/run×4, native160, source/SDK/cleanup과
artifact 최종 수용은 Bacon raw 감사 확정 대기**이며, 이번 담당은 그 실행을 중복 감사·재실행하지 않는다.
아래는 그 감사 뒤 적용할 최소 후속안이다. 현재 코드/WF/삭제/GHA 변경은0이다.

### 실제 소비자와 남길 역사 참조

저장소에서 `rg -l --hidden`으로 plannerd_ipc/test_plannerd_native_scheduler/ModelScheduler의
직접 경로·native scheduler env를 추적했다. .git/.analysis는 제외했고, generic unittest/pytest
검색도 별도로 했다. main rust.yml은 rg-l로 관련 참조 존재만 확인하고 전체 본문을 출력하지 않았다.

| 소비자 | 현재 역할 | 퇴역 전에 닫을 경계 |
| --- | --- | --- |
| planner_ci_controller/plan.rs | runtime 12leaf 중 ipc가 Python plannerd_ipc.py 실행, native scheduler env와160 요구 | 순서10/이름ipc/12개/첫 실패 전파를 유지하며 기존 native IPC 실행 경로로 교체 |
| rust/tools/plannerd_lifecycle_peer.py | plannerd_ipc의 OUTPUTS/peers_ready/policy 직접 import | 이 세 정의를 기존 lifecycle peer로 원문 그대로 이동. policy용 log import도 함께 유지. 새 Python module/worker는 만들지 않음 |
| rust/tools/check_plannerd_lifecycle.py | 위 peer의 Case/run을 import하는 실제 lifecycle leaf | 기존 Python lifecycle oracle·원본 daemon·기한·출력은 남김. IPC 파일 삭제로 import가 깨지지 않아야 함 |
| rust/tools/tests/test_plannerd_native_scheduler.py | 원래 adapter13의 AST/실제 native160 + collector1, 현재 총14 method | 원래13 의미를 native15와 실제 IPC로 폐쇄한 뒤 Python test 실행 연결과 파일을 함께 제거 |
| tests/planner_scheduler_client.rs | 원래13 의미 + parse35 관련2개 = native15. 실제160은 독립 finite76 관측에서40×4 선택 | include_str의 삭제 예정 .py 직접 의존부터 동결한 원문 text fixture로 이동 |
| tests/planner_ci_controller.rs | 12leaf 순서/argv/scheduler env160과 첫 실패 전달 검사 | 변경된 ipc 한 슬롯의 정확 argv/output/evidence 계약만 갱신. 나머지11개/메모리3/실패코드 유지 |
| tests/planner_controller_frozen.rs | source73의 원래5개 관측과 runtime-plan 전체 argv 비교 | 원래 JSON/SOURCE_PINS는 바꾸지 않음. ipc 한 슬롯의 명시된 전환 계약만 별도 정확 검사하고 나머지11개 원문 비교 유지 |
| native-units.yml | old adapter14의 unittest 실제 실행과 PLANNER_NATIVE_SCHEDULER_TEST_BIN | native15/parse35/real160 유지 후 old unittest 호출과 이 실행 전용 env 제거 |
| planner-caller.yml/native-arm-more.yml/main rust.yml | 실제 runtime helper, source/binding 준비와 IPC qualification/stager/native 실행 | 아래 선행 stager 순서와 단일 실행 IPC leaf로 부모가 동기화 |
| python-rust-523.yml/ci_rust_isolation.rs | source roster·hash pin·workflow/capture 정적 계약 | 정식 caller 전환·부재 계약을 정확히 반영. 원문 gate 삭제/완화로 통과시키지 않음 |
| corpus.rs/observed_tests.rs와 외부 audit의 source9 | 역사 aa79 producer의 plannerd_ipc.py SHA 참조 | 삭제 뒤에도 그대로 보존. verify_schema는 현재 schema4만 읽으므로 이 역사 path를 현 파일 존재 요구로 바꾸지 않음 |
| memory test private fixture·manager 이전 before/after SHA·원본 PROVENANCE/문서/원장 | 실행하지 않는 과거 경로와 negative fixture | 실제 import/exec와 구분. 역사 bytes를 삭제하거나 최신 source로 덮어쓰지 않음 |

다른 workflow의 rust/tools/tests 검색 결과는 UI/Xiaoge/Params/Carrot/radard의 명시 test pattern이다.
이번 adapter를 실제 실행하는 명시 호출은 native-units의 해당 unittest pattern이다.
문자열 rg 전체0을 퇴역 조건으로 삼지 않는다. 역사 source9/PROVENANCE/부정 fixture는 남아야 한다.

### aa79에 이미 있는 독립 입력과 원래 출력

기존 raw 입력은 HOST private12615278417/ARM private12615287431이다.
각각 mode×epoch별 manifest4, initial Params184/run, 실제 CarParams pending bytes4,
raw event401/run×4와 root index를 그대로 사용한다. 어느 mode/epoch의 header·snapshot도
다른 case에 재사용하지 않고 native로 message/Params를 생성하지 않는다.

cached functional12615802090(HOST)과12614614983(ARM)의 central directory와 case receipt를 읽었다.
두 archive 모두 원래 Python controller가 관측한 source-0/source-1/rust-0/rust-1에 대해
**case receipt4개와 raw publication121×4개가 실제로 존재**한다.
각 raw path는 원래 case receipt의 절대 path를 선행 slash만 제거한 ZIP member로 정확히 연결된다.
HOST의 case receipt 합계1809386B + raw484개270048B =2079434B,
ARM은 receipt1809514B + raw270048B =2079562B다. 전체 ZIP 재다운로드·전체 CRC 재검사는 하지 않았다.
source 두 case는 원래 Python daemon 출력이고 rust 두 case는 당시 Rust daemon 출력이다.
네 case를 모두 Python daemon golden이라고 부르지 않는다.

현재 stager는 functional ZIP에서 최상위 summary receipt 하나만 추출한다.
현재 reader도 그 summary와 네 identities/coverage를 인증하지만 **출력121×4 raw를 읽는 경로는 아직 없다**.
따라서 새 native 네 run끼리 동일하다는 사실만으로 과거 원문 출력과의 독립 동등성을 대신하지 않는다.

후속 최소 자료는 기존 input audit v1을 덮어쓰지 않는 별도 고정 outputs-audit JSON이다.
HOST/ARM별 origin source/run/artifact ID·ZIP bytes/SHA, 네 mode/epoch의 기존 case receipt
{member,bytes,sha256}와121 raw의 {member,bytes,sha256}만 담는다.
metadata는 기존 ZIP 원문에서만 채취하고 예상 data/value는 기존 Python case receipt 그대로 읽는다.
후보 출력·decoder·scheduler 계산으로 expected를 생성하지 않는다. raw corpus/ZIP의 Git 삽입도0이다.

기존 stager를 명시 allowlist 추출로 확장해 summary + case receipt4 + raw484만 안전 추출한다.
기존128MiB staging admission 안에서 추가량은 약2.1MB/host이며, 실제 filesystem 증가량을 다시
확인한다. 부모 승인 없이 cap/기한을 늘리지 않는다. 각 receipt1MiB, raw1MiB, case121/전체484,
합계8MiB 안의 별도 output proof와 exact roster/path/SHA/NOFOLLOW·전후 identity를 요구하는 안이다.

검증은 두 단계를 분리한다. 먼저121×4 원래 raw 각각을 native decoder로 읽고 원래 Python
receipt의 service/valid/data 및 timestamp와 대조해 full-schema projection 자체를 검증한다.
그 뒤 새 실제 run을 같은 host/mode/epoch의 원래 기록과 현재 decode::differences로 비교한다.
현재 comparator의 service/valid/data와 float bits, 이미 제외한 timing4필드만 그대로 사용한다.
새 field mask·tolerance·native expected·다른 case 대체는 허용하지 않는다.
raw header의 실제 bytes/SHA는 역사 자료 인증에 쓰며, 새 wall-clock publication header의 SHA가
과거 packet 전체 SHA와 같다고 요구하거나 같다고 주장하지 않는다. 불일치는 삭제 보류 근거다.

### 보존14일 →90일: 이미 업로드한 cohort와 다음 업로드를 구분

2026-10-26 run artifact API를 읽은 실제 만료 UTC는 다음과 같다.

| aa79 artifact | 현재 만료 | 조치 필요 |
| --- | --- | --- |
| HOST private12615278417 | 2026-11-08T21:54:46Z | private rawcapture의90일 보존 경로 필요 |
| ARM private12615287431 | 2026-11-08T21:57:37Z | private rawcapture의90일 보존 경로 필요 |
| HOST functional12615802090 | 2027-01-23T21:49:10Z | 이미90일. 후속 workflow에서는 의존하지 않도록90일을 명시 |
| ARM functional12614614983 | 2027-01-23T21:49:10Z | 이미90일. 후속 workflow에서는 의존하지 않도록90일을 명시 |

현재 private proof upload는 retention-days14이며 functional upload는 repository default다.
원본 rawcapture를 새 original 실행으로 다시 만들 필요는 없다.
[GitHub 공식 보존 정책](https://docs.github.com/en/repositories/managing-your-repositorys-settings-and-features/enabling-features-for-your-repository/managing-github-actions-settings-for-a-repository)은
설정 변경이 기존 artifact에 소급되지 않는다고 명시한다. 다음 upload를90일로 바꾸는 것만으로
위 기존 private 두 개의11월8일 만료가 연장되지는 않는다.

기존 helper ipc는 proof/ipc-historical에 audit.json/capture.zip/receipt.zip 원문 사본을 이미 보존한다.
부모가 허용하는 다음 자료 보존 upload에서 이 exact ZIP bytes를90일 carrier로 보존할 수 있다.
기존 qualification 성공을 되풀이하거나 원본/모델을 재실행할 이유는 없다.
새 carrier는 새로운 artifact ID/outer ZIP SHA를 갖는다. aa79 origin run/source/ID와 inner ZIP
SHA를 그대로 남기고 carrier provenance를 별도 고정해야 한다. origin을 새 source로 가장하지 않는다.
현재 stager는 원래 artifact API ID/source/size/SHA를 직접 요구하므로, carrier를 소비하려면
부모가 검토한 별도 transport 인증이 먼저 필요하다. 만료된 원래 API를 무조건 건너뛰는 fallback은 금지다.
새 transport가 없다면 원본을 못 읽을 때 현재처럼 fail-closed한다.

### 단일 actual IPC leaf로 바꾸는 최소 caller 제안

현재 순서는 runtime(안에 old Python IPC) → HOST memory → ipc-check → stager → native ipc다.
퇴역 뒤에는 stager가 runtime의 새 IPC leaf보다 먼저 와야 한다. stager는 controller ELF 없이
owned job/proof/source와 token만으로 동작하므로 기존 source/SDK/solver/Params/msgq 준비 뒤에
먼저 실행할 수 있다. token은 stager step에서만 쓰고 이후 compiler/daemon에 전달하지 않는다.

공유 QA framework를 새로 만드는 대신, 첫 최소안은 runtime plan의 ipc 슬롯이 기존 checked-in
Bash helper의 `ipc` mode를 정확 한 번 호출하도록 하는 것이다. 실제357줄 orchestration과
transport/scheduler는 그 안의 기존 `check-plannerd-ci ipc`가 계속 소유한다.
호출 graph는 runtime helper → native12 controller → helper ipc → native ipc이며 재귀하지 않는다.
기존 helper의 start_owned/setsid·kill-group/directwait/live-scan/held 후검증을 그대로 재사용한다.
모든 자손 reap 완료를 새로 주장하지 않는다.

이를 위해 helper의 ipc mode에 plan용 `--output` 전달만 엄격하게 추가하는 안을 부모가 검토한다.
Check.invocation이 붙이는 output은 정확 기존 planner-comparison/ipc이고, inherited
PLANNER_NATIVE_SCHEDULER_EVIDENCE는 기존 ipc-scheduler.tsv여야 한다. 현재 hardcoded
ipc-native output/별도 evidence는 이 plan 경로에서 두 번째 자료로 다시 만들지 않는다.
실제 client/server가 쓴160행을 그대로 바깥 run_checks의 scheduler_calls=Some(160)가 검증한다.
wrapper는 실제 owned child의 첫 nonzero를 cleanup 뒤에도 보존하고, 후검사 실패는 기존 실패를
성공으로 바꾸지 않아야 한다. 새 임의 output·caller·SDK·profile이나 Python fallback은 허용하지 않는다.

plan의 ipc 한 슬롯은 Python prefix 대신 위 Bash/native 경로를 만들고 나머지11개 argv/order는
그대로 둔다. 기본 runtime build에는 검증된 planner-ipc-native profile이 필수이며 feature/profile
부재는 명시 거부한다. 메모리3/ASan receipt·다른 Python reference path는 변경하지 않는다.
새 Options의 이미 qualified IPC600/1200초·peer build180초·512/128MiB 및 각 wire2초/close3초는
확대하지 않고, 바깥12leaf의 기존 timeout/operation admission도 보존해 각 범위를 별도 기록한다.
이것은 제안이며 이번에 helper/plan/CLI/WF 소스는 수정하지 않았다.

후속 순서는 stager → runtime12(그 안 native IPC1회) → HOST ASan3 → ipc-check4/Clippy →
source/SDK/원본/caller 후검증·업로드/cleanup이다. 현재 runtime 뒤 별도 native ipc step은 제거해
같은4×40을 중복 실행하지 않는다. 최초qualification인 source47fb는 그대로 증거로 재사용한다.

### 컴파일 의존과 original13의 semantic closure

현재 native15 test의 include_str!(../../../tools/plannerd_ipc.py)는 삭제 즉시 컴파일을 깨뜨린다.
원래 next_model_target 함수의233B를 원문 그대로 작은 비실행 source text fixture로 먼저 동결한다.
현재/원래 함수 SHA256은
`a04e889c725715395baae751a5b3526c0c686b0075d67ab95b95fb94bd987be1`다.
새 text를 include_str로 읽고 같은 SHA를 검사하며, 함수 내용/공백/2칸 return/trailing newline을
고치거나 Rust 수학에서 원문을 다시 생성하지 않는다. origin module/source/span을 PROVENANCE에 남긴다.
이 source text는 packet/Params rawcapture가 아니며 실행·AST eval하지 않는다.

기존13의 oracle bytes/실제 run loop1session1request/absent signedzero·NaN/persistent40/empty/spawn/
EOF·42·malformed·nonfinite·long·multiple/2초 timeout/latch/close42/normal TERM-KILL-wait/
예외 원오류 유지/실제4session160 의미를 native15에서 각각 보존한다.
finite fixture76개 중 경계4+기존36을 각40회×4로 쓰는 현재 독립160과 parser35의 frozen bytes/SHA는
바꾸지 않는다. parser35는 float+finite AST 단계만의 관측이며 full Python client 원본 전체
재현 증거로 과장하지 않는다. 실제 wire·native lifecycle 폐쇄는 Bacon의 새 C++ qualification과 함께 본다.
collector 추가1개는 삭제될 생산 수집기 회귀다. 원본자료는 이미 독립 수집됐으므로 새 native
message generator/expected writer를 만들어 collector를 흉내 내지 않고 reader4의 인증 경계로 남긴다.

source73 runtime-plan frozen JSON은 그대로 둔다. 나머지11개는 기존 완전 argv 비교를 유지하고,
ipc 한 곳만 old literal을 명시한 정확 native wrapper/--output/evidence 계약으로 전환한다.
새 native argv를 자동으로 expected로 저장하거나 unknown 경로 wildcard로 원문 비교를 완화하지 않는다.
역사 source9·scheduler-finite/IEEE·parser35/원본 run receipt는 최신 source hash로 덮어쓰지 않는다.

### 삭제와 최종 수량 인정 게이트

Bacon이 source47fb의 HOST/ARM actual121×4/native160/정리·봉인을 확정하기 전에는 삭제하지 않는다.
그 뒤에도 독립 old-output121×4 reader와 lifecycle import 이동, source text include_str, plan/helper/
WF actual caller 전환이 모두 닫힌 한 패치에서만 정확 두 Python 파일 삭제를 준비한다.
퇴역 후에는 native ipc 진입 전과 성공/실패 반환 전에 그 두 경로의 file/symlink/재등장을 거부하는
private fixture controls와 실제 caller 부재 검증을 추가하는 안이다. 무관한 원본/daemon은 금지하지 않는다.
부모의 영향 범위 Actions에서 변경된 실제 caller/부재·원래 출력 비교를 확인한 뒤에만 원장 credit을 준다.
이미 성공한 불변 검사나 original capture를 처음부터 다시 실행하지 않는다.

이번 작업은 rg·소스·기존 ZIP member/receipt·보존 metadata 조회와 이 문서 append뿐이다.
rawcapture Git 삽입/새 expected 생성/코드·WF·삭제·원장·이슈 변경/로컬 기능·Cargo·원본·모델 실행/
GHA dispatch·재실행·취소/stage/commit/push는 전부0이다.

## source47fb 첫 native IPC qualification 실제 감사 — 2026-10-26

감사 source는 `47fb84019a078f3d1c1de9c432cfe475f5b92be0`다.
[HOST40246606213/job121329384086](https://github.com/bin9208/openpilot-rust/actions/runs/40246606213/job/121329384086)은
`2026-10-25T23:58:20Z`,
[ARMmore40246606219/Planner job121329384161](https://github.com/bin9208/openpilot-rust/actions/runs/40246606219/job/121329384161)은
`2026-10-26T00:00:41Z`에 completed/SUCCESS다. 둘 다 실제 GitHub-hosted 해당 native architecture다.
무관한 WebRTC/currentC/PR1344/genuine/복구 작업은 이 감사 범위에 넣지 않았다.

running 중에는 단계 metadata만 관측했다. 완료된 두 job의 raw log를 각각 한 번씩 받고,
신규 private ZIP2개만 API expected bytes/SHA를 확인한 뒤 각각64MiB 상한 안에서 받았다.
functional ZIP은 전체를 다시 받지 않고 필요한 result/command/native receipt/provenance member만
HTTP Range로 읽어 CRC를 확인했다. ARM의 Range 전송은5581010B, 각 functional의 cumulative
상한은12MiB였다. functional 전체 ZIP의 SHA를 이번에 재계산했다고 주장하지 않는다.
aa79 원문4개 ZIP은 기존 cache를 사용했고 재다운로드·원본 실행·새 expected 생성은0이다.

| 자료 | bytes / SHA256 |
| --- | --- |
| HOST completed raw log | 278372 / `2259f2bdb4dd1c57fbf990098f7402a223b17bccb24de6321d079286d6d43031` |
| ARM completed raw log | 255050 / `6c81f5b64e9fe478dd7729d0c22084a8c488508be8fd4ac7018ac82f8f528f09` |
| HOST private12619255659 | 48852009 / `f5e10e1a5c82c73f962e194ae8a168ce408a9559ad27665dbbb94d33e24f2ef5` |
| ARM private12619265971 | 39584872 / `95368bacd1223526c346c968c901702d8c6db14283c5679a907780988d20b670` |
| HOST functional12619400356 API metadata | 57096502 / `2e221a7449eaa58194729c25b8a23c29b00f4b85f8138c8479032d418986838e` |
| ARM functional12619470399 API metadata | 46752539 / `5e29a944bbbaa868b671550fd32cd39a2980686b28a23014c96ad0bdaf6f0bfc` |

각 artifact는 해당 run/head47fb/repo1395501963의 expired=false다. private 두 ZIP은 전체 SHA와
전체 CRC/safe unique relative path/no-symlink·special-file 거부도 확인했다.
HOST는2442멤버/66196384B expanded, ARM은2431멤버/55074620B expanded다.

### 실제 feature/unit4/Clippy/stager

두 job은 `PLANNER_NATIVE_IPC_PROFILE=planner-ipc-native-v1` 및 job-level actions:read를 사용했다.
controller feature는 `ci-receipt-metadata-native,planner-ipc-native`이며 lib module/CLI를 실제 빌드했다.
raw log와 private의 ipc-observed.status=0/ipc-clippy.status=0에서 다음 네 unit의 실제 PASS와
4 passed/0 failed/0 ignored를 확인했다. 나머지83 filtered를 수행한 검사로 더하지 않는다.

- source425_original_cpp_packets_match_original_python_policy_fields
- missing_independent_model_input_corpus_is_rejected_without_generating_inputs
- writer_schema_selects_each_mode_epoch_without_reusing_or_generating_packets
- capture_auth_rejects_unpinned_swapped_incomplete_and_reappeared_data

실제 `cargo clippy --lib --bin check-plannerd-ci -- -D warnings`도 같은 feature/SDK/target에서 exit0다.
추가3단계 qualification→stage→native four-run이 양쪽 모두 성공했다. compile/feature 등록 실패나
stager 실패는 이번 실제 run에서 발견하지 못했으며 수정 제안이 필요한 실패는 없다.

stager6ffd의 actual status는 source47fb, 역사 sourceaa79, 각 원래 run40237263524/40237263421,
소유 root `/tmp/openpilot-ci.ZLUl0h/planner-ipc-original` 및
`/tmp/openpilot-ci.yjCm2x/planner-ipc-original`를 기록한다. root/proof는 uid1001/mode700이며
capture2349멤버·receipt1멤버를 준비했다. 외부 audit SHA는 HOST384bcef1 전체 SHA와
ARM7dd3abad 전체 SHA로 정확히 일치한다. private의 ipc-historical/audit.json과
capture.zip/receipt.zip을 각각 재해시해 같은 audit의 고정 값과 일치함을 확인했다.
stage admission은 각134217728B, 실제 free before는HOST86084661248B/ARM110024531968B이며
memory_admission=false/original_execution=false다. 이는 peak 측정값이 아니다.

### 순차 네 run, 160 scheduler 및 원래 baseline

순서는 **source-0→source-1→rust-0→rust-1**다. native source의 중첩 loop는 각 run_case 종료를
기다린 후 다음 run을 호출하며, 실제 provenance의 네 manifest 순서와160행 timestamp 증거가 맞는다.
각 case 안의 daemon/peer/scheduler는 함께 동작하지만 네 case를 병렬 실행한 결과가 아니다.

| arch | 네40행 scheduler 구간의 nominal timestamp 범위 |
| --- | --- |
| HOST | 393.653397335..395.603397335 →396.213688514..398.163688514 →398.846121497..400.796121497 →401.475982915..403.425982915 |
| ARM | 555.091570952..557.041570952 →557.760180412..559.710180412 →560.852837935..562.802837935 →563.948741912..565.898741912 |

네 구간은 서로 겹치지 않는다. ipc-native-scheduler.ndjson이라는 이름과 달리 실제 형식은
nominal/previous_sent/previous_model_done/target의 finite f64 **4열 TSV**이며 각 arch160행이다.
새 oracle를 로컬 실행하지 않았고, 실제 GHA의 verify_evidence(160) 통과와 저장된160행을 대조했다.

양쪽 각각 네 run 모두 output121개(longitudinalPlan41/lateralPlan40/driverAssistance40),
timing40행, daemon returncode=-15다. valid counts는40/39/39,
trigger modelV2=6/liveTracks=35, fast_overlays=34다. controller exit0와 daemon의 수집 종료
SIGTERM -15를 구별한다. current native receipt의 비교3개는 모두 differences=[]다.

이번 독립 inert 감사에서는 각 native run의 service/valid/data projection도 읽었다.
timestamp/path/timing은 원래 comparable 범위와 같이 제외하고 float를 f64 bit로 비교했다.
각 arch의 네121행 모두 기존 cached aa79 functional의 **original source-0 baseline**과 일치했다.
기존 aa79 original summary receipt의 비교3개도 differences=[]임을 확인했다.
native 출력으로 새 expected를 만들거나 비교 기준/허용오차를 바꾼 결과가 아니다.

기존12개 leaf도 HOST/ARM 각각 commands12행 returncode0/result PASS다.
HOST의 ASan3 abi/owner/fault도 exit0/PASS이며 generated solver C·allocator leak/ownership 범위다.
Rust·pinned external solver library 전체 계측으로 확대하지 않는다. ARM ASan은 기존 조건대로
skipped이며 ARM ASan3 성공으로 세지 않는다.

### same ELF, source/SDK 봉인과 cleanup의 실제 한계

각 arch의 기존 Python IPC receipt와 새 native IPC receipt의 current identities4개가 정확히 같다.
즉 동일 current daemon/Params binding/original cpp binding/solver manifest로 비교했다.
원래 aa79 identity를 현재 값으로 덮어쓰지 않았다.

| 실행물 | HOST SHA256 | ARM SHA256 |
| --- | --- | --- |
| 같은 controller ELF | `00248859670d30aa508798e486eac9b24b899526080971dfab06ee3230a3d138` | `1770560d814b35467cf87b69edcd7ce07ae7ca7c684b482bf5e23a299342e221` |
| 같은 current daemon ELF | `8629ac5db45aa4925791b31f2b47e4621b326fbb68027a038330e8388ce6e149` | `faf4d84e0342bbddff5c69be5f885817fc834c2bd1416fbca59266cba880c6ad` |
| 새 C++ peer ELF | `c51b46fb4fa2765ca54979342b0b15fa4e6899653b80bd461dce8f801ae3b7bd` | `191b658d3dc61ab818ade24106de63132de8e23d13701cba2761c9c3b8bf3bf8` |

같은 ELF라는 표현은 **해당 arch/run 내부**의 재사용/전후 동일성을 뜻한다. 두 arch ELF가
같다는 의미가 아니다. held-before/after는 Bash map 순서만 다르고12행의 정렬된 전체 set이 같다.
GHA는 새 C++ source/header/peer SHA와 현재 provider identities를 성공/실패 반환 전에 확인했다.

source-before/after normalized tar SHA는 양쪽 모두
`17f9c0037d42375a3096311a4d5c012fd158549863905c670a07630b7195112a`다.
index-before/after는 양쪽1178610B/SHA
`c9b855bd9c7f26ee55e484bccb5796f8542c487181f9cf42be34118a8d6f0880`로 bytes가 같다.
각 해당 Rust1.94 toolchain root의 SDK hash/roster도 HOST runtime/memory/ipc-check/ipc,
ARM runtime/ipc-check/ipc의 before/after와 일치했다. aa79 HOST에 없던 봉인을 끌어온 것이 아니라
이번 profile이 실제 업로드한 manifest를 확인한 것이다. global SDK/provider 전체 수용으로 확대하지 않는다.

finalizer는 양쪽 original_exit=0/cleanup_exit=0/cleanup_wait=0,
owned_groups_nonlive=true다. **all_adopted_children_reaped=false**도 그대로 기록돼 있으므로
모든 adopted child의 전역 회수를 입증했다고 주장하지 않는다. job의 source/index 후봉인,
private proof upload, source/alias/target/common owned temp cleanup step도 모두 SUCCESS다.

여기서 수용한 것은 source47fb HOST/ARM의 첫 native IPC qualification과 기록한 경계다.
두 Python 파일 삭제·모든 실제 caller 부재·전체 Python 퇴역·원장 credit은 아직0이다.
이번 감사의 로컬 동작은 source/원시 log/ZIP/JSON/TSV의 inert 읽기·hash/CRC뿐이고,
기능/Cargo/원본·모델 실행, GHA dispatch·취소·재실행, source 편집·stage/commit/push는0이다.


## 2026-10-26 source6c actual 감사: runtime 전 policy pin 차단과 별도 native units 수용

대상 source는 `6c5d5a9be691d18ca850af6ef9b42e47b668d767`로 고정했다.
API head/source, private `SOURCE_COMMIT`, 원시 완료 job log와 이 commit의 Git blob을
결합했으며, 이후 작업 중인 파일을 이 결과의 소스로 사용하지 않았다.
현재 section은 actual 기록 추가뿐이며 production/test/WF/pin/expected 변경은 하지 않았다.

### 최초 실패와 실제 도달 범위

[HOST run40255341871/job121355876469](https://github.com/bin9208/openpilot-rust/actions/runs/40255341871/job/121355876469)은
step17 `Compile and check the complete native planner`에서 FAILURE/exit101이다.
[ARMmore run40255341883/Planner job121355876763](https://github.com/bin9208/openpilot-rust/actions/runs/40255341883/job/121355876763)도
step15의 동일 명령에서 FAILURE/exit101이다. 컴파일러 진단 실패가 아니라
`rust/crates/dev-tools/tests/planner_ci_policy/contract.rs:137`의 공통 선행 SHA assertion이다.

- 해당 source helper의 실제 whole SHA256은 `cf510c5bb65b5fdd5bcad2356f988e026d64f8ad7d52c6117e49efcf082f1612`다.
- contract line139의 기대값은 이전 `df5eab6cbfecac8e0d5de52140fac9c8d3b006d1f5c2072db19bde77d238022c`다.
- 양쪽 로그 observed/expected가 각각 이 값과 일치한다. policy21 중 20개가 이 공통 assertion에서
  실패했고 Miri 로그 보호 1개만 PASS했다. 서로 다른 runtime 결함 20개로 해석하지 않는다.
- 부모가 검토된 helper와 이 literal을 동기화하는 최소 수정의 소유자다. 이번 감사에서 수정하지 않았다.

양쪽 모두 선택된 dev-tools test 실행물의 Rust test build가 `Finished test profile`에 도달했고,
`native_build_closure` 7 PASS와 `planner_ci_controller` 14 PASS가 실제 실행됐다.
그 다음 policy 실패 때문에 같은 cargo invocation의 frozen5/retirement6는 실행되지 않았다.
뒤에 배치된 `openpilot-plannerd --all-targets` test, bins/examples build 및 plannerd Clippy도
도달하지 않았다. step 이름만으로 complete Planner production build 성공을 주장하지 않는다.

| 경계 | HOST6c | ARM6c |
| --- | --- | --- |
| pinned solver/original binding 준비 | 실제 SUCCESS | 실제 SUCCESS |
| 선택된 dev-tools test build / native-build7 / controller14 | 실제 완료 / PASS / PASS | 실제 완료 / PASS / PASS |
| planner policy21 | 1 PASS / 20 FAIL | 1 PASS / 20 FAIL |
| 고정 IPC stager | step18 skipped | step16 skipped |
| Runtime12의 direct native IPC 포함 비교 | step19 skipped | step17 skipped |
| generated solver ASan3 | step20 skipped | 미실행; ARM 비대상 조건도 있음 |
| native IPC4 및 해당 feature의 strict Clippy | step21 skipped | step19 skipped |
| source/index 후봉인·private upload·source/alias/target/common-temp cleanup | 실제 SUCCESS | 실제 SUCCESS |

따라서 이 source에서 runtime IPC를 1회 실행했다고 수용하지 않는다. actual old-output484
조회/decoder 비교, current 121×4 publications, runtime scheduler160, current IPC ELF/peer/compiler
identity 전후 및 runtime helper의 whole SDK before/after는 모두 **미도달**이다.
functional ZIP 두 개에도 `planner-comparison/`, runtime IPC 결과 및 native-memory 결과 member가 없다.
Hypatia 소유 sibling capture/lifecycle도 이 두 Planner job에서는 실행되지 않았다.

### HOST/ARM 원문 ZIP 4개의 API·whole SHA·전체 CRC

이번 감사자가 네 archive의 유일 downloader다. API의 repo_id/head_repository_id
`1395501963`, origin run, head SHA, `expired=false`, exact bytes/digest를 확인한 뒤
각 `/actions/artifacts/<ID>/zip`을 한 번씩만 받았다. payload를 실행하거나 추출하지 않았다.
작업별 64MiB 가용 디스크 admission을 확인했고, exact-size 검사·kernel file-size 상한·120s
전송 기한을 사용했다. 네 payload 합계는 9,139,508B이며 기존47/0283 ZIP을 다시 받거나
원본을 재실행하지 않았다.

| 역할 | API artifact ID | archive bytes | whole SHA256 | members / expanded bytes |
| --- | --- | ---: | --- | ---: |
| HOST functional | 12622606189 | 4,693,521 | `2d70c37b1fe245db545bf8723ba5738c745680b001f8b69e8da5cf471e43764a` | 319 / 15,068,725 |
| HOST private | 12622491387 | 292,776 | `1bf3be114fbbce94c833480462efad6b8751a945cd6e4de8aa5878a75552ae71` | 12 / 2,362,946 |
| ARM functional | 12622680819 | 3,860,472 | `a7b5e73cb6cc77c3d0ea69ff36f684190152bb00b57bd8daed7a483b16dc3c9f` | 322 / 13,108,926 |
| ARM private | 12622651044 | 292,739 | `44814ff8cc7e53a4efc8e900512810c6324c5ab11322117064d99daccd73cf1c` | 12 / 2,362,789 |

네 ZIP 모두 API whole SHA와 전체 member CRC가 PASS했다. 중복 member/절대경로/`..`/
역슬래시/drive 표기 및 symlink·특수파일을 거부하고, directory와 regular member만 읽었다.
archive당 member≤10,000 / expanded≤256MiB / member≤64MiB의 inert audit 상한도 통과했다.
모든 member의 개별 SHA/CRC/bytes는 캐시의 `<ID>-members.json`에 있다.
이 네 artifact는 API에서 모두 `2027-01-24T01:40:11Z` 만료다.

공유 캐시 root는 `.analysis/scratch/2026-10-25-523-completion/ci/`다.
Hypatia는 `<ID>.zip`과 `<ID>-members.json`을 데이터로 읽고 중복 다운로드하지 않는다.
`planner6c-artifact-handoff.json`의 HOST/ARM 네 항목은 모두 `ready=true`,
`whole_crc_pass=true`다. 최초 두 ID와 예정 경로를 먼저 공유하고, ARM API ID 발행 뒤 두 ID도
다운로드 전에 공유했다. 별도 상대 agent 메시지 도구/식별자는 이 실행 환경에 노출되지 않아
이 채팅과 공유 handoff 파일을 인계 지점으로 사용했다.

HOST/ARM private proof는 다음 값으로 각각 전후 동일하다.

- normalized source tar SHA: `b62516ebb83aaf4dbb4bc020adb510e9b166fcb306c4d34bb5ff96a52e67b27c`.
- index: 1,179,244B / SHA `f095c5a1cb9602624d160da4997cbbce5a003f7dda2fc3c8fed01a4376e929e4`.
- HOST owner `40255341871:1:planner-runtime`, actual runner `GitHub Actions 1000029818` / github-hosted Linux X64.
- ARM owner `40255341883:1:planner-runtime`, actual runner `GitHub Actions 1000029828` / github-hosted Linux ARM64.

이 private proof 12 members에는 runtime helper SDK manifest가 없다. source/index 봉인을
whole runtime SDK 봉인으로 확대하지 않는다.

### native-units의 두 별도 역할

[run40255341784/planner job121355876323](https://github.com/bin9208/openpilot-rust/actions/runs/40255341784/job/121355876323)은
`2026-10-26T01:45:04Z` 완료 SUCCESS,
[planner-client job121355876655](https://github.com/bin9208/openpilot-rust/actions/runs/40255341784/job/121355876655)은
`2026-10-26T01:49:52Z` 완료 SUCCESS다. 둘 다 API actual runner `bin-actions`,
self-hosted Linux X64이며 source6c다. source6c WF와 proof의 `--no-default-features`,
build jobs1, incremental0, dev/test debug0가 일치한다.

| 실제 명령 역할 | 결과 |
| --- | --- |
| planner controller | 14 PASS / 0 ignored |
| controller retirement | 6 PASS / 0 ignored |
| controller frozen | 5 PASS / 0 ignored |
| native memory wrapper unit | 15 PASS / 0 ignored |
| 위 lib/bin/4 test target strict Clippy | rc0 |
| scheduler client | 15 PASS / 0 ignored |
| scheduler client lib/test strict Clippy | rc0 |

`native-exit-codes.txt`의 planner 역할 5개와 client 역할 2개가 모두0이다.
이 default-feature Clippy를 미실행인 `planner-ipc-native` native4/Clippy 대신 수용하지 않는다.
retirement6의 empty-root 실제 ABI42와 leaf 실패 뒤 file·live/dangling symlink 복원 탐지가
모두 PASS했다. 사전에 profile/context를 만들지 않아도 IPC 차례 전의 원래 42와 전후
absence guard가 보호됐다는 실행 증거다. frozen5는 source73 JSON 데이터 비교이며
Python 원본 재실행이 아니다.

controller14의 `failed_comparison_is_recorded_and_stops_before_the_next_check`는 Git6c blob에서
`ipc_timeout_and_controlled_cancel_keep_one_owned_group()`를 무조건 호출한다.
실제 NativeRunner timeout(2s) 및 ready 뒤 controlled cancellation 검사가 PASS했다.
이때 완전한 IPC17 옵션 형식의 process fixture는 leader와 `cpp-peer/daemon/scheduler/compiler`
네 label의 **sleep 자손** PID·PGID·startticks를 기록한다. 같은 PGID, 직접 leader wait,
자손의 부재/Z/X 또는 PID 재사용 구분과 later sentinel/result 미생성을 검사했다.
실제 production C++ peer/compiler를 돌린 검사가 아니며, 전체 adopted child 전역 reap이나
root process 자체의 강제 OS 종료까지 보장하지 않는다. 임시 PID receipt는 test 내부에서
검사된 후 TempDir과 함께 정리되며 ZIP에 raw PID 5행이 별도로 남아 있지는 않다.

client15의 actual driver frame-loop/session/40 requests, EOF/잘못된 응답/fail latch,
spawn 실패의 fallback 거부, 공유2s deadline, TERM→KILL→wait·exceptional close, frozen ASCII
parser/targets가 PASS했다. `real_native_binary_serves_four_sessions_160_independent_frozen_targets`
또한 PASS했으나 이것은 scheduler stdio4 sessions/160 targets다. 미실행인 실제 Planner
121×4 IPC publications와 runtime160 timing을 대신하는 증거가 아니다.

두 역할의 resource/source/SDK before/after와 cleanup step도 모두 SUCCESS다.
proof는 official Rust1.94 root
`/home/actions/.rustup/toolchains/1.94.0-x86_64-unknown-linux-gnu`의 294 file hashes와
322 NUL roster records를 각각 before/after bytes 동일하게 보존한다.
Rust는 `1.94.0 (4a4ef493e 2026-03-02)` / LLVM21.1.8이며 SDK installation0이다.
해당 root 외 SDK/DSO 전체 봉인으로 확대하지 않는다.

- SDK file manifest 56,647B / SHA `34ca36a4a7d3ec78878ae59c690755c9f3e513c816a05e5f2a71ab06f1b1191a`.
- SDK program manifest 574B / SHA `02dc3249995ea4f61a85de89ebaff8c4f2e642f1a8567d76ffc3078472ac536a`.
- SDK roster 63,514B / SHA `1216a3a311069910f96a8f3d4cc7167b9514cd9ce16e5035da6d9d3169bb3942`.
- 두 역할 source/index 값은 위 HOST/ARM 값과 같고, staged-index는 2,160,394B /
  SHA `0cd1d7005e8a7d52a0fd7d2ad8e597c9618a891d6dbaa329006946d4fd12195a`로 전후 같다.

완료 job의 별도 작은 native proof 두 ZIP도 동일 cache에 한 번씩 받았다.
planner artifact `12622471585`: 1,592,005B /
SHA `a67e30c460382601f83968c8b4a507561eb2317dcf2bf79411b0a8ab47d89715`,
42 members / 6,963,823B expanded.
client artifact `12622850683`: 1,587,673B /
SHA `c30c3f5e70a16ebe2aff57a61107246270748f5bbbcbc36ad7081b172facca6d`,
36 members / 6,955,206B expanded. 양쪽 whole SHA/전체 CRC PASS다.
API 만료는 각각 `2026-11-09T01:44:54Z`, `2026-11-09T01:49:41Z`다.

완료 raw log 캐시와 whole SHA256은 다음과 같다.

- HOST `planner-host6c-121355876469.log`: `23518855b09586f18710dfe7206f1a4549558f222bc4a82aec84dc70f7dc53d8`.
- ARM `planner-arm6c-121355876763.log`: `e55148ab4641a7b61644e94b118ccf89da6107ffd2138d7e683f2297ba59e18c`.
- planner `native-planner6c-121355876323.log`: `b09888898e6a8b5c07ce0cd8fb95dc77b9b001acc05b86e515c4e915da8bba34`.
- client `native-planner-client6c-121355876655.log`: `d26560e8a42a31d1efa831e7f40e5a44c269a55025cbd6cef418d8f2a8842788`.

이번 수용은 실제 실행된 default-feature unit/Clippy와 그 소유·봉인 범위다.
실제 Runtime12, original/current484 출력 비교, runtime160, native IPC4/Clippy 및 ASan3는
이 source에서 계속 미수용이다. expected/tolerance/guard를 바꾸거나 CI를 재실행·취소하지 않았다.
로컬 작업은 API·완료 log·Git6c blob·ZIP/JSON의 inert 검증과 이 기록 append뿐이며,
Cargo/기능/모델/원본 실행, 공유 source7파일 편집, stage/commit/push 및 artifact 업로드는0이다.
