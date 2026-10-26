# 대단원 #523 실행 기록

작성: 2026-10-24 KST. 상위 이슈: https://github.com/bin9208/openpilot-rust/issues/523

## 실행 기준

사용자의 전체 완료 지시에 따라 기존 #296의 승인된 전체 전환 설계와
`rust-repository-python-retirement-296-plan.md`를 이어서 실행한다.
기준 구현은 `7a3727e44`이며, 다른 작업의 미커밋 변경은 보존한다.
별도 작업 브랜치는 `codex/feat-523-python-rust`다. `dev-md` 런타임을
가져오지 않으며 검증되지 않은 Python 경로를 삭제하지 않는다.

## 진행 순서와 판정

1. 기존 실패의 실제 로그와 현재 소스를 대조하고, 이미 수정된 원인과 남은
   결함을 구분한다. 모델 수치·안전·시간·유효성 기준은 유지한다.
2. 독립적인 생성기와 개발 도구 전환을 진행하고, 공유 등록·검증은 부모가 통합한다.
3. 로컬 checkout에서는 문법·서식·diff만 확인한다. 기능·원본 비교·빌드·Clippy는
   GitHub Actions에서 실행한다. 작은 native 검사는 등록된 `bin-actions`의4코어/4GiB/80GB,
   jobs1·작업별 admission을 사용하고 ISA·큰 원본/ARM은 적합한 hosted 러너에 배정한다.
4. 기능군별 실제 호출부와 원본 부재 검사를 통과한 뒤 인벤토리와 하위 이슈를
   갱신한다. 성공한 불변 검사는 재사용하고 최종 필수 PR/병합 후 게이트는 보존한다.
5. 소스와 일치하는 전달 산출물까지 확인한다. 사용자 실기 수용과 전체 전환
   수용은 분리하며, 완료 조건이 남으면 #523을 닫지 않는다.

## 최초 확인

- 기준 인벤토리: 3,580개 중 교체 인정 288개, 부분 3개, 대기 3,289개.
- 기존 PR343의 workspace 실패는 누락된 `identity.rs` 모듈 경로였다.
  현재 소스에서는 해당 문법 오류가 해소되어 `cargo fmt`가 서식 차이 두 파일만 보고한다.
- AArch64 musl은 모델 카탈로그 발행기의 `renameat2` 링크에서 실패했다.
  원자적 비덮어쓰기 계약을 유지하는 수정과 실제 대상 재검증이 필요하다.
- 생성기·카메라 비교와 인증/터미널 도구는 분리된 GPT-6.1 Sol 작업자가 처리한다.

현재 상태는 실행 재개이며 전체 전환·필수 게이트·기기 수용 완료가 아니다.

## 첫 실제 Actions 결과와 용량 복구

소스 `fede87a91993bfe52e6959c6e3366d5b78884213`의
[집중 검사 40078269062](https://github.com/bin9208/openpilot-rust/actions/runs/40078269062)는
`bin-actions`에서 체크아웃 전에 중단됐다. 실제 가용 공간 1,512,144,896 bytes가
소스 증가량 2,147,483,648 bytes보다 작았다. 컴파일·기능·ARM 검사는 모두 미실행이다.
artifact `12554063245`의 ZIP CRC와 단일 용량 보고서를 직접 확인했다.

[첫 읽기 전용 조사 40078386189](https://github.com/bin9208/openpilot-rust/actions/runs/40078386189)의
artifact `12554341630`은 11개 멤버 CRC와 SHA256
`2bb63774928a15400fed95118f95a93be0ca94543f453e5468156214fe60193d`가 일치한다.
60GB 볼륨이 98% 사용 중이었다.
후속 YAML 재직렬화에서 `on`이 boolean 키로 변한 등록 실패는 바로 수정했다.
그 실패 실행은 job이 없으며 캐시 조사나 검증 실행으로 계산하지 않는다.

[세부 조사 40078603173](https://github.com/bin9208/openpilot-rust/actions/runs/40078603173)의
artifact `12554825279`는 13개 멤버 CRC와 SHA256
`23e5f5349b22c2263eb645e0966edbafa3e85d932a15113273524745d212b2a8`가 일치한다.
공유 `rust/target`은 11,529,187,328 bytes이고 과거 정책 검사 전용 target 네 개는
각각 약 1.07/1.07/1.07/1.72GB다. 소유자 marker와 실제 inode, 프로세스 목록을 확보했다.
공유 체크아웃은 `e0ef927ebb4010996feaf52137611cdf2e2c3b86`이며 tracked diff가 없다.

현재 #523 검사는 새로운 소스와 전용 target을 사용한다. 과거 정책 검사 target·registry는
새 구현의 입력이나 모델·미디어 캡처가 아니며, 이후 실행에서 해당 이전 실행 파일을
재사용하지 않는다. 과거 결과는 보존된 Actions 자료로 추적한다. 이 네 캐시와 공유 target의
Rust `rlib/rmeta`, pip HTTP 다운로드 캐시만 복구 후보로 삼는다. source/proof, 설치된
Rustup, 모델·provider, 공유 실행 파일·DSO와 진행 중 작업의 입력은 보존한다.
정리는 실행 직전 소유권·참조 확인과 보존 파일 전후 검증, 실제 free 측정을 요구한다.

검토 후 터미널 fd 복제는 원자적 `CLOEXEC`로 보완했다. 새 private-PTY 회귀와 실제
loopback POST/redirect의 Requests 대조 테스트도 추가했으며 아직 실행 전이다.

집중 검사는 과거 Git 객체 전체를 요구하지 않아 정확 HEAD의 shallow checkout을 사용한다.
이미 설치된 Rust 1.94.0을 재사용하고 rustc/cargo/rustfmt/Clippy 실행 파일을 전후 고정한다.
중복 toolchain 설치를 제거한 새 작업의 증가량 예상치는 6 GiB이며, 전용 registry·target과
필요한 musl 표준 라이브러리 추가분을 포함한다. 실제 공간이 이 예상치보다 작으면 중단한다.
기존 원본·ARM·전체 PR 게이트의 증가량이나 완료 조건을 낮추는 변경은 아니다.

독립 검토의 터미널 `CLOEXEC`, HTTP 비교의 proxy 환경 및 명시적 selector 누락은
소스에서 보완했다. 정리 후보에는 모든 UID의 `/proc` 참조를 root 읽기 전용 검사로
확인하고, 읽을 수 없는 프로세스가 있으면 정리를 거절하도록 반영했다. 보존 manifest는
디렉터리 symlink도 대상 내용을 따라가지 않고 링크 자체를 기록한다.

## 실제 정리 결과

[정리 40079156262/job120828879976](https://github.com/bin9208/openpilot-rust/actions/runs/40079156262/job/120828879976)은
소스 `2ab224c8907ea0fba596dca5fd279c939cbf50ae`에서 성공했다.
artifact `12554042677`의 8개 ZIP 멤버 CRC와 SHA256
`10216438944e08ec7de5752389ab988ac727d22330cd43638323f6df366a3c58`를 확인했다.
2,282개 계획 항목에 대한 4,564개 intent/removed 기록이 순서대로 완전히 대응하고,
보존 manifest의 29,269개 항목은 전후 동일하다. 일반 source/proof와 설치된 Rustup,
공유 실행 파일·DSO를 포함한 보존 자료의 내용이 유지됐다.

같은 볼륨의 실제 가용 공간은 1,509,208,064 → 9,439,432,704 bytes다.
이 순증가는 실행 중 다른 활동·증거 파일 증가도 포함하므로 영구 용량 보장이나
각 파일의 정확 회수량으로 해석하지 않는다. 다음 검사는 실제 시점별 admission을 유지한다.

## 첫 집중 기능 검증: 123개 성공, 잔여 검사 실패 구분

소스 `d0f53d15a96ef474ff48810b156437a6a6923c58`의
[40079353750/job120829459871](https://github.com/bin9208/openpilot-rust/actions/runs/40079353750/job/120829459871)은
최종 실패다. artifact `12554139608`의 28개 ZIP 멤버 CRC와 SHA256
`5ad366a8d5ee0d078917be9231c1850b0c3e8dba927a0adcf3a388da56bc8e55`,
추적 index와 source 전후 봉인을 확인했다. Rust 1.94.0과 작업별 6 GiB admission이
실제로 통과했다. 검사 시 가용 공간은 8,241,963,008 bytes였다.

| 실제 실행 범위 | 성공 |
| --- | ---: |
| x64 카탈로그 발행·충돌/보존 | 10 |
| KBHit·private PTY·CLOEXEC | 20 |
| 인증 저장/HTTP 정책 library | 19 |
| 로그인·URL·원본 21개 입력 대조 | 8 |
| 실제 loopback Requests POST/301·302·303·307·308 비교 | 1 |
| 개발 SSH·키 설정·Params 경계 | 27 |
| 주입형 GithubUtils | 26 |
| Paramsd CarParams CLI | 1 |
| Paramsd estimator controller | 10 |
| 기존 camera disk-boundary 실패 사례 | 1 |
| 합계 | 123 |

Paramsd의 실제 원본 wire/exec/NumPy 비교 3개는 ignored 상태로 남았으며 성공에 포함하지 않는다.
전체 원본 비교·런타임 수용을 위 합계로 대체하지 않는다. x64 카탈로그의 엄격한 Clippy는 성공했다.

도구군 Clippy는 HTTP의 `manual_range_patterns`와 GithubUtils의 `redundant_guards`
두 지적에서 실패했다. 각각 동일 의미의 `301..=303` 및 `JsonView::Text([])`로 보완했다.
ARM은 rustc의 target 정보 조회에서 `gnu-lld` flavor가 unstable이라고 거절되어
컴파일·실행에 도달하지 않았다. 안정판 호환 `ld.lld`의 인자 문법은 로컬 `--print cfg`로
확인했으며 빌드나 기능 검사를 로컬에서 실행하지 않았다.

다음 배치는 성공한 불변 cohort의 artifact hash/CRC·소스·tool hash를 확인해 재사용한다.
전체 변경 경로 allowlist와 검토한 네 코드 파일의 postimage hash가 다르면 재사용을 거절한다.
새 callback socket 3개, 영향 있는 HTTP/Github 대조, 미도달 Clippy 및 ARM build/QEMU를
새로 실행한다. ARM 실제 CLI의 최초 발행·동일 세대 재사용도 별도 ignored test를 명시적으로
선택해 검사하며 빈 `PATH`에서 실행해 Python이나 다른 compiler command에 의존하지 않음을 확인한다.

모델 #482의 별도 조사에서 과거 256 controls의 실제 성공을 확인했으나 historical half
operand 5건은 미해결이다. fresh `original` label도 같은 Rust worker를 썼으므로 독립 Python
compiler 대조로 확대하지 않는다. 추측 산술 수정은 하지 않았으며 다음 제한된 독립 compiler
대조는 `python-rust-523-model-482.md`에 기록했다.

## 두 번째 집중 검사와 다음 보완

소스 `9e7a3b39ebaa2d215e2cadbae0618cc8720f3649`의
[40080413343/job120832600964](https://github.com/bin9208/openpilot-rust/actions/runs/40080413343/job/120832600964)은
새 callback socket 3개와 영향 있는 HTTP 1개·Github 26개를 통과했다.
카탈로그의 새 ARM CLI test 소스에 대한 Clippy도 성공했다. 성공한 불변 범위는
앞선 artifact·source·tool hash와 변경 경로 제한을 확인해 재사용했다.

artifact `12554902305`의 32개 ZIP 멤버 CRC와 SHA256
`5e79482c0f26815da14a9d649fe1b467fd245143edefc2843ffde2aae013ab9a`,
source/index 전후 봉인이 일치한다. 최종 상태는 실패이며 아래 미완료를 구분한다.

- Clippy가 다음 SSH 테스트 지적까지 진행했다: `err_expect`, `redundant_closure`,
  사용하지 않은 내부 `Result`. 앞의 두 표현을 단순화하고, 마지막은 무시하지 않고
  성공 결과를 명시적으로 assert하도록 보강했다. 수정한 SSH 27개는 새로 실행한다.
- 안정판 `ld.lld` 인자는 수용됐으나 bare linker가 musl의 `-ldl`을 찾지 못했다.
  실행 파일은 아직 생성되지 않아 ARM 기능 성공을 주장하지 않는다. production
  dependency를 제거하거나 가짜 archive를 만들지 않고 저장소 기존 SDK인
  `cargo-zigbuild==0.23.4`·`ziglang==0.16.0`의 musl 링크 경로를 사용한다.
- 0.23.4의 [실제 CLI](https://github.com/rust-cross/cargo-zigbuild/blob/v0.23.4/src/bin/cargo-zigbuild.rs)와
  [test 구현](https://github.com/rust-cross/cargo-zigbuild/blob/v0.23.4/src/test.rs)을 읽었다.
  직접 `cargo-zigbuild test`가 Cargo test 옵션과 Zig linker 환경을 전달하므로
  명시적 QEMU runner를 통해 두 ARM unit을 선택 실행한다.

다음 배치는 두 선행 artifact에 묶인 불변 성공을 재사용하고, 보강한 SSH·미도달
엄격 Clippy·실제 SDK ARM build/QEMU/CLI를 실행한다. 재사용 allowlist와 코드
postimage에는 수정한 SSH 테스트도 포함한다. 단일 source commit의 전체 gate가
통과했다는 뜻으로 여러 단계의 성공을 합치지 않는다.


## 세 번째 집중 검사: 실제 ARM 경로와 엄격 lint 성공

소스 `40efe7ef964c0b08f9ba867edd9c863fc545bc0a`의
[40081388419/job120835512317](https://github.com/bin9208/openpilot-rust/actions/runs/40081388419/job/120835512317)은
성공했다. artifact `12555169058`의 43개 ZIP 멤버 CRC와 SHA256
`c9de529574ed693dfec4cd438b3da29d87fb26060bbe5b43347844dd579fe74d`,
source/index 전후 봉인이 일치한다.

- 보강한 개발 SSH 27개가 성공했고 도구 library 및 지정 integration test target의
  엄격 Clippy가 성공했다. 성공한 불변 범위는 검토한 앞선 artifact를 재사용했다.
- 고정 Zig SDK로 실제 `publish-model-catalog` AArch64 musl 실행 파일을 빌드했다.
  ELF는 AArch64이고 `INTERP`가 없다. SHA256은
  `f2809ac51b7c33434b978c988e460aaa42c74823e50a7bfefef71379d258c775`다.
- QEMU에서 원자적 이동 및 디렉터리·파일·symlink 충돌 보존 2개가 성공했다.
  별도 CLI 검사는 빈 `PATH`에서 최초 발행과 같은 세대의 재사용을 모두 확인했다.
  이것은 x64 호스트의 QEMU 사용자 모드 실행이며 물리 ARM/기기 검증이 아니다.
- 정확 #529 수정 소스의 집중 수용 근거를 확보했다. 기존 PR343은 아직 이전 HEAD이므로
  전체 PR 필수 ARM 게이트 성공이나 #296/#523 완료로 표현하지 않는다.

다음 독립 검사는 #482의 compiler 진입점 비교다. 별도 소유 source/venv/target을 사용하고
실제 LLVM18 SDK·worker를 준비하기 전에 전체 작업 증가량 3 GiB를 확인한다.
진단은 원본 Python `compile_to_obj`와 fresh Rust worker를 각각 한 번 호출하며,
생성 object·과거 ELF·모델은 실행하지 않는다. 원본과 같은 자연 CPU/feature/pass를
직접 기록하고 결과 bytes·provider/source/import 봉인을 비교한다. 이 진단용 shell의
내장 Python은 원본 oracle 실행용이며 Python 전환 완료 수량에 포함하지 않는다.


## 인증 실제 호출부·Paramsd 미실행 경계의 다음 묶음

일반/JWT/JSON 인증 CLI·JSON 영역 pprint와 주입형 원본 메인 비교 3개,
최종 API 정책 1개를 추가했다. `developer-auth` bin만 등록하고 dependency/feature/lock은
유지한다. `tools/op.sh`의 auth/ssh는 현재 checkout을 빌드하는
`rust/tools/run-developer-tool`로 전환했다. 부트스트랩의 명령 allowlist·명시적 host
빌드·인자 전달을 검토했으며, 실제 계정·SSH 대신 help-only 호출을 Actions에서 감사한다.
독립 Sol 검토에서 차단 결함은 발견하지 못했다. **이 묶음의 기능 결과는 아직 UNRUN**이다.

Paramsd는 실제 원본 CarParams 3 profile × 3 fingerprint의 9쌍과 NumPy 9,600개
관측을 새로 캡처한다. 현재 두 tolerance의 원본 checker 관측 2,432개도 별도로 확인한다.
준비한 현재 ELF 3개를 고정하고 기존 ignored selector 두 개만 각각 정확히 실행한다.
원본 atomic startup gate·전체 estimator 1,196행 검증은 이 좁은 경계 검사로 대체하지 않는다.

다음 집중 job은 성공한 `40efe7ef9` artifact와 그 안의 두 선행 artifact를
CRC/hash·source/tool·변경 경로·postimage로 고정해 불변 범위를 재사용한다.
Cargo manifest 변경도 reviewed bin 하나 외에는 구조가 같아야 한다. 변경된 auth 경로만
원본/기능/Clippy를 새로 검사하며 ARM의 성공한 불변 빌드를 반복하지 않는다.
ARM SDK를 새로 설치하지 않는 native-only 묶음의 예상 증가량은 4 GiB이고,
실제 시점의 가용 공간 admission을 유지한다. 전체 PR 작업의 별도 증가량은 낮추지 않는다.

작업을 시작하지 않은 이전 Fast 실행 `40080413341`, `40081388493`은 jobs가 없고
같은 작업 브랜치의 superseded pending 상태임을 API로 확인한 뒤 취소했다.
이미 진행 중인 Fast 실행과 현재 모델 진단은 유지했다. 새 호출부와 원본 경계 수용 전까지
기준 인벤토리와 원본 Python 파일을 유지한다.


## 첫 독립 compiler-pair 실행: 비교 전 SDK 경로 오류

소스 `562a29546717515ba9da34bc9c2809fd32a07e23`의
[40082144360/job120837717832](https://github.com/bin9208/openpilot-rust/actions/runs/40082144360/job/120837717832)은
실패했다. SDK package `libllvm18:amd64=1:18.1.3-1ubuntu1`, 원본 venv 및 fresh
`offline-llvm-compile` build는 성공했다. 진단이 존재하지 않는
`/usr/lib/x86_64-linux-gnu/libLLVM-18.so.1`을 지정하여 archive 다운로드·두 실제 compile 전에
거절됐다. compiler 대응이나 #482 해결의 결과로 해석하지 않는다.

artifact `12555444949`, 302,984 bytes의 SHA256은
`4a6fc440047bca4c3cab32e5f3552bf948ca3ce55e18790a0a1cf2d7b59ee50d`다.
30개 ZIP 멤버 CRC와 exact source/index 전후 일치를 확인했다. 실패 당시 실제 free는
7,948,083,200 bytes였다. 기대 SDK version은 정확하지만 진단의 filename이 잘못됐다.

저장소 기존 세 LLVM job과 설치 package의 파일 배치를 확인하여 경로를 고정 alias
`/usr/lib/x86_64-linux-gnu/libLLVM-18.so`로 바로잡는다. 원래 provider SHA256
`5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0`와 package pin은
유지한다. 다음 실행은 package file list와 realpath도 먼저 보존하며 provider fallback이나
CPU/feature 변경 없이 아직 미실행인 두 compile을 확인한다.

기존 Fast `40079353749`는 Python/Android·corpus·alert manager·alert·timing의 다섯 job이
성공했으나 routing은 Rustup의 `bin/cargo-clippy` 설치 충돌로 기능 검사 전에 실패했다.
원인과 영향을 별도로 조사하며 전체 Fast 성공으로 기록하지 않는다.

## 기존 bundle builder 후보의 최초 집중 검증 준비

기존 `82d4f4bb` 후보의 library 1개·integration 17개와 엄격 Clippy를 별도 job에서 검사한다.
이 checkout의 13개 source selector·7개 고정 파일에서 원본/native CLI가 만드는 context의
전체 roster·종류·바이트·SOURCE_COMMIT과 출력 메시지를 비교한다. 실제 Git blob 총량의
보수적 상한은 566개/10,914,295 bytes이며 두 context와 기록 증가량은 128 MiB다.
준비 전체는 별도 2 GiB admission을 사용한다. native lane은 Python 없는 PATH와 exec
기록을 확인하며 Docker image 발행·NAS 배포 workflow는 수정하거나 활성화하지 않는다.
이 작성 시점의 신규 bundle 검사는 **UNRUN**이고 원본 삭제는 보류한다.


## 네 번째 집중 배치: Paramsd·실제 native caller 성공, 인증 parser 1건 실패

소스 `41cfaffd310059f4f999c8d24dd491d9694baa91`의
[40082949276/job120840061385](https://github.com/bin9208/openpilot-rust/actions/runs/40082949276/job/120840061385)은
최종 실패다. artifact `12556331299`의 114개 ZIP CRC, SHA256
`5ed71546d1a7d50ffc0a94c1b8ba9266267fb2a7772de83d8ebfe4bcc3a2e279`, source/index 전후를 확인했다.

- 인증 일반/JWT/저장·재조회·오류 순서와 pprint 원본 비교가 성공했다. 기존 callback 3개와
  새 최종 API 정책도 성공하여 library 7개 중 6개가 성공했다.
- 인자 비교는 `apple --json unused`에서 실패했다. 실제 Python 3.12.15 원본은 `unused`를
  두 번째 optional positional에 받아 로그인 JSON을 반환했으나 native는 첫 positional
  구간에서 빈 두 번째 slot까지 소비하여 exit 2가 됐다. source parser를 남은 slot을
  옵션 뒤에도 유지하도록 수정하며, 실패한 원본 비교와 영향 있는 mode 검사를 다시 확인한다.
  그 단계 뒤의 기존 원본 로그인 8개와 새 Clippy는 아직 미실행이다.
- 실제 원본/native CarParams 9쌍의 전체 bytes가 일치했다. NumPy 9,600개와 원본 비교
  정책 2,432개를 생성했고 기존 ignored selector 2개가 각각 정확히 1개씩 성공했다.
  artifact의 source/provider/ELF/corpus 전후 봉인도 독립적으로 대조했다.
  별도 atomic startup/전체 runtime 수용은 미완료다.
- 실제 `tools/op.sh --no-verify auth|ssh --help`가 각 native CLI에 도달했다.
  각 lane의 7,774/159개 exec attempt에서 관측 Python 실행은 0이다. 이 수치는 실패한
  PATH 탐색도 포함하며 모든 attempt를 성공 실행으로 표현하지 않는다. CLI는 help-only이고
  source 전체 부재나 실계정/SSH 수용 증거가 아니다.

다음 묶음은 성공한 Paramsd·native helper와 영향 없는 native callback/API·ARM을 재사용한다.
새 SSH/키 설정의 독립 원본 52사례/3개 테스트도 준비했으며 기능 결과는 **UNRUN**이다.


## #530 prepared-toolchain 경로 및 다음 집중 검증

일반 bootstrap은 보존하고, 등록 runner의 검증된 Rust 1.94.0을 명시적으로 사용하는
opt-in 경로를 추가했다. 실제 bootstrap 두 호출에만 SDK root/manifest를 전달하고,
warm cache 실행 전과 cold build 직전에 검증한다. 지정 SDK의 절대 cargo/rustc만 사용하며
이 경로에서는 Rustup 설치·복구·다른 compiler fallback을 하지 않는다.

유지 중인 Fast의 routing job만 SDK 준비를 바꿨다. source/HOME/CARGO/RUSTUP_HOME/target
격리와 기존 2/6 GiB 작업 증가량, 전체 원래 routing selector를 유지한다.
기존 성공 SDK 도구 4개의 literal hash와 전체 regular file 내용·inode/종류·mode/owner·
mtime/ctime·symlink roster를 전후 대조한다. cold bootstrap exec에서 절대 compiler 도달과
Rustup/Python 실행 부재를 확인한다. 나머지 여섯 job 정의·trigger·aggregate는 구조가 같다.
새 거부/캐시 우회 방지 8개를 포함한 disk bootstrap target은 총 24개다.
독립 Sol source review에서는 차단 결함을 발견하지 못했으며 **실제 cold/warm·routing은 UNRUN**이다.

새 auth batch는 parser 수정의 원본 mode/인자 3개, 미도달 기존 로그인 8개,
독립 SSH/키 설정 52사례/3개 및 엄격 Clippy만 새로 실행한다.
Paramsd 9쌍/9,600개 및 native helper·기존 callback/API·ARM은 봉인된 성공을 재사용한다.
원본 Python interpreter는 실제 앞선 실행에서 확인한 `3.12.15`로 고정했다.
모델 SDK filename 수정과 기존 bundle 18개/실제 context 비교는 별도 실행한다.
기능 결과를 확인하기 전 원본 삭제나 전체 전환 완료로 집계하지 않는다.


## 독립 compiler-pair 실제 성공: 현재 자연 target 한 IR

소스 `97f7ad36dfa0919b4e5c216fb16f44ed29161663`의
[40084922315/job120845780499](https://github.com/bin9208/openpilot-rust/actions/runs/40084922315/job/120845780499)이
성공했다. artifact `12556379198`, 26,950,553 bytes의 SHA256은
`f8fae028c614f5efa33db25150d29347a52c7f9b49de6e4f6330aa069c063d8c`다.
68개 ZIP CRC와 source/index·provider/source/imports 전후 봉인을 확인했다.

실제 원본 Python `compile_to_obj`와 fresh Rust worker의 object 17,808 bytes가 전부 같고,
공통 SHA256은 `e29630e0aade6971df153c374afda4e5f7052e80cc495f7e1647e72170f9c9b3`다.
원본이 직접 선택한 target은 `x86_64-none-unknown-elf`, CPU `alderlake`,
`default<O2>`이며 LLVM18 provider hash는 기존 `5c3a6395...`와 같다.
원본 직접 compile 1회, native compile 1회이며 object나 모델을 실행하지 않았다.

이 성공은 현재 runner의 compiler 진입점 대응이다. 과거 EPYC 재생과 다른 target이므로
historical 5건·원래 모델 전체 검증을 통과한 것으로 확대하지 않는다.
과거 frozen code는 PKL에서 읽은 runtime image이며 최초 compile/cache context는
확정되지 않았다. 다음 조사는 같은 한 IR의 실제 원본 private cache miss/hit 및
ELF→runtime-image 연결로 좁히고, 성공한 compiler pair는 반복하지 않는다.


## Bundle 최초 실행: 기능 18개 성공, Clippy 뒤의 실제 CLI는 미실행

소스 `97f7ad36dfa0919b4e5c216fb16f44ed29161663`의
[40084922406/job120845780912](https://github.com/bin9208/openpilot-rust/actions/runs/40084922406/job/120845780912)은
실패다. library 1개와 integration 17개는 실제 원본 synthetic Git 비교를 포함해 성공했다.
그 뒤 테스트 코드의 `type_complexity`, `field_reassign_with_default`,
`cloned_ref_to_slice_refs` 세 Clippy 지적에서 멈춰 실제 checkout 전체 CLI 비교는 미실행이다.

artifact `12557345320`, 322,601 bytes의 SHA256
`b42c5f157976dc8dd421ce571833b31377a5a8b4799c9854473a0089fd8233c0`,
21개 ZIP CRC 및 source/index 전후 일치를 확인했다. 지적은 type alias·직접 초기화·
`slice::from_ref`로 보완하여 모든 기존 assert와 역할을 유지한다. 다음 실행은 성공한
기능 범위를 재사용하고 엄격 Clippy 및 미실행 실제 CLI context 비교를 진행한다.


## Runner 용량 제약과 97f7 auth 결과 반영

사용자는 60GB runner 볼륨 확장이 어렵다고 답했다. 기존 정리 후 약 9.44GB 여유를
기준으로 입력·중간 산출물의 실제 수명과 deep 검사 분할을 검토한다. 16.64GB 증가량이
필요한 기존 작업의 gate를 근거 없이 내리거나 현재 필요한 source·모델·provider를
삭제하지 않는다. 작은 집중 검사는 각 작업별 admission을 유지하여 계속한다.

소스 `97f7ad36dfa0919b4e5c216fb16f44ed29161663`의
[40084922332/job120845780576](https://github.com/bin9208/openpilot-rust/actions/runs/40084922332/job/120845780576)은
실패다. 옵션 뒤 위치 인자 문제는 통과했고, 원본 Python `3.12.15`의 잘못된 provider
안내가 `choose from google, apple, github, jwt`인 데 비해 native만 각 선택지에 따옴표를
붙였다. 관측 원본에 맞춰 이 문구만 보완했다. library mode/저장·재조회와 pprint 2개는
성공했으며 기존 로그인 8개·새 SSH CLI 52사례/3개·엄격 Clippy는 앞 단계 실패로 미실행이다.

artifact `12557545634`, 3,023,906 bytes, SHA256
`921c8b07db200945ee3c39279c63d4d4159a9ef391a1e3f0c64515d8b9dd2df0`의
31개 ZIP CRC·source/index 전후 일치를 확인했다. 다음 workflow는 성공한 2개를 재사용하고
인자 1개·기존 로그인 8개·SSH CLI 3개·Clippy를 독립 단계로 실행한다.
앞 단계 한 건의 실패가 다른 미실행 범위를 가리지 않도록 하되 각 실패는 전체 실패로 남긴다.

route sanitizer의 기존 native 구현을 중복 작성하지 않고, 독립 full-Cereal 원본 비교
5개를 추가했다. 기존 unit 15개와 새 원본 5개·Clippy를 분리해 검증할 예정이며 **UNRUN**이다.
상세 입력·unknown field 및 identity 증거 범위는
[별도 기록](python-rust-523-route-sanitizer.md)에 남긴다. caller 전환·원본 삭제·재고 집계는
실제 수용 결과를 확인한 뒤 진행한다.


다음 auth 실행에서는 기존 로그인 21 입력의 원본도 `AUTH_CLI_ORACLE_PYTHON`의
준비된 절대 interpreter로 고정한다. 이전 `env_clear()` 뒤의 `python3` 이름 탐색은
준비 venv와 다른 시스템 Python에 도달할 수 있었다. 원본 함수·입력·assert는 보존하고
누락 시 skip/fallback 없이 실패하도록 바꿨다. 해당 8-test target은 이번에 새로 실행한다.


용량 조사에서 기존 route-vault once의 16.643 **GB**는 15.5 **GiB**의 합산 추정치이며
전체 deep 작업의 실측 peak가 아님을 확인했다. 실제 구성은 source2 + SDK2 + registry2 +
target8 + Python1 + proof0.5 GiB다. 정리 후 9.439GB는 8.791GiB이며 단위를 섞어 계산하지 않는다.
기존 gate는 유지하고 SDK 중복·Clippy 중간물 수명·추가 proof tar의 겹침을 분리 후보로 삼는다.
직전 97f7 bundle artifact의 실제 free는 9,426,989,056 bytes(체크아웃 전) →
8,229,765,120 bytes(준비 시작) → 6,870,003,712 bytes(종료 전)였다. 이는 해당 실행의
관측 순증이며 실측 peak나 기존 held-provider 전체 수용을 뜻하지 않는다.
다음 bundle은 build·Clippy·최종 단계별 registry/target/venv의 할당량도 보존한다.


## 0e28 실제 결과: bundle·cache 성공, auth/sanitizer와 SDK 후속 수정

소스 `0e28b8601a02092b4103ff84191be6cbef2b576d`의 결과를 기능군별로 분리한다.

- [Bundle40087242278/job120852565401](https://github.com/bin9208/openpilot-rust/actions/runs/40087242278/job/120852565401):
  성공. 기존18개 계약 재사용, fresh CLI build·엄격 Clippy·실제 checkout 원본/native의
  **411파일/8,621,062 bytes** 비교 및 native Python exec attempt0을 확인했다.
  artifact12558055881/6,331,785B/SHA256
  `2913e7a3e3b7abbdd0696b99fcb6606dcd50ec6e19a0e6c94db0ff3ed746b3d2`,853 ZIP CRC/source/index 전후 일치.
  업로더의 기본 hidden 제외 때문에 `.vendor` 아래6파일/69,990B 원시 출력은 archive에 없다.
  archive의 나머지405쌍/8,551,072B는 전부 동일하다. CI의 전체411 비교와 보존된 full roster를
  구분하며 archive에서411쌍 전체를 직접 확인했다고 주장하지 않는다. 다음 실제 caller 검증에서
  proof의 hidden 파일도 보존하도록 고친다. 원본 삭제·Docker/NAS 전달은 아직 하지 않았다.
- [Cache40087242279/job120852565465](https://github.com/bin9208/openpilot-rust/actions/runs/40087242279/job/120852565465):
  성공. 원본 get2/put1/compile_to_obj1/jit_loader1의 실제 miss→hit를 관측했다.
  fresh object는 기존pair와 같은 `e29630e0...`이며 두 반환 image16,256B의 SHA는
  `f2427b64b89301d197dfd374a6ae89ca3166d3ca1d13203aca277ff9bb9a59ed`다.
  frozen14,442B와는 byte14부터 다르다. historical/model acceptance는 false이며 code는 실행하지 않았다.
  artifact12557553192/5,024,082B/SHA256
  `f193dcf00c29d4db0df066fba797a370f0f3fd3b11ae06737c86a702f7eb88fc`,70 ZIP CRC 및 source/index·provider/input/imports 전후 일치.
- [Auth40087242286/job120852565373](https://github.com/bin9208/openpilot-rust/actions/runs/40087242286/job/120852565373):
  실패. 인자1·기존로그인8·SSH 정상20와setup21의 두 그룹·strict Clippy는 성공했지만
  `--help --h`의 원본은 help로 종료하고 native는 먼저 ambiguity를 거절했다.
  잘못 붙인 `ambiguous-before-help` 기대를 수정하여 기존 입력은 help 사례로 보존하고,
  실제 ambiguous-first `--h --help`를 추가한다(총54사례/3개).
  CPython3.12.15의 [consume_optional](https://raw.githubusercontent.com/python/cpython/v3.12.15/Lib/argparse.py)을
  확인해 SSH 전용 deferred-ambiguity opt-in을 추가한다. 기존 scanner 진입점의 eager 정책은 유지한다.
  artifact12557463586/3,646,065B/SHA256
  `aa50d837e00804df7132e721865ebd4d39e7654dec86216ac5e2bc1056cab641`,34 ZIP CRC/source/index 전후 일치.
- [Sanitizer40087242329/job120852565522](https://github.com/bin9208/openpilot-rust/actions/runs/40087242329/job/120852565522):
  실패. original5/library15/Clippy 모두 테스트 코드의 잘못된 generated API에서 컴파일이 멈췄다.
  실제 API `init/get_kp_b_p`와 `StructSchema::new(raw).get_proto()`로 양쪽 테스트를 보완한다.
  입력·assert·unknown field 검사와 production sanitizer는 유지한다.
  artifact12557518748/493,413B/SHA256
  `ab503357cdb0ad47040263bf79a82ead6464a7f9192ea6457b1428fbe21feb04`,45 ZIP CRC/source/index·SDK/venv 내용/identity 전후 일치.

Bundle 종료 시 실제 allocated bytes는 registry163,643,392/target633,548,800/venv13,488,128/
proof22,466,560이며 source-owner 전체1,811,054,592는 target을 포함한다. 이 항목을 합산해
중복 계산하거나 실측 peak로 표현하지 않는다.

### #530 SDK 권한 준비 — 검증 기준 유지

[97f7 Fast40084922340/native120850033970](https://github.com/bin9208/openpilot-rust/actions/runs/40084922340/job/120850033970)은
cold 컴파일 전에 SDKroot775를 정확히 거절했다. 실제 SDK는 UID/GID1000의322항목이며
28 directory775·11 regular664·나머지 regular644/755였다. artifact12557358454/189,911B/
SHA256 `47c856f5e841bbde9cd07f57f9a761d94b3a610068bef3994538bcc08f7a80f6`,17 ZIP CRC와
SDK hash/roster 전후 일치는 확인했다. 이 실패에서는 기존 Fast의 조건 때문에 source/index
after가 없으므로 전체 source seal 성공으로 표현하지 않는다. cold/warm·routing24는 미실행이다.

다음 준비는 소유권·종류를 먼저 확인한 SDK의 shared-write bits만 held descriptor를 통해 제거하고,
내용·identity·owner/group·mtime·0022 외 mode bits를 보존한다. 권한 작업은60초 자체 process group으로
제한하고 실패해도 before/after를 남긴다. 그 뒤부터 기존 SDK 전체의 정확한 before/after seal을
적용한다. `0022` 거절을 완화하거나 SDK contents를 바꾸지 않는다. native routing의 source after도
실패 때 실행하도록 고친다. 나머지 여섯 job·trigger·기존2/6GiB guard는 유지한다. 실제 수정은 **UNRUN**이다.

### #531 원본 Python runtime의 관측 정정

앞선 기록의3.12.15는 설정/parent 관측이었다. 0e28의 환경을 비운 SSH 자식 metadata는 실제
`sys.version=3.12.3`을 보였다. parent는3.12.15를 보고하고 선택된 executable은3.12.15 toolcache에
있었지만 `env_clear()`가 `LD_LIBRARY_PATH`도 제거했다. 공유 libpython 선택 차이를 조사하며 옛 자식의
map이 없으므로 특정 DSO 경로를 소급 확정하지 않는다. 이 문제를 [#531](https://github.com/bin9208/openpilot-rust/issues/531)에 기록했다.

후속 oracle는 절대 interpreter와 명시한 단일 runtime-library directory만 전달하며, 환경을 비운
자식의 실제3.12.15 버전·process map의 library path/inode를 먼저 확인한다. libpython·argparse·interpreter
해시를 전후 검사한다. 이 provider context가 바뀌므로 auth3/로그인8/SSH54은 새로 검증한다.
모델 cache의 실제 metadata는3.12.15였으며 성공 cache/pair를 반복하지 않는다.


독립 source review에서 뒤의 ambiguity가 앞의 잘못된 port 변환보다 먼저 반환되는 경계도
확인했다. SSH 값 변환을 소비 시점 callback으로 옮기고 `--port=bad --h --help`를 추가하여
총54사례로 검증한다. 기존 scanner 진입점은 callback 없이 원래 정책을 유지한다.
또한 SDK chmod 이전의 도구4 pin과 root device 일치를 확인하고 mount boundary를 거절한다.
source/SDK seal 실패 때 native job의 source/target을 정리하지 않도록 cleanup 조건을 강화했다.
원본 자식은 새 `-X pycache_prefix`와 `-B`로 startup부터 stale bytecode를 읽거나 쓰지 않는다.


SDK cold 감사는 Python에서 Rust로 옮긴다. 신규6개 parser/무결성 negative-control과 실제
증거 전용 ignored1개를 추가하여 disk bootstrap target은 기본30개+ignored1개가 된다.
실제 검사는 source/job owner·SDK manifest·trace identity/hash를 묶고, 이전 owned receipt를
먼저 무효화하여 실패 후 낡은 PASS가 남지 않게 한다. 절대 cargo/rustc의 성공 exec가 필요하며
실패한 PATH 탐색을 포함한 Python/Rustup attempt·FD exec·잘린 trace는 거절한다.
실제 감사는 기존6GiB routing admission 뒤, 원래 functional selectors 전에 실행하여
같은 target을 재사용한다. strict Clippy도 새 target을 포함한다. 아직 **UNRUN**이다.
원본 focus venv 전체 파일/링크도 전후 봉인한다. 독립 review의 차단 지적은 반영했고,
다음 게시 전 마지막 source pin 갱신과 문법/diff 확인을 진행한다.


## 2026-10-24 a134 결과 및 다음 집중 검증

- 사용자 답변: 60GB runner 볼륨 확장은 어렵다. 작업별 증가량 승인과 소유 output 정리를 유지하며,
  기존 16.64GB deep 예상량을 자의적으로 낮추지 않는다.
- SAN [40089943421](https://github.com/bin9208/openpilot-rust/actions/runs/40089943421):
  원본5/native15/strict Clippy 및 provider/source seal 성공. 원문5JSON과 source 부재 native checker로
  삭제 후보를 준비했으며 삭제 후 검사는 UNRUN, inventory credit은 보류한다.
- 집중 [40089943318](https://github.com/bin9208/openpilot-rust/actions/runs/40089943318):
  auth3/login8/alias20/setup21/SSH27/scanner2 및 strict Clippy 성공.
  parser13 envelope의 과거 count11 때문에 per-case assertion 전에 중단됐다. count13으로 수정했다.
  실제 Python runtime3.12.15와 loaded libpython/argparse identity를 확인했다.
- Rust1.94 Unicode17과 원본 Python3.12 Unicode15 alias 소문자화의 차이를 지지하는 primary source를
  찾아 U+A7CB/U+1C89 두 exact CLI 회귀를 추가했다. 구현은 변경하지 않았으며 fresh 원본 비교로
  실제 실패 여부를 먼저 확인한다. 이번 alias22와 parser13만 fresh 실행하며 unchanged 수용 cohort는
  exact artifact/source/provider pin으로 재사용한다. 새 두 사례는 UNRUN이다.
- Fast [40089943270](https://github.com/bin9208/openpilot-rust/actions/runs/40089943270):
  다른5worker 성공. routing에서 실제 prepared cold 감사1과 디스크30 검사는 성공했으나
  workflow policy6개 실패로 aggregate가 실패했다. 강해진 always/seal/cleanup 및 분리 guard의
  실제 조건을 검사하도록 진단 중이다. #530 전체 성공으로 기록하지 않는다.
- Bundle public helper feature mapping, 기존 비활성 publish caller 한 줄, deterministic fixture Git 및
  실제 original23회 출력 capture를 준비했다. 기존17개 assertion을 유지했다. 새 workflow는
  fresh17+library1, strict Clippy, public helper를 통한 실제 committed context 전체 비교와 hidden file
  artifact 보존을 실행한다. 기존 repository/ref 조건을 유지하며 Docker/NAS publish는 활성화하지 않는다.

현재 전체 inventory는 3580 = replaced-local288 / partial3 / pending3289이며 변경하지 않았다.
본 기록의 다음 후보들은 로컬 syntax/fmt/diff만 확인한다. 기능·컴파일은 등록 Actions에서 수행한다.


### a134 routing 실제 증거 보충

artifact12559545652(419610B)의 SHA256은
`88755f3b00729fb0647bd151ffcf7afdf7138c2bd5da61fa2a63bf888f80782f`다.
부모의 36CRC 감사 및 source/index/SDK/permission 전후7쌍 비교가 일치했다.
권한 변경은39entry/journal78행, 실제 SDK regular294개 내용 불변이다.
실제 cold receipt는 exec289(실패139 포함), absolute cargo 성공1/rustc 성공37,
Python0/rustup0을 기록했다. 이후 policy6개 실패와 분리한다.

policy 수정 후보는 실제 native after/cleanup 조건과 참조 ID를 정확히 검증하고,
실제 strace로 감싼2GiB guard의 전체 명령을 검사한다. 조건 약화·ID 삭제/중복·fork 우회를 거부하는
음성 회귀를 추가했다. #523 workflow6개에 `queue: max`를 보완했다.
model workflow의 queue 정책 수정으로 이전 성공 cache probe도 다시 트리거된다.
새 전체 Fast/정책 검사는 UNRUN이다.


## fb1228 집중 결과와 #532

`fb1228f02e0546b6fe7a9d8d0eec098e650a86bd`의
[40092771379/job120868686487](https://github.com/bin9208/openpilot-rust/actions/runs/40092771379/job/120868686487)는
새 Unicode alias 회귀의 실제 차이로 실패했다. backend alias U+A7CB/query U+0264에서
원본은 exit1/no exec, native는 다른 합성ID로 SSH exec를 준비했다. 실제 기기에 접속한 것이 아니다.
[#532](https://github.com/bin9208/openpilot-rust/issues/532)로 분리해 고정 UCD15 lower와
원본 문맥 속성 적용을 준비한다. 기존 ASCII공백 제거 순서·substring/API/exec는 보존한다.

별도의 parser13 selector와 strict Clippy는 성공했다. 원본은 U+1C89/U+1C8A 두 번째 사례도
exit1/no exec로 관측했으나 native comparator는 첫 차이에서 중단됐다.
artifact12560030996,7775163B,SHA256
`21c33f18576748c77664d0267b4c51edca8aedf0f8b3c345e8e91d477d739878`의47CRC 및
source/index/venv 전후4쌍을 부모가 확인했다. 전체 job 성공으로 기록하지 않는다.
후속 private helper의 scalar1112064개 및 Σ문맥154개 원본 비교와 CLI26/영향 있는SSH 회귀는 UNRUN이다.


### #532 및 Bundle 다음 source 후보

#532는 고정 UCD15 mapping1432개/확장1개/Cased157range/Case_Ignorable437range로
Rust SDK의 Unicode 의미를 분리했다. 전체scalar1112064개 length-prefixed UTF8 SHA와 Σ154개
문맥을 준비한 독립 CPython3.12.15에 대조하는 gate, alias26개 및 영향SSH27개를 fresh로 실행한다.
변경되지 않은 auth3/login8/setup21/scanner2는 a134, parser13은 fb1228 성공 artifact/source/provider
핀으로 재사용한다. 새 source/runtime/Binary 봉인과 strict Clippy를 포함하며 정상 수용은 UNRUN이다.

Bundle은 fixture/capture23행의 생성 방식이 바뀌어17개를 fresh로 실행한다. 공개 helper의
추가 host build3GiB와 context128MiB를 device별 합산 승인하고, 기존 guard를 유지한다.
strict trace의 positive2/negative16 controls와 실제 helper/Git/final ELF 성공·argv를 구분한다.
숨김 vendor파일도 artifact에 포함하며 전체411개로 고정하지 않고 현재 source의 실제 whole roster를 비교한다.
원본 builder는 보존한다. nativecaller/context 수용은 아직 UNRUN이다.


## Sanitizer 파일1개 수용 반영

fb1228 source-absence [40092771424](https://github.com/bin9208/openpilot-rust/actions/runs/40092771424)는
frozen5+음성1/Clippy/source·index·SDK seal이 모두 성공했다. artifact12559378200의39CRC 및
전후4쌍을 독립 감사했다. raw trace는 frozen Cargo 범위로 제한한다.
기존 original5/native15의 정확 source핀, 원문 fixture5개, 실제 caller 부재 검토를 연결해
`openpilot/tools/lib/sanitizer.py` 한 파일을 replaced-local로 기록한다.
새 전체 inventory는 **3580 = 289 replaced-local / 3 partial / 3288 pending**이다.
나머지 작업의 과거 pending/수용 보류 기록은 해당 시점의 상태로 유지한다.


### #531 개별 종료와 #482 다음 단서

#531의 준비 runtime/provider mismatch는 a134의 auth3/login8/alias20/setup21/SAN5 및
fb1228 parser13의 원본 수용을 연결해 개별 종료했다. 실제 runtime3.12.15, maps의 SDK
libpython identity, source/provider 전후 seal을 확인했다. PR343 통합·#532·전체523/296와는 별개다.

#482는 역사 artifact12365522894에서 당시 native exporter의 kernel92.o15928B를 찾았다.
전체 ZIP digest를 독립 검증한 것으로 확대하지 않고 selected Range/CRC/SHA와 원래 fixture pin만
수용한다. 기존 `cached_elf::reconstruct`로 전체 frozen BINARY14442B와 같을지 확인하는
비실행 진단을 준비 중이다. ISA/raw 수치/모델 수용은 그대로 미완료다.


임시 model/paramsd 비교 shell 및 #523 workflow의 Python 원본·증거 해석 의존성도
execution-edge 원장에 pending으로 추가했다. 이 신규 경로를 baseline3580 수용 건수에
포함하거나 일반 runtime의 native 수용으로 은폐하지 않는다. 이후 진단 고정/폐기까지 추적한다.


## #532 실제 원본 수용

source121aa5b86의 [40093557829](https://github.com/bin9208/openpilot-rust/actions/runs/40093557829)는 성공했다.
전체Unicode scalar1112064개 SHA, Σ154개, alias26개와 SSH27개 및 strict Clippy가 일치했다.
artifact12559898006(15832926B,SHA256276e6306a25069b071f9c037c6cbaf84fd76408e6ce7d1562109c057ea0d7fba)의
55CRC/source·index·venv 전후4쌍을 독립 감사했다. #532 개별 종료이며 전체auth/API 소스 삭제 credit은0이다.
fb1228 Fast의 남은2policy 실패는 Bundle queue:max가 그다음121aa5 commit에 분리돼 있던 결과다.
현재source에는 보완이 있어 추가 코드를 바꾸지 않고 다음Fast를 확인한다.


## Bundle 121aa5 부분 수용

[40093557875](https://github.com/bin9208/openpilot-rust/actions/runs/40093557875)는 fresh1+17/Clippy와
원본23회 capture가 성공했다. 부모가 artifact12560845618의889CRC와 실제411파일/8621062B 전체
raw byte쌍을 확인했으며 hidden6개도 보존돼 있다. source/index/venv/script 전후 seal은 일치했다.
전체 job은 actual trace parser가 SIGUSR1/SI_TKILL metadata를 아직 처리하지 않아 FAIL이다.
관측형식만 엄격히 보완해 actualcaller를 다시 검증하며 이미 성공한1+17은 재사용한다.


## #482 역사 object 비실행 gate 성공

sourcee1bf의 [40094270972](https://github.com/bin9208/openpilot-rust/actions/runs/40094270972)는
역사object 재구성 결과14442B가 frozenimage와 전체byte/SHA324697e4...까지 일치했다.
text14244/alignment16이다. artifact12560955339의CRC/source·index·SDK 전후4쌍과 heldinput/ELF seal을
독립 감사했다. ISA와 raw수치5건은false로 유지하고 기존 triplet/ABI/hostISA 판정 단계로 이어간다.


### 다음 한정 batch

changed-files 원본2파일과 unittest를 정확히 제거한 후보를 준비하고 기존14proof는 source/provider
overlay로만 재사용한다. 새 native metadata bin은 현재 CargoJSONL의 정확한 CLI/test ELF를 골라
context를 만들고 기존Fast 회귀 명령 직전에 연결된다. 삭제 후 caller5/normalreceipt5 및 Clippy와
전체 source/SDK 봉인을 실제 CI에서 확인하기 전까지 inventory credit은0이다.

모델은 기존ISA module을 의미 변경 없이 공유화하고 cached source rosters를9→10으로 강화했다.
새 fixedtriplet/ABI/현재hostISA와 기존unit5/closure거부1/Clippy는 다음 readonly 검사다.
기본 cached-disabled/guarded-add8 및 raw수치 gate는 그대로다.


## d2c18c37e 집중 수용과 다음 경계

changed-files [40097058000](https://github.com/bin9208/openpilot-rust/actions/runs/40097058000)는
14독립 proof의 current overlay/원본 부재 caller5/receipt5/Clippy 및 source·SDK·GNU8 전후 seal이 성공했다.
artifact12561756241의144CRC/fullSHA를 부모가 감사해 정확한 원본2파일만 인정했다. 전체3580은
**291교체/3부분/3286대기**다. Bundle [40097057916](https://github.com/bin9208/openpilot-rust/actions/runs/40097057916)은
실제 public helper/411파일·hidden6·strict trace를 통과했다. 원본23과 기존17본문을 고정한 삭제 후 검증은 남는다.

모델 [40097057962](https://github.com/bin9208/openpilot-rust/actions/runs/40097057962)는 strict triplet이
성공했으나 AVX512VL instruction에서 ISA 거부로 실패했다. ISA5/roster1/Clippy·두 consumer compile은 성공했다.
정확 opcode/현재CPU 진단만 추가하며 실행·수치·모델 수용은 그대로 false다.

121aa5 Fast는 새 diagnostic workflow의 source 신뢰 predicate가 빠진 데서 policy1 실패했다.
8개 workflow의 첫 Deny 단계는 무조건 실행하고 뒤 단계에 explicit source조건을 AND로 추가한다.
실패 뒤 seal/upload/cleanup 조건은 그대로 보존하고 Rust policy는 완화하지 않는다.
60GB runner 확장 없이 작업별 growth guard를 유지한다. 전체PR/통합/전달·실기 수용은 계속 미완료다.

### 확장 없는 runner의 다음 읽기 조사

새 capacity 후보는 옛 Rustup home8개, shared `rust/target`, 현재 준비 SDK의 정확한 root만
읽는다. mode/UID/device/inode/nlink/blocks/mtime/ctime·regular 전체SHA·symlink target과
owner를 기록하고, 전 UID의 cwd/exe/fd/maps에서 해당 옛 캐시 참조를 전후 관측한다.
argv/env/credential 내용은 수집하지 않고 파일 삭제·SDK 실행·빌드·입력 실행은0이다.

`/tmp/openpilot-ci.4o8uWv/rustup-home`의 compiler 원본과 현재 준비 SDK는 보호 대상으로
명시한다. 이 조사는 과거 크기를 새 free 또는 회수량으로 계산하지 않는다. 실제 결과 뒤
현재·예정 caller 및 proof/provider 의존성이 없는 정확한 캐시만 별도 정리 후보로 판단한다.
controls의 원본109요청은 재캡처 없이 재사용하되8GiB build/추가512MiB 및 proof 성장 검사를
완화하지 않는다. 로컬은 이 새 reader와 workflow의 문법·diff만 확인했고 실제 관측은 미실행이다.

### bc8e26 자원 조사 중단과 범위 조정

[40099354066/job120888474922](https://github.com/bin9208/openpilot-rust/actions/runs/40099354066/job/120888474922)는
기존 기본 조사 뒤 새 전체 metadata 사전 순회의65,000개 상한에서 중단됐다. 새 root별 전체
inventory·process 참조 수용은 성립하지 않았고 실제 삭제는0이다. source 동작 실패나 공간 수용으로
해석하지 않는다. artifact12562631130은 기본 관측12개만 보존한다.

다음 조사는 회수 후보인 정확한4policy SDK와 shared target/현재 SDK의6root로 좁혔다.
1V9i3J·4o8uWv·pMuJhG·W1gAdy의 다른4SDK는 uninspected protected path로 기록하고 삭제에서
제외한다.65,000항목/32MiB metadata 상한은 올리지 않는다. root별 사전 순회 event와 상한 초과
경로를 남겨 실패 시에도 어느 경계에서 중단됐는지 확인한다. 실제 삭제 후보는 regular nlink1 및
전 UID inode/path 참조 없음 등 새 최신 관측을 충족해야 하며 현재는 미수용이다.

### 642d01 최신6root 조사 수용과 정확한 SDK4 정리 후보

[40100476281/job120891425564](https://github.com/bin9208/openpilot-rust/actions/runs/40100476281/job/120891425564)는
source `642d01cc2296790226d7f9058e4f19bca8dc9b1e`에서 성공했다. artifact12563005351
(2,098,652B, SHA256 `81cd3b7e8fcd4c6e917ac90eb05d26c0d4e85792b2e2e812ad93d9ac9f85acc9`)의
18CRC를 부모가 감사했다. inner `cache-inventory.json`7,698,013B의 SHA256은
`cf0b7bfe33614bd8898d6da694d7ae67c5666279e3e2980285aad7340fdd01bf`다.

hRmquO/s9WqUE/Rt5Jlt/eQd1Pi의 old policy SDK는 각각187항목/regular161이며 모든644regular가
nlink1이다. 합계 allocated2,473,824,256B이며 보호 root의 inode와 중복0이다. 전후 전 UID 참조·
unknown·active compiler가 모두0이고 current SDK321하위항목/regular294 및 shared target도 기록됐다.
관측 free9,394,921,472B는 해당 시점의 값이다. 제외한4SDK는 여전히 미조사·보호 상태다.

다음 reclaimer는 이 exact ZIP/manifest·원본owner/root inode와 각 파일의 metadata/SHA를 새로
대조한 뒤 해당4SDK만 지운다. 각 삭제 직전 전 UID 참조를 다시 확인하고 current SDK root 자체와
전체 SDK/source/proof/shared ELF·DSO를 retained 전후에 대조한다. 원본compiler를 보유한4o8uWv와
다른3SDK·current source/SDK/입력/증거는 정리 대상이 아니다. 새metadata 성장128MiB admission을
유지한다. 현재는 준비 후보이고 실제 삭제·새 free·controls native109 수용은 아직0이다.

### e719c4 실제 SDK4 회수 수용

[40101334963/job120893926067](https://github.com/bin9208/openpilot-rust/actions/runs/40101334963/job/120893926067)는
source `e719c4d39f14b3317493e09c1a5815ab808e3ae0`에서 성공했다. artifact12563256407
(5,792,008B,SHA256 `bf03510a71d356191bc8527c56be8d6747bc6640db2793139baf45044355ce1a`)의
18CRC를 부모가 감사했다. 실제4SDK 제거/8journal과6회 참조 검사, 보호28,844항목의 전후 JSON
전체byte 동일성을 확인했다. current SDK root 자체도 보존했다. free는9,387,831,296→
11,853,766,656B다. 차이는 작업 중 증거 증가도 포함한 관측이며 전부 순수 회수량으로 계산하지 않는다.
원본compiler4o8uWv·다른3SDK·소스/입력/증거/shared ELF·DSO는 정리하지 않았다.

### controls 원본109의 다음 집중 검증 후보

새 `python-rust-523-controls.yml`과 `tools/ci/controlsd_fixture_523.py`는 원본 capture를
다시 실행하지 않고 기존 artifact12462355309의 요청109/원본 wire/독립 pycapnp projection 및
177 source·provider 기록을 연결한다. 현재 핵심7개 source는 원래 fixture150과 동일하며,
공유 Cargo/lib/dependency는 현재 전체source/SDK/실제 CLI·auditor ELF·DSO와 별도로 봉인한다.
현재 compiler-artifact JSONL 전체를 읽어 정확한 CLI/test 하나씩 선택하고 같은 ELF를 실행한다.

새 범위는 native109 생성1회, 기존 exact ignored full-schema/F64-bit auditor1회와 Clippy다.
기존 regular5/원본109/normal40·rejection27·health5/continuous controller를 재실행하거나 새 수용으로
바꾸지 않는다. source2GiB, build8GiB/추가512MiB, proof2GiB 및 raw1GiB/512MiB-file 상한을 유지한다.
immutable seal과 비교 완료를 각각 필수 확인하며, 실패 job의 소스 정리는 업로드 및 전후 immutable
봉인이 성공한 소유 checkout에만 허용한다. 기능 실패는 계속 job 실패다.

로컬 AST/YAML/Bash·diff와 독립 source 검토만 통과했다. 현재 raw109의177 mapping·actual native
생성/ignored1/Clippy/공간 적합성은 미실행이다. 새 diagnostic interpreter2경로는 별도 pending으로
등록하며 기준3580의 추가 교체로 세지 않는다. controls runtime·wire 전체byte 일치·물리 수용을
미리 주장하지 않는다.


## 2026-10-25: Bundle 원본1개 제거 수용

[40103403086/job120899996843](https://github.com/bin9208/openpilot-rust/actions/runs/40103403086/job/120899996843)는
source `5d376b306c93ec799b0c5346cb60e9425b1618f7`에서 성공했다. artifact12563363998
(15,244,279B, SHA256 `f11e9e6200a0abae98cc790fc5cba04f69f771878372028c2be8e4b1968a157c`)의
470멤버 CRC·중복명 부재와 source/index/venv 및 context source/index의6쌍 전후 byte 동일성을
부모가 감사했다. frozen18검사와 strict Clippy가 성공했으며, 기존17본문을 유지한 원본23고유row
184,435B는 과거121 capture와 그대로 같다. 새 원본 producer 실행은0이다.

실제 공개 `rust/tools/run-developer-tool`가 만든411파일 중410파일 전체 bytes가 d2 원문과 같고,
나머지 `SOURCE_COMMIT`1개는 정확한 새 HEAD다. 전체 file/directory roster도 이전과 같다.
strict trace1185시도/실패229/Git성공412/final ELF성공1/이름으로 식별한 Python시도0을 확인했다.
실제 CLI ELF SHA256은 `a3c9b87d121992c213f8570b8492cbd37ce968da6596491b5a7dd1b39deca20b`다.
원본 부재·재출현/held입력변경 거부를 포함한 한정 경계로 `tools/carrot_route_vault/build_bundle.py`
1개를 replaced-local로 인정한다. metadata용 diagnostic interpreter는 pending이며,
무제한 전체 Python 부재·Docker/NAS 발행·실기 수용은 인정하지 않는다.

기준3,580개 인벤토리는 **292교체/3부분/3285대기**다. paired inventory/families의 해당1행만
갱신했다. #523의 나머지 하위 이슈·전체 전환·필수 PR/병합 후·전달/실기 조건은 계속 열린다.

### 5d376b 집중 검증 재사용 admission 복구

[40103403093/job120899997095](https://github.com/bin9208/openpilot-rust/actions/runs/40103403093/job/120899997095)는
source `5d376b306c93ec799b0c5346cb60e9425b1618f7`에서 성공했다. artifact12563952000
(15,834,369B,SHA256 `f32e796ddd7e8e4b5ff9c50d0af1bddefe460a6d4b5b489d975e449483a4ef6c`)의
55CRC/중복0/source-index-venv4쌍,84literal pin·98exact path admission과 과거 cohort의
원문·provider 재사용 경계를 독립 검토했다. 현재 alias26/Unicode scalar1,112,064·Σ154,
기존 native6/affected SSH27 및 선택 lib/bin/test strict Clippy는 성공했다.

과거 a134/41cf 전체 실패 로그를 PASS로 바꾸지 않는다. 각 성공 selector와 원본 Paramsd9쌍/
NumPy9,600/ignored2의 기존 범위를 그대로 보존한다. native ELF 자체가 archive에 없는 receipt는
로컬 재hash했다고 표현하지 않는다. 전체 auth 원본 제거 credit은0이며, maintained atomic gate·
controls fullschema·모델 수치·실기 수용은 각각 별도 미완료다.


### controls109 실제 결과와 #533

[40103403250/job120899997437](https://github.com/bin9208/openpilot-rust/actions/runs/40103403250/job/120899997437)는
native109 생성72.93초와 current CLI/auditor build·strict Clippy 성공 뒤 full-schema auditor가
1,200.27초에 exit124로 중단됐다. artifact12564184881(29,767,475B,SHA256
`acfb025b64ebdc48441e441ea895a93b26b23f66d985e5223555a24ad084f9c7`)의168CRC/중복0,
source/index/전체source/SDK의4쌍, 기존 held347개 불변과 native.jsonl 추가1개를 감사했다.
immutable seal은 성공했으나 complete/comparison receipt가 없어 전체 job은 실패로 유지한다.

앞108행의 native projection1,550,521B는 완료됐고 마지막 `cases`행에 원본70.88MB/native100.95MB/
expected projection153.22MB가 모여 있다. RAM4GiB/swap2GiB의 실제 runner에서 세 거대 문서를
동시에 보유하는 구현을 [#533](https://github.com/bin9208/openpilot-rust/issues/533)으로 추적한다.
OOM kill은0이며 원인을 정확 peak로 단정하지 않는다. 같은 전체 schema/F64/30cases/8605frames와
1,200초 기준을 유지하는 case별 감사 후보를 준비한다. full comparison/원본 제거 credit은0이다.

### 확장 없는 runner의 다음 한정 후보

[현재 native ARM probe 계획](python-rust-523-arm-probe.md)은 두 bin만 빌드하고 원래 QEMU2회/
3frames를 수행한다. prepared SDK의 musl std/CRT를 재사용하며 기존 full ARM12GiB gate는 유지한다.
독립 source 검토와 로컬 문법만 통과했고 실제 실행·8GiB 범위 적합성은 미실행이다.

공유 e0ef target의 `debug/examples`는 현재 private #523/남은 원본 oracle의 필수 입력 참조가
발견되지 않은 오래된 build cache다.642 inventory와 e719 보존 기록에서 regular2728개/
unique inode2274·allocated5,041,766,400B를 확인했다. 내부 hardlink454쌍의 모든 alias가 해당
디렉터리 안에 있으며 당시 외부 alias/nlink 잔여는0이다. 이는 실제 회수량 보장이 아니다.

후속 정리 후보는 그 정확한 flat directory의 파일만 선택하고 원래 SHA/전체 metadata/name,
현재 alias/nlink/전 UID 참조·unknown·compiler 부재를 삭제 직전에 다시 검사한다. 나머지 target/
전체 current SDK/공유 tracked source의 identity·SHA를 전후 비교하고, 후보를 가리키는 retained
symlink도 거부한다. held directory/inode를 이용해 원래 내부 alias만 제거하고 journal을 남긴다.
SDK/deps/DSO/source/proof의 정리로 확대하지 않으며 미조사 SDK/proof는 삭제 namespace 밖의
보호 자료다. 해당 자료 전체를 새로 qualification했다고 표현하지 않는다. 실제 examples 제거는
아직0이며128MiB metadata growth guard를 유지한다.

### #530 routing 수용과 남은 Fast 소비 경로

[9c95 Fast40098470381/routing120897002032](https://github.com/bin9208/openpilot-rust/actions/runs/40098470381/job/120897002032)는
전체 Fast6leaf/aggregate와 함께 성공했다. routing artifact12563995133(469,038B,SHA256
`9a1cfae49efebdd235cd1ae77427d2aac31cddb7569eccb3df0afea173c90b4a`)의46CRC와 source/index/전체SDK
files·roster 봉인을 독립 감사했다. 실제 cold receipt1, bootstrap30, 원래 routing·service-tools,
changed-files caller5를 완료했고 trace는 절대 cargo1/rustc37을 기록한다. 초기 permission 준비39개는
현재 다시 필요하지 않아 새 후보가 global SDK 권한을 변경하지 않는다.

후속 [bc8 Fast40099354080/Python-Android120908225815](https://github.com/bin9208/openpilot-rust/actions/runs/40099354080/job/120908225815)는
일반 bootstrap의 같은 cargo-clippy 설치 충돌로 실패했다. prepared 경로가 routing에만 연결돼 있어
다른5consumer에 read-only SDK helper를 연결하는 후속 후보를 작성했다. 일반 bootstrap API와
cold1/기존30·원래 selectors/2048·6144MiB/7분을 유지하고, 각 소비 경로의 source/SDK after seal과
artifact 성공을 cleanup에 결합했다. workflow 결합을 거부하는 mutation2개를 추가했다. 새25정책검사와
실제5consumer 연결은 아직 미실행이며 #530을 열어둔다.

### #533 및 Paramsd A 후보의 검증 대기

case별 감사 수정은 기존 normalize/compare/wire-count 함수와 non-cases 본문을 유지한다.
과거 core6와 고정 기존 함수 prefix를 그대로 묶고 새 audit source를 별도 SHA로 고정한다.
현재 libtest의 stream 경계3개를 새로 선택·실행한 뒤 원래 native109와 exact fullschema ignored1을
같은1,200초 조건으로 실행한다. 아주 짧은 synthetic3만 sampled mapping의 필수 관측을 적용하지
않으며 실제 exec/관측 maps 검사는 유지한다. 실제 native109/전체 auditor의 관측 필수 조건은 같다.
원문30cases/8605frames 중 최대case는1000frames다. 현재 수정의 실제 메모리·완료 시간은 미실행이다.

Paramsd 다음 A는 기존 child-FD1과 maintained generator exec1만 현재 source로 검사한다.
Eigen native stager/C hook/daemon/generator/두 test ELF와 source/SDK/provider를 봉인하고,
분리된 자식 PGID를 subreaper·starttime·pidfd로 추적/회수한다. 원본1196/CarParams9/NumPy9600/
regular7/continuous1202+8/별도 startup B는 각각 원래 수용 범위를 보존하고 반복하지 않는다.
새 metadata helper와 WF의 interpreter2경로는 execution-edge pending으로 명시해 현재 **25교체/
56대기/1native-existing/2non-executing-text**이며 baseline3580 교체에 추가하지 않는다.

### 52e4 actual examples 회수 수용

[40108377406/job120914732060](https://github.com/bin9208/openpilot-rust/actions/runs/40108377406/job/120914732060)는
source `52e4dec6d0b1a3b919ddb8c7c3ab11ea1975ba9a`에서 성공했다. artifact12565503899
(7,848,863B,SHA256 `a5e437df4b314ca64e2a3008de8bae4a369b447ea17442d2627e1c8847b8de36`)의19CRC/
중복0,2,274 intent와2,728 removed의5,002 journal rows, 원래 alias·inode·SHA·link 감소를 감사했다.
보존39,446항목의 전체 JSON bytes와 tracked source index는 전후 같고,3회 전 UID process scan의
reference/unknown/compiler는0이다. 원래 examples directory inode/owner는 유지되고 남은 파일은0이다.

실제 free는11,846,819,840→16,874,405,888B다. 순증가5,027,586,048B는 실행 중 metadata/proof 등
변화도 포함한 관측이다. old allocated5,041,766,400B 전부의 순수 회수나 영구 여유를 보장하지 않는다.
SDK/deps/DSO/소스/원본·proof는 제거하지 않았다. 미조사 외부 proof/SDK 전체를 새로 qualification한
결과는 아니며 각 후속 작업의 실제 admission을 유지한다.

### 52e4 Focus 성공과 Paramsd staging 실패

[Focus40108377329/job120914731524](https://github.com/bin9208/openpilot-rust/actions/runs/40108377329/job/120914731524)는
성공했다. current89literal pin/104exact path·기존 한정 cohort 재사용이 실제 통과했다. 새 artifact
12566365817(15,834,439B,SHA256 `73830a12d909e9e78492c878d52905f6d102cc9fcece329f963c78bb70224579`)의
전체 raw ZIP 감사는 아직 진행하지 않았으며 새 auth retirement credit은0이다.

[Paramsd40108377396/job120914731807](https://github.com/bin9208/openpilot-rust/actions/runs/40108377396/job/120914731807)는
Eigen staging에서 실패했다. artifact12565713049(9,232,451B,SHA256
`829f9a2d27a4ba272cb7187306885917fa302c22b3984ef8acc15b40d78a306a`)의121CRC/중복0과 source/index
전후 동일성을 확인했다. 원문 stderr는 `Eigen wheel filename differs from release pin`이다.
다운로드를 일반 stdout 파일명으로 전달한 진단 caller 오류이며 daemon build/child-FD/exec 감사에는
미도달이다. Eigen baseline 부재는 final seal을 올바르게 FAIL로 만들었고 자식은 모두 회수됐다.

진단 helper는 curl의 output을 원래 release basename으로 저장한 뒤 같은2,250,658B/SHA를
검증해 stager에 넘기도록 보완했다. stager의 filename/size/hash/기존 destination 거부는 유지한다.
원본·수치·runtime 코드를 바꾸지 않았다. 수정 후 staging 및 A2 실제 검증은 다음 CI에서 확인한다.

### ARM 측정 경계 수정과 controls prefix·signal 다음 검사

52e4 ARM의 실제 musl build는 성공했으나 host build 중 사라진 target 하위 `rmeta` directory를
용량 watcher가 fatal로 처리했다. QEMU와 해당 Clippy는 미실행이다. 자세한 실제 artifact와
source/SDK 봉인 근거는 `python-rust-523-arm-probe.md`에 기록했다. 수정은 mutable target의
정확한 ENOENT만 최대3회 전체 재측정하고, 부분합 초과·그 외 오류·재시도 소진을 계속 거절한다.
watcher는 stop 후 wait하며 불변 source/SDK 검사와8GiB admission·기존 full ARM12GiB gate는 같다.

새 `python-rust-523-controls-prefix.yml`는 기존 prefix library4개와 exact
`actual_cli_int_and_term_stop_the_owned_trace_group_without_success`1개만 선택한다.
현재 Cargo JSONL 전체를 읽어 유일한 lib/test/CLI ELF를 고정하고 prepared SDK·source/index를
전후 봉인한다. 기존 내부 readiness10초/shutdown5초/child30초/aggregate180초와 build8GiB,
Clippy 추가512MiB를 유지한다. `strace --kill-on-exit` 및 owned session/timeout으로 추적 대상의
생존을 제한하고 direct leader를 wait한다. 모든 adopted descendant의 wait/reap을 증명한다고
표현하지 않는다. raw16MiB는 command 뒤/EXIT의 검사이고 hard quota나 실시간 sampling이 아니다.

두 workflow의 독립 source 검토에서 P1/P2는 없었고 YAML/Bash·diff만 로컬 확인했다.
현재 prefix4/signal1·해당 Clippy와 수정 후 ARM/Paramsd A2는 모두 미실행이다. Focus에는
새 workflow exact path1개·literal SHA1개를 추가해105path/90pin으로만 재사용 허용 범위를 넓힌다.
원본 제거 credit·controls runtime 전체 수용은 추가하지 않는다.

### 52e4 controls109 전체 감사 수용 — #533 한정 해결

[40108377364/job120914731786](https://github.com/bin9208/openpilot-rust/actions/runs/40108377364/job/120914731786)는
source `52e4dec6d0b1a3b919ddb8c7c3ab11ea1975ba9a`에서 성공했다. artifact12566018810
(33,406,501B,SHA256 `19737291df1f9ad0ec7c63a0b0a55df317fb0ecb2d02c06f0e8f9a406e46aa4d`)의179CRC/
중복0,158개 proof file의 size/SHA와 complete manifest, source/index/full SDK의5쌍 및
held349개 불변·native output 추가1개를 독립 감사했다. 실제49commands는 모두 exit0이다.

현재 CLI1회73.341초와 exact full-schema ignored1회93.054초가 완료됐다. 기존1,200초 timeout,
30cases/8,605frames/109requests와 full-schema·F64 기준을 유지했다. 새 case별 framing 경계3개와
strict Clippy도 통과했다. 기존 원본3개 전체 bytes와177 loaded-project source pin을 다시 확인했다.
원본/현재 projection109행 전체를 독립 inert 대조해 type/key/길이/순서/모든 값과 `$f64`3,100,676개가
정확히 같았다. 실제 packet138,861쌍 중138,425쌍의 raw bytes만 같으며 나머지436쌍은 wire layout
차이로 그대로 남긴다. full-schema/F64 성공을 모든 wire byte 일치라고 표현하지 않는다.

actual ELF3개의 성공 exec와 관측된 executable mapping/provider 봉인을 확인했다. mapping은
sampling이며 전체 pre-dlopen/data closure가 아니다. 매우 짧은 synthetic3에는 actual exec를
요구하되 자기 ELF의 mapping 관측 필수만 제외했고 native109/전체 auditor에는 같은 예외가 없다.
현재 ELF/DSO 본체가 ZIP에 없어 원격 FD를 로컬에서 다시 열거나 빌드를 재현했다고 주장하지 않는다.
기존 regular5는 과거 core6·함수 prefix 봉인으로 재사용했고 새 regular5를 실행한 것으로 세지 않는다.

이 증거로 #533의 큰 cases 감사 timeout 결함만 해결했다. normal40/rejection27/health5,
continuous IPC·전체 controls runtime·원본3개 제거와 baseline inventory credit은 모두0이다.
전체 #523/#296·현재0538007 prefix/ARM/Paramsd·필수 PR/병합 후·전달/실기 조건은 계속 미완료다.

### PR343 현재 구현 연결

기존 PR343 head `adff0392e61608c9a576550be78dd7eefb8b9065`가 현재 HEAD의 조상임과 원격 ref가
그대로임을 확인하고, force 없이 `0538007a74256ec1dbdbfe0c86d4cc5c33cefea1`로 fast-forward했다.
[PR343](https://github.com/bin9208/openpilot-rust/pull/343)은 계속 dev 대상 draft다. 현재 필수
[Integration40111549950](https://github.com/bin9208/openpilot-rust/actions/runs/40111549950),
[Rust40111549951](https://github.com/bin9208/openpilot-rust/actions/runs/40111549951),
[Fast40111549923](https://github.com/bin9208/openpilot-rust/actions/runs/40111549923),
[docs40111549937](https://github.com/bin9208/openpilot-rust/actions/runs/40111549937)는 실행 대기다.
이 연결은 보호 브랜치 통합·기기 배포·검사 성공이 아니다. PR 본문도 한국어의 현재 구현/증거/한계로 갱신했다.

### 0538007 Focus 현재 admission 수용

[40111455561/job120923877879](https://github.com/bin9208/openpilot-rust/actions/runs/40111455561/job/120923877879)는
현재 source `0538007a74256ec1dbdbfe0c86d4cc5c33cefea1`에서 성공했다. artifact12567066049
(15,834,418B,SHA256 `0382f0692320e52727fbe8f7d0d6669ea5802eb256fa203538397111d101eeac`)의55CRC/
중복0/source-index-venv4쌍을 부모가 감사했다. 실제 changed-path105개가 Git diff와 byte 동일하고
각 exact allowlist에 있으며90literal SHA를 해당 HEAD의 Git blob과 다시 대조했다. embedded prior3ZIP은
앞서 감사한5d ZIP과 전체 raw bytes가 같아 원래 실패/수용 구분을 유지한다.

새 alias26/native6+UCD scalar1,112,064·Σ154/SSH27/strict Clippy가 성공했다. original receipt의
native/source SHA와 현재 HEAD도 일치한다. 현재 ELF 본체가 없는 한계·과거 원본 reuse 범위는 같고,
전체 auth 원본 제거 credit은0이다. ARM/Paramsd/prefix 및 전체 PR gate는 별도로 판정한다.

### 다음 normal40의 명시적 historical admission 후보

기존 original40 원문 artifact12456242812/run39844308544/job120055270738의 raw input/source/result/log
4개를 고정한 별도 `legacy-normal40-12456242812-v1` origin을 추가했다. fresh capture의 기존 필수
source/provider/수치 검사는 그대로이며, 두 모드 혼용·알 수 없는 ID·자동 fallback을 거부한다.
과거 provider 전체 봉인이 미기록인 사실과 과거 hosted runner/source6f25 provenance를 현재 source나
현재 SDK로 채우지 않는다. `capture.json`을 새로 만들거나 native output을 expected로 쓰지 않는다.

현재 native CLI와 trace를 같은 실행·Cereal normalize·비교 core로1회 연결하고40cases/8655frames/
705948 numeric comparisons 및 실제 max error를 새로 계산한다. 기존600초/512MiB·수치 허용오차·
NaN/Inf·input 전체 shape/길이/순서·현재 numerics ABI/roster와 PASS finalizer를 유지한다. 원본109/
rejection27/health5/continuous는 재실행하거나 이번 결과로 수용하지 않는다.

Rust4파일의 독립 source 검토에서 기존8 test body·comparator/numerics/runtime core 불변을 확인했다.
새 negative의 첫6개가 다른 필수 옵션 오류로도 성공할 수 있던 검토P2는 공통 argv를 완비하고
정확한 오류/exit와 archive pin 단계 기준 호출을 요구하도록 수정했다. metadata controller의
정상 PASS hardlink·TempDir 정리를 오류로 오인하던 두 검토P2도 수정해 독립 재검토했다.
exact 게시 pair의 inode·data·추가 alias 검사는 유지하고 정상 정리와 겹친 표본만 최대3회
전체 재수집한다. compile·기능/normal40·메모리 적합성·Clippy는 모두 미실행이다.
현재 source/SDK/ELF/numerics/provider 전후 봉인과 child 회수는 실패 때도 별도로 검증한다.

새 임시 Python metadata2경로는 native 전환·진단 폐기까지 execution-edge pending으로 계속 추적한다.
이 추가는 baseline3580 교체·전체 controls runtime/원본 제거 수용이 아니다.

### 0538007 Paramsd A2 실제 수용

[40111455444/job120923877389](https://github.com/bin9208/openpilot-rust/actions/runs/40111455444/job/120923877389)는
source `0538007a74256ec1dbdbfe0c86d4cc5c33cefea1`에서 성공했다. artifact12567043237
(9,438,312B,SHA256 `7c16ac33b65861b9125cfb95f5c8aae1256086e17366d4650238884dd7b7f386`)의165CRC/
중복0/source-index-SDK-Eigen-system SDK9쌍, 기존 held227개 불변·생성물5개 추가와 완료/최종 봉인을
독립 감사했다. namespace1개는 기록한 실제 owned identity로 정리됐고 live child는0이다.
55개 직접/입양 자식은 supervisor가 회수했으며 관측441개 모두를 직접 wait했다고 표현하지 않는다.

정확한 기존 ignored child-FD1과 maintained generator exec1이 각각1PASS했다. 실제 generator1회,
현재 test2개 및 daemon1회의 runtime exec4를 확인했다. daemon의 child-only ready-write/release-read
FD는4/5이며 첫 empty wait1회 완료 후 두 번째 poll을 잡고 finish/TERM 뒤5초 내 exit0을 확인했다.
별도 generator auditor는 원래 생성1회의 정확 argv/전체 exec trace와288B CarParams를 검사했으며
새 generator를 다시 실행하지 않았다. strict Clippy도 성공했고 source/Eigen/system provider·SDK
전후 봉인과 artifact 업로드/소유 source·job cleanup이 모두 통과했다.

빠른 generator/auditor 자체 mapping이 sampling에 미관측인 한계는 actual exec/held ELF/test 결과와
구분한다. gate/daemon/hook은 실제 mapping에 관측됐고 관측 executable 경로는 held 집합 안이다.
이 runtime trace들의 Python attempt0을 helper 전체의 Python 부재로 표현하지 않는다.
원본9/NumPy9600/regular7/1196행/별도 startup B/continuous1202+restart8과 원본 제거 credit은0이다.

### 전체 PR 문서 단계의 runner ENOSPC — #534

[현재 docs40111549937/job120924154222](https://github.com/bin9208/openpilot-rust/actions/runs/40111549937/job/120924154222)는
매핑 검사에 성공한 뒤 checker 테스트 중 worker의 `System.IO.IOException: No space left on device`로
실패했다. worker 진단 log·후속 seal/upload/cleanup이 완료되지 않아 실제 테스트 assertion 결과와
현재 증가 경로를 확정하지 않는다. 이후 runner22가 online으로 복귀했지만 read-only 용량 조사
attempt2도 Worker 진단 log의 ENOSPC로 steps0에서 실패했다. Actions에서 새 복구 shell을
실행할 수 없다. [#534](https://github.com/bin9208/openpilot-rust/issues/534)의 primary는#527,
#523에는 cross 조건으로 연결했다.71primary/292·3·3285 상태를 그대로 유지한다.

로컬 SSH 설정·기본 outbound key·관리 browser session이 없어 직접 접속 방식을 사용자에게
확인 요청했다. 증설은 다시 요청하지 않았다. 사용자는 과거 build·cache를 모두 삭제하고 현재
문제의 build만 남기도록 명시했다. 현재 소스와 검증 원본 입력은 보존하며, 접속 후 해당 범위로
정리한다. 자기 PR343의 미시작 대규모 run40111549950/951만 실제 취소해 둘 다
completed/cancelled를 확인했다. 필수 검사는 미충족이며 다른 세션의 실행 작업은 종료하지 않았다.
접속 정보 대기 중에도 source-only 구현·검토를 계속한다.
상세 원문/재발 방지 후보와 미확정 사항은 `python-rust-523-runner-534.md`에 기록한다.

### 추가 interpreter 실행 경로의 누락 보완

3580개 파일 인벤토리와 별도로, 이번 #523의 Rust test source에 삽입된 실제 Python oracle5경로도
execution-edge에 추가했다. SSH plan/parser, auth/login, UCD15와 loopback Requests 비교는 실제로
interpreter를 실행한다. 새 Rust source 안에 있다는 이유로 Python 부재라고 세지 않는다.
setup21을 고정 데이터로 바꾸는 후보도 SSH의 남은 두 live oracle 제거를 의미하지 않는다.

normal40 metadata2경로와 합쳐 이 표는25교체/63대기/1native-existing/2non-executing-text다.
mutant 정책 검사에 들어 있는 비실행 Python 문자열이나 `PYTHONINTMAXSTRDIGITS`를 해석하는
순수 Rust 기능은 실행 의존성으로 잘못 추가하지 않았다. baseline292/3/3285는 그대로다.

### setup 키 설정의 한정 frozen21 후보

`developer_cli_original.rs`의 setup-effects21에 기존 a134 원본 JSON payload13,800B를 재직렬화0으로
수입했다. source `a134b32c5d50d346994d671c0882b147175021dc`의 실제 artifact12558498891/
run40089943318에 들어 있던 `prior-a134-focused.zip`에서 원문을 추출했고, SHA256은
`444a47ecba9579d4042f26dd89f8438908b56d244a0f2c98c8b0dd3e1ef3a6ef`다. 과거 metadata에 없던
native_sources/provider 정보를 현재 값으로 덧붙이지 않는다. 같은 authored input·criteria·21개
ID/순서/metadata를 고정 admission하고 기존 semantic verifier를 그대로 쓴다.

SSH plan26/parser13은 fresh 원본을 유지한다. 원본 source는 실제 case가 쓰는 tool만 읽도록
하여 setup 원본 제거에 대한 불필요한 결합만 끊었으며, SSH 원본 부재를 skip하지 않는다.
독립 source 검토에서 이 경계와 기존3 test body·Case·candidate·verifier 불변을 확인했다.
고정 원문/metadata/roster/types/입력/criteria 변조 negative3개와 기존 setup 비교1개를 새
전용 Actions에 연결했다. 별도의 공개 `rust/tools/run-developer-tool setup-ssh-keys` 무인자
실행1회는 실제 current ELF·정확 exit1/usage를 확인하도록 구성했다. frozen21 자체를 실제
네트워크 CLI21회 수용으로 표현하지 않는다.

WF bootstrap의 build가 반복 용량 감시 밖에 있던 검토P2는 같은 watched subtree와 bounded
7분 경로로 옮겼다. 원래 bootstrap2048/6144MiB admission을 낮추지 않았고 후속 growth는
준비한 동일 cache/SDK를 재사용한다. actual current replay·Clippy·공개 entry·봉인/회수는
모두 미실행이다. `tools/scripts/setup_ssh_keys.py`는 그대로 두며, pre-delete 실제 수용 뒤
별도 원본 부재 커밋과 같은 검증을 통과해야1개 제거를 인정한다. 현재 credit은0이다.

### 다음 소스 배치의 검증 경계

위 controls normal40·setup frozen21·#534 docs guard의 소스 독립 리뷰 지적은 수정·재검토했고
추가 P1/P2는 발견하지 못했다. uploader PC no-op 후보는 기존 `5b66042b0fd7808c2523df732c2433993d2314dc`
blob 전체 bytes를 재사용했다. 다른 PR500 수정이나 dev-md 작업은 가져오지 않았다.
Focus의 허용 경로는 exact120개, 개별 literal source pin은103개이며 해당 현재 bytes와 모두
일치한다. 변경 소스를 과거 성공 cohort로 확대하지 않고 별도 현재 실행 대상으로 유지했다.
execution-edge의 변경4경로 working blob도 동결 소스로 갱신했다.

로컬 workspace Rust 서식, YAML4개·내부 Bash43개·단독 Bash1개·Python AST와 embedded AST,
diff를 확인했다. 이 배치의 compile·기능·Clippy·새 normal40/setup entry·docs growth 실패 주입과
실제 정리는 미실행이다. 앞의0538007 성공은 이번 미실행 소스의 성공으로 사용하지 않는다.
원본 제거·baseline credit·필수 PR/병합 후·전달 및 실기 수용은 별도 조건으로 남는다.

### checkout 없는 과거 공용 build 회수 후보

새 b7ab74의 setup/normal WF도 실제 검사 이전 `actions/checkout` archive 다운로드 ENOSPC로
실패했다. worker가 해당 준비 단계까지 시작한 관측을 근거로, 외부 action0개인 inline rescue를
별도 시도한다. 첫 범위는 과거 공용 `rust/target` 하나이고 현재 private docs build·source·원본
입력·현재 SDK는 유지한다. metadata와 모든 UID의 process 참조를 확인한 뒤 literal root만
삭제하며, 작은 콘솔 요약으로 용량/inode/실제 삭제 여부를 남긴다. 독립 소스 리뷰와 문법만
확인했고 실제 삭제·복구는 미실행이다. 이 임시 Python 복구 진단1경로를 edge pending으로
추가해25교체/64대기/1native-existing/2non-executing-text(92행)로 추적한다. baseline credit은0이다.

### 2026-10-25 GitHub 기본 러너로 명시 전환 — #535

사용자는 bin-actions가 타관리자가 만든 현재 사용할 수 없는 러너라고 설명하고 GitHub 기본
러너를 사용하도록 명시했다. 이전 self-hosted-only 지시보다 최신 선택을 우선한다. 미시작
rescue40118006504는 실제 completed/cancelled를 확인했고 현재 복구 WF를 제거했다. 타관리
러너의 원격 삭제/복구 실행은0이며 SSH/증설 대기를 전환의 선행 조건으로 삼지 않는다.

#535에서 명시 Ubuntu24.04 기반 집중 검증과 실제 private 경로·현재 SDK 준비/전후 봉인을
연결한다. source/원문·Rust1.94의 기존4개 component pin·ELF/ABI·기능 기준과 필수 gate를
유지한다. 과거 manifest의 절대 경로는 원문 그대로 두고 현재 동일 component hash와 별도
대조한다. 기존 성공을 새 러너의 성공으로 재표기하지 않는다. 상세는
`python-rust-523-hosted-535.md`에 기록한다. 전환 후보의 현재 기능/빌드/Clippy/용량 실측은
미실행이며, baseline292/3/3285와 원본 제거 credit0을 유지한다. 임시 rescue edge의 삭제는
25교체/63대기/1native-existing/2non-executing-text/1removed로 구분한다.


### 2026-10-25 기본 러너 첫 실제 실행과 실패 보정

source f8e9d62b073d2c8a66c68f728247b9424df72ce8에서 docs40121817587과 Fast5leaf는 성공했다. Fast routing은 actual cold bootstrap1 성공 후 changed-files의 고정 caller RUN fixture가 새 prepared SDK prefix와 달라 실패했다. Focus40121818148은 현재 alias/Unicode/SSH27/Clippy·봉인이 성공했으나 과거 oracle JSON의 절대 cache 경로2비교가 실패했다. setup40121818144는 Bash local mib가 단위 상수를 가려 admission을 잘못 계산한 #536, normal40121818149는 static 여부를 구분하지 않은 gh ldd provider admission #537에서 중단됐다. 원본 비교가 실행되지 않은 경계를 PASS로 세지 않는다.

정확 job/artifact SHA·CRC·전후 봉인, source-only 보정 후보와 현재 실측은 `python-rust-523-hosted-535.md`에 기록한다. 실제 기본 러너 실행으로 전환했으며 bin-actions 복구를 기다리지 않는다. 보정의 기능/빌드 검증은 다음 Actions에 남아 있고 PR343 head053/dev draft 및 baseline292/3/3285를 유지한다. 원본 제거 credit0이다.


### 2026-10-25 보정19e180650 실행 결과와 남은 검증

source19e180650bae8b46f9d04056b796ba68666b364e의 Focus40122888950와 changed-files40122889010은 성공했다. 둘의 source/index/whole SDK 봉인·업로드/소유 정리와 실제 원문 artifact CRC를 확인했다. Fast는 caller5와 cold bootstrap1을 통과한 뒤 docs cleanup의 추가 SDK/recovery 조건 미등록으로 실패해 validator를 보정 중이다. setup은 frozen21/음성3/Clippy/public1의 기대 출력까지 도달했으나 SIGCHLD trace 문법 #539 및 재빌드 CLI identity #540 수용이 남는다. normal은 static gh를 수용한 뒤 고정 로그 API의 escape 출력 정책 #538에서 중단됐다. 아직 normal40 수치 비교·setup 완전 봉인은 수용하지 않는다.

상세 exact job·artifact SHA와 source-only 후속 후보는 `python-rust-523-hosted-535.md`에 기록한다. Integration/Rust/ARM/Mac 기본 러너 전환도 이어가며 PR343 head053/dev draft, baseline292/3/3285와 원본 제거 credit0을 유지한다.


### bdb819e59 기본 러너 실제 검사와 후속 원인 분리

source bdb819e59534ffba7c61751653ddaf360e8e324a를 dev 대상 draft PR343에 fast-forward했다. 실제 PR merge source는ba1b07c38702bac3bb49614d14b54adaacaa95df다. Focus40126286930과 docs40126295962가 성공했고 각각81CRC/54CRC 및 source·SDK 전후를 확인했다. Fast는5leaf 성공 뒤 공통 YAML alias key 구분자 문제(#541)로 routing·aggregate 실패다. Integration40126295997은14성공/2실패이며 빌드·pytest570 뒤 source 봉인 실패를 #546으로 추적한다. setup은 실제21+3/Clippy/public stdout·exit 일치와 CPU 주석 문법 통과까지 확인했으나 missing packed-refs의 Cargo 재link(#540)를 거부했다. normal은 기존 stager의 빈 marker를 임시 helper가 빠뜨린 #543에서 중단돼 수치40회 비교는 아직 미도달이다.

실제 Mac Rustup 배치 가정(#544), logical matrix와 보호된 upload assertion(#542), Panda venv 경로(#545), updater private TMPDIR trace(#547)도 원인별로 보완 중이다. 정확한 source/run/artifact·수용 한계와 후보 변경은 `python-rust-523-hosted-535.md`에 남긴다. 새 후보는 Actions 검증 전 UNRUN이며 baseline292/3/3285, 이번 원본 제거 credit0과 전체 미완료 상태를 유지한다.

### ded6051b3 실제 결과와 일반 소스 감시 수정

ded6051b33f51ffa3aa4b23e76e51c2eb4804d4b의 Focus40129042247은 alias26·Unicode7·SSH27·Clippy와 source/SDK 봉인까지 성공했고 artifact81CRC를 감사했다. Fast40129042267은 이전 YAML parse를 통과했으나 첫 unconditional fork guard를 잘못 분류한 정책 검사2개가 실패해 수정했다(#542). setup40129042246은21+3/Clippy/public 출력 비교 뒤 held CLI 교체를 거부했다. 실제 dangling source symlink의 Cargo 감시가 재빌드를 유발한 #540으로, 일반 missing/symlink 감시와 read-only Git 조회를 수정하고 회귀9개·Clippy를 setup Actions의 최종 CLI hold 앞에 연결했다. 새 소스의 기능 수용은 아직 미실행이다.

normal40129042248과 이전 필수 ARM 일부는 진행 중이며 완료로 세지 않는다. 실제로 job 시간 한도를 초과한 ARM5개만 추가 보완했다(#548). 별도 #549에서는 기존 shared logging 도구2개의 남은 원문/lifecycle/caller/absence 검증을 진행하며 원본을 보존한다. 정확한 source와 artifact·검토 범위는 `python-rust-523-hosted-535.md`에 기록했다. PR343은 bdb819e59/dev draft, baseline292/3/3285와 새 원본 credit0, 전체 #523 미완료 상태를 유지한다.

### 96a 실제 setup 수용과 shared logging 후보

96a904e6da44151785eb1206963fa6ecea019e31의 Focus40131329248과 setup40131329251은 성공했다. 각각81CRC/167CRC와 전후 봉인을 감사했고, setup은 watcher9/Clippy·원문21/음성3/public1 및 CLI inode·SHA 불변·upload/정리를 확인했다. #536/#539/#540의 개별 해결 근거와 별도 trace 성공 집계 공백 #551을 구분한다. Fast40131329250은 isolation84/policy29 성공 뒤 cutin의 예전 runner 비교에서 실패했고 alert-manager는 checkout 지연을 포함한7분 제한에 걸렸다. 같은 runner 비교3개를 한정 보완하고7분 한도는 유지한다.

normal40129042248은 이제 실제700초 timeout으로 실패했다(#550). raw40/8,655rows는 있으나 정식705,948비교/최종 native 수용은 아직 없다. 기존600/700초·수치·source/SDK 기준을 유지해 조사한다. shared logging2의 #549에서는 고정108개 inert 자료를 직접 읽는 native prepare-context와 core21/importer9/retained14/한 번의 raw 비교를 실행할 원본 보존 후보를 준비했다. 기존 old27·P2·전체 trace/FD·caller/absence gate와 구분하며 새 기능은 Actions 검증 전 UNRUN이다. 상세 exact source/run/artifact·리뷰·한계는 `python-rust-523-hosted-535.md`에 남긴다. baseline292/3/3285와 전체 미완료 상태를 유지한다.

### a4 수용·38ec 후속 검사와 다음 배치

a4b1db51c의 Fast40132971071과 Focus40132971275, setup40132971114는 성공했고 실제 산출물과
전후 봉인을 감사했다. setup은 public exec20/20성공·15signal까지 확인해 #551을 종료했다.
logging40132971238은 raw108/core21/importer9 및4 lifecycle body 성공 뒤 proof1GiB 상한을
넘어 실패했다. 반복 read 버퍼를 펼치지 않는 collector와 읽기 전용 native trace inspector를
준비하며, 새20검사와14 lifecycle/최종 raw 비교는 다음 Actions에 남는다. 원본2개는 유지한다.

38ec20936의 Focus40133699721은 성공했지만 Fast native는7분 한도에서 취소돼 #552로
추적한다. 기존72target을 정책4/도구53 등으로 빠짐없이 분할하고7분·격리/봉인·aggregate의
전원 성공 조건을 유지한다. Navi source 정책도 고정 원문 job과 같은 BUILD/CALLER bytes로
compact YAML을 비교하도록 보완하며 exact2selector만 Fast에 추가한다. 실제 새 분할의
성공은 아직 수용하지 않는다. normal40133699790의 SHA2 패키지 전용 opt3 후보도 진행 중이다.

정확 source/run/artifact·후속 후보와 한계는 `python-rust-523-hosted-535.md`에 기록한다.
PR343은 bdb819e59/dev draft이며 최신 배치의 필수 deep 결과는 아직 없다. baseline3580의
292교체/3부분/3285대기, 이번 retirement credit0과 전체 #523 미완료 상태를 유지한다.

### 99c 실제 결과와 원본1 삭제 후 검증 후보

99c0ac14c의 Fast6native leaf는72개 baseline와 Navi 정책2개를 통과했고 분할 native/tools가
305/296초에 완료됐다. 전체 Fast는 역사 fixture 끝 빈 줄로 Python/Android가 실패했다.
원문7333B를 고정 LF로 복원하는 저장7332B 방식으로 보완하며 공백 gate는 유지한다.
Focus는 새 tools prefix의 명령 수2→3 미등록에서 실패했고 fresh Unicode/SSH/Clippy 및
전후 봉인은 성공했다. logging은 raw108/core21/importer9 뒤 trace test의 oversized sparse
파일이512MiB 제한에 걸렸다. production/ulimit을 유지하고 작은 실제 경계 파일로 보완한다.

38ec normal40는 sha2 opt3를 실제 확인했지만700초에 재실패했다. JSON을 File에 직접 쓰는
trace 경로를 BufWriter/명시 flush로 감싼 다음 한정 후보를 준비한다. 계산/수치·입력/원문·
600/700초는 그대로며 성공을 미리 수용하지 않는다. exact source/artifact와 추정 한계는
`python-rust-523-hosted-535.md`에 기록했다.

setup의96a/a4 pre-delete 수용 후 Python1파일만 삭제하는 후보를 준비했다. 실제 삭제 commit의
부재/재출현 controls6과 기존21/3/public1·봉인/전달 검증 전에는 해당 row를 pending으로
유지한다. PR343 dev draft, baseline292/3/3285와 전체 미완료 상태는 변하지 않는다.

### setup 원본1 삭제 후 실제 수용

source602ea095f의 setup40137010381은 guard6·controlled21·negative3·public1/Clippy와
원문/122held·source/index/SDK·closure/upload/cleanup을 통과했다.172CRC와 실제 원본 부재
299개 관측을 감사해 해당1파일만 원장에 추가했다. 현재 baseline은 **293교체/3부분/3284대기**다.
receipt의 전체 Python 부재/retirement false는 그대로 유지하며 한정 원장 판정과 구분한다.
같은 source의 Focus40137010398도 성공했고85CRC·전후 봉인·exact setup D를 확인했다.

Fast40137010462는 Python/Android 등6leaf가 성공했으나 native cold trace의 불명237행에서
거부됐다. 이를 정상 기록으로 수용하지 않고 같은 source의 실패 작업만1회 재관측한다.
logging40137010352는20경계와5 lifecycle body가 통과했으나 metadata 경로 trace1.71GB로
1GiB 한도에서 실패했다. 실제 syscall/FD/path/결과를 유지하는 간결한 표시 후보를 준비하며
old27/full closure/caller/원본2 및 전체 #523은 계속 미완료다. 정확 evidence와 한계는
`python-rust-523-hosted-535.md`에 기록한다.

### 602 후속 실제 실패와 다음 검증 후보

Fast 실패 native만 재실행한 attempt2/job121004679000도 unknown raw310행을 거부했다.
성공6leaf는 재실행하지 않았다. same exec 범위의 seccomp-bpf 요청과 provider/stderr/status
기록을 보강하며 unknown 행을 허용하지 않는다. logging의 표시 압축과 같은 lexer 요약
schema2·경계6개, 별도 fresh-build-only C++ collision8 후보를 준비했다. 새 성공은 UNRUN이다.

normal40137010441은40cases/8655rows trace가 EXIT0으로 끝나700/600초 안에 비교에 도달했다.
첫 `controls[3].rows[0].state.lat_pid[0]`에서 F32 54 ULP 차이로 실패했고 #553에서
NN response 경계를 추적한다.208CRC·241held와 source/index/SDK 전후 동일성은 확인했지만
705948 비교·actual trace DSO mapping·native baseline과 최종 closure는 미수용이다.
normal 성공으로 기록하거나 expected/수치 허용오차를 바꾸지 않는다. baseline293/3/3284,
PR343 dev draft와 전체 미완료 상태를 유지한다.

## 2026-10-25 기본 러너 후속 검증

source079의 Fast7leaf+aggregate/Focus는 기본 Ubuntu 러너에서 실제 성공했다.
logging은13lifecycle 성공 뒤 금지 옵션의 오류 표시 차이 #555에서 실패했으며,
collision8은8case 성공 뒤 fixture-build model64MiB 경계에서 최종 실패했다.
한도를 유지하는 PID별 분할 감사와 작은 CLI 오류 보정을 검토했고 실제 후속 검증을 진행한다.
PR343 head419/mergee882의 docs는 성공했으나 나머지 deep 검증에서 venv 경로·TMPDIR·
종료 peer ESRCH #556·이전 runner 정책과 source 생성물 mutation #557이 드러나 보정 중이다.
controls54ULP 차이 #553은 기대값을 유지한 첫 연산 관측으로 좁힌다.
정확 Actions/ZIP/SHA·완료와 미완료 범위는 `python-rust-523-hosted-535.md`에 기록한다.
baseline3580의293교체/3부분/3284대기와 전체 #523/#296 미완료 상태는 유지한다.


## PR9ae의 현재 검증과 #574 테스트 관측 수정 후보

PR343 head `9ae67e412eeb8d9c4b3e19ee9f3b586fe2855739`/실행 merge
`75d0cfd49912b631946721e58979487935372828`의 Fast40187340400 및 docs40187340388은 성공했다.
Rust40187340482는 진행 중이며 original-model121150826818와 selfdrived121150826942의 실패를 확인했다.
전체 run 완료나 다른 leaf의 성공으로 두 실패를 덮지 않는다.

[selfdrived121150826942](https://github.com/bin9208/openpilot-rust/actions/runs/40187340482/job/121150826942)는
Params process unit9 중8PASS/1FAIL이다. kill/drop 뒤 사라지는 descendant의 `/proc/<pid>/stat` 읽기가
ESRCH를 반환했고, ENOENT만 인정하던 test observer가 panic했다. 원문209451B/SHA256
`5db624a280d751366feddb5fd64dc60894bb553e04333b667022a5fac361196c`를 확인했다.
이는 실제 누수나2초 제한 초과가 입증된 결과와 구분한다. 뒤 seam/원본 비교/IPC 기능은 미도달이다.

#574 후보는 cfg(test) 안에서만 종료 관측을 분리해 기존 zombie/NotFound와 정확 ESRCH를 인정한다.
live/malformed stat은 계속false, EACCES/EIO 등 다른 오류는 그대로 실패한다. 새 오류 분류 unit1개와
기존 actual finish/drop test를 유지하며 production Process/kill/wait/ECHILD/2초 제한은 byte 불변이다.
로컬 확인은 Rust 서식·diff·production prefix 비교뿐이며 기능은 새 Actions 대기다.
#572의 ARM caller 개별 완료와 controls288의 full normal40 두 성공은 각각 별도 기록에 남겼다.
baseline3580=294교체/3부분/3283대기와 전체 #523/#296/원본 제거·기기 미완료 상태는 유지한다.


### #569 historical RED 연결의 실제 수용과 Forms 후속

PR9ae/merge75d0cfd의 [UI40187340482/job121150827016](https://github.com/bin9208/openpilot-rust/actions/runs/40187340482/job/121150827016)에서
historical short-read exact selector가 실제 RED(0PASS/1FAIL/0ignored·partial frame 메시지)였고,
복원한 current recording9는9PASS/0FAIL이었다. compatible-tests와current-prefix18569B는 byte 동일하고
고정 SHA `5b26f9e5ba3db68693b00532930fd7ee509122f18372a1c6d308086cc855cddc`다.
artifact12597532630(15103032B/SHA256 `b9088ee98d53c986f3197ff3dfa6d41586c641d0014877ff0d0ea985277068f2`)543CRC,
private12597393030(290218B/SHA256 `e746615ea12a9341373f419dd3e356027cb53d783f4c8747477bec6fb2ed1217`)12CRC와
source/index 전후 동일성을 확인해 #569만 개별 해결했다. 뒤 Forms7PASS/3FAIL은 #575로 분리한다.
현재 original application.py는 #557 font staging의 검증된3구간 변경뿐이지만 Forms에는 #559의
기존 exact projection 연결이 없다. blanket SHA 허용이나 원본/draw 기대값 재생성 없이 보완한다.
전체 UI/PR/원본 제거·기기 수용은 계속 미완료다.


### #571: 제한 증가 없이 native model-family 전체 gate 완료

PR head9ae67/실제merge75d0cfd의 [40187340482/job121150826776](https://github.com/bin9208/openpilot-rust/actions/runs/40187340482/job/121150826776)은
15분28초에 SUCCESS였다. exact same-ELF/two-root selector는1PASS/0FAIL/0ignored/417.34초다.
sha2 dependency opt3만 적용하고 runtime/test opt0, 원래 job120분/command1200초와 모든 사례를 유지했다.
artifact12597496609(479091782B/SHA256 `ca99a8ed703e7735a9292cb3eb69326e6bf83dfdb773de743a51ee3a924abaae`)의
2915unique/safe CRC와12개 final accepted report의 실제 raw size/SHA를 감사했다.
동일held138record, 두root의6source부재/aftertraffic, driver16/startup8/USB6 case per root,
재등장24·badpolicy/foreignoutput8·missingunknown6 거부를 확인했다.55command는모두error=null,
각timeout안에서끝났고 positive12의합361.192267771초/최장55.2042493초였다.
전체command합408.369825746초와libtest의차이8.970174254초는미귀속이며 CPU·hash 단독비용으로주장하지않는다.

private12597566376(290208B/SHA256 `d510ad08bfdb05c3bb19e41755912524419cfa4eef3c7687263e74e37ee6be6e`)12CRC와
source/index 전후 동일성, actual API의Clippy/최종guards/upload/source seal/두cleanup 성공을확인해 #571만개별해결한다.
일반/raw·collector/no-collector·원래parsed tolerance1e-6/raw exact 기준은불변이다.
exec trace 없음/interpreter not_measured/device false를보존하고 전체ELF독립재해시/전체FD·DSOclosure를과장하지않는다.
이전bacda의2시간취소와oldZIPwholeSHA미확정은그대로남긴다. #482 수치·전체PR/원본제거·전달·기기수용은미완료다.


### PR1344 same-VM 진단과 #576 후보 등록

#482의 고정1344 첫 packet을 같은 VM의 전체SOURCE/object/image·ABI/ISA 인증 뒤 current daemon과
원본/native plain/tap3stage로 추적하는 private test/controller를 추가했다. 기존 main128/half5/cached10/
guarded8/기대값·수학·공개runtime API는 유지하며 D1 strict raw FAIL을 진단 성공으로 덮지 않는다.
새 임시 Python 실행은 provider 준비·metadata·controller·embedded판정2·고정base동적실행·collect/plain/tap
총9개 role를 execution 원장에 pending으로 명시했다. 원장은140edge(26교체/110대기/1기존native/2텍스트/1제거)이며
baseline3580=294교체/3부분/3283대기는 그대로다. 진단 자체도 전환 또는 폐기 전까지 완료로 인정하지 않는다.

#576 후보는 authored PID 음성을 정확 class/method/assignment AST로 제한하고 전체 원복동일성을 확인한다.
기존 actual normal·target/Replay·4+12·최종bool AND/시간·용량 기준은 유지한다. 두 후보의 로컬 검증은
소스·구문·서식·diff와 독립 검토뿐이며 실제 새4선택자/D1↔direct/4+12/봉인·회수는 Actions 대기다.

## 기본 러너 후속 검증·분리 수정 — 2026-10-25

GitHub 기본 러너 전환 상태에서 작업을 계속 진행 중입니다. 현재 issuebranch source는 `a3873834aefd264e37f3513c58c1f9a7ef4b32fc`이며 PR343은 draft/dev, source56f 단계와 분리해 유지합니다. dev-md는 수정하지 않았습니다.

- #570은 [40193190050](https://github.com/bin9208/openpilot-rust/actions/runs/40193190050)의 완전한 첫 du 실패 원문/82CRC 감사로 관측 누락 조건을 충족해 종료했습니다. 실제 target9개 소멸 경합 수정은 #579로 분리했습니다. 모델 strict Clippy/A/B/D1/D2는 아직 미도달입니다.
- #577은 authored4/12+게시5초와 필수 I/O의 기존 global52분을 분리한 source8aedd0dc2를 [40193481527](https://github.com/bin9208/openpilot-rust/actions/runs/40193481527)에서 검증 중입니다. 원래 normal/수치/기한·caps·최종 AND는 유지하며 성공으로 올리지 않았습니다.
- #560 후보 source131a789f8의 [40193797145](https://github.com/bin9208/openpilot-rust/actions/runs/40193797145)는 fork/vfork/thread3통제·protocol3·marker10을 통과한 뒤 nonleader exec의 `= ?`를 엄격히 거부해 실패했습니다. 700CRC/source-provider32쌍/held177 및 finalizer/upload/cleanup은 확인했으나 전체18/cold/auditor는 미도달입니다. TCB 교체 후 stale restart operator 경계는 #581에서 최소 수정 중이며 parser를 완화하지 않습니다.
- #549 새 원본·복사본 index journal sourcea3873834a는 [40194802656](https://github.com/bin9208/openpilot-rust/actions/runs/40194802656)에서 실행 중입니다. 이전8fcbc의 mode/TAR 통과 뒤 raw index 불일치 원인을 좁히며 원본2는 보존합니다. CLI20/raw21/policy3/원본 삭제 수용은 아직 없습니다.
- 기존 PR9ae [Rust40187340482](https://github.com/bin9208/openpilot-rust/actions/runs/40187340482)의 LLVM20 job은30target/108PASS/0FAIL로 source/provider 봉인·upload/cleanup까지 실제 성공했습니다. #355의 input-binding/strict-prefix 조건을 뒷받침하지만 AMD 전용 #260 검사를 대신하지 않습니다. workspace239PASS/1FAIL의 UI CI 경로 해석 문제는 #580 source0fa20a144에서 수정해 Fast에 정확 selector를 연결했습니다. 전체 run은 ARM UI 빌드가 남아 있고, 실패는 originalmodel/selfdrived/UIconnectivity/workspace 및 그 aggregate로 유지합니다.

baseline3580은 교체294/부분3/대기3283으로 추가 승격0입니다. 실행 원장은141edge=26replaced/111pending/1native-existing/2text/1removed이며 새 Fast Python 회귀 경로도 정직하게 pending입니다. #523/#296 전체 전환·PR 필수검사·통합/전달·실기 수용은 미완료이고 이슈를 닫지 않았습니다. 로컬은 소스/문법/format/diff 및 저장 artifact의 inert 감사만 했습니다.

후속 #579 source batch는 새 metadata provider 호출점1개도 pending으로 등록한다. 원장은142edge=26replaced/112pending/1native-existing/2text/1removed이며 baseline3580의294/3/3283은 그대로다. 아직 기능 GHA_UNRUN이다.

## #574 후속 컴파일 교정 — 2026-10-25

source56f PR의 selfdrived121174299458은 새 cfg(test) 회귀의 `Errno::ACCES` 상수 오타(E0599)와 그에 따른 E0282로 libtest compile에 실패했다. compiler가 제시한 실제 `Errno::ACCESS`로 한 식별자를 교정하며 production prefix/ESRCH·NotFound·Zombie 판단/기존기한·kill/wait는 byte 불변이다. 원문로그210707B/SHA256 `fd143cbf55f03be18c10d3cc58da25e403bd7f8ea06de678d1b9c29fd31aeff7`를1회 읽었다.

Fast native-tools에 기존 feature-gated `selfdrive_params_check::process::tests::`5개를 별도선택하고 정확5PASS/0FAIL/0ignored를 요구한다. 기존70여 native target/선택/순서·초기예산·source/SDK후검사·업로드·cleanup은 유지한다. 기본feature에서미컴파일됐던단위모듈을실제GHA에서검증하는연결이며 새테스트/production동작/기기접근/전체PR성공을추가하지 않는다. 로컬rustfmt/diff만하며 이교정의기능은GHA_UNRUN, #574는열린상태다.

Fast 정책의 기존73개 suite 선택과 baseline command signature는 유지하고, 추가 selfdrive-params-native library5 command·결과5PASS grep을 native-tools에서 정확히1회씩 service suite 뒤 실행하도록 검증한다. 기본 feature에서 빠지는 cfg(test) 컴파일을 이 좁은 실제 CI가 담당하며 로컬 기능 검증은 실행하지 않았다.

## GitHub 기본 러너 후속 — 2026-10-25

source `5bee0d30b3a824cc4f2dbcc23c4caba2fa813a64`의logging#582/cold#583/selfdrived#574후속을push했다. [logging40199544123](https://github.com/bin9208/openpilot-rust/actions/runs/40199544123), [cold40199544139](https://github.com/bin9208/openpilot-rust/actions/runs/40199544139), [Fast40199544187](https://github.com/bin9208/openpilot-rust/actions/runs/40199544187)은실행중이며수용을미리부여하지않는다. #585 exactfont Network source경계와새4는 `febb3917e`에추가했고기존8/full12·실제CLI·Clippy는다음PR정확source수용이필요하다.

#579는68acurrent2frame원본/native whole-output12개와PR1344의402동시metadata검사,2양성12음성/raw GNUdu2일치·strictClippy로개별종료했다. actual ENOENT재시도는0이며race_recovered=false다. PR1344 D1은첫ModelV2파일1개뒤180초제한실패(#584)로원래12frame/33publication과D2는미수용이다. #578는lint수정성공과D2미도달을분리하여열린상태다. #576/#577은8acontrols4양성12음성/40실제case·8655frame·705948비교와타이머7단계,fullartifact/immutable/회수/후검사로개별종료했다. 전체109target재실행·half5·정확source필수PR·3580인벤토리294/3/3283·전달/기기수용은완료하지않았다.

## #530 CarrotMan x64 호출부 후속 후보 — 2026-10-25

PR56 actual merge f8c781ede/job121174299331은 첫 disk guard의 non-prepared bootstrap에서 Rustup 부분 설치 복구·cargo-clippy 충돌로 실패했다. APT/PIP·마지막 명시 installer·CarrotMan 기능은 미도달이었다. 설치 전 partial 상태의 최초 생성 원인은 여전히 미확정이다. 현재 OPEN #530에 연결하며 새 알고리즘 결함으로 분류하지 않는다.

main의 x64 CarrotMan 한 job에만 fresh SDK before와 독립 always after를 넣고, 마지막 installer 한 명령을 준비된 전체 manifest 검증으로 바꿨다. 기존 helper/일반 bootstrap·다른43 job·native ARM8·25분·원래 기능/빌드 argv는 보존한다. private SDK/Cargo/bootstrap prefix가 이미 있으면 채택·수리하지 않고 거절한다. source 잔여2GiB+SDK3+원래 operation8+guard1+registry1+reserve0.5GiB를 같은 filesystem의 동시 잔여 growth로 승인한다.

기존 native-json11의 +1.94.0 argv를 유지하기 위해 helper가 내보내는 SDK PATH는 private proof에 기록하고, 공식 init과 byte가 같은 private rustup/cargo/rustc proxy만 실제 PATH에 앞세운다. guard는 prepared root/manifest로 절대 Cargo/rustc를 사용한다. 자동 설치는0, SDK after 성공은 두 cleanup의 필수조건이다. 새 exact caller 정책과 partial/admission/자동설치/proxy/순서/cleanup 변조12개 거부 회귀를 추가했다. 로컬은 구문/서식/소스 역투영만 확인했으며 실제 설치·cold bootstrap·기능·전체 봉인/시간/공간 적합성은 GHA UNRUN이다. main은498470B이고500000B 한도 안이다.

#574 후속 Fast [40199544187/job121187581139](https://github.com/bin9208/openpilot-rust/actions/runs/40199544187/job/121187581139)는 source5bee에서 process library5개를 실제0FAIL/0ignored/88filtered로 통과했다. 새 dead/missing 판정과 finish/error-Drop 회귀를 모두 포함하며 전체 Fast도 성공했다. 부모는 artifact12601940985(321672B/SHA `73e236c3ea5b64f7f18a34ba041566d904c902578a0d52259b27283327c8ebde`/35safeunique+CRC)의 source/index·SDK files/roster/rejected 전후5쌍과 실제 test log를 감사했다. 이 결과는 cfg(test) 오타와 좁은 회귀 범위의 수용이며 selfdrived 전체 seam/IPC/정책·원본 비교의 후속 PR 수용은 남아 있다.

## 세션 인계와 기능군 병렬 실행 — 2026-10-25

사용자가 이 세션에서 #523 전체 완료를 이어서 수행하도록 명시했다. 완료는 대단원 이슈의 모든 수용 조건 충족이며, 계획 작성·PR 생성·CI 등록만으로 종료하지 않는다. 이전 세션은 재부팅을 위해 중단됐고 HEAD `f9879504a544a9d7e475206841b218b6d8d81cad` 및 미커밋 10개 경로를 보존했다. 새 관리 worktree `python-rust-523-completion`의 `codex/feat-523-completion`으로 Git binary diff와 필요한 재개 기록을 복사하고 경로별 내용/삭제 상태 일치를 확인했다. 이전 worktree와 기본 dev-md checkout은 변경하지 않았다. 기존 원격 #523 브랜치 및 dev 대상 draft PR343을 통한 검증·통합 경로를 이어서 사용한다.

초기 병렬 범위는 #549 로깅 postdelete 완성, #584 모델 진단의 SHA 비용 보정, Planner의 SDK 계약 회귀 수정, F02 compiler의 다음 독립 기능이다. 부모는 공유 소스 등록·SDK 준비 실패·CI·통합과 원장을 맡는다. 모든 보조 작업자는 `gpt-6.1-sol`이다. 로컬은 소스/구문/서식/diff, 기능·빌드·원본/모델 비교는 GitHub Actions에서 실행한다.

11월 15일 완료를 목표로 10월 27일까지 실제 미구현/검증 대기/CI 차단을 기능군별로 구분하고, 11월 7일 구현·원본 제거, 11월 11일 정확 소스 통합 검사, 11월 12~15일 전달 및 가능한 사용자 실기 확인의 내부 목표를 둔다. 날짜는 검증 생략이나 미완료 항목 제외를 허용하지 않는다. 기기는 현재 사용자 소유 검증 경계이며 없는 실기 결과를 만들어 기록하지 않는다.

인계 시 baseline3580=294교체/3부분/3283대기, 추가 실행142=26교체/112대기/1기존native/2비실행/1제거다. 로깅 원본2 삭제 WIP는 실제 삭제 후 CI 수용 전까지 추가 실적0이다.

### 로컬 Actions 러너 활용 조건 — 2026-10-26

사용자는 로컬 Actions 러너의 속도 이점과 디스크80GB/i9 할당4core/RAM4GB를 명시했다. API에서 runner22 `bin-actions`의 online/idle 및 `self-hosted/Linux/X64/dev-md-local`을 확인했다. [40202955990](https://github.com/bin9208/openpilot-rust/actions/runs/40202955990)의 읽기 전용 inventory artifact12603175511은 여유38786977792B와 기존 공식Rust1.94 SDK의321entry를 기록했다. 현재의 영구 가용량 보장으로 해석하지 않는다.

F01/F02의 순수 native 검사부터 작업별 디스크·메모리 admission과 `CARGO_BUILD_JOBS=1`을 사용해 별도로 실행한다. 모델 수치·CPU ISA·전체 PR 요구는 해당 검사에 맞는 플랫폼에서 유지한다. 기존 원격 실행은 취소하거나 러너 변경만을 위해 반복하지 않는다.

### 집중 검사 수용과 다음 호출부 묶음 — 2026-10-26

source `0e521792e8473be142d6c8bbc7aaaa9457e32a56`의
[40207816621](https://github.com/bin9208/openpilot-rust/actions/runs/40207816621)은
등록된 로컬 러너에서 control/model 두 job 모두 성공했다. 모델 artifact12604721803은
1574408B/SHA256 `479bdd4bfd2d25561f8459d94e9fabb882c33d579418774350cd95cc8639b76d`,
38개 unique/safe 멤버와 전체 CRC가 일치한다. native 단위, 실제 원본 schedule73개,
strictClippy의 세 returncode가0이며 source/index·전체 SDK 전후 확인·업로드·소유 회수가 성공했다.
#589의 feature-off private 선언 경계 수정을 검증하며 전체 모델 수치 수용을 뜻하지 않는다.
동일 source의 history job은 native feature를 포함한 ISA11개까지 성공했으나
결과 grep의 옛8개 고정 때문에 실패했다. 다음 묶음에서 실제11개로 연결한다.

#587은 [40205795864](https://github.com/bin9208/openpilot-rust/actions/runs/40205795864)의
동일 SHA profile을 사용하는5개 빌드/검사와2개 strictClippy, 불변 ELF·SDK·회수 증거로 개별 해결했다.
해당 run 전체는 AVX512_FP16 admission에서 실패했고 #590의 ISA 보강을 적용했다.
후속 모델 run40207816680 역시 전체 실패이며 실제 후속 원인을 조사 중이다.
수치·타이머·기대값을 바꾸거나 모델 전체 성공으로 합산하지 않는다.

다음 묶음은 로깅 원본2의 실제 부재와 재등장 거부, retained/schema14/collision8 검증,
유지되는 main logger 호출부, uploader 사후 실패 전 실제 종료 receipt를 연결한다.
Planner native 구조계약21개와 release fixture3개도 별도 local Actions tools role에 연결한다.
Fast의 native 한 worker는 실제 local runner/SDK를 검증하고 Cargo 병렬1을 사용한다.
나머지 worker·cold bootstrap 명령·strict trace parser·시간/용량 기준은 보존한다.

부모 소스 검토에서 Focus의 module 정렬 역투영과 block 밖 pin 변경의 기존 봉인 불일치를
확인해 수정했다. 현재263개 source pin은 모두 실제 바이트와 일치하며 main workflow는
499477B다. 기존44 job 중 CarrotMan SDK 진단, support uploader 회귀, logger 부재정책의
세 step만 변경했다. 로컬은 YAML7/Python1/Bash887+shell2의 문법과 Rust10의 서식,
소스 역투영/diff만 확인했다. 새 기능 결과는 GHA 대기다.

baseline3580의294교체/3부분/3283대기는 그대로이고 추가 실행 원장은144개
(26교체/114대기/1기존native/2비실행/1제거)다. 로깅 실제 삭제 WIP와 사용되지 않는
새 API에 선행 수용 실적을 부여하지 않는다. 필수 PR/통합·전달·사용자 실기 수용은 남아 있다.

### 실제 호출 제어·버퍼 경계와 후속 묶음 — 2026-10-26

source126의 등록 local tools job121217757597에서 Planner 구조21/release3와 strictClippy가 실제 성공했다. 부모가 artifact12605193497의36CRC·source/index/SDK 전후·회수를 확인했다. 같은 source의 Fast 서비스 job121217757599도 release3를 포함해 성공했고 artifact12605009898의35CRC/전후 봉인을 확인해 #588을 개별 해결했다. 모델 #590은 source0e의 실제 ISA11·whole91/strictClippy와2049CRC로 개별 해결했으며 signed-zero/NPY·전체 모델 수용은 분리한다.

새 묶음은 기존 export::BufferTable을 재사용한 compiler buffer metadata/initializer 경계를 정식 library에 연결하고 native6+원본1(32 descriptor/147 recipe)을 검증한다. Planner는 실제12-leaf 명령 계획·첫 실패/로그 보존 controller와 지속 stdio scheduler를 등록한다. Python max의 첫 피연산자 유지·signed-zero/NaN을 보존하고 실제 IPC 송신 루프의 native 선택 경로를 연결했다. native11/원본4와 adapter13(실제 Rust160요청, skip 금지)을 집중 Actions에 연결한다. 유지되는 main12-leaf/메모리·ARM caller의 전환과 full IPC는 아직 남아 있다. 기존 Python IPC/원본 함수와 leaf는 명시적으로 남으며 새 API만으로 원본 삭제 실적을 부여하지 않는다.

로깅 삭제 후 실패는 source SHA pin1개와 absent 정책키 추가 후 옛 환경6개 고정을 보정한다. 기존6키/evidence순서와 present6/absent7의 정확한 키 검증을 유지한다. 모델 original replay의 NPY 입력 쓰기도 기존 비교 도구의 byte-view 분기만 복구한다. 기대값·산술·시간·CPU/ISA 조건은 유지한다.

집중 unit workflow는 workflow 자체 변경 때 전체 role를 확인하고, 이후 source 변경은 필요한 control/model/tools/buffers/planner role만 명시 dispatch하도록 분리한다. 성공한 불변 검사 반복을 줄이며 필수 PR/통합 검사는 유지한다. local Cargo 병렬1·현재 메모리/디스크 증가량 admission과 SDK/source 전후 확인은 같다.

Fast local routing40209583336/job121217757300도 strict cold trace의157행 `???( <detached ...>`를 거부했다. artifact12605218513은626538B/SHA256 `434a226361dd588f9e7ba1acad539284617cf44d3b6009e01c2ad51d2f85f79a`,66 unique/safe/CRC를 확인했다. 실제 local kernel7.0.2-6-pve/strace6.8이며 플랫폼 변경만으로 #560이 해결되지 않았다. 별도 patched observer의 성공을 maintained Fast 성공으로 대체하지 않고 producer 연결을 검토한다. 전체 Fast는 실패, source126 Focus40209583375와 user-docs40209691291은 성공, 필수 Rust/통합은 별도 상태다.

추가 실행 원장은148개(26교체/118대기/1기존native/2비실행/1제거)다. 새 compiler/Planner 원본 비교, Python adapter 회귀 및 native controller의 기존12 Python leaf 호출을 모두 pending으로 등록했다. baseline3580=294/3/3283과 전체 전환·전달·사용자 실기 미완료는 유지한다. 로컬 기능/모델/원본 실행은0회다.

### #530의 확인된 초기 단계 순서 수정 후보

source126의 필수 Rust40209691630/job121222236242는 실제 legacy Rustup shim 복원 뒤 fresh SDK의 line7에서 중단됐다. 기존 비어 있어야 할 prefix를 채택·수리하지 않고, CarrotMan 한 job의 동일 shim 발견 항목만 fresh 준비 뒤로 옮긴다. 원래 preparation/후검사 body·실행물 checksum·자동설치 금지·기능/ARM·예산/기한은 같다. policy는 선행 legacy 발견을 거부하고 기존 SDK escape12에 재삽입1을 더한다. 이 변경은 실제 기능 GHA 대기이며 #530은 열린 상태다.

sourceDD의 신규 buffers/planner 단위는 이미 각 독립 role에 배정했다. 다음 workflow 변경의 자동 push는 필요한 tools role만 선택하며 다른 불변 성공을 일괄 반복하지 않는다. 수동 선택의 all/control/model/buffers/planner와 기존 필수 PR/Fast/통합은 유지한다. 이 추가 tools에는 Carrot SDK selector1 및 library/해당 target의 엄격Clippy를 연결한다.

### DD 원본 실행 연결과 #591 수용

sourceDD native40212410286의 Planner는 일반11/adapter13(실제 Rust160호출)/Clippy가 성공했으나, 원본4개는 부모 workflow가 `PLANNER_REFERENCE_PYTHON`을 넘기지 않아 실패했다. 선택한 실제 `/usr/bin/python3`의 canonical 실행물/version/hash를 기록·전후 확인하고 명시 env를 전달한다. buffers도 일반6/Clippy는 성공했지만 isolated Python 경로에 checkout root가 없어 실제 openpilot dependency import에서 실패했다. 원본 코드·기대값을 바꾸거나 stub을 넣지 않고 root 한 줄을 추가한다. 두 수정 후 원본 검증은 GHA 대기다.

모델 DD40212410219는 original/native×plain/tapped 네 mode와 전체6출력 bytes가 일치했고 source/index·SDK6쌍·회수가 성공했다. artifact12606905172의2098CRC/실제 SHA를 부모가 감사해 #591만 해결했다. D1의 frozen -0/current+0는 그대로 sticky FAIL이다. 현재 original과 native도+0로 같다는 관측만으로 기대값을 교체하지 않는다. 필요한 다음 관측은 기존 writer의 실제 half 입력2B이며 산술은 변경하지 않는다.

sourceb961의 Carrot 분리 검사도 prefix 거부가 남아 원래 순서 관측만으로 전체 원인을 확정하지 않는다. 고정 prefix의 내용 없이 존재/소유/시각 metadata만 추가 관측한다. 신규 Carrot CI 진입점도 pending으로 등록해 추가 실행149(26교체/119대기/1기존native/2비실행/1제거), baseline3580=294/3/3283을 유지한다.

### 로컬 Actions 자원과 postdelete 실제 결과 — 2026-10-26

사용자가 지정한 `bin-actions`의 배정 자원은 i9 4코어/4 GiB, 저장공간80 GB다. 작은 native 단위 검사는 jobs1/incremental0/debug0와 실제 cgroup 여유2 GiB·파일시스템별 합산 잔여 성장 admission을 유지한다. 모델 ISA/큰 원본 비교·필수 ARM은 각 실제 환경이 맞는 hosted 검증과 분리한다. 실행 중 검사는 취소하지 않으며 성공한 불변 cohort를 수동으로 반복하지 않는다.

sourceDD의 retained postdelete [40212410474/job121226283640](https://github.com/bin9208/openpilot-rust/actions/runs/40212410474/job/121226283640)는 전체 성공했다. artifact12607036666은136138137B/SHA256 `fcdd3b591e544cf57ccfd0038ec7ef6c7a1f7da6126ae92175f82d5c6aafc681`, 9696개 safe unique/non-symlink/CRC 정상이며 expanded1973631690B다. 부모가 sourceDD, source/index와 전체 SDK file/roster/program 전후 동일성, candidate exit/held/untraced/paired0, 원본 부재 CLI20 및 policy14, legacy INT/TERM2의 독립 실제 성공을 확인했다. 각 receipt의 old27/R4/full acceptance/retirement credit0은 그대로다. collision8 [40212410241/job121226282810](https://github.com/bin9208/openpilot-rust/actions/runs/40212410241/job/121226282810)는 다른 실패로 남아 별도로 수정한다.

필수 PR source126의 [Rust40209691630](https://github.com/bin9208/openpilot-rust/actions/runs/40209691630)는 support runtime와8개 표준 ARM VM family가 실제 성공했다. original route logger job121222236443은 Azure HTTP APT가 `libclang-18-dev`28.8MB를 받던 중 원래20분 job 상한에 도달해 취소됐다. 새 native CLI caller19·후속 logger 기능은 실행하지 않았으며, 지원 런타임 성공이나 retained 별도 성공으로 이 호출부를 통과 처리하지 않는다. workspace/aarch64와 모델 실패도 전체 PR 성공으로 합치지 않는다.

source33923f8의 Carrot 분리 [40216388430](https://github.com/bin9208/openpilot-rust/actions/runs/40216388430)는 실제 private `rustup-home` 디렉터리가 fresh SDK 전 이미 생긴 것을 관측했다(uid1001, mode755, mtime/ctime17:25:44.951294751 UTC). 시각은 setup-python의 pip upgrade 구간 안이다. 생성 코드의 원인은 추가 조사 중이며 시각 일치만으로 Python 또는 Rustup을 확정하지 않는다. strict fresh guard를 유지한다. source561f20d9f의 model tap은 기존 writer arg1의 실제 half99 두 바이트만 읽고 whole6-output/plain↔tap·D1 sticky·수학을 보존하며 [40216927059](https://github.com/bin9208/openpilot-rust/actions/runs/40216927059)에서 새 수용을 기다린다. 인벤토리3580=294/3/3283과 추가 실행149개 및 전체 #523 미완료 상태는 유지한다.

### #530 Python probe 격리와 #592 logger 준비 후보

보존한 실제 Python3.12.14 archive의 pip25.0.1은 User-Agent 수집 중 PATH의 `rustc --version`을 호출하고, Rustup1.29.1 proxy 초기화는 toolchain 선택 전 RUSTUP_HOME을 만든다. 부모는 원문과 실제 시각을 대조하고 CarrotMan의 setup-python/venv에 별도 job-private 두 홈과 자동 설치 금지0을 적용했다. syscall 직접 귀속 증거와 코드 경로 근거는 [조사 기록](python-rust-523-sdk-530-python-rustc-probe.md)에서 구분한다. strict fresh·whole SDK after는 유지하고 부재 검사를 우회하지 않는다. source 기준 body pin과 원본 ARM 회귀를 갱신하고 SDK escape17개를 새 source에서 확인한다.

별도 [#592](https://github.com/bin9208/openpilot-rust/issues/592)는 original route logger의 APT 준비 중20분 취소를 추적한다. 명시 패키지는 그대로 두고 권장 패키지 설치만 끄며, 이 deep job의 외부 상한을45분으로 둔다. 내부 기능·Miri·ASAN·회수 상한과 다른 Fast7분/로컬 자원은 바꾸지 않는다. 새 `python-rust-523-logger-caller.yml`은 현재 main logger job/global env 전체에 main 전체 SHA read-only pin 하나만 더한 별도 진입점이다. 모든 기능/봉인/업로드/회수 객체의 동일성을 소스에서 확인했고 실제 검증은 GHA 대기다. 새 Python 호출 진입점도 pending으로 등록해 실행 원장150개(26교체/120대기/1기존native/2비실행/1제거), baseline3580=294/3/3283을 유지한다.

collision DD artifact12606243075는 실제8개 충돌 case 자체와 그 trace 검사까지 성공한 뒤, 앞선 checker build의 새 thread raw `???( <unfinished ...>`를 strict lexer가 거부했다. unknown을 허용하지 않고 build capture에 기존 retained와 같은 seccomp-bpf 요청을 적용하며 최초 명령 실패와 finalizer별 세부 status를 보존한다. 이것이 producer를 고친다는 결론은 새 실제 GHA 전까지 내리지 않는다. 기존 cap·타이머·runtime trace와 성공한 retained cohort는 그대로다.

Carrot sourcef1의 분리 실행40217812565는 Python probe 격리 뒤 strict fresh와 공식 SDK 설치/before, SDK/source/index after에 도달했다. 다음 실패는 caller line24의 private proxy regular/nonsymlink 조건이다. 공식 Rustup1.29.1은 기본 symlink 우선이므로 준비 호출에 공식 `RUSTUP_HARDLINK_PROXIES=1`만 추가하고 기존 strict proxy 소유/원본 byte 조건을 유지한다. 설치 후 수리·symlink 허용·부분 채택은 하지 않는다. main은499723B이고 escape18개와 새 source GHA에서 확인한다.

source561 모델 [40216927059](https://github.com/bin9208/openpilot-rust/actions/runs/40216927059)의 actual half99 관측은 원본/native 모두 little-endian `0080`(half -0)이었다. kernel92/call175/view365/alloc16/offset9472/arg1/worker0 및 source/object/image 대응을 확인했고 whole6/plain↔tap 출력도 동일했다. output99는 두 현재 경로에서+0이고 frozen만-0이므로 D1 strict 실패를 유지한다. artifact12608182890은27434534B/SHA256 `b7a9337a893a1addaa97804d3bc25ca8da7dc23d6f066478a053f2dffb425780`,2103safe unique/non-symlink/CRC 정상, final-seals failures=[]/pass/owned reaped=true다. 현재 입력의 최초 upstream 산술 producer나 역사 provider를 관측했다는 주장과 분리하며 expected/수학은 그대로다. Planner source33923의 native11/원본4/adapter13(real native160)/strictClippy 검증40216487452도 실제 성공했고 재실행하지 않는다.

### #549 원본2 수용과 다음 actual caller 묶음 — 2026-10-26

predelete5bee의 동일 checker present/absence·재등장 음성, DD full absent, f1collision8과 [main exact logger40217812664](https://github.com/bin9208/openpilot-rust/actions/runs/40217812664) 전체 성공 및 부모의 두 artifact 실물 감사를 결합해 #549의 Python 원본2만 교체 인정했다. inventory/families는 **3580=296교체/3부분/3281대기**다. old27의 역사17/대기10과 R4/전체 manager/기기 미수용을 그대로 둔다. sourcef1의 deep logger 준비/기능/Miri/ASAN/after/upload/cleanup 성공으로 #592의 완료 조건도 충족한다.

Carrot source836의 [40218262329/job121244017130](https://github.com/bin9208/openpilot-rust/actions/runs/40218262329/job/121244017130)는 strict fresh/private hardlink proxy·전체 SDK before/after, original policy/실제 owned Params·IPC·socket/upload 및 cleanup까지 성공했다. private artifact12608431465=322456B/SHA `7ed75344375fc8b1b2c1dded587afe40f08c3218fee50129f4d4b4e9ab9b05b7`,36CRC,source/index/SDK5쌍 동일; 기능 artifact12608985622=10028070B/SHA `5366cf6dc03e2822d567cb430f3a4a6641bf343e342cfc3e17623b62c6b18dbe`,147CRC다. 정책32/ingress19/publication24/time6와 실제 owned12가 성공했다. 기존 #530의 Fast actual cold/routing 전체 조건은 다음 수정본 수용까지 열린 상태다.

source836 Focus는278개 source pin 뒤 workflow outside-block 기준 hash 하나가 오래돼 중단됐다. 부모의 등록 누락을 확인했고 실제 허용 변경을 다시 대조한 뒤 검토한 바깥 원문의 정확 hash로 갱신한다. 검사를 제거·완화하거나 이를 runtime 실패로 분류하지 않는다.

다음 묶음은 Planner host/ARM12·sanitizer3의 실제 호출을 native controller로 연결하고, Fast private patched tracer/STOP 승인/strict native auditor와 bootstrap jobs1을 연결한다. Fast7분·6GiB·provider120초/512MiB·원래18통제/marker10/protocol3 및 실패 전파는 유지한다. 실제 fit와 새 controller 기능은 GHA 대기다. 신규 memory wrapper unittest5와 Bash의 기존 Python memory 호출도 pending으로 등록하며 원본 outer controller는 독립 원본5/관측 corpus 수집용으로 보존한다. 원본 삭제나 compiler/Planner 전체 퇴역 credit을 앞당기지 않는다.

이번 actual caller 묶음의 추가 실행 원장은153개(26교체/123대기/1기존native/2비실행/1제거)다. 새 host Planner 진입점, memory wrapper5, 이동한 Python memory dispatch도 대기로 추적한다. standalone ARM Planner도 같은 helper로 연결했다. source5의 실제 원본 JSON/bit/raw-log는 기존 Python process 수를 늘리지 않고 별도 bounded corpus로 보존해 향후 원본 퇴역 근거로 쓴다. 현재 corpus reader/남은 import 제거·원본 outer 삭제는 미완료다. Fast 새 경로는 별도 Sol 동료의 읽기 전용 검토에서도 구체 계약 불일치를 찾지 못했으나 실제 기능/fit 수용은 여전히 GHA 대기다.


### 73e17 실제 native 검증과 Fast 시간 분리 — 2026-10-26

source `73e17ff10ec9be7a7001ab9cbf415a779c436f78`의 [Focus40220990604](https://github.com/bin9208/openpilot-rust/actions/runs/40220990604),
[로컬 native units40220990602](https://github.com/bin9208/openpilot-rust/actions/runs/40220990602),
[실제 host Planner40220990726](https://github.com/bin9208/openpilot-rust/actions/runs/40220990726),
별도 tracer40220990622, Carrot40220990621, logger40220990610, ARM WebRTC40220990592,
ARM radar/navi40220990627, setup40220990615가 성공했다. unit은 도구/버퍼/Planner 세 작업이다.
성공 상태와 artifact 본문 감사 범위는 구분하며 본문 수용은 각 기록에 추가한다.

ARM Planner만 helper의 초기 SDK 경로 조건에서 중단됐다. 기존 standalone ARM은
`$CI_JOB_TEMP/rustup-home`인데 새 helper가 prepared arm530 경로만 기대했다. solver/비교가
시작되지 않은 준비 실패다. 기존 SDK 출처 검증을 보존하면서 두 실제 caller 경로를
명시적으로 구분해 수정하고 다시 검증한다. 원본 삭제/전체 Planner credit은 아직0이다.

Fast의 실제 cold+strict native auditor는 성공했으나 후속 routing 빌드가7분에 취소됐다.
[실물 감사와 분리 변경](python-rust-523-fast-native-cold-560.md)에 기록했다. 동일 audit 모듈의
작은 crate와 별도 필수 hosted routing 작업으로 compile 비용을 나누며 local jobs1/RAM4GB,
전체 cold6GiB/provider120초/512MiB, Fast7분과 모든 원래 검사를 유지한다. #530/#560은 열린다.
인벤토리3580=296/3/3281과 전체 #523 미완료 상태는 유지한다.


host Planner의 private artifact12609399553은295,885B/SHA
`5961fee67e933e0b84923cc1f5f9ab0079e06f8301245b67912983d6b5544f4e`,29CRC/source/index동일,
controller runtime/memory exit0이다. functional artifact12609249847은55,184,901B/SHA
`e1d196ac2bb999867fd45fdff50702c6fa60f0917b7d1d251576c64a7e9df030`,
34,282 safe unique/non-symlink/CRC다. source73e17·x86_64의 actual12행 모두 exit0,
원본/native policy/main/real IPC/lifecycle PASS와 sanitizer subset abi/owner/fault3행 exit0을
본문에서 확인했다. sanitizer의 계측 범위는 generated solver C이고 Rust/external solver
전체가 계측됐다는 뜻은 아니다. ARM·기기 수용과는 구분한다.


6d0bd2의 새 hosted routing은 changed-files 양성 policy가 여전히 옛 `native` job을 찾는
잔여 연결 때문에5개 중1개 실패했다. 양성/음성 대상 두 곳을 실제 `native-routing`으로
맞춘다. Focus의 기존 raw Cargo 행 보존도 유지하도록 새 YAML 들여쓰기만 복구한다.
검사를 삭제하거나 성공으로 우회하지 않으며 실제 local cold·ARM·다른 진행 중 job은
그대로 완료를 기다린다.


### 3b7492 Fast 전체 수용과 다음 Python caller 전환

[Fast40224700444](https://github.com/bin9208/openpilot-rust/actions/runs/40224700444)의8개 worker와
aggregate가 전부 성공했다. local cold는308초/7분, thin compile12.44초/actual audit47.89초다.
source/SDK/provider after·upload·owned cleanup과 actual exec cargo1/rustc37/Python0/Rustup0을
실물에서 확인했다. [근거와4개 artifact SHA](python-rust-523-fast-native-cold-560.md)를 기록했다.
#530의 결함 범위와 #560의 원래 root/PR 조건, PR343 전체 통합은 구분한다.
3f57cccff Focus40224625786도 raw Cargo 행 서식 복구 뒤 성공했고 기존 조건은 유지했다.

ARM standalone [40224180784/job121261950074](https://github.com/bin9208/openpilot-rust/actions/runs/40224180784/job/121261950074)는
helper40e07의 실제 aarch64 twelve-check를 통과했다. private artifact12611155653은294044B/SHA
`f26e3c4060edcfacfa3e985c6368414757fd8b6b2c65ff0779946ea445bd7268`,22CRC/source-index 동일;
functional12610522791은44840908B/SHA
`34ba7193fbe3ea4da36c766c292dded8da5797d4712bfc18b5caf9050204d834`,29679safe/CRC로
source6d0bd2/aarch64/runtime12 PASS를 확인했다. 이 standalone은 기존 job rustup home이며
canonical reusable ARM의 whole arm530 SDK를 이 artifact가 검증했다고 하지 않는다.
host 재검증40224251309도 성공했다. 새 native memory 호출의 실제 ASan3은 다음 source 수용이다.

다음 변경은 실제 Bash memory 호출을 기존 Rust CLI로 옮기고 독립 원본5 경계 관측을 남긴다.
source73의 원본 관측 JSON5는 byte 불변 데이터로 보존하여 새 native5 비교에 사용한다.
compiler의 실제 offline emit은 기존 native CLI에서 buffer/launch/stage 결과를 소비한다.
float 정밀도 손실에 대한 peer finding은 JSON float_roundtrip과 실제 CLI 회귀로 보완한다.
driver는 원본 PKL model44/warp1과 고정 object witness를 독립 기대값으로 쓰고 실제 ABI/ISA,
raw/prestate를 거친다. fresh 실패·전체 수치 기준은 그대로다. #474는 같은 빌드의 complete
기능 lane과 별도 strict exec lane을 준비한다. 이 후속들은 실제 GHA 수용 전이고 삭제 credit0,
전체3580=296/3/3281 및 #523 미완료 상태를 유지한다.


이번 후속 batch의 실행 원장은 **161개=26교체/131대기/1기존native/2비실행문자열/1제거**다.
새 native memory 원본 비교와 compiler의 genuine Python 호출 각1, driver 임시 Python
orchestration/PKL metadata/original retained worker3과 CI heredoc3을 모두 pending으로 등록했다.
새 frozen JSON5는 실행하지 않는 독립 과거 관측 데이터다. 기존 original/source5·memory wrapper/test5는
유지하며 새 native8·frozen5·compiler4+1·실제 memory3 및 driver/recovery 수용 전 삭제하지 않는다.
source3b7492의 Focus40224700438도 성공했다. 이후 helper/feature/실제 caller 변경은 새 source에서
검증하며 이전 성공을 새 기능의 수용으로 바꾸지 않는다.


## 2026-10-26 Planner 소스4개 퇴역 후보·buffer 원문 보존

source425의 actual host runtime12/ASan3, native controller14/source5/frozen5/memory8+
원본 memory1, compiler native4/maintained caller1/strict Clippy 성공을 각각의 원문 artifact로
확인했다. source dd3의 Fast40227564050도 전체 성공이다. 세부 SHA/범위는
[Planner 호출](python-rust-523-planner-actual-callers.md),
[memory 원문](python-rust-523-planner-native-memory.md),
[compiler 실제 caller](python-rust-523-compiler-export-caller.md)에 남겼다.

새 후보는 Planner controller/pytest 및 memory wrapper/unittest의 정확4경로를 삭제하고
원래 raw 관측5+5를 frozen 자료로 사용한다. main/ARM/진단 copy의 실제 pytest 소비자도
maintained native policy/controller/frozen/absence 검사로 닫았다. 삭제 전 성공을 삭제 후
수용으로 세지 않으며 현재3580 중296교체/3부분/3281대기와 추가161개 원장은 유지한다.
4개 중baseline은3개, 나머지1개는 추가 실행 경계다. 실제 삭제 후 host/ARM·native15/guard6은 대기다.

buffer의 기존 original179경계 비교는 같은 원본 프로세스1회의 request/raw stdout/stderr/
종료/source/Python 전후 binding을 bounded 자료로 남기도록 연결했다. native6+original1과
179경계 수는 유지한다. 새 자료가 수용되기 전 원본 모듈은 삭제하지 않는다.

복구는 같은 실행물의 두 full harness가 성공했지만 strace unknown 행1개 때문에 strict audit가
실패해 #474를 유지한다. driver source425는 fresh4case가 성공하여 failure-only retained
진단이 미도달했다. 실제 exit0과 미도달 baseline을 구분하도록 기록을 보완하며 fresh 미재현을
억지 실패나 cached 수용으로 바꾸지 않는다. #476/#482/전체 PR·대단원 완료 조건은 계속 남는다.


## 2026-10-26 Planner 세 baseline·추가 실행 경계의 한정 수용

삭제 후 source7a82의 actual host12/ASan3/ARM12, controller14/policy21/frozen5/runtime guard6와
source100 native memory15/strict Clippy 및 source/index/SDK 원문 봉인을 수용했다.
인벤토리는 **299교체/3부분/3278대기 =3580**, routing은25교체/3부분/3278대기다.
삭제4개 중baseline3개와 추가memory unittest1개를 구분했다. 추가 실행 원장162행은
30교체/128대기/1기존native/2비실행/1제거이며 old source5·memory 원본5 실행과 직접wrapper
Python dispatch의 퇴역도 해당 범위로 기록했다. 상세는
[Planner 삭제 후 수용](python-rust-523-planner-retirement-acceptance.md)에 있다.

Buffer179 원문 수집은 source100의 native6/original1/Clippy 실제 성공을 확인했고 artifact
독립 감사를 진행한다. 이전 source7a82의 소유 표식 줄바꿈 가정은 수정했다.
새 [#593](https://github.com/bin9208/openpilot-rust/issues/593)은 source425에서 미등록 VBMI를
정확히 거부한 ISA 경계를 추적한다. sourceee74에 mapping/OS-aware 탐지·unit14와 실제 호출을
연결했다. 원래 historical old image는 자연 host의 avx512bw 부재로 계속 거부되며, ISA14의
성공이나 새로운 mapping으로 모델 수치/전체 #482를 대신하지 않는다.

원래 fullsix 복사본을 추가하면서 actual private source roster78/ARM24와 맞지 않았던
두 policy count를 sourceee74에서 수정했다. fullsix는 같은 원래 selector·기한·원문·수치/
부재/복원·봉인 조건으로 다시 검증한다. #474 provider 준비와 실제 IPC/컴파일러 전환을
계속하며 대단원 전체·필수 PR·전달·실기 완료 조건은 남아 있다.

## 2026-10-26 buffer native frozen 및 ISA #593의 실제 범위

source7a928의 원문 buffer179 frozen 비교는 일반7/ignored0·feature union1·strict Clippy2와
source/index/SDK 전후 봉인이 통과했다. 추가 실행경계1개만31교체/127대기로 갱신했으며 기준3580은
299교체/3부분/3278대기다. production offline emitter의 Python 계산은 아직 다음 전환 중이다.
sourceee74의 ISA14/Clippy/whole91은 자연 Intel(VBMI=true)·AMD(false)에서 모두 성공했다.
[#593 원문 증거](python-rust-523-model-482-425-vbmi-audit.md)는 current의 세 signed-zero 및
PR1344 frozen99 실패를 명시적으로 남긴다. ISA 경계 완료는 #482 수치/전체 모델 완료가 아니다.

sourceaa79는 기존 host/ARM runtime12 안에서만 IPC raw401x4/initial Params를 수집하고 같은
adapter14 interpreter에서 parser35를 수집한다. 원래 수학·ModelScheduler·기존 adapter13 class
bytes와 실제 native160 비skip 기준을 보존했다. 새 native IPC candidate는 아직 미등록이며,
raw 수집 성공·reader 연결·actual native 비교와 Python 삭제는 각각 남아 있다.


## 2026-10-26 stage73의 실제 native 수용

source `a62109fa706012b91b24206e7d8c875653746aec`의
[run40241338985/job121313583472](https://github.com/bin9208/openpilot-rust/actions/runs/40241338985/job/121313583472)은
launch9·stage8/0ignored, preserve_order union1, 원본launch4411 corpus1과 strict Clippy를 모두
성공했다. 새 Python subprocess/현재 원본 AST 조회가 없는 고정 reader다. 원본73 raw 입력·
기대값·SHA와 과거13 witness를 유지했으며 native 결과로 expected를 만들지 않았다.
artifact12616208518은1,583,513B/SHA256
`ece5ed6e6be36855b4180fe3400b97bbf9aed2d9bb55eb0a72a5b42c0018acc4`,
40 safe unique regular member/전체 CRC, expanded6,929,918B다. command4개 exit0,
source/index/index-stage와 whole SDK file/program/roster 전후 동일, SDK install0,
업로드 뒤 source/owned temp cleanup까지 실제 성공했다.

추가 실행 원장은 **162개 =32교체/126대기/1기존native/2비실행/1제거**다.
기준3580은 **299교체/3부분/3278대기**, production offline emitter와 전체 F02 수용은 그대로
미완료다. 이 좁은 비교 실행경계의 제거만 인정하며 source-file baseline credit은0이다.


## 2026-10-26 원본 launch unittest31 삭제 후 실제 수용

source `581c84520fc9d3c014ad155bf0eff13f96289db4`의
[run40242309674/job121316568836](https://github.com/bin9208/openpilot-rust/actions/runs/40242309674/job/121316568836)은
삭제된 `rust/tools/tests/test_offline_qcom_launch.py`의 working/index 부재를 검사 전후 모두
확인했다. 원래31개 parameterized case에 대응하는 native launch9, 독립 원본4411 corpus1,
stage8/ignored0·preserve_order union1·strict Clippy와 source/index-stage/wholeSDK 봉인·업로드/
소유 source cleanup이 모두 성공했다. 기존 Python file을 forwarding/skip으로 남기지 않았다.

artifact12618110662는1,586,311B/SHA256
`b05efdb316aaba463e68f47087c455e80268aefe81c26b8b27aac4af868bdf45`,
46 safe unique regular member/전체 CRC, expanded6,935,713B다. 명령4개 exit0,
source/index/index-stage·SDK file/program/roster6쌍 전후 byte 동일, SDK install0이다.

기준3580은 **300교체/3부분/3277대기**, routing3306은 **26교체/3부분/3277대기**다.
새 genuine compiler 비교 실행경계1개를 pending으로 추가했으므로 추가 원장163은
**32교체/127대기/1기존native/2비실행/1제거**다. production ordered emitter의 실제 genuine
검증은 별도 진행 중이며 이 테스트 파일1개 수용으로 전체 compiler/모델/Python 제거를
완료 처리하지 않는다.


## 2026-10-26 Planner native IPC 등록과 fresh/cached 모델 구분

source `47fb84019a078f3d1c1de9c432cfe475f5b92be0`에 native IPC reader/CLI/feature/C++ peer와
독립 aa79 HOST·ARM capture 감사 JSON을 등록했다. 기존 HOST12/ASan3/ARM12 뒤에 lib4개·
strict Clippy·원본 공급·새 native source/rust×epoch0/1 비교를 추가했다.
[HOST40246606213](https://github.com/bin9208/openpilot-rust/actions/runs/40246606213),
[ARM40246606219](https://github.com/bin9208/openpilot-rust/actions/runs/40246606219)의 실제 새
feature 수용은 아직 대기다. 원래 Python scheduler/collector/daemon/oracle은 유지한다.
Rust driver의 source 비교용 Python daemon 실행도 새 추가경계1개로 pending에 등록했다.
기준3580 **300교체/3부분/3277대기**, 추가164 **32교체/128대기/1기존native/2비실행/1제거**다.

[ef172 current 모델40245597205/job121326369244](https://github.com/bin9208/openpilot-rust/actions/runs/40245597205/job/121326369244)은
자연 Intel8573C에서 native15/ISA14/strict Clippy/A whole91과 명시 cached 경로C12가 성공했다.
기존 원래 출력12개 총5138816B와 C가 byte exact며 두 C 전용 helper 사본의0700/nlink1,
원본 불변·최종 봉인·자식 회수를 확인했다. 전체 job은 B fresh 경로의 세 signed-zero 차이로
실패했다. [PR1344 같은 source40245597206](https://github.com/bin9208/openpilot-rust/actions/runs/40245597206/job/121326369441)도
D1 sticky 외에 D2 model index99의 실제−0/+0 불일치가 있으므로 과거 sticky만으로 분류하지 않는다.
새 expected·tolerance·CPU/ISA 강제·default cached 활성화 없이 원인을 좁힌다.
전체 #482/모델·PR343·전달 및 필요한 기기 수용은 미완료다.

source398의 [복구40242668854/job121317637641](https://github.com/bin9208/openpilot-rust/actions/runs/40242668854/job/121317637641)에서
carrot-recovery strict Clippy와 untraced exact readiness1/동일ELF·heldFD·자식 정리가 실제 성공했다.
source/SDK 후봉인과 소유 source cleanup도 확인했다. 이 증거로 #474 전체 gate나 #560 broad
cold trace까지 완료 처리하지 않는다. 상세는 각 기능 기록에 남겼다.


sourceef172의 genuine compiler 검증은 [attempt2/job121329475106](https://github.com/bin9208/openpilot-rust/actions/runs/40245597111/job/121329475106)에서
전체 SUCCESS로 완료됐다. native10/COPY1/genuine mixed1/order7/strict Clippy가 모두 exit0이며,
original-prefix 원문·실행물·source/index·전체 host+ARM SDK 후봉인과 업로드/소유 정리를 감사했다.
이는 native ordered allocation/launch/stage 경계의 수용이고 production Python 파일 제거/F02 전체
완료는 아니다. #596은 자연 host detector/production 어휘와 whole91/C12의 한정 조건을 충족해
종료했지만 #482/#476·전체 #523/PR·전달·필요한 기기 수용은 남아 있다.


원래 driver provenance를 유지하는90일 보존 carrier 인증과 별도 data-only 진입점도 등록했다.
helper의 기존11개 function/class body와 원래 RETAINED_OBJECTS/bridge bytes는 유지한다.
새 운송 인증의 실제 성공·#476 수치 수용은 대기다. 이 Python 진입점2개를 숨기지 않고 추가 원장에
pending으로 더해 현재 **166=32교체/130대기/1기존native/2비실행/1제거**다.


## 2026-10-26 IPC 실제 감사와 다음 고립 비교

source47fb의 HOST/ARM에서 native4/strict Clippy/stager/four-run과 기존12·HOST ASan3가
실제 성공했고, 각121개 출력이 기존 aa79 source-0 baseline과 bit 동일함을 감사했다.
새160행과 source/SDK/same-ELF/소유 그룹 정리도 확인했다. 새 original-output reader는
과거4개 case receipt와 raw484도 native에서 직접 읽고 각 현재 run과 비교하도록 등록했다.
새 stager의 추가 추출·새 reader의 실제 수용은 다음 Actions 대상이다.

sourceec43598의 [driver transport-only40248425907/job121334855271](https://github.com/bin9208/openpilot-rust/actions/runs/40248425907/job/121334855271)은
실제 PASS다. payload/proof API·원본 불변·258CRC·held17·source/index19616·명령6/child6·
업로드/cleanup을 확인했다. 별도 모델·Cargo·SDK 실행 및 carrier 재업로드는0이다.

다음 writer D는 같은 원본 compiler context의 두 fresh 입력만 바꾸며 원본90image·기존12출력·
B sticky·default cached 거부를 유지한다. 원본 모델0과 원본 Python compiler2회를 명시적으로
구분한다. #513도 pre-fix/current Python 각1회 fake HTTP 관측으로 남은 한 경계를 확인한다.
이 임시 Python 경계3개를 추가해 현재 **169=32교체/133대기/1기존native/2비실행/1제거**다.
기준300/3/3277·대단원 전체·필수 PR·전달/필요한 기기 수용은 아직 미완료다.
