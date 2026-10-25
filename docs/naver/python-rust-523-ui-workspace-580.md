# #580 UI CI private workspace 경로 검사

이슈: https://github.com/bin9208/openpilot-rust/issues/580. 기존 PR343의 source head9ae67e412eeb8d9c4b3e19ee9f3b586fe2855739, 실제 merge75d0cfd49912b631946721e58979487935372828의 [Rust workspace](https://github.com/bin9208/openpilot-rust/actions/runs/40187340482/job/121150827072)는 pytest239PASS/1FAIL/9subtests다. 실제 로그1229350B/SHA256 `a2ff88d587abf489bb630c49dc56f9aa1122d3e82876b866716f025900e6b1d5`를 한 번 내려받아 최초 실패를 확인했다.

`test_arm_ui_commands_resolve_workspace_and_python_paths`가 `${{ env.CI296_SOURCE }}/rust`를 ROOT에 상대 경로로 붙였다. 실제 ARM workflow의 owned private source 작업 디렉터리를 바꾸지 않고, 정적 검사에서 해당 표현식을 정확히 요구한 뒤 현재 검사 checkout의 ROOT/rust를 읽는다. 기존 script 존재·diskguard4개 argv/증가량·절대 실행 경로·Cargo ui-application member 검사는 모두 유지한다. 범용 템플릿 해석이나 환경변수 실행을 추가하지 않는다.

Fast native-tools의 기존 pytest9.0.2/PyYAML6.0.2 환경에 이 selector1개를 연결한다. 다른 Fast job/원래 Cargo selector/순서·source/SDK after·업로드/cleanup은 불변이다. source 파일 수정·동적 import·실제 ARM 실행을 이 단일 정적 pytest로 대체하지 않는다. 이미 존재하는 Python 검사 의존성은 원장에 pending으로 기록하며 baseline3580의 제거 credit0이다.

로컬 검사는 AST·YAML·Bash-n·diff만 한다. 실제 selector·Fast·전체 PR workspace 통합은 **GHA UNRUN**이다. 이전 workspace의 전체 기능을 PASS로 바꾸지 않으며 exact-source 결과가 필요하다.
