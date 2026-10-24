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
