# #474 recovery 기능과 exec 관측의 분리 검증

2026-10-26. 추적: [#474](https://github.com/bin9208/openpilot-rust/issues/474),
[#523](https://github.com/bin9208/openpilot-rust/issues/523).

source126fdbea의 [workspace121222236348](https://github.com/bin9208/openpilot-rust/actions/runs/40209691630/job/121222236348)은
recovery13 unit/Clippy 뒤 실제 eight-controller trace에서
`ptrace(PTRACE_LISTEN): Input/output error`로 멈췄다. 현재 로그를 읽었으며 이것을
애플리케이션 실패나 SIGHUP의 stopping-signal 의미로 단정하지 않는다. 기존 #474의
요청대로 같은 소스의 완전한 기능 lane과 별도 완전한 exec lane을 준비한다.

새 `python-rust-523-recovery-lanes.yml`은 기존 native controller·독립 frozen fixture·
`audit-recovery-exec`·eager `list-cargo-tests`를 그대로 사용한다. 새 Rust/controller나
Python oracle는 추가하지 않는다. 기존 main workspace job/회귀/임계값도 변경하지 않는다.

- 한 번 빌드한 server/actions/upload/process-child/CWP test/tmux/harness 일곱 경로의
  SHA256·device/inode/size/mode/uid/gid/mtime를 기록하고 각 lane 뒤 대조한다.
  정상 actions_effects가 hardlink를 만드는 actions/process-child 두 ELF의 nlink/ctime는
  정보로 보존하고 기존 authorized alias receipt와 strict pre/post program seal로 검증한다.
  나머지5개는 nlink/ctime도 동일해야 한다. 두 hardlink 대상과 job evidence가 같은
  filesystem인지 실제 stat device로 입장한다. 정상 link 생성의 ctime 변화를 실패로 세지 않는다.
  shell 단계 사이에 같은 FD를 계속 보유했다는 주장은 하지 않는다. 각 native controller의
  기존 Pin 검증과 trace의 기존 pre/post program seal도 유지한다.
- 첫 lane은 ptrace 없이 기존 `eight_native_recovery_controllers_same_elf_present_and_absent`
  전체를 실행한다. checkout/private 두 루트의 기존8 controller·독립 원본 관측·
  lifecycle/reset/descendant/upload 비교를 그대로 요구한다.
- 두 번째 lane은 별도 evidence/native 디렉터리에서 같은 selector를 다시 실행한다.
  원래 strace `-ff -qq -s 32768 -xx -e trace=execve,execveat`와 source-authored pre/post
  audit를 그대로 사용한다. 첫 lane 실패도 두 번째 관측을 막지 않으며 첫 실패를 성공으로
  덮지 않는다. 부분 trace나 tracer EIO를 PASS로 인정하지 않는다.
- 각 lane의 command 상한은15분/외부 step16분, 전체 hosted job45분이다. 기존 내부
  lifecycle/timing/cleanup 정책은 바꾸지 않는다. 기존 각 evidence200MiB 검사를 유지하고
  두 lane status·raw·selected ELF 및 source/SDK 전후 proof를 upload한다.
- cleanup은 upload id/digest와 source/SDK final 외에 **두 lane 성공**을 요구한다.
  미완료/관측 실패를 회수 승인으로 쓰지 않는다.

호스트는 `ubuntu-24.04`/jobs2/incremental0/dev-test debug0이며 로컬4GB 러너를 차지하지 않는다.
checkout 전 실제 filesystem별 동시 추가 성장량을 합산한다. source2+target6+registry2+
SDK3+proof1+APT2+reserve0.5=16.5GiB의 예상이며 고정 여유 floor나 peak 보장이 아니다.
SDK는 기존 private read-only before/after helper와 절대 toolchain 경로를 사용한다.
원래 명시 host 도구에 필요한 capnproto/compiler를 설치하고 Python 설치는 없다.

이 문서는 구성/예상만 기록한다. 로컬은 YAML·Bash 구문/diff 검토에 한정하며 Cargo/build/
기능/원본/tracer 실행은0이다. 실제 exact-source GHA의 두 lane과 산출물을 모두 감사하기 전
#474 또는 recovery Python 원본 전체 퇴역을 완료 처리하지 않는다.


source425f29의 첫 [run40227415644/job121271640979](https://github.com/bin9208/openpilot-rust/actions/runs/40227415644/job/121271640979)은
Set up job에서 defaults의 동적 `env.DIAG_SOURCE`가 아직 빈 값이어서 GitHub template admission에
실패했다. worker run command/build/기능은 시작하지 않았다. job defaults에는 shell만 두고,
DIAG_SOURCE 할당 뒤 실행하는 네 step에 working-directory를 각각 명시했다. source/SDK/
하드링크/기한·성장량·두 lane·실패/cleanup 계약은 변경0이다. 수정본 GHA에서 다시 확인한다.


## source dd3 실제 두 harness 성공·추적 감사 실패 — 2026-10-26

[run40227564162/job121272080733](https://github.com/bin9208/openpilot-rust/actions/runs/40227564162/job/121272080733)의
source는 `dd3a41f44fadda755c90224785a84554fa62318d`다. 같은 빌드 실행물의 untraced8과
traced8 전체 harness가 모두 성공했다. 두 번째 harness는318.68초였다. 그 뒤 원래 strict
`audit-recovery-exec`가 `unclassified strace line`으로 실패해 exec lane은 수용하지 않는다.
원래 source/SDK 최종 봉인과 upload는 성공, cleanup은 올바르게 생략됐다.

artifact12612666505는69,256,924B/SHA256
`caed9ba3737ea19a60dea46e4f0265880b77e5b062dcd64e0ab9d6b31ef1d696`,
12,211 members/expanded219,753,266B다. 원문 `recovery-trace/exec.16876`의5행은
`???( <unfinished ...>`이며 허용된 exec/signal/exit 문법 밖의 행은 이것 하나다.
unknown 행을 무시하거나 parser 허용 목록에 추가하지 않는다. ptrace provider/종료 상태의
실제 원인을 계속 조사한다. 이 cohort에서 과거 `PTRACE_LISTEN` EIO는 관측되지 않았으나
옛 재현 문제가 해결됐다는 증거로 쓰지 않는다. #474는 OPEN이다.
