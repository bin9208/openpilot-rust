# #523 GitHub 기본 러너 전환 — #535

2026-10-25 사용자: bin-actions는 다른 관리자가 만든 현재 사용할 수 없는 러너이므로 GitHub Actions 기본 러너로 진행한다. 이는 이전 local-only 선택보다 우선한다. #523/#296의 dev 트랙을 유지하고 dev-md 런타임은 섞지 않는다.

이슈: https://github.com/bin9208/openpilot-rust/issues/535
연결: #523/#527/#296/PR343. 로컬은 소스/문법/서식/diff·inert 원문 확인만 수행하며 기능/빌드/Clippy는 Actions에서 실행한다.

## 기존 러너의 작업 중단

source ba9528d05의 미시작 rescue40118006504/job120943271254는 queued/steps0을 재확인하고 취소했으며 실제 completed/cancelled를 확인했다. 타관리 러너의 실제 삭제/복구 실행은0이다. 임시 rescue WF를 현재 코드에서 제거하고 execution-edge에서 removed로 구분했다. 원본 Python의 Rust 교체나 baseline credit으로 세지 않는다. 자기 issue branch의 미시작 Fast b7/ba도 검사 후 취소했으며, b7 run40116974809와 ba run40118006474의 completed/cancelled를 각각 확인했다. 다른 세션의 실행 중인 작업은 종료하지 않았다.

## 현재 소스 후보

- 집중 Linux x64는 명시적인 ubuntu-24.04/github-hosted를 사용한다. RUNNER_NAME이나 /home/actions 경로를 위장하지 않는다. 기존 private source/job, 신뢰 source, 원본 입력과 owner/ELF/ABI·소유 child/전후 봉인 조건을 보존한다.
- private job/rustup-home에 Rust1.94.0 minimal/rustfmt/clippy를 준비한다. official Rustup1.29.1 다운로드의 기존 고정 SHA를 확인하고, 기존 cargo/rustc/rustfmt/clippy-driver4개 ELF pin을 그대로 검증한다. SDK 전체 파일/roster를 before/after 봉인한다. 부분 SDK나 해시 불일치는 수리/완화로 통과시키지 않는다.
- SDK 설치 예상 증가량3GiB를 별도 admit한다. source/build의 기존 자원 admission을 대신하지 않는다. 명시적으로 Android를 사용하지 않는 focused/normal/setup/docs/Fast native job에서만 불필요한 preinstalled Android SDK 한 경로를 job 전용 GitHub VM에서 회수하며 df/inode/du 전후를 기록한다. Android를 빌드하는 다른 job에는 적용하지 않는다. 실제 여유·runner image·CPU·OS는 각 Actions 실행에서 새로 관측해야 한다.
- 과거 Focus artifact의 /home/actions tool manifest는 원문 그대로 유지한다. 과거 정확4개 경로·roster·해시를 읽고 현재 private SDK의 같은4개 component bytes와 대조한 별도 매핑을 남긴다. 과거 source/provider 경로를 현재 경로로 덮어쓰지 않는다.
- controls normal40·setup21 및 공개 무인자 entry·docs guard의 기존 기능 범위/수치/원문을 유지한다. fixed source pin은 새 실제 소스로 갱신하고 변경 소스를 과거 성공 cohort의 실행 증거로 바꾸지 않는다.
- Fast6개 leaf와 aggregate를 ubuntu-24.04로 전환하고, CI 정책 검증기는 명시 hosted label만 추가 허용한다. 기존 bootstrap 음성 fixture6개는 synthetic legacy branch를 명시해 실제 hosted 증거와 분리한다. source 준비 caller는 current SDK의 절대 Cargo를 실행한다. changed-files의 원본2부재/5cases와 과거 독립 codec/lifecycle data admission도 실제 경로로 전환한다. metadata Python3.12.15의 lib pin과 과거 GNU provider 동등성은 계속 요구한다. 새 image에서 bytes가 달라지면 실패를 기록하고 영향받은 현재 provider 경계를 별도 검증해야 하며, 경로 전환만으로 과거 증거를 확대하지 않는다.
- 필수 Rust/ARM/integration/docs와 Darwin/APFS 플랫폼 요구는 유지한다. 기본 러너 전환만으로 필수 검사나 전체 #523을 완료하지 않는다.

## 검증 상태

소스 동결 뒤 YAML6개·내부 Bash128개·단독 Bash5개·embedded Python AST8개와 helper Python AST1개, Rust workspace 서식과 diff를 확인했다. Focus의 literal pin104개와 검토한 실행 영역의 별도 SHA가 현재 bytes와 일치한다. 이번 push로 실행되는 모든 job은 명시 ubuntu-24.04로 확인했다. 이는 로컬 소스/문법 검사이며 기능 검증은 아니다.

현재는 전환 소스 작성/검토 단계다. GitHub-hosted 실행·새 기능/빌드·Clippy·전후 봉인·자원 회수는 아직 미수용이다. 앞의 bin-actions 성공 또는 실패를 새 GitHub VM 결과로 재표기하지 않는다. whole3580 baseline은292교체/3부분/3285대기 그대로다. 임시 rescue의 폐기만으로 baseline credit을 늘리지 않는다.

GitHub 문서는 표준 Linux label과 job 전용 VM, 환경 변수에 따른 실제 경로 사용을 설명한다. 이 문서의 일반 사양을 실제 job 여유 공간이나 CPU 기능의 실측으로 사용하지 않는다: https://docs.github.com/en/actions/reference/runners/github-hosted-runners
