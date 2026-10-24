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
3. 로컬에서는 문법·서식·diff만 확인한다. 기능·원본 비교·빌드·Clippy와
   필요한 ARM QEMU 검사는 등록 `bin-actions`에서 실행한다.
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
