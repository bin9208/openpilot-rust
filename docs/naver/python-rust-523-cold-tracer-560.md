# #560 cold tracer 환경 관측 보완

2026-10-25. source `acc716e47875f352f1521d0211f218499aa34603`의
[Fast40155092407/job121054673203](https://github.com/bin9208/openpilot-rust/actions/runs/40155092407/job/121054673203)는
실제 cold 감사에서 `unrecognized trace line 169`로 실패했다. 원문은
`2560  ???( <detached ...>`이며 바로 앞 PID2559는 실제 `rustc -vV`를 실행했다.
debug에는2559의 CLONE/2560 startup과 두 task의 EXIT0이 있다. syscall-info 실패 errno는
아직 관측하지 못했으며 unknown은 계속 거부한다.

artifact12583359346=589832B/SHA256
`764c74c3095d96401cf92c175c73aa551e4f0832a0389ccd9ca1b46659f73a9f`를
전수 CRC/SHA로 확인했다. 기존 #560의 dying-task 유형과 같지만 원인이 해결됐다는 뜻은 아니다.
실제 kernel release와 dpkg source revision이 원문에 없으므로 다음 최소 관측을 먼저 연결한다.

구현 전 계획은 기존 strace provider 전후 기록에 kernel release/version/machine,
`/proc/version`, strace의 실제 dpkg binary/source package/version/architecture와 조회 provider의
metadata/SHA를 추가하는 것이다. 각 channel은64KiB/file로 제한하고 실행 실패를 전파한다.
전후 raw bytes를 비교하며 collector의 원래6개 통제를 보존하고 새 metadata drift4개도 거부한다.
실제 cold 명령·timeout·stderr cap·자손 회수·source/SDK gate와 unknown parser는 바꾸지 않는다.

이는 관측 후보이며 기능은 Actions UNRUN이다. 배포판 strace 교체, 추가 ptrace 호출,
실패 행 삭제/허용, 성공까지 재실행 또는 원본 제거 credit은0이다. errno/stop 관측과 근거 있는
provider 수정은 별도 후속이며 이 기록만으로 #560을 종료하지 않는다.
