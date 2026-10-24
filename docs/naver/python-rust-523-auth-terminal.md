# #523 인증·터미널 독립 작업 기록

- 이슈: https://github.com/bin9208/openpilot-rust/issues/523
- 작업 기준: `/home/fma1/.codex/worktrees/python-rust-523/openpilot-rust`, `7a3727e44`.
- 기록일: 2026-10-24. 상태: **소스 후보 작성·검토, 기능 검증 대기**.
- 공유 Cargo 등록, 필수 게이트, workflow 및 Actions 실행은 부모 작업 담당이다.
  이 작업에서는 push·workflow 편집·Actions 실행을 하지 않았다.

## 원본과 기존 후보 검토

정확한 원본 경로는 `rg`로 확인했다. 인증은 `openpilot/tools/lib/auth.py`,
`auth_config.py`, `api.py`이고 `openpilot/common/api.py`의 기기 키 기반 API와 다르다.
SSH 도구는 `tools/scripts/ssh.py`, `tools/scripts/setup_ssh_keys.py`이며,
터미널은 `openpilot/tools/lib/kbhit.py`이다. `opendbc_repo/examples/kbhit.py`는
별도 외부 코드이므로 이번 대상에 포함하지 않았다.

| 기존 구현 | 검토한 범위 | 확인한 실제 누락 |
| --- | --- | --- |
| `b3b4afa41986f71deafca9b6eb453a07b12dca65` | SSH 조회·계획·exec 경계, 키 설정 효과 순서, GET transport 및 synthetic 테스트 | SSH CLI는 등록되어 있지만 브라우저 인증 CLI는 없음 |
| `12656fa38f4c9f105caafc52532b8cc947f75a17` | 명시적 root의 auth 읽기·쓰기·삭제, 파일 효과·권한·링크 테스트 | OAuth callback, 토큰 교환, 사용자 확인 후 저장 흐름 없음 |
| `c676b4191c3e37a9afe926c4724774e1fe76b11d` | 명시적 fd, termios·select·UTF-8·arrow·atexit 순서와 private PTY 테스트 | 기존 `KBHit()` 호출부가 사용할 stdin 및 동일 fd 읽기 어댑터 없음 |

기존 테스트는 소스 후보 검토 근거이며 실행 성공 증거로 간주하지 않는다.
기존 auth 파일 효과 테스트의 비 root 권한 검사와 KBHit 원본 비교·실제 C exit
복구 게이트도 유지해야 한다. 과거 contract overlay 해시는 과거 소스 증거이며
이번 수정 소스의 검증 결과로 재사용할 수 없다.

## 직접 반영한 변경

- `src/developer_ssh/login.rs`: provider URL, 최초 `/auth`·`/auth/` 결과 채택,
  blank·중복 code/provider·오류 callback 검증, JSON 상태 및 주입 가능한 로그인 흐름.
  `v2/auth/` 토큰 교환 → 문자열·공백 검사 → `v1/me` 확인 → 저장 순서를 유지한다.
  시간 만료·브라우저 오류·API/저장 실패는 원본 JSON 오류로 반환한다.
- `src/developer_ssh/login/native.rs`: loopback ephemeral callback 서버,
  요청당 1초 제한과 독립 handler, `open`/`xdg-open`, 180초 로그인 대기,
  30초 API timeout, 검증 후 기존 config-root/token writer 연결.
  import나 테스트 중 브라우저·실제 인증을 시작하지 않는다.
- `src/developer_ssh/http.rs`: 기존 transport에 form POST와 301/302/303의
  GET 전환, 307/308의 body 유지 추가. 기존 redirect 수·쿠키·인증 헤더 제거·
  압축 해제·socket timeout 정책은 유지한다.
- `src/developer_ssh/auth_json_main.rs`: 브라우저 JSON 전용 native bin 진입점.
  `--json` 성공/실패 상태는 exit 0, 인자 오류는 exit 2이다.
- `src/developer_ssh.rs`: 위 login 모듈 공개.
- `src/kbhit.rs`: 명시적 `from_stdin()`, `has_input()`, `read_char()`,
  `read_arrow()`와 동일 fd의 `Read` 구현. 원본의 명시적 복구/atexit 순서,
  ICANON·ECHO 외 termios 유지, 비차단 플래그 미변경 정책은 유지한다.
- `src/kbhit/tests.rs`: 동일 private PTY에서 readiness·UTF-8·arrow 소비·복구 회귀 추가.
- `tests/developer_auth.rs`: 8개 테스트. provider·callback·잘못된/미검증 토큰·
  timeout·실패 시 저장 방지·CLI 제한을 검사한다. 그중 원본 비교는 원본
  `auth.login()`과 URL 함수를 `python3 -I -S`에서 불러와 21개 synthetic 입력의
  상태·외부 효과 순서·저장 결과·URL을 native와 비교하도록 작성했다.
  원본 import 전에 API/auth_config를 교체하고, 서버·브라우저·저장을 주입하므로
  기존 credential을 읽거나 실제 인증·SSH를 실행하지 않는다.

위 파일 경로는 모두 `rust/crates/dev-tools/` 기준이다.

## 부모 통합용 등록과 최소 검증

`lib.rs`·새 feature·dependency·lock 수정은 필요 없다. 공유
`rust/crates/dev-tools/Cargo.toml`에는 부모가 다음을 등록한다.

```toml
[[test]]
name = "developer_auth"
required-features = ["developer-ssh-native"]

[[bin]]
name = "developer-auth-json"
path = "src/developer_ssh/auth_json_main.rs"
required-features = ["developer-ssh-native"]
```

feature union은 `developer-ssh-native,kbhit-native`이다. 다음 명령은
**부모의 bin-actions용 제안이며 로컬에서 실행하지 않았다**.

```sh
cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --features developer-ssh-native,kbhit-native --test developer_auth --test developer_ssh
cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --features developer-ssh-native,kbhit-native --lib developer_ssh::
cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --features developer-ssh-native,kbhit-native --lib kbhit::
cargo build --manifest-path rust/Cargo.toml -p openpilot-dev-tools --features developer-ssh-native --bin developer-auth-json
```

auth 권한 테스트는 비 root runner에서 실행해야 한다. 원본 비교에는 stdlib를
갖춘 `python3`가 필요하다. 실제 로그인 bin을 실행하지 말고 빌드만 검증한다.
기존 필수 게이트·엄격한 Clippy·대상 ARM 검증은 부모 정책을 그대로 적용한다.

## 검증 결과와 남은 작업

로컬 Rust 2021 `rustfmt` 구문·포맷 확인, embedded Python의 `ast.parse` 구문 확인,
변경 범위 `git diff --check`를 수행했다. 빌드·기능·Clippy·원본 비교는 실행하지
않았으며 exact-SHA Actions 결과와 URL은 아직 없다. native callback 실제 socket,
POST/redirect wire 동작과 실제 프로세스 종료 복구 증거도 대기 상태다.

이번 CLI는 Cabana가 사용하는 브라우저 JSON 계약만 대상으로 한다. 일반/JWT
`auth.py` CLI와 `pprint` 출력, argparse 진단 전체, Python TextIOWrapper의 locale·
read-ahead·newline 동등성은 완료되지 않았다. 이 한계를 성공으로 처리하거나
기존 게이트를 낮추지 않았다.

실제 호출부 `openpilot/tools/replay/py_downloader.cc::authenticate()`는 여전히
`openpilot.tools.lib.auth`를 실행한다. 부모가 검증 후 native bin 경로로 연결해야
한다. `openpilot/tools/joystick/joystick_control.py::Keyboard` 역시 유지하며,
향후 native 호출부는 `KBHit::from_stdin()` 및 동일 fd 읽기 메서드를 사용할 수 있다.
이 작업은 공유/기존 호출부를 수정하지 않았다.

**원본 Python 파일 삭제·호출부 활성화·전체 전환 완료 선언은 보류한다.**
실제 credential 읽기, 브라우저 로그인, SSH, 기기 접근은 수행하지 않았다.

## 2026-10-24 후속 배치: 실제 callback socket 단위 후보

부모가 전달한 상태는 초기 CI 용량 실패 뒤 캐시 회수로 free 9.44 GB를 확보했고,
`d0f53d15a`의 focused [40079353750](https://github.com/bin9208/openpilot-rust/actions/runs/40079353750)가
queued라는 것이다. 이는 전달받은 당시 상태이며 이번 추가 소스의 실행 증거는 아니다.
기존 CLOEXEC 보완과 HTTP redirect/Requests 대조는 부모 소관으로 그대로 유지한다.

이번 직접 수정은 `rust/crates/dev-tools/src/developer_ssh/login/native.rs`의
새 `#[cfg(test)] mod tests`와 이 기록뿐이다. production 함수·timeout·호출부,
공유 Cargo·workflow는 변경하지 않았다. 다음 **3개 단위 테스트는 UNRUN**이다.

| 테스트 | 실제 socket에서 확인하도록 작성한 범위 |
| --- | --- |
| `socket_routes_return_status_and_preserve_first_auth_result` | private ephemeral loopback의 실제 HTTP 요청/응답으로 무관 경로 204와 빈 결과, `/auth` 및 `/auth/` 각각의 정상 200·응답 본문·code decoding, 뒤의 error callback에도 최초 결과 유지 |
| `incomplete_preconnection_does_not_block_second_callback` | browser 없는 `NativeLogin`에 private listener만 주입하고 `poll_callback`으로 정확히 두 연결만 수락. 첫 요청의 미완성 header를 유지한 채 두 번째 정상 응답을 받은 다음 첫 요청을 완성하고 그 응답까지 확인. 첫 연결이 timeout된 뒤 두 번째만 성공하는 직렬 처리 회귀는 통과하지 못하도록 구성 |
| `closed_client_does_not_discard_received_callback` | 완전한 요청을 전송한 client를 shutdown/drop한 뒤 직접 `handle_callback` 호출. 응답 write 성공/실패와 무관하게 수신한 결과를 유지하고 새 callback도 덮어쓰지 못하도록 확인 |

앞뒤 경로·닫힌 client 테스트는 handler를 직접 호출하므로 추가 thread/process가
없다. preconnection 테스트는 production `poll_callback`이 생성하는 최대 두
handler만 사용하며 기존 accepted stream의 read/write 1초 timeout을 유지한다.
성공 경로에서 두 응답을 EOF까지 읽어 양쪽 worker의 연결 종료를 확인한다.
client connect/read/write는 각각 2초로 제한하며 무제한 재시도 루프는 없다.

닫힌 client 테스트는 커널마다 마지막 write가 성공하거나 연결 종료 오류가
발생할 수 있다는 조건을 허용한다. 강제 RST나 특정 write 오류를 입증했다고
주장하지 않는다. 보존 대상은 메모리의 수신 callback이며 실제 token 파일 저장이나
실계정 인증의 증거로 확장하지 않는다.

실계정·browser·API·credential·현재 stdin을 접근하지 않으며 `start`, `exchange`,
`verify`, `auth_json_cli`를 호출하지 않는다. production 보완 필요를 입증하는
실행 결과는 아직 없다. 원본 Python과 callers는 유지한다.

로컬 확인은 Rust 2021 `rustfmt --check`, `git diff --check` 및 변경 diff 검토에
한정했다. `#[cfg(test)]` 앞 production prefix의 SHA256
`9555b7d2ac46f94815fc697d28516e75abe77b5abc85fb37ffe0863ed46946e4`가
추가 전후 동일한지 별도로 확인했다. 빌드·기능·Clippy·socket 테스트 실행은
하지 않았으며 다음 부모 bin-actions 배치의 검증을 기다린다.

공유 등록이나 feature 추가는 필요 없다. 부모 Actions에서의 단위 필터 예시는
다음과 같으며 **로컬 실행 명령이 아니다**.

```sh
cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --features developer-ssh-native --lib developer_ssh::login::native::tests::
```

## 2026-10-24 후속 배치: 일반/JWT CLI와 pprint 후보

부모가 JSON 후보·실제 loopback socket 테스트의
[40079353750](https://github.com/bin9208/openpilot-rust/actions/runs/40079353750),
[40080413343](https://github.com/bin9208/openpilot-rust/actions/runs/40080413343)
성공을 전달했다. 위의 초기 UNRUN/queued 기록은 당시 이력이다. 이번 일반/JWT
추가는 그 뒤의 **새 소스 후보이며 기능·원본 비교·빌드·Clippy는 UNRUN**이다.
부모의 wrapper/op.sh·CLOEXEC·기존 Clippy 보완과 #482/#529 작업은 수정하지 않았다.

### 원본에서 보존한 동작

- 인자를 생략하면 google 브라우저 모드이다. 브라우저 모드의 두 번째 positional은
  원본처럼 무시한다. `--json`은 성공/오류 모두 JSON status와 exit 0이며,
  JWT와 `--json`을 함께 주면 argparse 오류와 exit 2이다.
- 일반 브라우저 모드는 로그인 오류를 stderr에 출력하고 exit 1이다. 성공한 뒤에는
  저장한 token을 **다시 읽고** 최종 사용자 API를 호출한다.
- JWT 인자 누락은 stdout의 `method JWT selected, but no JWT was provided`와 exit 1이다.
  빈 문자열을 포함해 제공된 JWT는 원본대로 **저장한 다음 다시 읽어** 검증한다.
  이 원본 수동 모드 순서를 브라우저 JSON 모드의 검증 전 저장 허용으로 확대하지 않았다.
- 최종 main 호출은 원본의 `CommaApi(token=get_token()).get('/v1/me')`이다.
  API_HOST 뒤에 `/`를 추가하는 원본 특성 때문에 URL의 `//v1/me`를 보존하며,
  login 내부의 `v1/me`/timeout 30초와 달리 최종 호출에는 timeout을 지정하지 않는다.
- 최종 조회 성공 뒤에만 `Authenticated!`와 기본 `pprint.pprint(me)` 형식 및 newline을
  출력한다. 최종 API의 truthy error와 HTTP 401/403이 함께 있을 때만
  `Got invalid JWT`를 stdout에 출력하고 exit 1이다. HTTP status만으로
  원본의 JSON error 조건을 대체하지 않는다. 다른 의존성 오류는 전파한다.
- help·인자 오류는 인증 작업보다 먼저 처리한다. `--`·long option 축약·명시적 flag 값
  오류·help action 순서와 두 optional positional의 소비 순서를 원본에 맞춰 작성했다.
  특히 `apple --json unused`는 뒤의 positional을 다시 채우지 않고 extra로 처리한다.

### 변경 파일과 노출 범위

- `rust/crates/dev-tools/src/developer_ssh/login/cli.rs`: private `CliIo`와 CLI 처리.
  token 저장·재조회·최종 API를 주입하므로 실제 계정 없이 main 동작을 비교할 수 있다.
- `rust/crates/dev-tools/src/developer_ssh/login/pretty.rs`: JSON 값 영역의 기본 pprint.
  dict key 정렬, width 80/indent 1, list·dict 줄바꿈, 문자열 word/line 분할·최상위 괄호,
  Python scalar repr을 구현했다. CPython `Lib/pprint.py`의 관련 PSF 라이선스 동작을
  소스로 확인했으며 기존 저장소의 scalar repr을 재사용한다.
- `rust/crates/dev-tools/src/developer_ssh/login/cli/tests.rs`: **3개 새 lib 테스트**.
  원본 파일을 바꾸지 않고 메인 AST를 실행하되 login/config/API 의존성만 주입한다.
  stdout·stderr·exit code/전파 오류 메시지·호출 순서·저장 결과를 native와 비교한다.
  mode/오류, argparse action, pprint JSON 영역을 각각 검증하도록 작성했다.
  `-S`에서 없는 site `exit`는 `sys.exit`로 주입해 SystemExit만 비교하며 실제 stdin을
  닫는 site.Quitter 효과는 실행하거나 동등하다고 주장하지 않는다.
  help 사례는 auth dependency 호출·저장 효과가 모두 비어 있음을 별도로 요구한다.
- `rust/crates/dev-tools/src/developer_ssh/login/native.rs`: 기존 login과 새 CLI 연결,
  token root 재사용, 최종 API의 typed Unauthorized 경계. 새 private request/response
  단위 1개는 double-slash URL·timeout None·falsey token·API error 규칙만 검사한다.
  기존 callback socket 테스트와 accepted-stream timeout은 그대로 둔다.
- `rust/crates/dev-tools/src/developer_ssh/login.rs`: 두 private 모듈과 `auth_cli` 공개.
  기존 `auth_json_cli`는 같은 함수의 호환 별칭이다. 주입 trait·pprint·parser는 공개하지 않는다.
- `rust/crates/dev-tools/src/developer_ssh/auth_main.rs`: `developer-auth`용 새 진입점.
  기존 `auth_json_main.rs`는 수정하지 않으며 `developer-auth-json` 호환 경로를 유지한다.
- 이 기록 문서만 추가 갱신했다. 기존 Python, Cargo, workflows, shared docs/inventory,
  `rust/tools/run-developer-tool`, `tools/op.sh`는 수정하지 않았다.

### 부모 등록과 다음 Actions selector

부모가 `rust/crates/dev-tools/Cargo.toml`에 다음 bin만 추가하면 된다.
기존 `developer-auth-json` 등록은 그대로 둔다. 새 feature/dependency/lock/테스트 target은
필요 없으며 wrapper의 `developer-ssh-native`로 두 entrypoint와 lib 테스트를 빌드한다.

```toml
[[bin]]
name = "developer-auth"
path = "src/developer_ssh/auth_main.rs"
required-features = ["developer-ssh-native"]
```

다음은 부모 Actions용 selector이며 로컬에서는 실행하지 않았다.

```sh
cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --features developer-ssh-native --lib developer_ssh::login::cli::tests::
cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --features developer-ssh-native --lib developer_ssh::login::native::tests::final_me_request_retains_source_url_timeout_and_api_error_rules
cargo build --manifest-path rust/Cargo.toml -p openpilot-dev-tools --features developer-ssh-native --bin developer-auth --bin developer-auth-json
```

기존 `--test developer_auth`·기존 auth 파일 효과·native callback·원본/안전 게이트는
그대로 유지한다. 새 원본 비교 subprocess는 `python3 -I -S`, 비밀 없는 환경,
private 임시 cwd와 stdlib만 사용하며 original login·실제 token/config/API·browser·
stdin·SSH는 호출하지 않는다. 이 subprocess 및 socket/functional 체크는 로컬 미실행이다.

### 로컬 확인과 남은 한계

Rust 2021 `rustfmt --check`와 변경 범위 `git diff --check`, embedded Python의
`ast.parse` 구문 확인 및 diff 검토만 수행했다. 빌드·기능·원본 oracle·Clippy의 새
성공 결과는 없고 commit/push/Actions 실행도 하지 않았다.

pprint 대상은 실제 JSON API 값이며 Python 임의 객체·cycle·tuple·비문자열 dict key는
대상이 아니다. Python 버전별 Unicode repr 표, 비표준 터미널 폭/locale에 따른
argparse help 재배치, 출력 stream 실패의 부분 쓰기, OS 비 UTF-8 argv는 아직
동등성 완료 근거가 없다. 일반 API/파일 예외는 native error로 전파하며 Python의
traceback stack/path 바이트를 재현하지 않는다. 새 원본 oracle은 이 예외들의
전파 메시지·그 전까지의 출력·효과 순서를 비교하며 traceback 자체를 비교하지 않는다.
실제 account/browser/API/config/stdin/SSH, wrapper 실행·caller 설치와 원본 retirement는
검증하지 않았으며 **원본 삭제·전체 대체 완료 선언은 계속 보류**한다.


부모 통합에서 `developer-auth` bin을 등록하고 기존 `developer-auth-json`은 유지했다.
새 oracle 테스트에는 `AUTH_CLI_ORACLE_PYTHON`으로 준비한 Python 3.12 venv의 절대
경로를 전달한다. pprint의 원저자 Fred L. Drake, Jr. 및 기존 보존 PSF 라이선스
`src/changed_files_check/LICENSE-CPYTHON`의 위치도 명시했다. 실제 `op` 호출부의
help-only exec 검증과 기존 7개 login/8개 원본 비교가 다음 집중 CI 범위다.
이 통합 기록 작성 시점에는 모두 **UNRUN**이며 원본 제거 수량은 바꾸지 않는다.
