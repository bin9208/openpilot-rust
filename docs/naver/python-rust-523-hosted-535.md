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
