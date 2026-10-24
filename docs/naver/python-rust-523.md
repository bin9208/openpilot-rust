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
