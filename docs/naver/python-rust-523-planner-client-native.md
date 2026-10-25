# #523 Planner scheduler client의 첫 native 수용 경계

2026-10-26. 상태: 구현·source/문법·서식 검토, 실제 native15/Clippy UNRUN.
전체 IPC 후보를 등록하기 전에 client/frame loop 두 파일과 integration test, 원본 parse35
fixture만 분리했다. Python adapter/oracle/runtime fixture의 삭제와 baseline credit은0이다.

기존 native scheduler 정책과76개 원본 timestamp/expected bits를 그대로 소비한다.
실제 `check-plannerd-ci schedule` 네 process session×40=160요청의 target bits와 evidence를
확인하며 request/input/expected를 현재 native 출력으로 재생성하지 않는다. 원본 Python
`next_model_target` 함수의 SHA도 텍스트로 고정한다. Python 코드는 실행하지 않는다.

15개 일반 검사에는 process 없는 direct policy, persistent child, empty/spawn/EOF/exit42,
malformed/nonfinite/256-byte/복수행, 실패 latch, 한 요청의 공통2초 write/read deadline,
close의 TERM/KILL/wait와 parent error 보존이 포함된다. 같은 후보 frame-loop core를 fake
events와 실제 client에 연결해 한 session40요청/model40/live40/대기80 순서도 검사한다.
전체 C++ peer/Cap'n Proto/daemon publisher는 이 작은 경계에 아직 연결하지 않는다.

원본 parse35는 sourceaa79의 실제 Actions artifact12615269988 원문9160B이며 SHA256
`afe5ea47497bf9ea1af07231400055f348c3c8e555020c1f747c97eb51168acd`다.
17개 finite bits와 nonfinite7/invalid9/nonASCII2 오류 종류를 비교한다. ASCII VT/FF,
underscore, signed zero·exponent를 명시적으로 해석하고 std f64 변환 뒤 finite를 요구한다.
17개 양성 원문을 한 shell session에서 실제 client로 받는 검사도 별도로 유지한다.
이 관측의 원본 범위는 parse AST 단계이며 전체 framing/lifecycle 관측으로 확대하지 않는다.
상세 원본 provenance는 fixture README에 있다.

기존 native-units WF에 `planner-client` choice를 더하고 다음 push의 작은 cohort로 선택했다.
일반15/0ignored와 strict Clippy를 정확 count로 요구한다. 실제 runner/bin-actions·jobs1/debug0,
작업별 memory/cgroup/성장 admission, source/index/SDK before/after·업로드/소유 정리를 유지한다.
새 feature/Cargo dependency/공유 module/CLI 등록과 전체 IPC actual HOST/ARM 수용은 후속이다.

## client904 실제 native15/Clippy 원문 감사

source `904d86b7b260cedc707522a7e31dfea31ce80702`,
[run40241800509/job121314983718](https://github.com/bin9208/openpilot-rust/actions/runs/40241800509/job/121314983718)
SUCCESS의 원본 artifact12617560729를 읽기 전용으로 감사했다.
GitHub metadata의 run/head SHA와 ZIP의 SOURCE_COMMIT이 위 값으로 일치했다.
실제 artifact 이름은 `native-units-planner-client-40241800509-1`이다.

- ZIP1,585,109 B, SHA256 `c15692386bc2f96eb29982234e7f6003dc707cb348aa02b97352748b489316d6`.
- 36 unique safe members, symlink/경로 이탈/암호화 없음, 전체 CRC 정상, expanded6,943,295 B.
- `planner-client-units=0`, `planner-client-clippy=0`; 일반15 passed/failed0/ignored0/filtered0,6.08s.
- 실제 runner는 bin-actions/self-hosted/Linux/X64이며 jobs1/incremental0/dev·test debug0,
  no-default-features다. SDK 새 설치는0, model/external numerics provider 실행도false로 기록됐다.

`planner-client-units.log`에서 다음 실제 검사의 `ok`를 확인했다.

- `real_native_binary_serves_four_sessions_160_independent_frozen_targets`
- `frozen_original_ascii_parser_stage_matches_native_bits_and_error_classes`
- `persistent_client_accepts_frozen_original_ascii_responses_in_one_session`
- 나머지12개 client/frame-loop/direct-policy/framing/latch/deadline/cleanup 검사 전부.

focused-source SHA와 source904의 Git 원문을 대조해 동결5파일이 동일함을 확인했다.
따라서 native160 검사는 실제 check-plannerd-ci binary의4session×40요청과 기존 frozen76 중
정해진40개 입력/expected bits 및 `verify_evidence(...,160)`을 실행한 성공이다.
검사 내부 tempfile의160행 TSV는 이 artifact에 별도 보존되지 않았다. 이번 감사는 성공한
unfiltered test와 그 고정 source/assertion을 확인한 것이며, 없는160행 원문을 합성하지 않았다.

원본 parser35 JSON9160 B/SHA `afe5ea47497bf9ea1af07231400055f348c3c8e555020c1f747c97eb51168acd`도
실제 focused-source 기록과 source904에서 동일했다. 17 finite bits/RuntimeError7/ValueError9/
UnicodeDecodeError2 비교 및 finite17개를 한 stdio session에서 받는 검사가 실제 통과했다.
원본 capture의 범위는 계속 ASCII float+finite AST 단계이며 전체 Python client35회 관측으로
확대하지 않는다. Python oracle source는 텍스트 hash만 읽으며 실행하지 않는다.

Clippy는 실제 source904 WF의 `--no-default-features -p openpilot-dev-tools --lib
--test planner_scheduler_client -- -D warnings` 범위가 exit0이었다. check-plannerd-ci bin은
native160을 위한 test build에서 실행됐으며, 이 Clippy 명령에 `--bin`은 따로 지정되지 않았다.
전체 IPC/C++ peer/Cap'n Proto reader·decoder의 lint/실행 성공으로 확대하지 않는다.

source/index/staged-index 전후 bytes가 각각 같고 source tar SHA도 전후
`2cb4f0915a2af4c0f8c4122e4e7a8f650091ba03675aff7f1340b941e8441dc8`으로 일치했다.
prepared SDK의 전체 file manifest294행·roster322 NUL entries·program manifest4행은 각각
전후 동일하고 rejected roster는 양쪽0 B다. outcomes의 source_before/source_final,
sdk_before/sdk_final 및 source-preflight exit0을 함께 확인했다. SDK file manifest의 SHA는
`34ca36a4a7d3ec78878ae59c690755c9f3e513c816a05e5f2a71ab06f1b1191a`다.

보존 원문은 `.analysis/scratch/2026-10-25-523-completion/ci/12617560729.zip`이다.
이 세션은 artifact/API metadata/source/ZIP bytes만 읽었으며 프로젝트/원본/테스트/빌드/CI를
새로 실행하지 않았다. 현재 실제 수용은 client15와 위 strict Clippy 경계다.
전체 IPC의 shared registration/actual HOST·ARM, Python adapter/fixture 삭제와 credit은 후속이다.
