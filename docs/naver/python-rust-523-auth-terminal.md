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
