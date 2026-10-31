# ARM QEMU 원본 peer 시작 준비 — #609 (검증 대기)

#598/#605/PR606의 정확 source `bfffa2d30d1e8037cc3f7eb80b5a2b5264c8070c`에서
[job121552443742](https://github.com/bin9208/openpilot-rust/actions/runs/40315420260/job/121552443742)
는 owner/source binding 빌드 단계가 성공한 뒤 owned 비교에서 실패했다. job 시간 예산
초과나 compiler 오류가 아니다. 원본 peer는 살아 있었으나 첫8초 기능 대기에 실제 수신
publication이 없었다. 보존 artifact12657221712의 source stdout/stderr는0바이트였고,
source 초기화 완료 marker도 아직 없었다. 입력 publisher는 joined=true/failure=null이다.
이 관찰로 실제 장치 성능이나 원본 runtime 결함의 원인을 단정하지 않는다.

비교 harness에 별도의 최대120초 시작 준비 단계를 추가했다. 실제 `carrotMan` publication을
받아야만 준비된 것으로 기록하며, 임의 sleep이나 성공 표식으로 대체하지 않는다. 준비
단계는 selected 비교 결과에 packet을 추가하지 않는다. 준비 후 기존8초 기능 predicate,
유효성/주기/정책/입력/업로드 시나리오와 원래 실행 명령은 그대로다. source/native 모두
같은 준비 조건을 사용하고 elapsed/예산/수신 수/실행 모드를 startup-readiness.json에 남긴다.
identity assertion 실패도 실제 값을 보존하도록 identity 기록을 assertion 전에 저장한다.

집중 `carrot-peer-readiness.yml`은 기존 CarrotMan owner job 전체를 복사하며 같은 pin,
원래 모든 명령·검사·X64/ARM matrix와 보존 경로를 유지한다. 모두 bin-actions에서 실행하고
ARM은 QEMU/PRoot userspace로 기록한다. 로컬은 AST 구문·diff 및 원래 job의 읽기 복사만
확인했다. 실제 전체 비교 결과는 Actions 대기 중이며 PR606의 다른 진행 중 검사는 그대로
보존한다. 기기·운영 runtime 변경이나 사용자 기기 수용으로 표현하지 않는다.
