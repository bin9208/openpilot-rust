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
