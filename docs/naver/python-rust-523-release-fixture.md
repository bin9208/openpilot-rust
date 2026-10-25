# #523 Fast release-files fixture 실행 경계 수정

작성: 2026-10-26. 상위 이슈: [#523](https://github.com/bin9208/openpilot-rust/issues/523).
독립 결함 이슈 등록과 커밋·push·GitHub Actions 실행은 부모 담당이다.

## 실패와 소스 기준

- 사용자 제공 실패 source: `3b819ccc5eb788b319d6b150a3288b6635c0e16f`.
- [run 40205298711 / job 121204961892](https://github.com/bin9208/openpilot-rust/actions/runs/40205298711/job/121204961892).
- 실패 테스트: `release_files::shell_bootstrap_reserves_before_build_and_keeps_stdout_as_only_file_names`.
- 실패 지점: 당시 `tests/release_files.rs:110`의 `.output().unwrap()`에서
  `Os { code: 26, kind: ExecutableFileBusy, ... }` / `Text file busy`. 다른 두 테스트는 사용자 보고상 PASS.
- 수정 기준 로컬 HEAD: `5c1b1bfe8ececc2a71401c92925e9ef865ff7409`.
  수정 전 테스트 파일과 생산 `release/release_files`는 위 실패 SHA와 동일하다.

fixture는 `fs::write`가 반환한 뒤 실행한다. 추가 fork/쓰기 핸들의 상속 등 상세 원인은
입증되지 않았다. 이번 수정은 작성 직후 bootstrap 파일을 직접 exec하는 의존만 없애며,
모든 `ETXTBSY`의 원인 규명이나 재발 방지 완료로 주장하지 않는다.

## 제한 변경과 보존한 검증

실제 생산 원문의 첫 줄은 `#!/usr/bin/env bash`다. `BASH_SOURCE[0]`, `pipefail`을
사용하는 Bash bootstrap을 그대로 fixture에 복사한다. 테스트의 직접 호출 두 곳만
`Command::new("/bin/bash").arg(&bootstrap)`로 변경한다. 문자열 명령 조합이나 `-c` 없이
원래 경로를 단일 인자로 전달하므로 bootstrap의 `BASH_SOURCE[0]`, `$0`, 후속 `$@`
해석에 사용하는 script 경로를 유지한다.

다음 테스트 본문·설정·assertion은 그대로 둔다.

- guard가 cargo보다 먼저 호출되는 `guard\ncargo\n` 순서.
- stdout의 `README` 경로와 guard JSON 미포함, 실패 시 빈 stdout.
- guard의 exit 42, cargo의 exit 43 및 각각의 호출 trace.
- 소스 트리 내부 cache 거부와 guard 미실행 확인.
- cross-target/flags/wrapper 환경 주입과 fake cargo의 환경 정리 검사.
- host target/target-dir을 읽어 동일 binary 경로를 만드는 fake cargo, toolchain 인자,
  실제 release-files binary로의 `--root` 전달과 파일 선택 검증.
- 나머지 두 테스트의 원본 regex/심볼릭 링크/FIFO 선택 및 없는/일반 파일 root 처리.

이 테스트 파일에는 kernel 직접 exec나 shebang 선택 자체를 확인하는 별도 assertion이
없다. bootstrap의 직접 exec가 암묵적으로 제공하던 shebang/실행 권한 검증은 이제 이
테스트의 증거가 아니다. 기존 chmod 설정과 guard/cargo/생성 binary의 실행 경로는
유지한다. 생산 wrapper, guard, Cargo 실행 방식과 release 동작은 수정하지 않는다.
기존 cross-target 오염 검사는 유지하지만 새 host/플랫폼 실행 증거를 추가하지 않는다.

## 검증 및 인계

로컬 결과: `rustfmt --check --edition 2021`과 추적/새 파일 diff 검사 성공.
두 호출 치환만 되돌린 테스트 파일 전체가 실패 SHA 원문과 바이트 단위로 같다는
역투영 검사도 성공했다. 따라서 기존 assertion·fixture·환경·인자 설정은 모두 불변이다.
생산 wrapper가 실패/수정 기준 SHA와 같고 Bash 헤더인 것도 소스 비교로 확인했다.
Cargo/테스트/빌드/Clippy와 bootstrap 실행은 로컬에서 하지 않았다.

부모가 정확한 통합 SHA의 GitHub Actions에서 실행할 selector:

```sh
cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --test release_files
```

세 테스트의 실행 결과와 통합 SHA/Actions URL은 부모가 기록한다. 재시도·skip·기한 연장이나
새 기능·플랫폼 성공 주장은 추가하지 않는다. 수정 파일은 이 기록과
`rust/crates/dev-tools/tests/release_files.rs` 두 개뿐이다.
