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


## 19e180650 보정 후 실제 결과

source `19e180650bae8b46f9d04056b796ba68666b364e`에서 [Focus40122888950/job120958068576](https://github.com/bin9208/openpilot-rust/actions/runs/40122888950/job/120958068576)와 [changed-files40122889010/job120958069116](https://github.com/bin9208/openpilot-rust/actions/runs/40122889010/job/120958069116)가 성공했다. Focus의 두 역사 JSON은 각각 정확4개 경로 대응과 나머지 버전/provider/stdlib bytes 동일성으로 수용됐고, 새 alias/Unicode/SSH27·Clippy·봉인과 소유 정리가 성공했다. changed-files는 원래5tests·strict Clippy와 current2원본 부재/held/source-data 조건을 수용했다. 새 baseline 교체 credit은 추가하지 않는다.

Focus artifact12572356676은15,867,375B/SHA256 `59712bbca4a6bb1e8ab3d4aaa81a1e38668c8665526ce45b4a2d5d1af3e32ac0`,81member CRC 확인. changed-files12572146266은51,782,770B/SHA256 `eda43811fd935852d8fdfe776e148fd667a5a60ab78e6f3a691274f981c80574`,166member CRC와5개 성공 receipt를 확인했다. 두 산출물 source/index·whole SDK file/roster 전후는 각각 byte-identical이다.

[Fast40122889061](https://github.com/bin9208/openpilot-rust/actions/runs/40122889061)은5leaf성공/본 routing·aggregate실패다. actual cold bootstrap1·caller5·여러 기존 regression 뒤 ci_workflow_policy에서23성공/3실패가 나왔다. docs의 강화된 SDK/recovery cleanup 조건을 exact validator가 아직 등록하지 않아 거부한 것이며 cleanup guard를 삭제해서 통과시키지 않는다. 해당 validator 등록과 guard 누락을 거부하는 음성 검증을 후속 소스에 추가했다. 후속 컴파일/기능은 아직 미실행이다.

[normal40122889070/job120958069525](https://github.com/bin9208/openpilot-rust/actions/runs/40122889070/job/120958069525)는 static gh 수용을 통과했다. 실제 ET_EXEC/x64/program headers6/no PT_INTERP/no PT_DYNAMIC receipt를 확인했다. 그 뒤 고정 역사 로그 API가 최신 gh의 escape 출력 정책에 막혀 #538로 추적한다. 고정 raw 로그를 private 파일에 수집할 때만 공식 --allow-escape-sequences를 명시하는 보정을 준비하고, 원문 LOG_SIZE/LOG_SHA를 유지한다. artifact12572146888은14,218,679B/SHA256 `a6d24f9ba88ee0fffaa1dcedc78ae4fae5d6cd7ea9fff6bfe5917fe335fd28d3`,152member CRC 확인. normal40 수치 비교는 여전히 미실행이다.

[setup40122889053/job120958069286](https://github.com/bin9208/openpilot-rust/actions/runs/40122889053/job/120958069286)는 #536의 예산 계산을 지나 frozen21·음성3·Clippy·public1를 실제로 실행했다. child/monitor 상태와 공개 무인자 stdout/exit1은 기대와 일치했다. 그러나 strace의 SIGCHLD CPU-time 주석 미지원(#539)과 공개 helper가 logging 종속물을 재빌드해 preheld CLI identity를 바꾼 문제(#540) 때문에 immutable/exec 전체 수용은 실패다. 원본/공개 CLI 전체 완료로 세지 않는다. artifact12571688781은10,575,291B/SHA256 `fb6e039142b4f7dcc2378415d21d04dd8f7969fae5d7ed43a01cc71aebf4c964`,147member CRC 확인. 재빌드의 최초 invalidation 원인은 아직 미확정이며 현재 identity pin을 유지한다.

후속 Integration/Rust Linux·QEMU·실제 Mac 전환도 작성 중이다. Mac DBC는 같은 소스의 reusable workflow로 분리해 main Rust의500000B 등록 제한을 지키고, 기존41dependency와16개 현재 Mac selector를 유지한다. 재출력된 main의42개 non-Mac job model과 run 문자열은 앞서 검토한 Linux 후보와 동일하다. 새 Mac/APFS 실행·후속 정책 등록·실제 ARM transport는 아직 미수용이다. 현재 실제 성공을 이 미실행 소스의 성공으로 확대하지 않는다.


## 필수 검사 전환과 다음 실행 후보

Integration5개 workflow를 명시 Ubuntu24.04로 전환했다. 네 required dependency와 all-success aggregate, Java/Android SDK·Gradle·native 기능 명령을 보존했다. Rust main의42개 non-Mac job은 Linux 전환 동결 model/run 문자열을 보존한다. source/owner/ABI/provider·정확 source/index 봉인과 업로드 뒤 소유 정리 조건을 유지하며, QEMU가 없는 실제 x64 VM에서는 별도1GiB APT admission 후 준비한다. guest에서 host RUSTC/RUSTDOC/prepared SDK와 host PATH가 섞이지 않게 분리하고 PRoot/rootfs/Python 원본 pin·physical_arm_cpu=false를 유지한다. 역사4-file 계약은 그대로 두고 local-arm1개 검토 postimage만 명시적인 #535 overlay로 등록했다. 이를 새 ARM 기능 성공으로 세지 않는다.

실제 Mac DBC는 `rust-dbc-darwin.yml`의 workflow_call1job으로 분리했다. main은 동일 source 상대 경로로 필수 호출하고 기존41needs/DBC 성공 요구를 유지한다. main496865B, callee25295B로 각각500000B 등록 제한을 지킨다. scalar alias를 보존한 main 재출력 전후의42개 non-Mac model과 run 문자열은 동일하다. Mac은 실제 macOS15/ARM64/APFS를 요구하고 준비된 GNU 도구/Apple SDK·private Rust1.94의 실제 버전·Mach-O·before/after를 검증한다. Linux ELF pin이나 Linux 성공을 대신 쓰지 않는다. 기존 selector의 현재16tests(11+5)를 그대로 실행하며 외부 SDK symlink target/OS dyld cache 전체 봉인으로 확대하지 않는다.

정책 검증기는 Mac의 같은 source callee와 실제 검사 본문을 해석하고, fork 거부 바로 뒤의7개 연속 실제 platform guard를 요구한다. 주석 대체·Linux fallback·재귀 caller를 거부한다. docs의 두 명령은 source seal 전에 와야 하며 SDK/소유 복구 결과를 cleanup의 필수 조건으로 등록했다. Mac도 SDK seal 성공 전에 cleanup하지 않는다. 누락/이동/주석 대체/guard 변경 음성 검증을 추가했다.

setup #539는 관측된 숫자 CPU-time 주석만 수용하고, #540은 prebuild/public에만 Cargo fingerprint info 및 실제 Git input의 부재/type/identity/mtime를 기록하도록 했다. identity 교체 거부와 원래 예산·21+3/public1를 유지한다. packed-refs 부재는 현재 원인 후보이며 아직 확정하지 않는다. normal #538의 고정 로그 API에는 공식 escape 보존 flag1개만 추가한다. raw 로그를 private 파일로 수집해 기존 size/SHA로 검증하며 화면 렌더링/ANSI 변환을 하지 않는다.

독립 소스 검토 지적을 반영했고, 로컬 YAML9개·내부 Bash854개·embedded AST8개·helper AST1개·단독 ARM Bash 문법·workspace Rust 서식과 diff를 확인했다. Focus의 exact 허용134경로/literal115pin이 현재 검토 bytes와 일치한다. 실행 영역의 별도 SHA는4ee29d23ba43e6824f19a550898b5678db549a0d283bace60f177f9713a9ef89다. 이 후보의 컴파일/기능/실제 hosted Mac·ARM·필수 PR 결과는 아직 없다. 새 Actions에서 확인할 대상으로 기록한다.


## bdb819e59의 기본 러너 실제 결과와 원인별 보정

PR343을 source `bdb819e59534ffba7c61751653ddaf360e8e324a`로 force 없이 fast-forward했다. dev 대상 draft이며 실제 PR merge source는 `ba1b07c38702bac3bb49614d14b54adaacaa95df`다. push source와 구분한다. [Rust40126297037](https://github.com/bin9208/openpilot-rust/actions/runs/40126297037)의41개 필수 의존성과 실제 Linux/x64·private ARM QEMU·macOS15/ARM64 경계를 유지한다. 아래 결과는 이 source 범위의 실제 관측이며 미완료 worker를 전체 PASS로 세지 않는다.

- [Focus40126286930/job120968310136](https://github.com/bin9208/openpilot-rust/actions/runs/40126286930/job/120968310136) 성공: alias26·전체 scalar/Σ context·SSH27·strict Clippy·source/index/whole SDK 봉인. artifact12573742381은15,867,483B/SHA256 `ea25319a41cf7f2bff5278ee9eefb3a6ae3f2de683f96d15ece3a7d1a463498e`,81CRC와 전후4쌍을 확인했다.
- [docs40126295962/job120968337170](https://github.com/bin9208/openpilot-rust/actions/runs/40126295962/job/120968337170) 성공:13tests·guard 정상2경로·source/SDK·upload/cleanup. actual mapped CLI는 base 대비7,528paths를 확인했다. Fast6+docs1 artifact 전체285CRC를 감사했고 각source/index·SDK 전후는 동일하다. docs artifact12573662207은328,479B/SHA256 `08447f6cefbe68720626c770f1065030e374b98097cdf5d3ab73eff4edc9873a`다. guard 실패 주입의 수용은 아니다.
- [Fast40126296032](https://github.com/bin9208/openpilot-rust/actions/runs/40126296032)는5leaf 성공/routing·aggregate 실패다. 실제 cold1/caller5/lib83/athena2 성공 뒤 beep integer workflow parse가 unknown anchor byte6149에서 실패했다. camera/model-family/model-pipelines/encoder/UI/lagd에도 같은 parser 원인이 관측됐다(#541).
- [setup40126286865/job120968309889](https://github.com/bin9208/openpilot-rust/actions/runs/40126286865/job/120968309889)는 frozen21/음성3/Clippy 및 public stdout·exit1이 일치했다. #539의 SIGCHLD 숫자 CPU-time 주석15개가 실제 parser를 통과했다. #540은 Cargo가 `stale: missing .../.git/packed-refs`를 최초 재build 원인으로 명시했다. HEAD/index/current-ref identity·mtime는 그대로였다. public 재link가 held CLI를 교체해 finalizer가 거부했다. artifact12573042824는10,585,156B/SHA256 `faa165516e0ef1a43e4aadf5f792f1e105600e45c2017aba0243541745c27168`,151CRC/14전후봉인쌍을 확인했다. 전체 trace/immutable 수용과 원본 삭제는 아니다.
- [normal40126286876/job120968310111](https://github.com/bin9208/openpilot-rust/actions/runs/40126286876/job/120968310111)는 고정 역사raw4·current build/Clippy 이후 numerics staging roster에서 중단됐다. 고정 wheel의 `numpy.libs/`0바이트 directory를 기존 stager가 `numpy.libs`0바이트 일반파일/manifest member로 보존하지만 임시 qualification helper가 directory를 모두 건너뛰어 extra1/missing0이 됐다(#543). artifact12573753108은45,528,966B/SHA256 `cce5ca9405f3bb16ffebe1f76bc144685fef858bdbc2fb4eb619ef1ab6be9f24`,179CRC를 확인했다. normal40 수치 비교는 미도달이다. finalizer의 numerics/native baseline unavailable은 미도달 경계를 실패로 보존한 것이며 source/SDK 변경 성공을 뜻하지 않는다.
- Mac120968344635는 실제 Darwin/ARM64 guard와GNU8종 성공 뒤 Rustup admission 실패다(#544). image provider가 없었는지 sibling cargo 검사 실패인지는 기존 silent 로그로 확정하지 않는다. 실제 image의 Homebrew formula는 wrapper/libexec 배치를 사용하므로 Mach-O/sibling cargo 가정이 보장되지 않는다. private SDK/build/DBC16tests는 미도달이다.
- [Integration40126295997](https://github.com/bin9208/openpilot-rust/actions/runs/40126295997)은14성공/2실패다. release build/모델변환/카메라/pytest570은 성공했고 최종 source tar 봉인과 파생 gate가 실패했다. baseline(19:39:49UTC) 뒤 setup의 LFS pull(19:40:49–58)이 관측돼 pointer를 payload로 바꾸는 순서 문제가 확정됐다(#546). exact6개/83,615,137B의 attr-selected LFS roster를 source-before 이전에 검증하는 보정을 준비한다. 모든 source 차이가 이6개만이었다는 근거는 아직 없다.

추가 독립 실패는 #542(현재 logical matrix/보호된 upload 조건과 예전 assertion), #545(Panda venv 실행 경로 canonicalize로 base Python이 실행돼 usb1 누락), #547(updater trace 정책의 `/tmp/updated-source-` 고정 prefix가 owned private TMPDIR를 거부)에서 추적한다. updater artifact12573793106은1,618,914B/SHA256 `9959481e92e1c90b59c501bf1dc6a0651898574f4ca6055557cbdec17c0e81e3`,187CRC를 확인했고 실제 git-upload-pack argv의 private root를 확인했다. 원본5개 비교는 성공했지만 checkout/absence 전체 trace 수용은 실패다.

후속 소스는 원인별로 최소 보완한다. main YAML은 실제 AliasToken472개와 colon 사이에만 공백을 넣어 전체 parsed model/run bytes를 그대로 유지한다(497,337B/41needs). 텍스트 배치에 의존한 stats/soundd/emoji 검사는 실제job/step을 파싱하면서 기존 assertions를 보존한다. setup은 source/build/hold 전에 `git pack-refs --all --no-prune`과 HEAD/index/ref roster 불변을 요구해 정상 metadata를 준비한다. normal은 정확한 빈 marker를 검증·보존하고 실제3DSO만 ldd하며, 이미 관측된 CPU 주석 문법만 trace metadata에 등록한다. Mac은 image provider를 실행하지 않고 진단한 뒤 공식 Darwin Rustup1.29.1을 owned private homes에 별도512MiB admission/16MiB download 상한 아래 설치한다. installer/proxy 전체 identity·SHA 전후를 기존 SDK seal에 연결하고1.94 manifest·16selectors·cleanup을 유지한다.

위 후속 소스의 기능/빌드/Clippy는 Actions 수용 전 UNRUN이다. source 봉인/FD/DSO/provider·숫자·원문 expected·예산을 완화하지 않는다. 이번 추가 retirement credit0, baseline3580의292교체/3부분/3285대기와 전체 필수 PR·병합 후·전달·실기 미완료 상태를 유지한다.


후속 배치의 로컬 source 확인에서는 YAML5개/내부Bash774개/embedded AST8개/별도Python AST4개와 diff를 확인했다. main YAML의748개 run 문자열과 전체 모델은 구분자 보정 전후 동일하다. Focus는 exact142허용경로/123literalpin, 실행 영역 SHA `4ee29d23ba43e6824f19a550898b5678db549a0d283bace60f177f9713a9ef89`를 유지한다. execution-edge 원장은 기존92행의 현재 blob 식별자만 갱신하고25교체/63대기/1native-existing/2nonexecuting-text/1removed를 유지한다. 이 기록은 로컬 기능/빌드 성공이 아니며 원본 credit을 늘리지 않는다.


추가 종료 관측에서 Xiaoge memory120968344543은4Miri모드와ASan/UBSan9개가 성공한 뒤 모델 LFS hydration으로 source-final이 실패했다. workspace120968344589도 공통 YAML 오류와 LFS 후 source-final 실패를 보존했다. #546의 같은 순서 보정을 Rust5개 job key에만 확장한다. driving1 또는xiaoge2의 exact source OID/size를 기존 baseline 전에 검증할 source-only Bash bootstrap을 준비하며 기존 모델 다운로드·기능 명령은 유지한다.

CarrotMan ARM120968344415와 Carrot Navi ARM120968344549의 cancelled는 부모 취소가 아니다. 두 Actions annotation이 기존25분 maximum job execution 초과를 명시했다(#548). 이2개의 전체 job 한도만60분으로 보완하고 기능별 deadline·QEMU/ABI·메모리/디스크·source/provider/수치 기준은 유지한다. 기능 검사 도달·수용은 아직 미완료다. startup UTF7의 같은 고정 block split 실패도 #541의 parsed-job 보완에 포함한다.


최종 source 후보에서 Rust main은497,973B다. source baseline 앞 LFS 준비5개 단계와 ARM job 한도2개를 제외하면 이전 전체 job model/748개 기존run bytes가 동일하다. CarrotMan은 matrix.arch==aarch64에만60분을 적용하고 x86_64의25분은 유지한다. Carrot Navi ARM은60분이다. source-only helper `tools/ci/prepare-lfs-source-535`는exact3model 중 필요한1/2개만 선택하며, 실제 Git/LFS FD·identity·SHA/전용cache/source-index·나머지tracked파일 전후·원래2GiB source cap과bounded hydration을 요구한다. main의 same-source literal SHA 검증 뒤 실행되며 기존 Git LFS 및 기능 명령은 유지한다. 새 compiler·Python loader·모델 expected 생성은 없다.

Focus의 최종등록은exact144경로/125literalpin이며 실행영역SHA는불변이다. registration UTF7의같은text-split을파싱한startupjob으로교정해기존assert3개를보존했다. 이추가후보도Actions 실제수용전UNRUN이며앞선성공에소급하지않는다.

## ded6051b3 실제 결과와 소스 감시 보정

push source는 `ded6051b33f51ffa3aa4b23e76e51c2eb4804d4b`다. PR343은 아직
`bdb819e59534ffba7c61751653ddaf360e8e324a` head의 dev 대상 draft이며, 두 source의
검사 결과를 구분한다.

- [Focus40129042247/job120976604102](https://github.com/bin9208/openpilot-rust/actions/runs/40129042247/job/120976604102)는 성공했다. alias26·Unicode7의 전체 scalar/Σ 비교·SSH27·strict Clippy·125개 literal pin·source/index/SDK 봉인과 업로드/소유 정리를 확인했다. artifact12574067660은15,867,566B/SHA256 `083ed74ca0886aed27e9a8daa57323ca3d8d100977f126e76252bb359cb4414c`이며81개 CRC와 전후4쌍을 감사했다.
- [Fast40129042267](https://github.com/bin9208/openpilot-rust/actions/runs/40129042267)은5개 leaf 성공, native routing과 aggregate 실패다. 실제 cold1·caller5·lib83·beep2·disk bootstrap31은 성공했고 disk의 기존 ignored1은 별도다. YAML anchor 오류는 통과했으나 `ci_rust_isolation`이81성공/2실패였다. 정책 검사 `mandatory(job, &[""])`가 조건 없이 fork를 거부해야 하는 첫 단계에도 도메인 조건을 요구했다(#542). 정상 첫 경계는 기존 `validate_local_routing`으로 검증하고 이후 도메인 단계에는 종전 조건을 유지하도록 수정한다. 첫 단계 건너뛰기·신뢰 변수 제거·주석 대체·도메인 조건 제거를 거부하는 회귀1개를 추가했다. 이 수정의 기능 검증은 아직 미실행이다.
- [setup40129042246/job120976604614](https://github.com/bin9208/openpilot-rust/actions/runs/40129042246/job/120976604614)은 packed-refs 준비·frozen21·음성3·strict Clippy·public stdout/exit1 비교를 통과했으나 public 호출의 CLI 교체를 올바르게 거부했다(#540). 직접 원인은 `logging/build.rs`가 실제로 대상이 없는 tracked symlink `openpilot/system/hardware/comma/agnos.json`을 `rerun-if-changed`에 등록한 것이다. Cargo는 missing 입력을 매번 dirty로 판단해 logging과 의존 CLI를 다시 빌드했다. artifact12574897637은10,566,663B/SHA256 `94b884bfbc0981bd81fe74a9e39af20b8387acca979b4f37b497ca89c9778083`이며166개 CRC와 전후14쌍을 감사했다. 전체 trace/held identity 수용과 원본 삭제는 아니다.
- normal40129042248은 이 기록 시점에 진행 중이다. 앞선 빈 marker 수정 이후의 수치40회/8,655frames/705,948comparisons 수용을 미리 주장하지 않는다.

빌드 메타데이터 보정은 missing 파일 자체 대신 존재하는 부모를 감시하고, file symlink의
부모와 target도 감시한다. 존재하는 directory symlink는 원래 경로를 유지한다. 실제 root
directory symlink5개와 in-tree Cargo output 조합을 포함한 회귀9개를 추가했다. 감시 대상이
Cargo OUT_DIR를 포함하면 실패하며, 순환/과도한 chain도 제한한다. Git 조회에는
`--no-optional-locks`를 적용해 읽기 전용 관측의 optional index refresh를 피한다.
commit/tree 계산과 source identity 정책은 유지한다. Cargo0.95.0의
[missing 입력 검사](https://github.com/rust-lang/cargo/blob/0.95.0/src/cargo/core/compiler/fingerprint/mod.rs)와
[directory/symlink mtime 처리](https://github.com/rust-lang/cargo/blob/0.95.0/crates/cargo-util/src/paths.rs)를
대조했으며, 실제 두 번째 Cargo 호출의 fresh 여부는 Actions에서 확인할 대상이다.

setup의 임시 pack-refs 준비 단계는 제거한다. 새 감시 소스2개를 FD로 묶고, 최종 CLI를
hold하기 전에 회귀9개/ignored0의 정확한 summary1개와 strict Clippy를 기존 bounded
build 단계 안에서 요구한다. 원래 frozen21/음성3/public1·ELF/DSO/SDK 봉인과 미등록
linker 거부는 유지한다. 가짜 agnos 파일 생성이나 교체된 CLI 재수용은 없다.

이전 bdb ARM job7개의 cancelled는 Actions annotation에 기록된 전체 job 시간 초과다.
사용자/부모 취소가 아니다. CarrotMan/CarrotNavi의 ARM60분 후보에 이어 실제로 초과한
planner/WebRTC/radard/webcam/RadarCAN의 ARM job 한도만90분으로 조정한다(#548).
x86_64 한도와753개 run 본문·기능별 deadline·QEMU/ABI/메모리/수치 기준은 유지한다.

후속 소스의 Focus는 exact146경로/127literal pin이며 별도 실행 영역 SHA
`4ee29d23ba43e6824f19a550898b5678db549a0d283bace60f177f9713a9ef89`를 유지한다.
실행 경계 원장은92행의 현재 blob만 갱신해25교체/63대기/1native-existing/
2non-executing-text/1removed를 유지했다. 독립 소스 검토에서 P1/P2는 추가 발견되지
않았으나 새 회귀/Clippy/현재 setup의 기능 수용은 Actions 전까지 UNRUN이다.
baseline3580의292교체/3부분/3285대기와 이번 원본 제거 credit0을 유지한다.

## 96a904e6d 실제 수용과 다음 한정 후보

[Focus40131329248](https://github.com/bin9208/openpilot-rust/actions/runs/40131329248)은
source `96a904e6da44151785eb1206963fa6ecea019e31`에서 성공했다. artifact12575446425는
15,867,310B/SHA256 `b7a876eada164cc0dfaf090006ea9e913b18b6d1307da2e2f4acd675ebd8d500`이며
81CRC와 source/index/SDK 전후4쌍을 감사했다. alias26·Unicode7·SSH27·Clippy와127pin의
실제 수용 범위다. 아래 새 source의 성공을 소급하지 않는다.

[setup40131329251/job120983468155](https://github.com/bin9208/openpilot-rust/actions/runs/40131329251/job/120983468155)은
같은 source에서 검증·봉인·upload·두 cleanup까지 모두 성공했다. logging watch9/ignored0와
strict Clippy, frozen21/음성3/public 무인자1의 exact stdout/exit1이 일치했다. 공개 호출은
2.10초의 fresh Cargo 정보 조회였으며 compile/link를 실행하지 않았다. CLI의 inode/mtime/
ctime/SHA가 hold 뒤 유지됐고122개 held input/provider도 두 번 같은 값으로 확인됐다.
missing packed-refs는 실제로 계속 없는 상태였다. artifact12575221909는10,464,638B/SHA256
`35a3f8d7c6bb968b4b101876e98188629ca644df6f80410ddc3d6f577def042f`,167CRC·14봉인쌍이
일치했다. #536의 예산 단위, #539의 CPU 주석, #540의 재link 결함에 대한 개별 해결 근거다.

별도 원문 감사에서는 public exec20개가 전부 성공했는데 기존 summary는16성공으로
집계한 공백 파싱 문제를 발견했다(#551). `)             = 0`의 정렬 공백이 기존 결과
분리를 통과하지 못한 것이다. 네 provider 모두 실제 held 목록과 전후 SHA가 일치함을
별도로 확인했지만, 자동 성공 분기 누락은 수정한다. 이미 검증한 전체 exec grammar를
유지하면서 마지막 ` = ` 뒤 결과를 분리하도록 한정 보완한다. 새 parser의 Actions 수용은
아직 미실행이며 #540의 inode 교체 해결과 구분한다.

[Fast40131329250](https://github.com/bin9208/openpilot-rust/actions/runs/40131329250)은4leaf 성공,
native 실패·alert-manager 시간 초과로 aggregate 실패다. native의 isolation84와 policy29는
모두 성공했다. 다음 cutin 검사는 현재 hosted job을 예전 LOCAL_RUNNER와 비교해1성공/2실패였다
(#542). Fast가 선택하는73target을 소스로 조사해 같은 문제의 cutin/ratekeeper/encoder3개를
현재 정확한 HOSTED_RUNNER로 보완하고 legacy·임의 label·matrix 거부를 유지했다. encoder의
첫 경계는 전체 routing 검사 후 도메인 조건과 구분했다. 실제 cutin 실패와 아직 실행되지
않은 ratekeeper/encoder의 예측된 결함을 구분한다. artifact6개231CRC 및 전후 source/SDK와
실패·취소 뒤 upload/cleanup을 확인했다. alert-manager는 checkout153초를 포함해 기존7분
전체 한도를 초과했으며, 전체 corpus4성공 summary는 없다. 7분 한도는 유지하고 다음
현재 source 실행을 확인한다.

## normal40 실제 실행의 새 시간 제한 실패 — #550

[normal40129042248/job120976604351](https://github.com/bin9208/openpilot-rust/actions/runs/40129042248/job/120976604351)은
ded source에서 raw4·build/Clippy·정확한 numerics marker/3DSO·음성2를 통과한 뒤 실제
fixture의700초 제한에서 실패했다. artifact12575287289는52,138,342B/SHA256
`871c5e190a923f7f800f823f9d0ea8192eb7bc5972c20715194f6ebf0c806b43`,207CRC·해제382,823,947B를
감사했다. native raw는 완전한40cases/8,655rows였지만 process.log·정식 비교 결과·최종
native baseline이 없으므로705,948비교와 수용을 주장하지 않는다. source/index/SDK 및
held/numerics 전후6쌍은 동일했고 자손 회수·upload를 확인했다. 강한 수용 실패로 정리는
skipped다. #537/#538/#543의 이전 admission은 통과했지만, 이 이슈들의 전체 normal40 완료
조건은 아직 남는다. hash·입력 준비·native 실행·감독 시간을 구분해 조사하며600/700초와
수치·봉인 기준은 유지한다. #533의 기존30case/F64 수용과는 별도다.

## shared logging2 원본 보존 후보 — #549

기존 Rust retained checker에 `prepare-context --root ROOT --archive ARCHIVE --output OUTPUT`을
추가한다. exact R11 artifact12477566277의 outer ZIP2·TAR788·inner ZIP2185를 inert로 읽어
TAR70+inner ZIP37+변하지 않은 current C++ source1의108개/700,585B를 그대로 검증한다.
고정 원문 pin을 새 출력으로 만들지 않는다. 새 context는 source19 index와 함께 private
directory에 NOREPLACE로 게시하고 외부 sidecar `${OUTPUT}.prepare.json`에 FD·source/provider
전후 관측과 수용 한계를 기록한다. source19는 전체 Cargo/DSO 의존성 봉인이 아니다.

새 집중 workflow는 기존 private source/SDK·APT admission을 재사용한다. 원문2파일의
존재·exact SHA를 유지하고 현재 checker/probe/lib/fixture를 Cargo JSONL에서 유일하게
선택한다. core21·새 importer9의 정확한 이름/수와 ignored0, 기존 retained14 selector 각각
1성공, 하나의600초 잔여 예산과 한 번의 raw 비교를 요구한다. probe는 새 single-link copy를
hold한다. 다운로드 뒤 native/trace는 비밀 없는 명시적 env만 전달하고 strace EXITKILL을
요구한다. 준비 단계 실패 뒤 새 Cargo를 실행하지 않으며 source/SDK final·upload는 항상
보존한다. 전체20여 프로그램의 실행/FD 폐쇄를 이 후보만으로 수용했다고 주장하지 않는다.

독립 검토에서 importer의 P1/P2는 없었고, 실행 조건·EXITKILL 지적을 보완했다. 이 새 코드의
컴파일/기능/Clippy는 모두 Actions 전 UNRUN이다. old27은 역사17/미완료10으로 별도이며,
현재 source bridge도 아직 HOLD다. 새14를 old10의 대체 credit으로 세지 않는다. P2 adversarial·
전체 build/trace/FD closure·caller1·원본 부재 단계도 별도로 남는다. 두 Python 원본·caller와
baseline292/3/3285를 보존하며 새 retirement credit은0이다.

## #550의 SHA 패키지 전용 빌드 후보

현재 a4b1db51c의 Fast40132971071은6leaf와 aggregate가 모두 성공했다. alert-manager도
기존7분 한도 안에서 이번에는 완료됐다. Focus40132971275도 성공했으며 source별 산출물
감사를 계속한다. 새 logging40132971238은 실패로 종료돼 실제 phase/원인을 별도 조사한다.

normal40 actual Cargo JSONL3개는 sha2 0.10.9의 opt_level이0임을 보여 준다. 소스와 실제
held 길이에서 trace 전 SHA 처리량 약1.263GB, trace의40case별 numerics 해시 약1.099GB를
확인했다. 이 read 양을 함수별 시간 측정으로 해석하지 않는다. 직접 CLI CPU는312.97초이며
native trace CPU와 다르다. fixture가 CLI를700초에 종료한 뒤 별도 PGID의 trace가 나중에
완료된 것이므로 retained40/8,655rows를 기한 안의 수치 비교 성공으로 세지 않는다.

최소 후속 후보는 임시 normal helper의 모든 현재 Cargo build/test/strict Clippy에
`profile.dev.package.sha2.opt-level=3`와 `profile.test.package.sha2.opt-level=3`를 동일하게
전달하는7줄 보완이다. 수치 계산 crate·feature·DSO·ISA·허용오차·원문·봉인 횟수와
600/700/1,200초·52분 및 디스크 예산은 바꾸지 않는다. 최종 ELF hold 순서도 유지한다.
로컬 AST/diff와 독립 소스 검토만 끝났으며, 실제 sha2 opt3와 이 변경만으로 normal40가
완료되는지는 다음 Actions에서 확인한다. 다른 성공을 이 새 빌드 후보에 소급하지 않는다.

## a4 실제 수용과 logging 증거 용량 — #549/#551

source `a4b1db51cdf0d10a55f647c184dc2b639a726657`의
[Fast40132971071](https://github.com/bin9208/openpilot-rust/actions/runs/40132971071)은6leaf와
aggregate 모두 성공했다. native418초와 기존7분 한도를 구분하며,6artifact231CRC 및
source/index/SDK 전후·upload/정리를 감사했다. Focus40132971275도 성공했고 artifact12575699052
15,867,399B/SHA256 `e4f16813b11aec4dec9daae349c47d6adf52f26a99c0c9f0c4d2f8073694645c`의
81CRC와 전후 봉인을 확인했다.

[setup40132971114/job120988452950](https://github.com/bin9208/openpilot-rust/actions/runs/40132971114/job/120988452950)은
watch9/Clippy·frozen21/음성3/public1·최종 봉인·upload·두 정리까지 성공했다.
artifact12576141678은10,513,855B/SHA256
`d6cb2963e41be2dd4d3b7f9513ad067f09e9e7a8ba7664fcfee284c156e8905f`이며167CRC와
20개 immutable data쌍,122held행의 두 번 동일성을 확인했다. public exec는20시도/20성공,
signal15건이다. 이전14건을 새 실행에 강제하지 않는다. 마지막 상태 구분자만 고친 parser가
빠뜨렸던4성공 분기를 수용해 #551을 종료했다. setup 원본 삭제 credit은 아직0이다.

같은 source의
[logging40132971238/job120988453169](https://github.com/bin9208/openpilot-rust/actions/runs/40132971238/job/120988453169)은
실패했다. artifact12576275047은162,614,632B/SHA256
`5344db8c580fbe312d81e125942f5163b2772b000b8f950553de625a4e84b7e8`,1,203CRC·해제1,267,392,170B다.
현재4개 build/Clippy2·raw108·core21·importer9가 통과했고, 첫4 lifecycle body도 성공했다.
네 번째 `runtime_directory_int` 직후 proof1,269,481,472B가1GiB를 초과해 사후 admission이
거부했다. 나머지10 lifecycle과 retained-once는 미실행이다. source/index·SDK 내용은 전후
동일하고 held-final0·upload 성공을 확인했다. 기능 assert·ENOSPC·source 변조 실패로
분류하지 않는다. 관측된 exec를 새로운 실행 allowlist로 만들지 않는다.

후속 collector는 `read/pread64`의 FD·주소·길이·결과를 raw hex로 남겨 반복 SHA 버퍼만
펼치지 않는다. exec/argv/path와 memory/network/session 호출을 보존하며 packet/context
원문은 별도 봉인한다. strace6.8의 [공식 raw 옵션](https://github.com/strace/strace/blob/v6.8/doc/strace.1.in)을
확인했다.1GiB proof/512MiB file/600초 shared lifecycle/95초 case 한도는 유지한다.

새 `inspect-trace`는 raw 불변·엄격 lexical 해석·byte exec/span 관측만 제공한다.
출력의 동일 inode 재작성/교체/alias를 거부하는 saved identity·SHA 검증을 보완했고,
독립 소스 검토에서 추가 P1/P2는 없었다.20default/ignored0 및 기능 cohort 뒤15prefix
검사를 연결한다. PID/cwd/FD/DSO 폐쇄·full acceptance·retirement는 모두 false다.
실제 컴파일·20개 검사·14 lifecycle·raw 비교·inspection은 다음 Actions에서 확인한다.
old27 현재 source bridge·전체 closure·P2·caller/absence gate와 Python 원본2개는 보존한다.

## Fast 7분 한도 유지 분할 후보 — #552

source `38ec20936fa775d15902976baab4b66ce23d624c`의
[Fast40133699716/native120990634001](https://github.com/bin9208/openpilot-rust/actions/runs/40133699716/job/120990634001)은
7분 전체 한도로 취소됐고 aggregate는 실패했다. 수동 취소는0이다. 다른5leaf는 성공했다.
cold1/caller5/lib83/isolation84/policy29/cutin3 뒤 diagnostic4개 중3개만 완료했고, communication
snapshot의 최종 assert 결과는 없다. 이전 a4의418초 성공을 이번 성공으로 대신하지 않는다.

기존72target을 빠짐없이 한 번씩 유지하면서 native의57개를 정책4개와 도구53개로 나눈다.
새 `native-tools`가53개와 service-tools를 맡고, native는 cold/caller/lib/정책4개를 유지한다.
나머지5leaf·7분·격리 SDK/source·작업별 증가량·실패 뒤 봉인/upload/소유 정리를 유지하고,
aggregate는7leaf의 실제 success를 모두 요구한다. 새 leaf의7분 내 완료는 아직 미측정이다.

Navi compact YAML 경계는 원래 retained job7333B와 기존 BUILD/CALLER hash를 유지하는
parsed-model 정책 비교로 보완한다. 명시된11개 private infrastructure overlay만 검토했고
원래 traffic·caller bytes와 변경 거부 의미를 보존했다. alias 양성/미검토 overlay 음성과 기존
정책 검사, exact2selector를 native에 추가한다. 전체 Navi traffic 재실행이나 새 expected 생성은
하지 않는다. 이 배치의 Actions 전 기능/Clippy는 UNRUN이다.
독립 검토에서 Navi test target의 required feature 누락을 찾아 두 명령과 고정 정책·문서에
`--features carrot-navi-compare-native`를 추가했다. 검사를 생략하는 우회는 넣지 않았다.

38ec Focus40133699721은 성공했다. artifact12576905538은15,867,583B/SHA256
`61709ee27a09448789a0b85e7aa2956b7e1272881baa3e4047cab21f20040e26`,81CRC이며 현재
source/index/SDK4쌍과 내부 과거 봉인쌍도 동일했다. 별도 normal40133699790은 진행 중이며
아직40case/705,948 비교를 수용하지 않는다. baseline292/3/3285와 이번 retirement credit0,
PR343 dev draft 및 전체 #523 미완료 상태를 유지한다.

이 배치의 재사용 허용 경로는 exact161개, literal pin은142개다. 새 Navi3파일/trace2파일과
변경된 Fast/정책/logging helper의 현재 SHA를 고정했고 별도 execution SHA
`4ee29d23ba43e6824f19a550898b5678db549a0d283bace60f177f9713a9ef89`는 불변이다.
로컬에서 YAML2·Bash block92·embedded AST6·단독 Bash1 및 Rust 서식/diff만 확인했다.
실제 컴파일·기능·Clippy는 Actions에서 수행한다. execution-edge92행과 인벤토리 credit은
이번 소스 추가로 변경하지 않는다.
고정7333B Navi 정책 fixture는 원문 끝 빈 줄까지 hash 보존하며, 해당 파일의
`blank-at-eof`만 diff 공백 검사에서 제외했다. 나머지 staged diff 검사는 통과했다.

## 이전 PR의 일반 ARM 실제 완료

[Rust40126297037/job120968344517](https://github.com/bin9208/openpilot-rust/actions/runs/40126297037/job/120968344517)은
PR head bdb819e59의 실제 merge source `ba1b07c38702bac3bb49614d14b54adaacaa95df`에서
성공했다. GNU/musl workspace와 Card/selfdrived/navd·VisionIPC ION·camera/encoder/UI
교차빌드, synthetic procLog/Params/msgq QEMU probe와 Panda raw Params2개를 확인했다.
실물 ARM/AGNOS·차량 runtime 수용이나 현재99c 소스의 성공으로 확대하지 않는다.

다운로드한3 ZIP의99CRC와 SHA를 감사했다. artifact12575863446은288,868B/SHA256
`190c62cbc867e6aa1b817baa037858c6833cad78a2ed85402512952c14a52eb3`,12576420886은
306,999B/`55f7cf5b449810191912841fe5ced8d81fecc8c78299e3b4050a0d63d06b1e55`,12576910338은
75,767,138B/`31e38ed8a40e91f0f2b5ce979ecb50696e1d245b2c06674ec109bd0dc78bffbc`다.
source/index 전후 봉인·upload·소유 정리는 성공했다. 이 job은 전체 SDK/held-FD 전후
manifest를 제공하지 않아 해당 수용은 미제공으로 남긴다. 원본 제거 credit은0이다.

## 99c 실제 검사와 다음 한정 보완

[Fast40135740465](https://github.com/bin9208/openpilot-rust/actions/runs/40135740465), source
`99c0ac14c7d85b7f6ca8903edc2c217bcc00e2cf`에서6native leaf는 모두 성공했다.
native305초/tools296초, 나머지180/299/222/369초로7분 한도를 지켰다.72개 baseline target
각각1회와 cold1/caller5/lib83, Navi exact 정책2개가 실제 통과했다.7artifact265CRC와 각각의
source/index/SDK 전후·upload/cleanup을 감사했다. Python/Android leaf만7333B fixture의
끝 빈 줄을 whitespace gate가 거부해 실패했고, 후속 Python/Android 검사와 aggregate는
성공으로 세지 않는다. 저장7332B에 고정 LF1개를 복원해 기존7333B/SHA `fe4b08ff…160e34cf`
검증 뒤 해석하도록 보완했다. 원문 pin·도메인·공백 gate는 유지한다.

같은 source의
[Focus40135740416](https://github.com/bin9208/openpilot-rust/actions/runs/40135740416)은142pin을
통과한 뒤 과거 Fast 명령의 동일 prefix가2→3개가 된 것을 거부해 실패했다. 새 tools가 추가한
정확한 prefix1개만 허용하도록 보완하며72개 본문/순서는 별도 current 정책으로 검증한다.
fresh alias26/Unicode7/SSH27·Clippy 및 최종 봉인/upload/정리는 실제 성공했다.
artifact12576074785는15,866,617B/SHA256
`1fbe81b447d829ff7e7db7daea7a715870388a181d0457acbf6e7e08c9f28655`,80CRC이며 전후 봉인은 같다.

[logging40135740435](https://github.com/bin9208/openpilot-rust/actions/runs/40135740435)은
현재4build/Clippy2·raw108/core21/importer9 뒤 trace20의 여섯 번째에서 SIGXFSZ153으로
종료됐다. 앞5개만 통과했고 뒤14개와 lifecycle14/raw-once/inspection은 미실행이다.
`set_len(512MiB+1)` 음성이 기존512MiB 파일 크기 제한에 먼저 걸린 것이다. 같은 metadata
검사를 private bound 함수로 분리해 실제4096B 수용/4097B 거부를 확인하도록 했다.
production은 고정512MiB를 쓰고 함수도 그보다 큰 bound를 거부한다. 기존 line 음성·20개
selector·ulimit과 다른 검사는 유지한다. 독립 소스 검토에서 추가 P1/P2는 없었다.
artifact12576479338은7,391,196B/SHA256
`991b3566ca8658617ebbd60a0e828f84c351d343868b75ad2e261325a2e13ff7`,198CRC다.
held-final0/source-index/wholeSDK 전후·upload는 성공했고 전체 기능 수용은 실패다.

## normal40 SHA2 opt3 실제 재실패와 출력 버퍼 후보 — #550

[38ec normal40133699790](https://github.com/bin9208/openpilot-rust/actions/runs/40133699790)은
3개 actual Cargo JSONL 모두 sha2 opt3를 확인했지만 fixture700.42초 timeout으로 재실패했다.
artifact12576384518은52,124,725B/SHA256
`d3353b4ae1413ee9f6251b0e8ca44917652508ce41a6f2cc53f887d327743468`,207CRC·해제382,909,550B다.
source/index/private SDK·원문·system SDK3개의 실제 files 전후가 같고 자손 회수/upload를
확인했다. system SDK before는 root/files wrapper, after는 files이므로 그 필드를 대조했다.
유일한 final 실패는 native baseline unavailable이다. raw가 있어도705,948 비교 성공은 없다.

exec 원문의 직접 CLI CPU는199.85초/시스템9.21초다. child start tick상 trace는 CLI 뒤
218.58초에 시작했고 outer command908.18초는 caller 종료 뒤 자손의 완료/회수도 포함한다.
이를 trace 함수별 CPU 측정으로 해석하지 않는다.17,334,185B native JSON을
`serde_json::to_writer(File, …)`로 직접 쓰는 소스를 확인했다. 이 경로의 작은 write가
추적 비용을 키울 수 있다는 소스 기반 가설이다. strace6.8 [공식 설명](https://raw.githubusercontent.com/strace/strace/v6.8/doc/strace.1.in)은
일반 추적이 syscall마다 정지하며 seccomp-bpf가 그 정지를 줄이는 별도 옵션임을 설명한다.
새 옵션으로 추적 범위를 바꾸지 않고, 기존 writer를 `BufWriter`와 명시적 `flush()?`로
감싸는 한정 후보를 추가했다. serializer/계산/JSON/입력/feature/DSO·600/700초는 불변이다.
current trace source pin과 해당 path trigger만 갱신했고 성능 개선 폭·기한 내 성공은 UNRUN이다.

## setup1 실제 삭제 source의 후속 후보

96a/a4 pre-delete 수용 뒤 `tools/scripts/setup_ssh_keys.py` 하나를 삭제하고 setup WF를 literal
`absent`로 전환한다. frozen21/거부3/public1·Rust 두 파일/entry와 고정 원문13800B는 불변이다.
작은 Bash helper로 checkout HEAD/index empty roster, exact leaf 부재와 부모dev/ino/mode/uid/gid,
명령 전후·2초 sampling·최종 receipt/source/closure/cleanup을 연결한다. 별도 synthetic private
root에서 absent 양성1과 regular/directory/symlink/dangling/parent-symlink 음성5를 정확한
오류로 검증한다. 다른 오류를 거부 성공으로 세지 않는다. 연속 filesystem 감시나 전체 Python
부재 수용으로 확대하지 않는다.

원본은 checkout 뒤 숨기거나 삭제하지 않는다. 실제 commit부터 없는 경로여야 하며 Focus도
base 원문 SHA와 exact D·현재 HEAD/index 부재만 허용한다. 기존 input/provider hold는
원본1의 FD 대신 부재 증거와 guard helper를 결합하며 나머지 계약을 유지한다. cleanup은
전체 부재 stage/controls를 검사한 closure의 실제 성공도 요구하도록 검토 지적을 보완했다.
이 후보의6controls/21/3/public1/Clippy·봉인·upload/정리 actual 수용 전 inventory는 pending,
baseline292/3/3285와 이번 credit0이다. logging 원본2/caller와 old27 gate는 그대로 남는다.

setup 후속3파일, Navi 저장 복원3파일, sparse 경계2파일과 BufWriter 후보를 각각 독립 소스
검토했고 보완 후 추가 P1/P2는 없었다. 현 배치의 exact allow164개/144literal pin과 별도
execution SHA는 일치한다. 로컬 YAML3·Bash block38·embedded AST8·단독 Python AST1·
Bash helper1, Rust 서식/diff만 확인했다. compile/test/Clippy/원본 프로그램은 실행하지 않았다.
새 실제 Actions 전 성공은 UNRUN이다. execution-edge92행의 세 current blob만 갱신했다.

## 602 실제 setup 원본1 수용과 Focus 성공

[setup40137010381/job121000556049](https://github.com/bin9208/openpilot-rust/actions/runs/40137010381/job/121000556049),
source `602ea095f7cd89769ffd9233a04a28983ca59d1f`에서 실제 삭제 checkout의 guard6·controlled21·
거부3·public1·strict Clippy와 watcher9가 모두 성공했다. 공개 exec20/20, signal15는
SIGCHLD14와 SIGUSR1/SI_TKILL1로 구분된다. 정확 stdout102B/exit1과 같은 helper→native PID,
최종 ELF inode/SHA 불변을 확인했다. raw13800B/SHA·과거 metadata는 그대로다.

artifact12577386813은10,462,974B/SHA256
`1ee86d2c9abfe5ff3141fec71ed582e373f5c27878552650a86013a67532361c`,172CRC·해제152,844,779B다.
실제 source HEAD/index에서 원본1이 없고 absence 관측299행과6control·핵심 명령 전후가
모두 일치했다.122held파일의 두 번 검사와 source/SDK 등15쌍, stable roster10쌍이 같다.
closure→upload→마지막 cleanup 부재 관측→소유 정리의 실제 순서를 확인했다. 삭제한 원본1의
FD가 빠지고 guard helper가 들어간 결과122행인 것이며122라는 수만으로 수용하지 않는다.

이 경로 하나를 원장에 추가해 **293교체/3부분/3284대기=3580**이다. routing map은
19accepted/3287pending·partial이다. 실제 receipt의 `retirement_credit:false`·전체 Python 부재
false·network CLI0 필드는 그대로 두며, 한정된 원본1의 원장 판정과 구분한다.

같은 source의
[Focus40137010398](https://github.com/bin9208/openpilot-rust/actions/runs/40137010398)은 성공했다.
artifact12577357167은15,868,420B/SHA256
`0b60e9e69f7ae420050432ff4df631dff2cedddfc0ebfcd0bfdd87a7e1054439`,85CRC다.
current source/index/SDK4쌍 및 내부 과거 봉인쌍도 동일했고 exact setup D/base SHA·HEAD/index
empty roster를 직접 확인했다. 전체 PR/통합·logging/normal40·실기 수용으로 확대하지 않는다.

## 602 Fast의 불명 cold 기록

[Fast40137010462/native121000557107](https://github.com/bin9208/openpilot-rust/actions/runs/40137010462/job/121000557107)은
7분 한도에 닿기 전 실제 cold audit가 `2604  ???( <detached ...>`를237행에서 거부했다.
그 뒤 routing/Navi2는 미도달이다. 나머지6leaf는 성공했고 Python/Android의 공백·Gradle 검사도
실제 통과했다.7ZIP254CRC 및 각 source/index/SDK·upload/정리를 확인했다.
capture는 이후507행의 disk-space guard exec와 foreground exit0까지 이어졌지만 PID2604의
syscall/부모·종료 의미는 알 수 없다. 정상 thread라고 가정하거나 이 행을 skip하지 않는다.

strace의 [공식 v6.8 구현](https://raw.githubusercontent.com/strace/strace/v6.8/src/strace.c)은
공통 TCB 제거 경로에서도 미완성 출력을 `<detached ...>`로 끝낸다. 이 문자열만으로
실제 PTRACE_DETACH를 확정할 수 없고, syscall 식별 실패와 양립한다는 것까지 확인했다.
발생 원인은 미확정이며 기존 FAIL과237행 거부는 유지한다. 같은602 source의 실패 native와
그 집계만1회 재관측하며 성공6leaf는 반복하지 않는다. 다음 성공도 이 원인을 해결했다는
증거로 바꾸지 않는다. 아직 PR343 fast-forward와 새 필수 deep는 진행하지 않았다.

## 602 logging 실제 metadata trace 초과

[logging40137010352](https://github.com/bin9208/openpilot-rust/actions/runs/40137010352)은 새 trace20과
앞5 lifecycle body까지 통과했다. 다섯 번째 runtime-directory TERM 뒤 proof1,710,235,648B로
1GiB 사후 한도를 넘어 실패했다. 이후9 lifecycle/retained-once/15inspection은 미실행이다.
artifact12577451344는74,226,142B/SHA256
`d42654c9d0bb1c45c2e372320e0326c059ff2309e049917580f3587741b6d651`,2,005CRC·해제1,705,848,612B다.
source/index·wholeSDK 내용은 전후 같고 held-final0·upload는 성공했다. 최종 source/SDK 단계는
동일 proof 크기 guard에서 실패했다. 이를 source/SDK 변조로 해석하지 않는다.

inert 원문의 호출별 byte 집계에서 newfstatat897,261,941B/getdents64 266,178,070B/
openat175,872,114B가 대부분을 차지했다. read는3,489,009B다. 실제 syscall 인자와 별도로
`-xx/-yy`가 반복한 hex 절대경로·FD annotation이 주된 비용이다. 이 관측을 새 허용목록으로
만들지 않는다. 후속 후보는 `-x`로 byte escape를 보존하면서 파생 FD-path/protocol annotation을
생략한다. FD 번호·실제 path/metadata/호출/결과·원문 packet/context는 유지한다.
전체 classes·raw read·512MiB/1GiB·600초와 lexical scope의 closed PID/cwd/FD/DSO false는
변하지 않는다. 초기 FD 표 및 semantic closure는 계속 미완료이며 actual fit·inspection 성공은
새 실행에서 확인한다. 현재 성공5를 old27나 최종 raw 비교 성공으로 올리지 않는다.

## 602 후속 cold 재관측과 normal40의 첫 수치 실패

Fast의 실패 native만1회 재관측한 attempt2/job121004679000도 raw310행의
`2664  ???( <detached ...>`로 실패했다. 성공6leaf의 start/end가 attempt1과 같아
새 실행6개로 세지 않는다. 원인 미확정·unknown 거부는 그대로이며 재시도를 추가하지 않는다.
후속 수집 후보는 같은 `execve,execveat` 범위에 `--seccomp-bpf`를 요청하고 실제
strace provider/version/시작·종료/status/stderr를 남긴다. fallback이 가능하므로 BPF 활성이나
원인 해결을 주장하지 않는다. stderr 수집도 별도 유한 대기·현재 소유 PID 회수가 필요하다.

[normal40137010441/job121000556319](https://github.com/bin9208/openpilot-rust/actions/runs/40137010441/job/121000556319)은
trace40/8655를 EXIT0으로 완성하고700/600초 안에 비교에 도달했다. BufWriter 이후 이전
timeout 장벽은 넘었지만 실제 비교는 `controls[3].rows[0].state.lat_pid[0]`에서 실패했다.
역사값 `-0.034913528710603714`와 현재 `-0.03491372987627983`은 F32 54 ULP 차이다.
[#553](https://github.com/bin9208/openpilot-rust/issues/553)에서 NN response/PID 전 경계를
추적하며 기대값·허용오차·DSO·ISA 정책을 바꾸지 않는다. 최초 layer 원인은 미확정이다.

artifact12577798385는52,127,695B/SHA256
`9212f3233b3fd4d479334519e0e7667df1672005823eed5ee23e1bc3e03caf4c`,208CRC 정상·해제382,908,540B다.
241held와 source/index/SDK/numerics 내용은 전후 같고 failure-seals marker도 pass다.
전체 native baseline unavailable로 final closure는 실패, upload만 성공하고 cleanup은 skipped다.
이를705948 비교 성공·actual trace PID의 DSO mapping 수용으로 확대하지 않는다.

## logging 기록량과 현재 collision8의 분리 후보

같은 lexer가 모든 원문 줄을 검사하는 `inspect-trace-summary`를 별도 schema2로 추가한다.
모든 exec 시도·signal·terminal·중단 call은 상세 Event를 보존하고 나머지는 kind/operation/outcome
개수와 원문 SHA를 남긴다. `full_event_index:false`를 명시하고 다른 syscall 세부값은 immutable
raw에 남는다. full schema1과 기존 trace20, 모든 raw/file/model/event/line 상한은 유지한다.
summary 경계6개를 CI에서 별도 실행하며 dispatcher가 옵션만 scanner에 넘기도록 검토 지적을
수정했다. semantic PID/cwd/FD/DSO closure의 대용이 아니다.

기존 pending collision8은 별도 기본 Ubuntu workflow/helper로 분리한다. 현재 source에서
checker/builder/probe/fixture를 빌드하고 원래 pin/recipe의 fresh C++ reference는 build만 한다.
현재 fixture1이 만든8receipt·checker8exec/exit1·child spawn 시도0·외부 output 전후 동일성을
byte argv/env와 원문으로 검사한다. provider를 과거 ELF나 가짜 wrapper로 대체하지 않는다.
operation별 동시 증가량은 같은 filesystem에서16.5GiB이며 SDK/빌드/proof 상한·소유 정리·
실패 후 봉인/upload를 결합한다. actual fit·build·8case·cleanup은 아직 UNRUN이다.
old27/runtime2/caller/source absence/full closure와 퇴역 credit은 그대로 미수용이다.

현재 cold stderr 후보는 tracer status를 먼저 noclobber 저장한 뒤 현재 collector의
PID/start ticks/PPID를 확인한다.10초 drain·2초 회수 뒤 terminal/gone에만 wait하며,
회수는 단일 현재 collector PID만 대상으로 한다. tracer/과거 PGID에 신호하지 않는다.
수집 실패에도 provider-after 기록과 비교를 수행하고 전체 단계는 FAIL로 남긴다.
정상·tracer 실패·collector 실패·pipe 잔류·잘못된 start ticks/PPID의6사례를 기존 CI 정책
test에 추가했다. writer ACK/release와 별도 postcondition marker로 음성 결과1에 다른
검사 실패1이 섞이지 않게 보완했다. 독립 검토 뒤 actual은 아직 UNRUN이다.

새 후보 경계3개를 등록해 Focus exact allow167개/literal pin147개다. 기존 실행부의
별도 SHA `4ee29d23ba43e6824f19a550898b5678db549a0d283bace60f177f9713a9ef89`는 불변이다.
원장의 execution-edge92행 분류/credit은 유지하고 Focus current blob만 갱신했다.

로컬 최종 확인은 YAML3·Bash 본문102·embedded Python AST6·Bash helper2의 문법과
Rust 서식/diff·147literal pin·167exact allow 및 원장3580/3306행 집계다. 모두 확인했으며
컴파일·test·Clippy·원본/후보 CLI 기능은 로컬에서 실행하지 않았다. 새 성공은 Actions
실행과 artifact/봉인/정리 감사 뒤에만 기록한다.

## 419 실제 Fast/Focus 수용과 PR343 연결

source `419244279fd42ca7a8d83272b8e659bd4136c3ec`의
[Fast40140187055](https://github.com/bin9208/openpilot-rust/actions/runs/40140187055)는7leaf+aggregate
모두 성공했다. 최장routing392초/tools339초이며 기존420초 이내다. 기존72suite/build
selector 계약·Navi2·cold actual과 stderr collector 정상1/음성5를 실제 검사했다.
Rust554 PASS/FAIL0, 기본ignored2 중 cold는 별도 실제 실행했고 UI Xvfb1은 미실행이다.
Python306/44/34와 Gradle도 성공했다.

ZIP7개 총2,359,152B/279CRC 및 전후봉인42쌍을 감사했다. 각index19428 unique paths,
SDK165files/188roster/4program pin과 source SHA가 같고7leaf 전부 upload/소유 정리가
성공했다. cold raw1,170,129B/505줄 SHA256
`67d5249b9983a78098aef05d3d9a1948a9f4dee9bd7826c6683c7757c4b196bf`에 unknown/detached가
없고 strict parser가 exec323/실패172·SDKcargo1/rustc37·Python0/rustup0을 수용했다.
실제 strace6.8 provider2087432B/SHA
`28f957c227012de0b18d1bd7fff2d396cb693ea60ed8013be68de071e84b5001`, 전후4쌍이 같다.
collector wait/collection0·reaped true·timeout false다. BPF activation은 계속 unproven이며
602 실패237/310행의 최초 원인을 해결했다고 하지 않는다. #552의7분 분할 조건을 충족해 종료했다.

같은 source의 [Focus40140187062/job121010107682](https://github.com/bin9208/openpilot-rust/actions/runs/40140187062/job/121010107682)도
성공했다. artifact12579025471은15,868,394B/SHA
`3734cfbd0cd784862150a6bbc05eb3bed7dbf75832eea9c01f25945ca63ad360`,85CRC·해제24,608,749B다.
source/index/전체SDK·원본 D/base SHA/HEAD-index empty와 내부 역사봉인을 확인했다.
13개 불변 봉인쌍 및4program pin이 동일하며 disk/memory 관측의 차이를 변조로 세지 않는다.
source/SDK final→upload→소유 source/job cleanup이 모두 실제 success다.

PR343 head를 force 없이419로 fast-forward했고 dev 대상 draft를 유지한다. 새 필수
Rust40140991825·Integration40140991803·PR Fast40140991807·docs40140991906을
정확 merge SHA로 끝까지 확인한다. 아직 새 deep 성공이나 전체 #523 완료가 아니다.

## 419 logging의 실제 backpressure 경계 — #554

[logging40140187029/job121010107517](https://github.com/bin9208/openpilot-rust/actions/runs/40140187029/job/121010107517)은
raw108/core21/importer9/기존trace20/새summary6 및 앞3lifecycle body를 통과했지만
runtime-directory INT에서 backpressure2초를 넘겨 실패했다. actual2.148831170초는
start-peer1.872016784 + command0.171116195 + finish0.105698191이다. 원문 PID12731의
Git status15212줄, exec1792882565.811218→exit1792882567.575329(1.764111초)가 준비
구간의 대부분이다. 전체 호출의 ptrace 영향과 양립하지만 VM/각 syscall의 개별 CPU 원인은
확정하지 않았다. timer/기존 Git검사·2초 기준을 옮기거나 줄이지 않는다.

artifact12579190406은33,923,432B/SHA
`c0f0556551d355310dacd2fafc72b85ff0640e26ba7e83044cd29c366c98241c`,950CRC·해제388,310,252B다.
이번까지 proof는1GiB 안이며 source/index/wholeSDK/held 전후가 같고 upload는 성공했다.
이 workflow에는 명시적인 source/job cleanup 단계가 없어 owned cleanup 성공으로 세지 않는다.
나머지10lifecycle/retained-once/15inspection은
미실행이다. #554에서 이 timing 실패를 추적한다. summary6의 성공을 전체15prefix lexical
성공으로 확대하지 않는다.

단일 변수 후보는 두 collector에만 `--seccomp-bpf`를 추가한다. 같은 전체 trace classes와
인자·결과·-x/rawread·file512MiB/proof1GiB·shared600초·2초 기준을 유지한다. 미선택 syscall
정지를 줄일 수 있지만 선택된 Git metadata15212줄은 남으므로148.8ms 개선이나 실제 통과는
미확정이다. BPF activation unproven/fallback·unknown 거부와 parser·transport·원본2/caller는
불변이다. 독립 소스 검토 P1/P2 없으며 새 실제 기능 검증은 UNRUN이다.

## 079의 실제 기본 러너 검사와 남은 실패

source `0795088a40a9873daddc3912060f741265bff837`의
[Fast40141223843](https://github.com/bin9208/openpilot-rust/actions/runs/40141223843)는
7leaf+aggregate 모두 성공했다. 최장 alertmanager369초이며 routing349초/tools291초다.
ZIP7개 총2,358,986B/279CRC를 읽어 source/index/전체SDK와 program pin을 확인했다.
봉인49쌍에는 rejected SDK의 빈 manifest7쌍을 포함한다. 앞선419의42쌍과 집계 정의가 다르다.
cold exec323/실패172·cargo1/rustc37·Python0/rustup0, collector exit0/reaped true이며
Navi2와 collector6도 실제 성공했다. raw SHA는
`856e1832621c160cfdeadd77f7bb10baa4a81a00bce798573d032a0d80dcca77`다.
BPF activation은 여전히 미확정이고 모든 leaf의 source/SDK/upload/소유 정리는 성공했다.

같은 source의 [Focus40141223822/job121013198103](https://github.com/bin9208/openpilot-rust/actions/runs/40141223822/job/121013198103)도
성공했다. artifact12579071714는15,868,493B/SHA256
`57144802b18f925983c2bd2b5f1630d1974ecd951985aba7d8156106d3ab01ec`,85CRC다.
전후 source/index/전체SDK, 역사13쌍과4program pin, setup 원본의 정확한 D/base SHA 및
HEAD/index 부재를 확인했다. final seal→upload→source/job cleanup까지 성공했다.

[logging40141223827/job121013198226](https://github.com/bin9208/openpilot-rust/actions/runs/40141223827/job/121013198226)은
13lifecycle을 통과한 뒤14번째 `retained::forbidden_executable_options`에서 실패했다.
runtime-directory INT는1.376547681초(start1.176446517+command0.095065644+finish0.105035520),
TERM은1.356430955초로 이번 source의2초 경계를 통과했다. 이를 일반적 timing 원인 해결이나
BPF 활성 증명으로 확대하지 않는다. artifact12578418898은69,251,068B/SHA256
`f22dbf32b7ead5cef0717a0296aace415e5da5782101b33868f0d42f8c7f3686`,2292CRC다.
해제884,960,336B/실제 proof 할당890,032,128B로1GiB 안이고 held final/source/index/전체SDK가
같다. foreign fixture의 `providers.before.json`과 `providers.after.json`은 서로 다른
원문 dummy 파일이므로 봉인쌍으로 세지 않는다. retained-once/15inspection은 미실행이며
workflow에 별도 cleanup 단계가 없어 upload 성공과 소유 정리 성공을 혼동하지 않는다.

실패 #555의 후보는 shared scanner를 그대로 두고 이 CLI의 정확한 금지 옵션3종에만
정적 옵션 이름을 오류에 덧붙인다. 값은 출력하지 않으며 separate/inline/nonUTF inline9경계와
required/help 우선순위를 보존한다. 독립 소스 검토에서 P1/P2는 없었고 실제 수정 후 검사는 대기다.

## collision8의 fixture-build 전체 감사 한도

source419의 [collision40140187132/job121010107869](https://github.com/bin9208/openpilot-rust/actions/runs/40140187132/job/121010107869)은
current Cargo3/fresh ELF4/C++ reference build 뒤 실제 collision8 selector를 통과했다.
case trace의 fixture1 exit0/checker8 exit1·spawn 시도0·정확 argv/env·외부 output 전후 동일성을
확인했지만 fixture-build 전체 schema1 model이64MiB 상한을 넘어 최종 실패했다.
fixture raw1791files/72,410,692B/6796줄 중 exec948행이71,685,836B다.
artifact12578292747은20,256,477B/SHA256
`f863974eb445166c08419836acf415c2ffd78a773958e450ec5cdcf353ec1110`,6296CRC다.
held152/source/index/전체SDK/트리5쌍은 같고 finalizer0/upload 성공이나 cleanup2개는 skipped다.
reference inspect와 전체 program audit 미실행 상태의 CPP/probe 실행 결과는 null이며0으로 세지 않는다.

후보는 원본 PID 파일을 자르지 않고 raw1MiB/128files 이하의 별도 복사 batch로 나눈다.
각 batch를 같은 full schema1로 읽고 원본·복사본 identity/SHA/전체 roster, 정확히 한 번의 coverage,
전체 root1/exec program 검사를 결합한다. 누락/중복 음성 통제도 추가한다.
단일 operation180초·raw/proof1GiB·8192files·1000000events·2000000lines·model64MiB 상한은
유지하며 전체 검증과 deadline 확인 뒤에만 complete/candidate를 만든다.
summary schema로 대체하거나 기존8case/27검사/원본2/caller의 수용 조건을 바꾸지 않는다.
독립 검토에서 P1/P2는 없었으며 실제180초/1GiB fit와 최종 수용은 대기다.

## PR343의 실제 merge 검증과 경로 보정 후보

PR head419/merge `e882cd1dd2f709572c94fe35bcdb4cfc85997ab8`의
[docs40140991906/job121012509747](https://github.com/bin9208/openpilot-rust/actions/runs/40140991906/job/121012509747)은
mapped7546 changed files와 docs13검사를 통과했다. artifact12578288604는328,954B/SHA256
`e09001b762facf807006d536c0b0431258cc832909b306eb8c241ca58738cb7c`,54CRC다.
merge 부모 dev b555/PR419, source/index/전체SDK5쌍 및4program pin을 확인했다.
growth guard2/recovery가 성공했고 source/job cleanup도 성공했다.
direct leader wait1/group nonlive1을 관측했으며 all adopted/peak 증명은 false로 남긴다.

전체 Rust/Integration 검증은 별도로 진행하며 실제 실패에 대한 다음 후보를 작성했다.

- planner와 model-family compiler의 Python 실행 경로를 canonical ELF로 바꾸면 venv 패키지를
  잃었다. #545 후속으로 실행에는 venv entry를 유지하고 ELF/entry 전후 검증은 보존한다.
- model-pipelines의 종료 peer `/proc` 읽기에서 ESRCH가 발생했다. #556 후보는 기존 ENOENT와
  같은 종료 분기에 ProcessLookupError를 추가하며 남은 model 검사와 종료 기준은 보존한다.
- camera policy fixture는 이전 runner guard 이름을 가정했다. 승인한 첫 guard 이름과 일치시키고
  Focus에서 기존 정책2개를 exact selector로 실제 실행하도록 추가했다. camera 기능 수용은 아니다.
- startup-services 비교 도구는 `/tmp`만 가정했다. current TMPDIR와 역사 `/tmp` 경로를 모두
  정규화하며 동일한 runtime suffix/nonempty/직계 parent 경계를 유지한다.
- estimation의 옛 bin-actions label assertion을 현재 승인한 Ubuntu24.04 고정값으로 바꾸고
  공용 trust/private-layout 검증과 음성5경계를 연결했다. 실제 수정 후 검사는 대기다.
- Integration release의 DBC2/font4 source mutation은 #557에서 생성 출력과 실제 소비 경로를
  함께 분리하도록 조사한다. source seal을 제외하거나 결과를 덮어쓰는 방식으로 통과시키지 않는다.

## #553 첫 수치 차이 관측 후보

normal40의 case3 frame0 `lat_pid[0]`은 역사 `-0.034913528710603714`와
native `-0.03491372987627983`의 F32 54ULP 차이로 실패했다. 허용 오차와 원문 기대값은 유지한다.
새 default-off `diagnostic-553` feature와 별도 기본 Ubuntu workflow는 같은 case/frame의
원본과 native를 각각 plain/tapped로 실행해 cast/subtract/normalize/dot/bias/exp/sigmoid 경계를
관측한다. plain 원본은 역사값, plain native는602 값과 일치해야 하며 taps 유무의 최종 출력 bit도
같아야 한다. 원본 AST는 taps를 제거하면 전체가 같아야 하고 padding 정수식은 tap하지 않는다.
실제 수치식은 한 번만 평가하며 최대16calls/512events/vector256/record2MiB를 제한한다.
현재 CPython3.12.15/NumPy2.5.3/3DSO pin과 source/SDK/자손 회수를 확인하지만 과거 전체
provider/kernel/normal40/전체 runtime closure나 원본 제거 credit을 주장하지 않는다.
operation별 전체 증가량18.5GiB와 source/SDK/proof/venv/global Python root의 개별 상한을 둔다.
독립 소스 검토와 문법 검사를 마쳤으며 실제 진단은 대기다.

이번 후보의 새 Python/WF 진단 경로는 execution-edge 원장에 pending으로 추가한다.
baseline3580의293교체/3부분/3284대기와 #523/#296의 전체 미완료 상태는 유지한다.

새 후보의 Focus exact allow는178개/literal pin은158개다. camera 정책2개 추가 실행부를
독립 검토한 뒤 별도 실행 SHA를
`f46f208c0273d4a678dec98ebecd574063b54e939ca3355740ce3b15723330ba`로 갱신했다.
과거 성공을 새 수치/로깅/경로 기능 성공으로 소급하지 않는다. execution-edge94행 중 새2행은
진단 Python pending이며 기존 baseline 집계는 그대로다. 로컬은 YAML3/Bash본문40/embedded
Python AST8/standalone Python2/Bash2, Rustfmt/TOML2/서식/diff 및158literal pin을 확인했다.
기능·빌드·Cargo/test/Clippy·원본/프로젝트 CLI는 로컬 미실행이며 새 Actions 결과를 별도로 확인한다.


## 412 실제 검사: 수용과 실패 원인 분리

source `41219fd1bac8ac5c4ae7f8d3417288155852b9a6`의
[Focus40143973684/job121021319298](https://github.com/bin9208/openpilot-rust/actions/runs/40143973684/job/121021319298)은
reuse admission의 `retained_tests.rs` 누락으로 최종 실패했다. 별도 SSH alias/고정 Unicode/27회귀/
Clippy와 새 camera exact2는 성공했다. 허용 명단에 이 검토 완료 test 파일의 정확한 경로·SHA를
추가하는 후보를 작성했으며 일반적인 경로 예외를 만들지 않는다. 현재 baseline 대비 모든 변경
경로를 다시 대조해 추가 누락이 없음을 확인했다. artifact12580215266은15,868,361B/SHA256
`b16b07c7693f1dda2c6dd81b5631ed5be9214fb33edb0ed3edb8fed0ab966f38`,82CRC다.
source/index/전체SDK와 역사13쌍이 같으며 최종 seal/upload/source·job cleanup은 성공했다.
재사용 admission 성공이나 전체 Focus 성공으로 기록하지 않는다.

[Fast40143973691/job121021319557](https://github.com/bin9208/openpilot-rust/actions/runs/40143973691/job/121021319557)의
cold audit는 raw236행 `2875  ???( <detached ...>`를 거부했다. 앞선602의 unknown 유형이
재현됐으므로419/079의 성공을 원인 해결로 확대할 수 없다. raw1,169,717B/498줄, tracer0/
collector0/reaped true와 strace6.8 ELF 전후 동일성을 확인했다. stderr에는 Cargo compile
성공이 있으나 unknown task를 안전한 syscall로 추정하거나 parser를 완화하지 않는다.
BPF requested true/activation unproven이다. artifact12579908024는388,712B/SHA256
`c3da4b33b56e122d1a7c37c77c1d73f12b9997d46122d0bea6036b0e67b13059`,64CRC다.
source/index/전체SDK/불변12쌍은 같고 메모리·디스크 관측4쌍은 봉인쌍이 아니다.
strict parser 거부는 유지하며 tracer/provider 원인을 조사한다.

PR343 mergee882의 Fast40140991807도 별도로7ZIP/2,358,846B/279CRC와 불변42쌍을 감사했다.
7leaf220~322초/aggregate3초, source/index19428·SDK165files/188roster/4program이 같고
모든 final/upload/source·job cleanup이 성공했다. cold raw SHA는
`d28eb7cabce6b1d452513f0c095851912c122c8d921e1f3d3af124238bc72a4c`이며323exec/172실패·
Python0/rustup0/cargo1/rustc37이다. 로그 요약의 cold grep 재출력1개를 중복 제외하면
실제 Rust554PASS/0FAIL/ignored2다. 이 성공도 새412나 전체 Rust/Integration 수용을 대신하지 않는다.


같은412의 [logging40143973687/job121021319250](https://github.com/bin9208/openpilot-rust/actions/runs/40143973687/job/121021319250)은
앞3lifecycle 뒤4번째 runtime_directory_int에서2.217586553초로 다시 실패했다.
start1.931232239+command0.180113188+finish0.106241126이며2초 기준을 유지한다.
#555의 retained-core21 안9옵션·우선순위 unit 경계는 실제 통과했으나14번째 실제 CLI
lifecycle과 나머지 검사에는 도달하지 않았다. 원래 #554를 계속 추적한다.
artifact12579508913은33,051,239B/SHA256
`a145de9bb09419681b0e2f19a7c049f076ffef617653ba02d37e64bc4a550757`,950CRC·해제380,787,878B다.
held final0/source/index/전체SDK가 같고 upload는 성공했으며 workflow의 명시적인 cleanup은 없다.
seccomp 요청 한 번과079의 성공으로 일반적인 timing 해결을 주장하지 않는다.


collision412의 전체80fixture batch는1791files/72,409,442raw bytes/6796events·lines/root1과
정확 coverage/누락·중복 음성 통제를 통과했다. complete.json은 전체 검증 뒤 생성됐다.
이후 reference-build schema1 정책이 false여서 최종 실패했으며 원인을 조사 중이다.
artifact12580450586은38,876,854B/SHA256
`dcb849c512a2c141393f09433f1d4144e5166181af30d18df3ae47b83ff76f2e`,8382CRC·해제393,594,969B다.
held-before165행/165경로와 held-after328행/165경로는 재검증 반복 때문에 byte 동일하지 않지만
각 경로의 모든 identity/SHA 값은 같고 누락0/충돌0이다. foreign provider dummy3쌍은
서로 다른 원문 파일이며 변조 봉인쌍으로 세지 않는다. finalizer0/immutable_seals true이나
최종 candidate/full build closure false·CPP/probe 실행 결과 null·retirement0을 유지한다.
업로드의 비지원 entry2경고는 별도로 원문 metadata를 확인한다.

412 Fast의 전체7ZIP도2,308,763B/268CRC·불변42쌍을 확인했다.6leaf는 성공했고
cold native와 aggregate는 실패했다. source/SDK final/upload/소유 정리는 leaf별로 확인하며
일부 성공을 전체 Fast 성공이나 새 재사용 범위 수용으로 확대하지 않는다.


## 다음 source-only 보정 후보

- cold collector의 `--debug` 단일 변수 관측은512MiB stderr 상한/overflow 실패·7분·
  기존6경계·unknown 거부를 유지한다. debug 실제 증가량과 root 원인은 미확정이다.
- SSH39/setup21의 고정 원문 재생과 importer 음성7을 기존 target10에 연결한다.
  private artifact2/metadata2는32MiB 증가량+0.5GiB reserve로 stage하고 metadata16KiB와
  ZIP 고정 크기의 KiB 올림 상한을 각 다운로드 전에 적용한다. 원문 ZIP을 다시 추출하지 않으며
  기존 parser admission도 같은 ZIP을 재사용한다. 전체 input hash/identity roster를 전후 비교한다.
  현재 candidate/verifier/39+21 input·criteria는 그대로이며 원본 ssh.py 삭제/실제 SSH entry/credit은0이다.
- #558은 planner의 고정25GiB floor만 제거하고 기존128MiB operation 추정치를 유지한다.
  peak를 실측했다고 주장하지 않으며 기존13cases×source/native를 Actions에서 확인한다.

Focus의 missing retained test 및 새 SSH module/docs를 정확 등록한 현재 명단은181allow/160pin이다.
새 SSH 실행 연결의 독립 검토 지적(다운로드 전 상한)을 반영했다. 검토한 실행부 SHA는
`2a05a74b9bc4c5d4c4793ac11259d1c4f02b2fcaa599ebd03a3f92416e80c84a`로 묶는다.
추가 기능 수용과 원본 제거 성공은 아직 없으며 실행-edge94/baseline293·3·3284를 유지한다.


#553의 [진단40143973775/job121021319648](https://github.com/bin9208/openpilot-rust/actions/runs/40143973775/job/121021319648)은
정확 source412에서 native-plain의602 상태 bit를 재현하지 못해 최종 실패했다. 현재 원본 plain은
역사값을 재현했고 양쪽 plain/tapped의 출력 bit는 같았다. 현재 축소 실행의4calls/2329개
F32/F64 중간 bit는 원본과 native가 모두 같아 first_divergence=null이다. 이 사실은 해당
단일 프레임 축소 맥락만 설명하며602의 전체 trace 수치 차이를 해결했다는 증거가 아니다.
기존 경계값을 바꾸거나 complete를 성공시키지 않는다. 실행 맥락/입력/호출 순서를 계속 조사한다.
artifact12579644746은89,422,284B/SHA256
`969a95631b016965433e9be0135c9656e93158a2e5a317f478daa6311f200a4f`,139CRC·해제359,776,736B다.
final-seals pass/failures[]/all_owned_children_reaped true, finalizer_errors[]와 source/index/
전체SDK 불변성을 확인했다. 동적 module provider before/after2쌍 차이는 별도로 읽으며
그 차이를 file 변조나 complete provider closure로 단정하지 않는다. 실제 BLAS/kernel 관측·
역사 전체 provider closure·normal40/전체 runtime 수용·원본 기대값 재생성·retirement는 false/0이다.

## f55 정확 소스의 집중 수용과 PR 후속 상태

source `f55b3b6d34ca709b924c68d93a02061186fc27b9`의 다음 네 실행은 성공했고 부모가
원문 ZIP의 CRC와 봉인·최종 단계까지 감사했다. 이 수용은 각 실행 범위에 한한다.

| 실행 | 실제 증거와 범위 |
|---|---|
| [Fast40145793038](https://github.com/bin9208/openpilot-rust/actions/runs/40145793038) | 7leaf/aggregate 성공, leaf215~366초. 7ZIP2,557,680B/279CRC/불변42쌍. source index19433과 SDK165files/188roster/4program이 같고 final/upload/source·job cleanup 성공. cold323exec/172실패·cargo1/rustc37/Python0/rustup0. raw SHA `5d0558cf1454b397317bbc6237dec3d48cb32881f7dcec25ba2b56808319d859`. |
| [Focus40145793112/job121026720320](https://github.com/bin9208/openpilot-rust/actions/runs/40145793112/job/121026720320) | target10의 고정SSH26/parser13/setup21와 importer 음성7, 기존 Unicode/SSH27/camera2/Clippy 및 불변15쌍 성공. artifact12580262358=31,703,880B/SHA `8b4e484889e27a26cfafe0f4fb8122ba3400a229c46983199e659160b71b1c8f`/95CRC. 현재 실제 SSH entry/원본부재·제거 credit은0. |
| [setup40145793011/job121026720154](https://github.com/bin9208/openpilot-rust/actions/runs/40145793011/job/121026720154) | 원본부재·고정21/거부3/공개 missing-argument1(exit1)/부재 통제6, watch9와 Clippy 성공. held123 unique 전후 동일. Git 입력4행 차이는 moment 라벨뿐이다. artifact12581191841=10,514,292B/SHA `561801c384126f863c3305077d5ce46bd43a4b6406c8d1743ed5ddb1cd97f69e`/173CRC. source/index/SDK/upload/소유 정리 성공. sampling은 연속적 부재 보장이 아니며 직접 소유 group 회수와 전체 adopted-child closure를 구분한다. |
| [collision40145793015/job121026720274](https://github.com/bin9208/openpilot-rust/actions/runs/40145793015/job/121026720274) | reference env 정렬 보정 후 current8 checker exit1/spawn0/foreign 불변,80batch1791files/72,410,679raw bytes/6796events·lines/root1/정확1회 coverage와 누락·중복 통제 성공. held210 unique 전후 동일, finalizer0/immutable true. artifact12581116962=38,870,012B/SHA `ce77c8d6058abb6342ab1f2b1bd2d0ee2532787684b1c2ac8378f32724721544`/8384CRC. |

current8은 원문 manifest의8개이며 단순2×4조합이 아니다. Unix socket2개는 regular bytes
대상이 아니며 metadata와 foreign 보존 증거를 사용한다. old27/runtime2/실제 caller/원본부재/
닫힌 PID·cwd·FD·DSO 및 build closure는 여전히 미수용이다. #549 전체 종료·retirement credit은0이다.

PR343을 force 없이 f55로 fast-forward 했다. base dev는
`b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`, 실제 merge는
`d75103ba3114f90a1a2686d772e468065b600ea2`다. PR은 draft/open이며 dev/main에 병합하지 않았다.
필수 검사는 새 merge별로 읽으며 과거 성공을 대입하지 않는다.

- [PR Fast40146470979](https://github.com/bin9208/openpilot-rust/actions/runs/40146470979)는
  routing의 cold raw166행 `2775  ???( <detached ...>`를 거부해 실패했다. native ZIP12580607643은
  584,380B/SHA `20c5f2847a07e8755cf9422609bf6e1f79ceb461a9dbd3ed5ef058eef28cc934`/64CRC다.
  debug21,780,398B에서2774 rustc -vV의 EVENT_CLONE과2775 startup/종료0를 확인했다.
  syscall-info 실패 errno와 해당 stop의 syscall은 아직 미확정이다. tracer/collector0·reaped true와
  `seccomp filter enabled` 관측은 unknown 해독 완전성의 대체 증거가 아니다. parser 거부를 유지하며 별도 결함 [#560](https://github.com/bin9208/openpilot-rust/issues/560)에서 추적한다.
- [Integration40146471172](https://github.com/bin9208/openpilot-rust/actions/runs/40146471172)는
  build의 source-final에서 같은 DBC2/font4 변경으로 실패했다. artifact12580983711은
  5,520,134B/SHA `b27994b149fba438035f46246fb21a4a26f5d2abf257aa65fd566bf246b33335`/47CRC다.
  실패 진단의 manifest exit0와6파일 원문 변경을 확인했다. #557의 외부 producer/실제 consumer
  경로 후보를 준비했으나 새 빌드 수용은 아직 없다. 기대값·tracked 원문을 덮어쓰지 않는다.
- PR docs40146471056은 성공했다. Rust40146470996은 이전 실행 뒤 대기 중이며 아직 수용하지 않는다.

#553의 다음 prefix600+target1은 같은 VM/held plain ELF·환경에서 이력만 비교하는 진단 후보다.
현재 oneframe의602 미재현 FAIL·기대값/normal40/tap16·기존 상한은 유지한다. #559 UI pending-close
경쟁의 deterministic RED/GREEN 후보와 #554 Git index preload의 한 변수 관측도 기능 검증 전이다.
baseline3580=293교체/3부분/3284대기와 전체 #523/#296 미완료 상태를 유지한다.

## ca8 진단 준비 실패와 다음 기본 runner 후보

[40148461061/job121034763604](https://github.com/bin9208/openpilot-rust/actions/runs/40148461061/job/121034763604),
source `ca8fd1119ca16aaefe0f99676d5b3ed7d5874df2`는 artifact metadata의 gh exit0 뒤
살아 있는 adopted child가 남아 실패했다. 수치1frame/prefix601/빌드에는 도달하지 않았다.
artifact12581955912=3,733,770B/SHA `9e0ff4ab02fb199ec6cd0ac2b049e48fc9bd2d3173b3e52f0329acb3b9859473`/68CRC·해제20,321,232B다.
PID3217(exit0) 뒤 같은 session의 PID3224/PGID3224/stateR을 발견했고 오류 cleanup이-15로 회수했다.
최종 all_owned_children_reaped true지만 numerics/originaldeps/packages/python-provider baseline 미준비로
final-seals false다. 원본부재·수치 성공으로 해석하지 않는다. [#561](https://github.com/bin9208/openpilot-rust/issues/561)에 분리했다.

공식 [GitHub CLI 환경 문서](https://cli.github.com/manual/gh_help_environment)의 GH_TELEMETRY=false를
인증 전용 env에만 고정하고 실제 gh --version을 남기는 최소 후보다. 공식 source의 telemetry
flusher는 detached gh send-telemetry/Setpgid를 쓰지만 실제3224의 exe가 없어 이 attribution은 가설이다.
수치 실행 환경·기대값·시간/자원 상한과 살아 있는 자식의 즉시 거부를 유지한다. 재발 시
환경/argv를 수집하지 않고 PID stat/exe만 실패 proof에 남긴다. 사용자/global gh 설정과 provider는 바꾸지 않는다.

#557 후보는 기존 scons/native_models/camera/exposure/pytest570을 유지하면서 external producer와
완료 후 consumer root2개 및 실제 회귀6개를 build_release에 연결한다. 실패 시 manifest/atlas4와
stage roster/SHA를 보존하고 검사 성공·source seal·업로드 뒤에만 소유 source를 정리한다.
독립 검토가 찾은 GitHub 미지원 YAML merge key는 명시적인 해당 cleanup step 복사로 보정했다.
이후 실제 빌드·동등성·전달은 대기이며 release pack overlay도 여전히 미구현이다.

#559 후보는 기존 writer8과 close의30/10/5·FIFO60/8·write_all을 유지하고 worker의 detach 취소만
제거한다. 새 ACK는 실제 첫 write_all 완료를 보류하고 닫기 뒤 queued frame2개가 끝까지 전달되는지
검사한다. 별도 clone의 과거 취소 overlay가 정확514560/1543680bytes·exit101로 RED여야 하고,
현재 selector1/전체9/Clippy/원문5mode의 실제 record·dynamic3frame GREEN을 새45분 job에서 요구한다.
합산20.5GiB admission/두 target8GiB/proof1GiB·20분 실행구간은 추정치이며 실제 fit은 대기다.
원래 record2/expected3의 실행 순서 자체를 아직 관측하지 못했으므로 root 확정이나 차량 수용은 아니다.

이전 e882의 planner/WebRTC90분·Xiaoge75분 취소는 check-run annotation으로 실제 timeout을 확인했다.
[공식 표준 runner 문서](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)는
공개 저장소에 ubuntu-24.04-arm을 제공한다. 신규 inventory workflow는 이 표준 ARM VM의
identity/df/memory/OS/설치 tool path·SHA만 최대1MiB로 읽는다. checkout/프로젝트/원본 실행이나
기존 ARM gate 전환은 아직 없으며 actual 기능·physical bare-metal·기기 수용을 주장하지 않는다.

실행-edge는96행(26replaced-local/66pending/1native-existing/2non-executing-text/1removed)이다.
f55가 실제 수용한 developer_cli_original의 embedded 원본1행만 replaced-local로 바꾸고,
#557 새 Python test/embedded stage 검증2개는 pending으로 추가했다. baseline3580 파일 교체는
293/3/3284로 유지하며 #523/#296 전체 완료는 아니다.

현재 후보의 exact reuse 명단은201경로/179literal SHA다. staged source 기준으로 모든 변경경로와
SHA를 대조했고 Focus 실행부의 기존 SHA `2a05a74b9bc4c5d4c4793ac11259d1c4f02b2fcaa599ebd03a3f92416e80c84a`는 그대로다.
로컬은 YAML5/Bash76본문/Python AST10/shell1과 diff만 확인했다. 기능·RED/GREEN·빌드 성공은
이 정적 확인에 포함되지 않는다. #554/SSH entry의 다른 agent WIP는 별도이며 이 후보에 섞지 않는다.

## 202a 실제 실패 분리와 다음 수정 후보

- [Fast40149186502](https://github.com/bin9208/openpilot-rust/actions/runs/40149186502)는
  tools의 UI application 검사3개와 routing의 workflow 정책4개 때문에 실패했다. UI는
  #557의 폰트 경로 변경을 과거 전체 source SHA가 거부한 것이며 FPS/stack 출력 차이가 아니다.
  정확한 font 변경3구간의 길이·SHA·유일성·순서를 확인한 뒤 메모리에서만 역투영해
  역사38593B/`d17a3d0e8cb67c2bd37e4f48d61dc79e5ee7c7af984f0d9dc434301121a85b00`을
  다시 요구한다. 기존5개 본문/원문과 새 공개 check() 경계4개를 Actions에서 검증한다.
- [build40149194155/job121036927945](https://github.com/bin9208/openpilot-rust/actions/runs/40149194155/job/121036927945)의
  전체 SCons 빌드는 성공했으나 외부 font output의 .ready가 없어 consumer admission이 실패했다.
  뒤의 native_models/camera/exposure/pytest570/생성물6회귀는 미실행이다.
  artifact12582183272=4,575,380B/SHA `cd17d3851cfd12263ca274e54c52003a3e64dca6676fbc65ca836b6416ddc31b`,46CRC다.
  source/index 전후는 동일하고 DBC103/manifest/.ready는 있으나 font root는 빈 directory였다.
  staged font에만 `Default(Dir("#"), font_nodes)`를 추가해 기존 root default와 외부 font target을
  함께 요구한다. 실제 새 build/consumer/동등성은 아직 UNRUN이다.
- [UI40149186556/job121036901407](https://github.com/bin9208/openpilot-rust/actions/runs/40149186556/job/121036901407)는
  preflight에서 `command -v true`의 builtin 이름을 외부 ELF 경로로 넘겨 실패했다.
  artifact12582366637=339,380B/SHA `67d32f837e6aa80a758094dd7d95ce8e17996c83548b1000a600dd741c800d2a`,54CRC다.
  새 후보는 provider loop만 `type -P`와 절대 경로 요구로 보정한다. RED/GREEN/빌드/원문5모드는
  여전히 미수용이며 unknown provider 허용이나 불변 검사의 생략은 없다.
- [diagnostic40149186523/job121036901448](https://github.com/bin9208/openpilot-rust/actions/runs/40149186523/job/121036901448)는
  gh version/metadata/download가 각각 exit0·소유 자식 회수에 성공해 #561의 인증 경계를 통과했다.
  전체 결과는 수치602 미재현으로 FAIL이다. artifact12581727858=89,703,307B/SHA
  `e5106677eb853b4bba3024868be70a6be159ed885f7aee7e166f2639d05c0dff`,146CRC/해제368,022,203B다.
  원문/native tap4call의2329bit는 동일하고 first_divergence는 null이다. 같은 held plain ELF·환경의
  prefix600+target1도 현재 oneframe과 동일하며 과거602의 native 차이는 재현하지 못했다.
  final-seals pass/실패0·all_owned_children_reaped true다. Python provider 전후 차이는
  first_observed_after 관측 목록뿐이고 files/held/libpython은 동일했다. BLAS 실제 호출·kernel 및
  과거 provider 전체 closure는 미관측이며 normal40 수용·수치 기대값 갱신·credit은0이다.

표준 ARM inventory의 실제 결과와 원래 CarrotMan job 하나의 native VM 후보는
`python-rust-523-native-arm.md`에 기록했다. 정책은 명시 intent와 실제 ARM source/runner 경계가
있는 정확 label만 추가하며 inventory queue:max와 generated consumer cleanup의 더 강한 success
조건을 검증한다. 기존 거부 경계와 source/provider seal은 유지한다.

#495의 실제 ACK 뒤 stop 지연은 proof 완료보다 producer exit 관측을 늦게 기다린 경계였다.
proof/inputs close 뒤 atomic 완료 marker를 게시하고 관측한 다음 기존20ms drain·strict100ms를
유지한다. stop 뒤에도 producer의 실제 exit0를3초 안에 요구하며 kill/timeout은 성공이 아니다.
기존8개 본문은 그대로이고 신규4함수11case는 미실행이다. #554의 한정 Git preload 환경3키와
2048파일/4상태의 true/false 상태 동등성 후보도 기존17case·2초/600초·trace 기준을 보존한다.

f55 PR Rust의 model-pipelines NumPy 누락은 lexical venv Python을 readlink로 base interpreter로
바꾼 caller에서 발생했다. 그 env 한 필드만 lexical 경로로 보정한다. startup retirement의 고정
/tmp helper2개는 현재 TMPDIR와 기존 /tmp 형식만 인정하도록 보정하고 음성 경계2개를 추가한다.
model-family의 collector 원문은 기존 no-collector와 다른 provenance pair이므로 #562에서 별도
검토한다. 기존 helper SHA를 무작정 갱신하거나 collector flag만 허용하지 않는다.

이번 수정은 독립 source review 뒤 로컬 문법/서식/diff만 확인한 후보다. 새로운 기능·빌드·Clippy는
Actions 전까지 UNRUN이며 실행-edge98행의 추가2개는 pending이다. baseline3580의293교체/3부분/
3284대기와 전체 #523/#296·필수 PR/통합/전달/실기 미완료 상태를 유지한다.

현재 index의 검토24파일만 선택했고 별도 SSH entry 및 #562 WIP는 포함하지 않았다.
YAML6/Bash본문761/Python AST4/독립 Bash3과 선택 Rustfmt·diff를 확인했다.
Focus exact 허용211경로/188literal pin 및 baseline delta211개가 일치하며 실행 영역 SHA는
`2a05a74b9bc4c5d4c4793ac11259d1c4f02b2fcaa599ebd03a3f92416e80c84a`로 유지됐다.
main Rust workflow498181B의 parsed 변경은 model-pipelines의 lexical Python env 한 필드뿐이다.
이 정적 확인은 실제 compiler/test/원문 실행의 성공을 뜻하지 않는다.

## 6a 실제 빠른 수용과 남은 실패

source `6a231dc938d35e9da80e0615cfb2570e64f23911`의
[Fast40151446382](https://github.com/bin9208/openpilot-rust/actions/runs/40151446382)는7leaf와
aggregate가 성공했다.7 ZIP 합계2,552,643B/279CRC와 source/index·SDK file/roster의 불변42쌍을
전수 감사했다. UI application 기존5+새4=9개가 모두 성공해 #557의 source projection 경계도
확인했다. routing의 현재 정책33개 및 cold 감사가 성공했다. cold raw SHA
`5fd676da17b329948ffd2eba9ce929163bd218ce76a2e0ec528f789c41d2c271`의323exec/실패172,
absolute Cargo1/rustc37·Python0/rustup0을 확인했다. 비재발 한 번으로 #560 경합을 종료하지 않는다.

[Focus40151446395/job121043654395](https://github.com/bin9208/openpilot-rust/actions/runs/40151446395/job/121043654395)도
성공했다. artifact12581479973=31,703,902B/SHA
`2bea91d94bcb77701ac566767aa6ec2b8dc8bb61924b193b901e43a42923fab1`,95CRC/해제40,476,535B와
불변15쌍을 확인했다. frozen SSH39/setup21·importer7을 포함한10tests, 영향 SSH27·Unicode·camera
정책·Clippy와 실제 source/SDK/venv·고정입력 전후가 성공했다. 아직 새 SSH entry actual 경계나
원본 파일 추가 제거 수용은 아니다. native CarrotMan12개 실제 owner 비교도 성공했고 별도 ARM
문서에2 ZIP/146CRC 및 실행 범위를 기록했다.

UI40151446362는 아직 preflight 실패다. type-P 이후 which.debianutils의 정확 `#! /bin/sh`
표기를 기존 script provider가 거부했다. artifact12581699376=380,996B/SHA
`7cf00d499a497d0a6b77a7ed19975202a8a076246de5271171d3cd750fd50377`,54CRC이고 held312개 및
source/index/SDK5쌍은 동일했다. 같은 /bin/sh를 가리키는 정확 표기 하나만 추가한 후보이며
RED/GREEN/전체9·원문5모드·빌드는 여전히 UNRUN이다.

collision40151446301/job121043653805는 checker-build 중 oversize find가 사라진 target entry를
보고한 뒤 resource watcher가 종료해 실패했다. artifact12581509647=7,088,783B/SHA
`6d7a36bbcabbcad0375905dbe2092713bef636f77b4dfdf6ac3eda3d58c33313`,2437CRC다. actual narrow8은0회,
held141개와 source/index/SDK는 유지됐다. 후속 조사에서 job log의 target ENOENT5개를 확인했고
관측 파일의 최종 덮어쓰기는 #563에 기록했다. 사건 당시 용량 부족·Rust 오류·#554와 같은
원인으로 단정하지 않는다. 기존 usage와 같은 target 한정3회 재관측·oversize 즉시 거부 및
실패 순간 sidecar 보존을 준비하며 실제 수용은 대기다.

logging40151446356/job121043653993는 Git preload4state 동등성·env schema1·retained core21·prepare9·
trace20·summary6·Clippy와 처음3lifecycle을 통과했으나 runtime_directory_int의 실제 backpressure
총3.265316554초가 원래2초보다 커 실패했다. artifact12582772050=33,846,507B/SHA
`67e6020444172f624ac438fa6a46fbd3bc15c8e9cd313d6fde3540863e4dcb20`,2686CRC/해제390,108,825B다.
start_peer2.97520767/command0.182980405/finish0.107128479초이며 해당 Git status11개가 각각 약
2.8~3.0초/37318syscall을 기록했다. preload false만으로 해결됐다는 가설은 수용하지 않는다.
공식 untracked cache의 filesystem 검증·현재 status byte 동등성 및 private index 준비를 다음
한정 후보로 검토하며2초/600초·전체 trace·dirty 판정은 유지한다.

#561은 exact202a의 인증·자식 회수·고정 archive·수치 단계 진입과 final-seals 결과를 확인해
개별 종료했다. #553 수치602 미재현 FAIL 및 전체 normal40는 계속 열려 있다. build40151465044는
원래 전체 SCons/native_models/camera216assertions·4cases/exposure972transitions/pytest570과
새6회귀·source/index seal·upload·소유 정리까지 성공했다.3 ZIP196CRC와 DBC97/103·font14쌍·
실소비 기록을 감사하고 #557을 개별 수용했다. 자세한 SHA와 한계는 generated-assets-557 문서에
기록했다. 전체 PR/패키지 전달 또는 원본 제거 수용은 아니다.
PR343은 아직 f55 head/dev draft다. 새 #562 tuple 및 SSH actual 연결/native planner·WebRTC는
검토 후보이며 baseline3580의293/3/3284와 전체 전환·통합·전달·사용자 실기 미완료를 유지한다.

## 다음 검토 배치 — 기능 UNRUN

새 SSH Actions는 현재 SDK로 만든 test/CLI/recorder와 실제 공개 helper를 exact4selectors로
연결한다. 기존 원문39/27·Clippy를 보존하고 별도 private Cargo home2GiB/target6GiB/build8GiB·
proof512MiB admission, env-i·FD/DSO/registry 전후·actual exec/mmap·중첩 worker marker/회수를
확인한다. 원격 계정/차량 접속과 원본 삭제/부재 credit은0이다. 두 독립 source review에서
추가 P1/P2가 없었으나 실제 registry/fingerprint/provider/자원 fit은 새 Actions의 판단 대상이다.

#563은 oversize 양성 목록을 즉시 거부하고 정확한 사라진 target entry만 최대3회 재관측한다.
proof/registry/권한/현재 존재·dangling/traversal/다른 code/혼합 오류는 즉시 거부한다.
모든 시도의 원문과 첫 실패를 보존하며18개 해석 통제는 synthetic으로 명시해 실제 compiler/
현재 collision8 수용과 구분한다. cap·예산·시간·trace와 원래8개 기준은 유지한다.

#554의 source Git cache는 private index에만 준비하고 실제 filesystem test와 현재 status의
모든 channel 동등성, tracked entries/flags/source/config/held/SDK를 다시 확인한다. 별도8상태
cache invalidation과 기존 preload4상태를 보존한다. source prime status1회와 이후 readonly
조회·기존 fixture 자식 환경을 구분하고2초/600초 및 dirty 판정을 유지한다. 캐시 효과나 실제
성능 성공은 아직 수용하지 않는다. native planner/WebRTC와 #562의 범위는 각각 별도 문서에 있다.

선택한 index25파일의 YAML5/Bash본문68/embedded Python AST6/단독 Bash4 및 선택 Rustfmt·diff를
확인했다. exact 허용223경로/197literal SHA와 baseline delta223이 일치한다. Cargo model은
기존 SAN/CFI 및 새 SSH test의 검토한3등록뿐이다. main Rust workflow498181B는6a와 동일하다.
새 SSH 실행 단계와 Clippy 등록에 따라 Focus의 실행 영역 SHA는
`9b851b03193ced1673bfe00c9e8ccdc35204d9ab60c20b916a7bb89f4ee5560c`다. 이 새 영역을 과거
실행과 동일하다고 재사용하지 않는다. 실행-edge100행은26교체/70pending/1기존native/2text/1removed,
baseline3580은293/3/3284로 유지한다. 로컬 기능/빌드/원본 실행0이며 모든 새 수용은 대기다.

fd3f3d022의 Fast40153172373/job121048794571은 현재 정책33개 중32성공/1실패다.
새 collision8 step의 trusted source 뒤 outcome 조건에 괄호가 없어 기존 정책의
untrusted source step bypass 거부가 발생했다. 조건식의 outcome만 괄호로 감싸 기존
형식에 맞춘다. predicate·fork 거부·정책 검사기 자체는 동일하며 새 기능 수용을 뜻하지 않는다.
source admission의 해당 workflow literal과 원장 blob만 함께 갱신한다. 같은 fd source의
SSH/UI/로깅/native ARM 실행은 계속 진행하며 별도 결과로 확인한다.


## fd/0f 실제 기본 러너 결과와 다음 최소 후보

- source `0f846b6461e87fadfd50fbc739182e05b9a327fa`의 [Fast40153700516](https://github.com/bin9208/openpilot-rust/actions/runs/40153700516)는7leaf와 aggregate 성공이다. artifact7개 합2,561,465B/279CRC를 전수 감사했고 source/index/SDK 불변42쌍이 같다. fd의 routing policy 괄호 오류는 같은 predicate의 문법 보완으로 해소했다. 디스크·메모리 관측 전후 변화는 불변 봉인과 구분한다.
- source `fd3f3d02293ff4ce71b40733fa7c46c9ae9e7d1c`의 [native ARM40153172365](https://github.com/bin9208/openpilot-rust/actions/runs/40153172365)는 Planner/WebRTC 두 job이 성공했다.4ZIP 합79,855,561B/29,855CRC/해제259,009,102B를 전수 감사하고 private source/index 각2쌍을 확인했다. Planner result는 aarch64/PASS이며 ABI12 bit-exact solves, policy·radar39·gap42, owner/main/fault/VW/stock-longitudinal/radar-off, 실제IPC4회 각121messages·비교차이0, lifecycle13 source/native 사례를 기록한다. source driver의12 commands는 모두exit0다. WebRTC는 기존 비교10명령을 유지했고 실제 ARM ELF·FIR5쌍·Params ownership2행의30→31→30 및 stopped/peer closed를 확인했다. 이는 GitHub ARM VM 증거이며 baremetal·기기·전체SDK/DSO closure·전체 전환 수용은 아니다. main required workflow의 기존 QEMU 범위 대체는 별도 후속이다.
- 같은 fd의 [collision40153172313/job121048793998](https://github.com/bin9208/openpilot-rust/actions/runs/40153172313/job/121048793998)는 성공했다. artifact12583532625=39,383,594B/SHA256 `21591b355402b1707018bf8b2dca4008441ea7cd9233bb0affc7723193096ea9`,10,180CRC/해제393,732,572B다. oversize 음성18개와 현재 실제 충돌8개 모두 성공했다. checker exec8/exit1=8/spawn0, foreign 보존true, 원본/probe실행0이다. source/index/SDK 및 명시 불변31쌍이 같고 초기held210경로는 종료 누적목록에서도 identity/SHA가 같다. foreign provider receipt의 의도된 테스트 값과 disk 변화는 불변 판정 대상이 아니다. #549의 나머지 runtime/old27/caller/absence/closed trace 및 credit0은 그대로다.
- [fd Focus40153172247](https://github.com/bin9208/openpilot-rust/actions/runs/40153172247)는 native7/public1 및 기본음성2 성공 뒤 초기held snapshot의 동일 ELF 중복 행 때문에 실패했다. #564에 원인과 정확14개 기준을 유지하는 최소 prefix 후보를 기록했다. 현재0f Focus40153700481도 같은 단계에서 실패했으며 성공으로 재표기하지 않는다.
- [fd UI40153172341](https://github.com/bin9208/openpilot-rust/actions/runs/40153172341)는 preflight 통과 뒤 baseline-build에서 필수 IMAGE_CODECS_SDK_ROOT 누락으로 실패했다. artifact12582939463=10,229,984B/SHA256 `3e6810e11f56b96162480080d5cc8d84949b549b13336ab55541156ffef2bc63`,3,082CRC·불변30쌍·held332개를 확인했다. 기존 native decoder stager/9archive pins/실제 disk guard를 연결하는 최소 후보를 작성했다. build8GiB/tools2GiB와1200초 총기준 및 RED/GREEN·기존9·원본5 기준은 유지한다. 실제 새 SDK stage/후속 기능은 UNRUN이다.
- [fd logging40153172264](https://github.com/bin9208/openpilot-rust/actions/runs/40153172264)는 source cache/8state 및 첫 lifecycle3이 성공했지만 runtime_directory_int가2.405423463초로 기존2초 기준을 넘었다. Git status는 실제 checker의 Peer::start에서 매회 수행되며 기존 원본 checker도 같은 시간 범위에 포함한다. logger/runtime-version의 추측 변경이나 dirty 검사 생략은 하지 않는다. 별도 같은held ELF 비계측1회와 현재계측 관측으로 원인을 좁히며 기존2초/600초·packet/expected를 유지한다.

#553은 같은 held plain ELF·입력·absolute output argv로 LD_LIBRARY_PATH만 다른 A/B2회를 추가한 진단 후보를 작성했다. 기존602 미재현 FAIL과 정상40의 수치 기준은 그대로다. 소스 독립 검토는 #553·UI SDK·#564 모두 추가P1/P2가 없었으며 로컬 검사는 AST/YAML/Bash/Rust 서식·diff뿐이다. 후보의 기능은 새 Actions 전까지 UNRUN이다. baseline3580=293교체/3부분/3284대기, execution100개 상태 및 #523/#296 전체 미완료는 유지한다.


#563은 fd 실제 `probe-build` watcher의 target 임시 archive ENOENT(find1/decision75/attempt1)와 두 번째clean scan, 독립 검토를 확인해 개별 종료했다. 첫 실패의meta/stderr/표식은 최종 admission 뒤에도 업로드됐으며 옛6a의 덮인 원문을 복원했다는 뜻은 아니다. actual18+8 및 마지막봉인/실패원문upload 조건은 위fd 실행으로 충족했다. 전체#549/retirement credit0은 유지한다.


## PR593 전체 검사에서 드러난 source 계약·fixture 보완

실제 source merge `40562f2d8068878a00b18e12f355799b4fe0405d`의 Integration40161295629와
최신 user-docs40161450821은 성공했다. Rust/Fast 전체는 실패이며 native8 전체 통합 수용은 아니다.

- Fast40161295530 routing은 main-only544/558, services는 main proxy에서 RadarCAN body lookup으로
  실패했다. 실제 cold audit는unknown0으로 통과했지만 #560의 다른 실패가 해결됐다는 뜻은 아니다.
  exact same-source caller/proxy8gate를 먼저 요구한 뒤 이동한 RadarCAN11/Navi3 guard만 callee에서
  조회한다. 기존558guards/37jobs·세부growth/실제artifact 실행 roster를 유지하고4test/23변조를 추가했다.
  두 ZIP12586692129/12586143750은89/34CRC와 전후20/6쌍을 각각 감사했다.
- Card job121073229217의1272CarParams/35520state·actuator/91316CAN 비교는 성공했다. 뒤의 old41needs
  계약만 새native-arm 항목을 거부했다. old41을 유지하고 exact42와 caller/8leaf/aggregate/proxy를
  연결하며10개 누락·skip·재연결 음성case를 추가한다. 실제 수정 후 source-contract는 Actions 대기다.
- startup job121073229115의 raw receipt fixture는 즉시 종료 /bin/true의 초기 write와 종료가 경합해
  EOF 대신 EPIPE를 받았다. [#566](https://github.com/bin9208/openpilot-rust/issues/566)의 후보는 이
  test만 immutable echo peer로 complete non-ready handshake를 만들고 모든 raw path/env/SHA/receipt
  검사를 유지한다. 제품 protocol/원문 expected는 변경하지 않는다.
- workspace job121073229126의 tunnel unit fixture는 존재만 확인한 descendant.pid가 아직 empty여서
  실패했다. [#484](https://github.com/bin9208/openpilot-rust/issues/484)를 다시 열고 test producer의
  leader/descendant PID를 같은 private directory의 part→rename으로 원자 공개한다. newline 정확
  파싱과 기존5초/2초·같은session/다른group/무관child·완전 회수 판정은 유지한다.

이 source 후보들은 로컬 source/문법/format/diff만 확인했으며 기능 성공은 다음 Actions의 정확
소스에서 판정한다. 원래 check 수치/모델/안전/유효성 기준과 전체 PR gate를 생략하지 않는다.


### 8ae Fast가 새로 노출한 final evidence condition 계약

[Fast40163930502/job121081003864](https://github.com/bin9208/openpilot-rust/actions/runs/40163930502/job/121081003864),
source8ae95f733의 ci_artifact_list가 새 resolver에서 기존 native ARM policy의 양성 검사를 먼저
실행하다 `native ARM stage condition changed: Preserve bounded final binaries and their identities`로
실패했다. 실제 radard artifact 보존 step은 원래 trusted AND always인데 validator의 기본trusted-only
분류가 잘못됐다. 기존 PR593에서는 더 앞선 ci_disk failure로 policy target 전체에 미도달했다.

수정은 이 정확 stage명 하나의 expected만 기존 TRUSTED_ALWAYS 상수로 연결한다. 다른 기능stage의
source 조건은 그대로이며 WF 실행 body/조건을 변경하지 않는다. 기존 실제 전체callee 양성은 유지하고
조건삭제/trusted-only/always-only/false/success-only5개를 거부하는 새 회귀1개를 추가했다.
원래 failures에서도 증거를 남기는 동작과 fork 거부를 모두 요구한다. 기능과 Fast 전체 수용은
다음 정확 source Actions에서 판정하며 숫자558/37 또는strict 원문 기준을 낮추지 않는다.


### db Fast의 남은 native stager 조회 scope

[40164444310/job121082845887](https://github.com/bin9208/openpilot-rust/actions/runs/40164444310/job/121082845887)의
ci_artifact_list는 앞선 조건 오류를 통과했으며, 뒤의 ci_native_staging이 main21/기존22 caller 차이로
실패했다. 이동한 RadarCAN의 Params caller 하나도 exact caller/proxy/callee 검증 뒤 기존 shared
resolver에서 조회하도록 연결한다. expected22와 template1, 모든 실제 stager 준비·명시 선택·음성 변조
본문은 그대로 유지하며 전체6 ARM 중복 합산이나 count완화는 없다. 기능 수용은 다음 Actions 대기다.


같은 db Fast routing은 strict cold 실제 audit1과 ci_disk_bootstrap35(ignored1)를 통과했다.
ci_workflow_policy는40PASS/1FAIL이며 새 Fast UI diagnostic target을 독립 exact suite roster에
추가하지 않은 것이 원인이었다. 기존72개와 순서/한 번 실행 판정은 그대로 두고 새 target1만
추가해73개를 요구한다. CI target을 추가한 부모 변경의 등록 누락을 보완하며 skip/중복/기존 제거를
허용하지 않는다. 이후 정확 소스 Fast 전체 성공은 아직 대기다.


독립 Sol source 검토가 발견한 추가 등록 누락도 반영했다. 새 UI target은 native-tools 부분 합계에
포함되므로53→54, native+tools 합계57→58로 정확히1개씩 늘린다. 기존72개/분할/순서/단일 실행 및
새1개 외 항목은 보존한다. 전체73만 바꾸고 부분 합계를 남기는 오류를 source 검토에서 정정했으며
actual 수용은 여전히 다음 GitHub Actions의 결과를 요구한다.


## a483 기본 러너 수정 묶음과 정확 PR 재검증

2026-10-25. `a4838fc4e7d09499d1dbc50045ec049af995ffbc`를 issue branch와 PR343 head에 force 없이 fast-forward했다. 정확 merge는 `e4f7d4a868f3aaee36a1b485af29923f2e24cee6`이며 dev/main 통합은 아니다. 새 필수 [Rust40166518729](https://github.com/bin9208/openpilot-rust/actions/runs/40166518729)·[Integration40166518680](https://github.com/bin9208/openpilot-rust/actions/runs/40166518680)·[Fast40166518662](https://github.com/bin9208/openpilot-rust/actions/runs/40166518662)·[docs40166518613](https://github.com/bin9208/openpilot-rust/actions/runs/40166518613)는 대기/진행 상태로 기록한다.

두 ARM 작업의 새 정확 SDK 준비와 policy 양성/음성 연결, SSH 용량 관측의 한정 ENOENT 재검사, controls getter 컴파일 구문과 UI alias jq 구문, Fast role/suite 등록 및 cold 최초 admission 실패 보존을 독립 검토했다. staged26의 YAML5/Bash135/embedded AST9/Bash파일6/Python AST2·Rust서식5·diff와 sourceguard가 통과했다. main489835B 불변/Focus allow251·pins218·외부 block SHA 불변이다. 신규 SDK metadata의 Python TOML 실행을 숨기지 않고 pending1을 추가해 execution114=26교체/84대기/기존native1/텍스트2/삭제1이다. baseline3580=294/3/3283은 그대로다.

branch [a483 Fast40166180682](https://github.com/bin9208/openpilot-rust/actions/runs/40166180682)의 routing은 strict cold audit `unrecognized trace line159`에서 실패해 후속 policy 실행에 도달하지 않았다. artifact12588492365는589313B/SHA256 `b8609e26daf995d4dc6961862caa62386db063760d02773e36d85058234d2d4e`,78CRC/expanded25678090B다. 불명 행을 허용하거나 재작성하지 않는다. 다른 leaf/Focus/UI/controls/cold의 현재 실행도 완료 전에 수용하지 않는다.

[593 collision8 40161287114](https://github.com/bin9208/openpilot-rust/actions/runs/40161287114)은 실제 성공이다. artifact12587195042는39396698B/SHA256 `68c84975b3084dc1ca36a5d16069e26ac231530629cad664d9da45d5ba316078`,10146CRC/expanded394194477B다. digest4와 raw10파일663858B의 SHA/전후identity, 실제fixture1/checker8의 exactargv/env·exit1·spawn/session변경0·고정8순서·foreign8쌍 및 source3쌍/final0을 부모가 확인했다. fixture-build80분할/1793파일/72419761rawbytes의 성공 보고 및 누락·중복 음성 거부도 확인했다. current collision8만 수용하며 old27/runtime2/R4/caller/absence·closed PID/cwd/FD/DSO false와 credit0은 보존한다.

이전 PR593의 Integration/docs와 LLVM20 개별 작업은 성공했지만 전체 Rust는 미완료/실패 범위를 가진다. generic AArch64는 진행 중이며 기존 compile/root 실패의 수정 후보를 전체 성공으로 확대하지 않는다. 모델 signed zero3·controls54ULP·logging strict2초와 필수 게이트·통합 후·전달/실기 수용이 남아 #523/#296는 계속 열린다.


### a483 후속 확정과 fixed-true-554-v1 후보

branch Fast40166180682는6leaf 성공/routing1실패/aggregate실패로 확정됐다. routing의 cold 불명행 거부 때문에 policy 후속은 미도달이며 native-tools의 role 등록 수정은 실제 통과했다. PR593 Rust40161295597도52job=40성공/12실패로 확정됐고 LLVM20와 generic AArch64 개별 작업은 성공했다. generic artifact12588816327은75766641B/SHA256 `2954ed56f1fefb0886e84f76264e74468faee23277936dd6a0c97dc2bfd87cba`,83CRC/expanded202614489B이며 ELF49개의 class64/machine183(AArch64)을 inert로 확인했다. 네트워크로 첫 다운로드가 실패해 빈 부분 파일을 제거한 뒤1회 재시도했다. 기기 검증·설치 수용은 아니다. LLVM20 private artifact12588405827은3596244B/SHA256 `6e670d97b5eaa2adfcb792a535f16a09b265941dfc9c1bebaf05075357835982`,35CRC와 source/provider 전후 동일을 확인했다.

54b normal40/run40164032028/artifact12588865005는 실제 numeric 실패다. ZIP52138327B/SHA256 `75dbe6ea56cb79a6c34f256f0c5f5ae2c7436d6a1abfbd4acdb85a62b316a31c`,208CRC를 담당자가 감사했다. build/Clippy·negative2·trace40case/EXIT0 뒤 첫 `controls[3].rows[0].state.lat_pid[0]` 기대 `-0.034913528710603714`/현재 `-0.03491372987627983`에서 실패했다. source/SDK/243held는 동일하고 native failure seals pass true지만 외부 baseline unavailable로 최종 seal은 false다. tolerance·원문·700/600초를 바꾸지 않고 getter 진단을 이어간다.

#554에서는 원래 필수값이 아닌 실험 고정false를 별도 `fixed-true-554-v1` 검증 profile로 명시 변경하는 후보를 구현했다. Git의 기본 병렬 preload를 선택하되 status/dirty 확인·packet/console/rate·전체2초와14본문·공유600초·15prefix 및 raw 한도를 유지한다. 고유 A1=false/B=true/A2=false는 각1회 진단이며 raw index 차이는 원문을 보존하고 성능 비교 HOLD로 남긴다. metadata exact positive/음성13과 고정 profile·ABA 영수증을 workflow에 연결했다. 실제 기능은 다음 Actions UNRUN이고, 과거 false receipt·old27/R4·원본2 퇴역 및 credit0은 유지한다.


## SDK8·cold marker·UI workspace 지문 후속 후보

7b 이후 main의 WebRTC와 RadarCAN에서도 같은 `bin/cargo-clippy` 충돌이 발생해, main caller의 정확8 ARM role을 fresh SDK로 격리하는 후보를 작성했다. 원래 x64·기존 pilot·일반 bootstrap과 main489835B는 보존한다. SDK6파일 동결 helper는 `e23447e67190cafaa9c98ce7a558fc7c5208b310271b2cd5e5fa3b36865af764`이며 before20/after10·SDK3GiB와155payload 검증은 그대로다. Python metadata reader는498행으로 이동했으며 pending 상태다. 부모 policy는8양성/112음성 및 첫 maintained 명령 전 SDK를 요구한다. RadarCAN의 첫 imports와 실제 dependency는 다른 step이므로 dependency 이름으로 선택하며, 전체 기능 원문 비교를 유지한다. 새 default disk39/policy42와 SDK8 실제 기능은 다음 Actions에서 확인한다.

[a483 cold40166180782](https://github.com/bin9208/openpilot-rust/actions/runs/40166180782)은 strict native1PASS 뒤 libtest progress prefix와 marker-start 검사 차이로 실패했다. artifact12588821869는11572636B/SHA256 `8cb2c36c9447f0a8b0498868470f0b88bd13062b7537b214e1620cfdc8bebe7f`,540CRC/expanded98720546B를 담당자가 감사했다. observer abnormal0은 해당 VM의 미재현이며 다른 Fast unknown159 해결이 아니다. 기존 raw strict 판정은 유지하고, bare/정확 test prefix의 유일 JSON을 native receipt 전체·현재 source·실제 trace SHA와 대조하는 후보와10통제를 추가했다. final resource1 원인은 여전히 미확정이라 최종 admission만64KiB trace로 진단한다. reader/writer 상태를 분리하고 overflow/누락은 FAIL이다. cap·3attempt·1200초와 원래 source/provider 검증은 바꾸지 않는다.

[a483 UI40166180677](https://github.com/bin9208/openpilot-rust/actions/runs/40166180677)은 baseline RED·exact6 alias/음성5·독립 복사·current Cargo build0 뒤 workspace144 artifact 중12 fresh:true에서 거부됐다. artifact12588248598는27490467B/SHA256 `3ecee0c086f20557f3fed7e4207d6d85bc9e9e02c73b787f40e5af5d7341064d`,15798CRC/expanded163208313B다. #568에 원인과 후속을 분리했다. baseline의 local12/registry191 이름이 분리되고 fingerprint408개 중 local147개가 분류되는 것을 inert 원문으로 확인했다. 독립 복사 검증 뒤 current-private target의 local fingerprint만 제거·기록해 최초 current 전체 fresh:false를 유지하는 후보를 작성했다. baseline/registry/SDK·원래20분/용량·source4/RED/GREEN/recording9/원문5mode/Clippy 기준은 보존하며 실제 새 기능은 UNRUN이다.

[a483 controls diagnostic40166180678](https://github.com/bin9208/openpilot-rust/actions/runs/40166180678)은 unit2/getter4·중간2329bits 일치·loader2출력 동일과 final seals를 통과했지만602 실패state를 재현하지 못해 FAIL이다. artifact12588319492는89724404B/SHA256 `22fe0caa368306b8250e0e5eb0bfbd176633d6bab6ba8547548219bba00c5540`,162CRC/expanded368221660B다. normal54의 EPYC9V74(AVX512 포함)와 진단의 EPYC7763(AVX2/FMA)이 달라 원인 후보를 같은 VM에서 구분할 계획이다. 실제 BLAS/kernel·역사 provider·full40 수용은 false를 유지한다.

7b logging40166934406은 Actions SUCCESS이며 산출물의 별도 감사가 진행 중이다. 이를 old27/R4/caller/원본2 퇴역이나 전체 #523 완료로 미리 승격하지 않는다. baseline294/3/3283과 execution114/대기84는 그대로다.


### #566 checkout receipt 경합의 개별 수용 완료

실제 merge `e4f7d4a868f3aaee36a1b485af29923f2e24cee6`와 head
`a4838fc4e7d09499d1dbc50045ec049af995ffbc`는 tree5de13f49c18e5a551b65199ce791663a21140b71이 같다.
[startup prerequisites121088724978](https://github.com/bin9208/openpilot-rust/actions/runs/40166518729/job/121088724978)와
[startup services121088724942](https://github.com/bin9208/openpilot-rust/actions/runs/40166518729/job/121088724942)는
전체 SUCCESS이며 checkout7/reference3/strict Clippy, source/index final·upload/소유 cleanup을 통과했다.
raw command17/transcript17(576records)/cleanup17/provenance7을 감사했고 checkout9시나리오 PASS,
paired7 결과 동일, remaining_children=[]와 PID관측127 exists_after=false를 확인했다.
비UTF8 byte assertion의 실제 수행은 전체7 PASS로 확인하며 unit TempDir 원문이 archive에 따로
보존됐다는 주장은 하지 않는다.

artifact12588619755(104014061B/SHA e389f7d01898fc166204c561131163fab52f103c055ba240b071313228f22259),
12588663869(12196680B/SHA74668935c8f02d1b558d5591c598aaa5dc23bc9bab6c9225fcb9aea6cafab5f7)와
private12588619760/12588354497의 총98643CRC/전개294769377B를 감사했다. 두 source/index raw쌍과
owner/source/index19469경로가 맞고 source tar SHA는
`ee69055d1ab3f3043f72f546e8f2187ceebb05c50e87f37e18427e1e483300e6`다.
[#566 완료 댓글6395356795](https://github.com/bin9208/openpilot-rust/issues/566#issuecomment-6395356795)로 개별 종료했다.
이 성공은 전체 PR/#523/#296·Python retirement·기기 수용으로 확대하지 않는다. 기존 단독 artifact를
재사용했으며 새 다운로드·로컬 기능 실행·옛 receipt 재도장은 없다.


### a1c collision8의 검토된 fixture SHA 연결

[40178153221/job121123233098](https://github.com/bin9208/openpilot-rust/actions/runs/40178153221/job/121123233098)은
빌드 전에 `native_logging_lifecycle.rs` checksum mismatch로 실패했다. a1c의 test-only INT/TERM 선택을
추가한 현재 fixture는 `b6031484652376dd40eeb8074f3449ebdfb5c4fc225c5441f8d01ed78058f4a8`이며
선택 없는 기존 본문은 byte 동일하고 독립 P1/P2 검토도 완료했다. collision8 helper의 예전95bad9d0
literal1개와 그 helper의 WF/Focus pin만 현재 검토 source로 연결한다. fixture 검사를 제거하지 않고
8case/selector/기능·원문·상한·시간·source/SDK/final·cleanup을 그대로 유지한다. 로컬은 source/구문/diff뿐,
실제 현재8case 수용은 Actions 대기다. #549 전체와 원본2 제거·credit은 아직 미완료/0이다.
