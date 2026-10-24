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

위 소스 동결 시점에는 GitHub-hosted 실행이 미실행이었다. 아래 첫 실행의 범위와 실패를 별도로 기록한다. 앞의 bin-actions 성공 또는 실패를 새 GitHub VM 결과로 재표기하지 않는다. whole3580 baseline은292교체/3부분/3285대기 그대로다. 임시 rescue의 폐기만으로 baseline credit을 늘리지 않는다.

GitHub 문서는 표준 Linux label과 job 전용 VM, 환경 변수에 따른 실제 경로 사용을 설명한다. 이 문서의 일반 사양을 실제 job 여유 공간이나 CPU 기능의 실측으로 사용하지 않는다: https://docs.github.com/en/actions/reference/runners/github-hosted-runners


## f8e9d62 기본 러너 첫 실행

source `f8e9d62b073d2c8a66c68f728247b9424df72ce8`을 push했고 실제 jobs API의 ubuntu-24.04/GitHub Actions 이름과 작업 단계를 확인했다. PR343은 head0538007/dev 대상 draft 그대로다.

| 실행 | 실제 범위와 결과 |
| --- | --- |
| [Fast40121818157](https://github.com/bin9208/openpilot-rust/actions/runs/40121818157) | 5 leaf 성공, routing/aggregate 실패. routing은 actual cold bootstrap1 성공 뒤 changed-files5개 중4성공/maintained caller policy1실패다. 준비된 SDK env prefix를 추가한 현재 caller와 고정 RUN fixture가 불일치했다. |
| [docs40121817587](https://github.com/bin9208/openpilot-rust/actions/runs/40121817587) | workflow 성공. 13tests와 guard의 정상 경로2개, source/SDK 전후 봉인 및 소유 정리 성공을 확인했다. negative growth/failure injection을 수용한 것은 아니다. |
| [Focus40121818148/job120954834780](https://github.com/bin9208/openpilot-rust/actions/runs/40121818148/job/120954834780) | 104pin/current source admission, alias26·전체 Unicode scalar/Σ context·영향 SSH27·strict Clippy 및 source/SDK/venv 봉인은 성공. 과거 a134/fb1228 oracle JSON이 새 실제 cache 절대 경로와 달라 두 raw cmp만 실패했다. 버전·libpython SHA·argparse SHA·환경 flag는 실제 artifact에서 동일했다. |
| [setup40121818144/job120954834766](https://github.com/bin9208/openpilot-rust/actions/runs/40121818144/job/120954834766) | private SDK before/after와 source 봉인 성공. growth 지역변수 mib가 단위 상수를 가려 source2GiB를16MiB로 잘못 계산했다(#536). test-build 전 중단이며 setup21/음성3/public1는 미수용이다. |
| [changed-files40121818156/job120954834869](https://github.com/bin9208/openpilot-rust/actions/runs/40121818156/job/120954834869) | 현재 caller5개 중 고정 RUN policy1실패. 이 결과를 원본 부재·현재 caller 전체 수용으로 새로 세지 않는다. |
| [normal40121818149/job120954834759](https://github.com/bin9208/openpilot-rust/actions/runs/40121818149/job/120954834759) | SDK 준비/outer source 봉인 성공, provider admission에서 ldd /usr/bin/gh exit1/not a dynamic executable로 중단(#537). normal40 수치 비교·내부 immutable acceptance는 미실행이다. |

Focus artifact12570879713은15,865,178B/SHA256 `8ff861a569e791f73d0943958a6232267f66efa991c41edbbf7772d0e8a5b880`, 76member CRC 전수 및 source/index·SDK file/roster 전후 일치를 확인했다. setup artifact12570894369는3,588,022B/SHA256 `48535d8ab50728d39f862448783f5ceec2dbb851d017938715c5859d75dd5c24`, normal12570954727은1,717,993B/SHA256 `7a5ebebbba6bec4e4668c439d4ea80780a8e3c05fe2a088d511ce8f5e8ea5e4f`, 각각96member CRC를 확인했다. 실패 artifact를 전체 수용 결과로 바꾸지 않는다.

setup의 실제 image는20261023.334.3, Focus는20261012.334.1이다. setup의 df는 준비 시90,502,328,320B, 명시 unused Android SDK 회수 후102,340,493,312B 여유였다. 일반 runner 사양을 이 실측과 혼동하지 않는다. 타관리 bin-actions 복구 증거도 아니다.

## 첫 실패 보정 후보 — 기능 미실행

- Focus의 역사 JSON8필드 중 정확한4개 cache 경로만 고정 과거 base에서 실제 RUNNER_TOOL_CACHE/Python3.12.15로 대응시키고, 나머지 필드 전체 동일성을 요구한다. raw ZIP/JSON은 그대로 두며 별도 대응표를 기록한다.
- Fast/changed-files 고정 RUN fixture는 승인된 prepared SDK prefix와 정확히 일치시키고 기존8개 음성 변형과 실행 selector를 보존한다.
- #536은 growth 인자 이름만 growth_mib로 바꾼다. source2GiB/proof1GiB/raw16MiB/build8GiB 원래 예산을 복원하며 증가시키지 않는다.
- #537은 held FD의 bounded ELF64/x64 program header로 valid static 실행 파일만 구분한다. executable LOAD/entry/segment 범위와 PT_INTERP/PT_DYNAMIC 부재를 확인해 empty DSO closure를 별도 기록하며, dynamic ELF의 ldd 성공·resolved DSO·전후 hold 검증은 유지한다. ldd 실패 문구를 포괄 허용하지 않는다.

이 후보는 소스/문법 검토 단계이며 새 기능/Clippy/빌드 성공은 아직 없다. Integration5개 workflow 전환은 별도 동결 WIP로 준비했으나 이 첫 실패 보정과 분리한다. required Rust/ARM/Darwin·PR통합·병합 후·전달·전체 인벤토리는 미완료다. baseline292/3/3285, 원본 credit0을 유지한다.
