# #610 지속 X64 runner의 기본 APT 조회 범위

상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598),
공통 CI [#605](https://github.com/bin9208/openpilot-rust/issues/605),
독립 결함 [#610](https://github.com/bin9208/openpilot-rust/issues/610)의 기록이다.

PR #606 source `bfffa2d30d1e8037cc3f7eb80b5a2b5264c8070c`의
[40315420260](https://github.com/bin9208/openpilot-rust/actions/runs/40315420260), job121552444108은
X64 webcam 의존성 준비에서 기본 `sudo apt-get update`가 Ubuntu archive에 ARM64 목록을
요청해404/exit100으로 실패했다. webcam/provider 컴파일·기능 비교에는 도달하지 않았다.
원래 startup UI cross job은 host에 arm64를 등록하고 명시적인 cross sources를 사용한다.
지속 host의 그 상태를 삭제해 다른 검증을 훼손하지 않는다.

job-temp의 자기 소유 BASH_ENV가 `native-apt-shell`을 읽는다. X64의 원래 shell 명령은
변경하지 않고 기본 `sudo apt-get` / `sudo -n apt-get` 호출만 실제 native architecture의
APT view로 실행한다. 추가되는 실제 argv는 `-oAPT::Architectures=amd64`이며 모드·source SHA를
자기 job의 JSONL에 남긴다. 명시적인 architecture/config/source 옵션은 원래 호출 그대로
전달한다. 다른 sudo 명령은 변환하지 않는다. ARM context에서는 sudo 함수를 정의하지
않으므로 기존 guest argv·입력 pin은 그대로다. 공유 dpkg 등록이나 APT source를 변경하지 않는다.

집중 workflow는 bin-actions에서 원래 실패 명령을 실제 실행한다. 이어 기존
`.github/apt/startup-ui-cross.sources`를 그대로 사용해 private index 디렉터리에 실제
amd64와 arm64 metadata를 내려받는다. architecture 목록은 process-only 설정으로 지정하며
dpkg에는 추가하지 않는다. 명시적 cross 호출에 native 제한을 덧붙이면 ARM index 검사가
실패한다. 모든 shared source 내용 hash·mode·UID/GID와 dpkg native/foreign 목록을 전후 비교한다.
무관한 sudo 통과와 ARM shell guard도 확인한다. 이 guard 검사를 실제 ARM 실행으로 표시하지 않는다.

원래66개 job의 명령·needs·matrix를 검사하는 기존 정책 회귀를 함께 실행한다.
로컬은 bash/Python/YAML 문법·diff·읽기만 확인하며 실제 APT와 회귀는 Actions에서 수행한다.
집중 검사와 정확 SHA 결과는 검증 대기다. 전체 webcam 비교 및 #605의 필수 PR/병합 후
검사는 별도 통과해야 하며 이 APT 조사로 완료 처리하지 않는다. 기존 진행 중인 CI는 보존한다.
