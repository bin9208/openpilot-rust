# #523 Planner 네 Python 경로의 삭제 후 수용 기록

2026-10-26. 상태: **source7a82 실제 host/ARM·native guard 성공, unused import 정리 후 strict Clippy 수용 대기**.
전체 #523, 원본 Planner leaf와 IPC/Cython builder의 퇴역, 기기 수용 완료를 뜻하지 않는다.

## 정확한 source와 기능 결과

삭제 source는 `7a82b4e21283cf540f57c88689f58b2e8aca9f07`다.
[actual host40230515011/job121281012901](https://github.com/bin9208/openpilot-rust/actions/runs/40230515011/job/121281012901)은
runtime12 및 native ASan3가 모두 exit0/PASS다.
[actual ARM40230515057/job121281012853](https://github.com/bin9208/openpilot-rust/actions/runs/40230515057/job/121281012853)도
runtime12가 exit0/PASS다. 두 compile step의 실제 로그에서 native build closure7,
controller14/policy21/frozen5/runtime guard6의 성공도 확인했다. 원래 남은 Planner test/build/
Clippy, 네 성장량과 host/ARM의 SDK·source admission은 보존했다.

runtime 순서는 `abi/policy/radar/gap/owner/main/fault/volkswagen/stock-longitudinal/radar-off/ipc/lifecycle`,
ASan은 `abi/owner/fault`이며 누락 없이 각 실제 commands.json의0을 읽었다. ARM ASan은 기존
조건대로 생략하며 host generated C 계측만 수용한다. Rust/외부 solver 계측으로 확대하지 않는다.

[작은 native40230515032/job121281013120](https://github.com/bin9208/openpilot-rust/actions/runs/40230515032/job/121281013120)에서는
controller14/frozen5/guard6/adapter13(실제 native160 포함)/memory15가 모두 성공했다.
마지막 strict Clippy가 memory test의 unused `PathBuf` import 하나를 거부했다.
후속 `1000464f57239a1d15bfba3bdb6c01550f12d8a0`은 import만 제거하고 body·fixture·검사 수를 유지한다.
수정 후 strict Clippy·최종 원문 감사가 끝나기 전 source 삭제 credit은 보류한다.

## 독립 inert artifact 감사

아래 전부 API의 정확 run/head/크기/SHA와 일치한다. ZIP은 unique canonical-relative 경로,
상위/절대/역슬래시 경로·symlink 없음과 전체 CRC를 확인했다. archive 코드는 실행하지 않았다.

| 역할/artifact | bytes | SHA256 | members / expanded bytes |
| --- | ---: | --- | ---: |
| host private12613226099 | 296353 | `a4c1c77e0dc2c7afba2dff59151ba66fddb727e925395464827c5bbbe8feab31` | 29 /2355334 |
| host functional12612956349 | 55186379 | `ee3dd53a297650e503862c28b403fdb9cd94b8787f1aff0f5578baa94229915f` | 34282 /170369563 |
| ARM private12613176526 | 294477 | `b14d025ac5bb895189e30bae8edc39a854f251b664d8044605ce7c0b9f09cb65` | 22 /2354510 |
| ARM functional12613790411 | 44841906 | `0f4ed451b7a941bdc8675d1d0dbcb2b70fc98db5308727bf64851632682489ce` | 29679 /141276071 |

두 private의 `SOURCE_COMMIT`, index-before/after 및 source-before/after 쌍이 동일하다.
index에 아래 네 삭제 경로가 모두 없으며 실제 실행에서 current native guard가 적용됐다.
host의 기존 일반 SDK와 standalone ARM의 기존 transport는 이 기록으로 whole-SDK 계측
범위가 늘어나지 않는다. canonical reusable ARM의 별도 prepared SDK gate도 유지한다.

## 삭제와 인벤토리의 구분

| 경로 | 원장 |
| --- | --- |
| `rust/tools/check_plannerd_ci.py` | baseline3580 |
| `rust/tools/tests/test_plannerd_ci.py` | baseline3580 |
| `rust/tools/check_plannerd_native_memory.py` | baseline3580 |
| `rust/tools/tests/test_plannerd_native_memory_controller.py` | 추가 실행 원장 |

원래 raw source5와 memory5는 source425에서 독립 관측하고 byte-identical frozen 데이터로
보존했다. 새 negative는 private root에 file/directory/live·dangling symlink를 복원하고
leaf/output 이전 거부 및 첫 leaf0/42 뒤의 사후 거부·원래 commands 보존을 검사한다.
프로젝트 root의 원본 파일을 임시 복원하거나 frozen expected를 native로 다시 만들지 않는다.

현재 baseline296교체/3부분/3281대기는 strict Clippy 수용까지 유지한다. 향후 인정할 변화는
baseline3개와 추가1개이며 baseline4개로 계산하지 않는다. 원래 leaf/IPC/원본 compiler·
Cython·실기/전체 PR와 같은 남은 경계를 원장에 계속 남긴다.
