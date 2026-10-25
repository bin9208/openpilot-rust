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
