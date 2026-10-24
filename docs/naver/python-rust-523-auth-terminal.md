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

## 2026-10-24 후속 배치: SSH·키 설정 CLI의 fresh 원본 비교

부모 통합 기준은 `41cfaffd310059f4f999c8d24dd491d9694baa91`이다. 부모가
[40082949276](https://github.com/bin9208/openpilot-rust/actions/runs/40082949276)의
queued 상태를 전달했으며 이 추가 테스트의 결과는 아니다. 이번 직접 변경은
`rust/crates/dev-tools/tests/developer_cli_original.rs` 신규 파일과 이 기록뿐이다.
기존 auth/SSH production, Cargo, wrapper, workflows, inventory는 수정하지 않았다.

### 단위 범위: 3개 테스트, 52개 synthetic 사례

| selector | 사례 수 | 범위 |
| --- | --- | --- |
| `ssh_cli_original_aliases_prefix_unicode_duplicate_order_and_exec` | 20 | ASCII 공백 제거·lower, 중복 dongle ID의 원래 위치 유지와 마지막 alias, 비문자열 alias 제외, 16자 prefix와 suffix, 15자/비 ASCII fallback, Unicode/비 casefold/NBSP, 포트 정규화·TCP 범위 미제한, 정확한 argv·debug의 의도된 미이스케이프 quote, HTTP 실패, direct ID에서도 전체 dict 구성 |
| `ssh_cli_original_port_argparse_help_and_no_auth_effects` | 11 | help 및 help 전/후 port 변환, 덮어쓴 잘못된 port, missing/extra/unknown 인자·누락 option 값, 도움말 앞의 ambiguous option도 분류 단계에서 실패, 인증/HTTP/exec 효과 없음 |
| `setup_cli_original_status_default_decode_and_write_failure_order` | 21 | missing username, 200/empty/201/204/301/404/500, Requests 기본 Latin-1·UTF-8/ASCII replacement·unknown codec fallback, `--help`의 literal username, Unicode username·extra args 무시, HTTP/Params/각 write/decoder 실패 순서 |

원본은 실제 `tools/scripts/ssh.py`, `tools/scripts/setup_ssh_keys.py`의 AST 전체를
읽어 실행한다. imported config/API/BASEDIR/Params/requests와 `os.execvp`를 주입하며
BASEDIR는 `/source`, argv[0]은 각 script basename으로 고정한다. 성공한 SSH의
exec는 별도 BaseException으로 가로채고 file/argv를 기록하며 실제 exec를 하지 않는다.
site의 `exit`는 `sys.exit`로 주입해 현재 stdin을 닫는 효과를 실행하지 않는다.

native 쪽은 `parse_options`, `prepare`, `setup_keys_with_decoder`를 호출한다.
후자의 observer는 실제 `RequestsText`를 감싸며 decode 시점만 기록한다. decoder
자체의 인위적 실패 사례를 제외하면 실제 `setup_keys`도 호출하여 stdout·예외·write·
HTTP/Params 순서를 재확인한다. 이 보조 비교에서는 observer가 추가한 `decode`
marker만 제거하며 다른 필드는 정확히 비교한다. non-200은 decoder 실패 주입
상태에서도 default 경로와 함께 decoder/Params 미접근을 확인하도록 작성했다.

source의 token read·native CLI의 username 선택/반환 code 등은 테스트 adapter가
이어 주는 경계다. 실제 `ssh_cli`, `setup_cli`, `Plan::exec`, config root 발견,
네트워크, SSH 및 실제 Params 구현은 실행하지 않는다. 주입 write failure는
메서드가 예외를 던지는 상황이며 Cython의 native I/O 반환 코드 무시나 실제 디스크
오류를 검증한 것으로 확대하지 않는다. 기존 별도 게이트를 유지한다.

### 독립 decoder oracle 및 실행 환경

원본의 `requests.get`은 synthetic Response만 반환한다. 실제 `requests.Response.text`
및 `requests.utils.get_encoding_from_headers`를 decoder oracle로 사용하며 native의
정답을 Python helper에 다시 구현하지 않는다. requests import 이외의 실제 request,
account·credential·stdin·SSH·Params 접근은 없다. subprocess stdin은 null이다.

부모가 준비한 **Python 3.12 + requests 설치 환경**이 필요하다. interpreter 선택은
`DEVELOPER_CLI_ORACLE_PYTHON` → 기존 `AUTH_CLI_ORACLE_PYTHON` → `python3` 순서다.
`-I`와 비밀 없는 환경, `COLUMNS=80`, private 임시 cwd를 사용한다. requests 패키지를
사용하므로 `-S`는 쓰지 않으며 다른 Python minor version은 명시적으로 실패한다.
정확한 Python/requests 버전은 매 캡처의 metadata로 남긴다. 추가 설치나 기능 실행은
로컬에서 수행하지 않았다.

### 재사용 가능한 기대값과 명시적 차이

`openpilot.developer-cli-original.v1` envelope에는 `group`, Python/requests metadata,
원본 두 파일의 **raw bytes SHA256**, `records`가 들어간다. 각 record는 `input`,
`criteria`, `expected_original`로 구성된다. 원본 outcome의 stdout/stderr를 그대로
남기고 exit code, 예외 type/message, 시간순 효과, 성공한 write의 byte 배열,
exec file/argv를 모두 보존한다. exec 교체나 uncaught exception에는 exit code를
null로 두어 정상 exit 0과 구분한다. Python traceback stack/path는 캡처·동등성
주장의 대상이 아니다.

비교 기준은 다음과 같이 사례에 고정했다. blanket trim·예외 제거·채널 정규화는 없다.

- `exact`: raw 채널과 exception/effects/writes/exec 전체가 같아야 한다.
- `help-boundary`: 순수 `parse_options`는 help 문자열을 출력하지 않으므로 native
  빈 채널·반환 성공과 원본 exit 0/원본 help의 필수 내용·부작용 없음을 각각 요구한다.
  원본 help 전문은 envelope에 그대로 남기며 native help 바이트 동등성은 주장하지 않는다.
- `parser-error-boundary`: 원본의 구체적 오류 suffix/exit 2와 native의 구체적
  `NativeParseError` 메시지를 별도로 고정한다. 원본 usage wrapping과 native scanner
  진단은 서로 다르며 인증/HTTP/exec 미발생 조건은 정확히 비교한다.
- `exception-pair`: missing alias 한 사례에서 원본 `KeyError("'alias'")`와 native
  `NativeError("missing alias")`를 각각 정확히 요구한다. 나머지 출력·exit·효과는
  같아야 한다. 다른 예외를 허용하는 포괄적 예외 비교 규칙은 없다.

fresh 테스트는 항상 원본을 새로 캡처한다. `verify_envelope`는 원본/source/Python에
의존하지 않고 JSON 기대값에서 native 결과를 **다시 계산**한다. group별 기대 사례 수와
중복 ID도 검사하며, SSH의 source root/API host는 input에 명시한다. 따라서 추후 부모가
실제 통과한 원본 데이터를 fixtures로 보존한 뒤 같은 checker를 연결할 수 있다.
현재 fixture loader/동결·원본 삭제는 수행하지 않았다.

Actions에서 `--nocapture`를 사용하면 `DEVELOPER_CLI_ORIGINAL_JSON ` 뒤에 각 group의
완전한 JSON envelope를 출력한다. assertions 실패에는 원본 기대값과 fresh native
결과를 포함한 record를 출력한다. 로컬 기대값 파일이나 암묵적인 output 경로를 만들지 않는다.

### 부모 등록과 검증 대기

부모가 공유 Cargo에 다음 target을 등록한다. 새 feature/dependency/source 등록은 없다.

```toml
[[test]]
name = "developer_cli_original"
required-features = ["developer-ssh-native"]
```

부모 Actions용 전체 selector:

```sh
cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --features developer-ssh-native --test developer_cli_original -- --nocapture
```

위 표의 각 이름으로 단위를 선택할 수도 있다. **3개 테스트 전부 UNRUN**이다.
로컬에서는 Rust 2021 `rustfmt --check`, 새 파일 공백·diff 검토, embedded Python과
원본 두 script의 `ast.parse`만 확인했다. 기능·원본 비교·빌드·Clippy·fixture 캡처와
commit/push/Actions 실행을 하지 않았다. 기존 source/안전 gates를 낮추거나 이 후보를
전체 CLI 교체·원본 retirement 완료 증거로 사용하지 않는다.


## 0e28 원본 추가 실행과 순서 보완

원본 SSH/setup 세 그룹 중 정상 SSH20 및 setup21은 성공했고 parser 그룹의
`--help --h` 기대가 실패했다. 원본은 앞선 help에서 종료했으나 native의 공유 scanner는
뒤의 ambiguity를 먼저 거절했다. 기존 입력을 help 사례로 보존하고 실제 ambiguous-first
`--h --help`를 추가해54사례/3개로 만든다. SSH에서만 명시적으로 ambiguity를 소비 시점에
거절하도록 opt-in하며 기존 scanner 진입점의 eager 정책은 유지한다. 기존 scanner1개와
새 순서/legacy/literal/값 경계1개, 영향 있는 SSH27개를 새로 검증한다.

실제 자식 sys.version3.12.3과 parent 설정3.12.15 차이도 드러났다(#531).
절대 interpreter뿐 아니라 명시된 runtime library를 전달하고 실제 env-clear 자식의
3.12.15·provider mapping 및 전후 hash를 확인한다. 새 provider context의 auth3/로그인8/
SSH54 및 Clippy는 **UNRUN**이다. 이전 성공을 이 context의 통과로 표현하지 않는다.
