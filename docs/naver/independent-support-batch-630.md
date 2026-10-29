# 독립 지원 서비스 병행 실행 묶음 — 2026-10-29 KST

실행 대단원: [#630](https://github.com/bin9208/openpilot-rust/issues/630).
전체 현황: [#522](https://github.com/bin9208/openpilot-rust/issues/522).

## 사용자 요청과 결정

진행 중인 [#598](https://github.com/bin9208/openpilot-rust/issues/598)의 종료를
기다리지 않고 진행할 수 있는 기존 이슈를 하나의 실행 대단원으로 모은다.
기존 대단원은 #523~#528의6개와 #598을 합쳐7개였으며, 대단원8 #630을 추가했다.
기존 분류·이력·하위 이슈 번호와 완료 조건은 보존한다.

아래9개를 선정했다. 대부분 기존 수정 구현과 개별 검증 근거가 있으므로
새로 재작성하지 않고 현재 기준의 필요한 변경·검증·정상 dev 통합을 처리한다.
과거 집중 성공은 최신 전체 후보 성공으로 대신하지 않는다.

| 이슈 | 내용 | 주 대상 |
|---|---|---|
| #392 | 음성 클립 경계와 유한 반복 초과 | soundd 정책·playback 회귀 |
| #394 | 이전 페어링 응답이 새 요청 화면을 지우는 경쟁 | Bluetooth BlueZ agent/state |
| #395 | 테더링 취소 후 소유 자손 프로세스 정리 | Wi-Fi tether |
| #398 | 모뎀 helper의 EOF·취소·자손 수명 | modem helper/PPP/runtime |
| #363 | tombstoned의 group 정리와 leader reap 순서 | apport |
| #351 | multipart 업로드의 중복 전체 파일 버퍼 제거 | web-upload multipart |
| #353 | snapshot xattr 버퍼 재사용 | bootlog snapshot |
| #397 | Wi-Fi snapshot의 불필요한 깊은 복사 제거 | Wi-Fi state |
| #439 | Cookie 헤더의 문자열 중간 할당 제거 | LPA HTTP cookies |

#395/#397은 같은 Wi-Fi 실행 단위로 소유권을 맞춘다. 상태는 #630의
체크리스트 한 곳에서 관리한다. 원래 native 분류 부모는 #524/#526으로 유지한다.

## 분리 확인과 한계

2026-10-29 20:34 KST에 다음 정확 head와 전체 PR 파일 목록을 조회했다.

| PR | 대상 | head |
|---|---|---|
| #611 | dev | `a5e2d10f7ea882f7edf4453c481c8dc5b5f9ffb5` |
| #606 | dev | `ec418df39d3b5820f2751978e3898f0a4d858cbc` |
| #603 | dev | `b42e5eaeec5a13bf83f24ccd31ab5e3364eb2d47` |
| #494 | dev-md | `868ea1cfff9fb46606a5e814a776b0ea93bff820` |

이4개 PR의 파일 합집합444개에 선정 런타임8개 crate 경로의 직접 교집합은0개다.
별도 결함 PR #500의 전체292개 파일에도 해당 crate 변경이 없다.
현재 원격 dev는 `e445c9d19c3041bc79d8624589e2eb6ba7d061c6`, 기존 PR343 후보는
`126fdbea36598566d1040be0a35a365c909be4e4`다.

`Cargo.lock`의 각 package 의존성 목록을 dev와 기존 후보에서 비교했다.
#393은 soundd의 회귀용 rustix/tempfile 추가, #450은 현재 dev에 없는
stats_alloc 추가가 필요하여 이번 묶음에서 제외했다. #392는 #393과 같은
기존 커밋에 있으므로 정책/playback 변경만 분리하고 manifest/lock을 섞지 않는다.
#398은 기존 rustix의 feature 설정을 유지하되 새 패키지 의존성으로 확대하지 않는다.

확인 범위는 현재 변경 경로와 의존성이다. 기능/빌드 성공이나 실기 효과의
증거가 아니며, 공유 러너·공통 helper·최종 dev 통합의 조율은 여전히 필요하다.
각 실행 단위에서 최신 head와 fixture/검증 경로를 다시 확인한다.
공용 lock/workspace/CI/provider/bootstrap/설치 경로 변경이 필요해지면
그 항목은 독립 병행 범위에서 보류하고 필요한 검사를 생략하지 않는다.

## 실행·종료 기준

- 최신 dev의 별도 worktree/이슈 브랜치에서 필요한 기존 변경만 재사용한다.
  #343 전체 전환과 중단된 다른 작업을 포괄 재개하지 않는다.
- 로컬은 읽기·문법·서식·diff 확인만, 기능/원본 비교·빌드·strict Clippy·ARM은
  GitHub Actions에서 수행한다. 다른 세션의 실행과 cache를 보존한다.
- 정확 소스의 필수 PR 검사·정상 dev 병합·별도 병합 후 검사와 관련 문서를 확인한다.
  필요하면 PR은 영향 범위로 나누되 사용자용 현황은 #630에 모은다.
- 개발·검증·통합9개를 마친 뒤 개발 실행 대단원 #630을 닫는다.
  원래 실기 조건은 원래 이슈 및 #528에 남기고 실제 충족 전에는 해당 이슈를 닫지 않는다.
- #598의 기존 완료 조건과 후보에 새 작업을 추가하지 않는다.
  새 독립 결함은 별도 이슈로 기록하고 이9개 묶음을 계속 확대하지 않는다.

## 이번 처리 결과

- #630을 kind:epic/track:dev/area:functions/area:optimization으로 생성했다.
- #522의 native 하위 이슈로 연결하고 #522/#524/#526에 실행 묶음 링크를 추가했다.
  기존 본문·제목·열린 상태와 원래 하위 부모는 유지한다.
- 런타임 소스 변경, 기능 테스트/빌드, Actions 실행·취소, PR 병합·배포·기기 작업은
  이번 이슈 구성에서 수행하지 않았다. 이 문서는 로컬 작업 기록이다.

조회 snapshot·선정 감사·변경 전후 이슈 본문·확인 결과는 private ignored
`.analysis/archive/2026-10-29/independent-support-batch-630/`에 보관한다.
기존 작업 파일은 수정하거나 정리하지 않는다.

## 2026-10-29 개발 착수 후 의존성 보완

실제 패치 분리에서 #398의 `spawn_session_redirected_with_env`와 #395의
`spawn_session_inherited_with_env`가 현재 dev에 없는 것을 확인했다.
선정 당시 crate 경로 대조에서 놓친 공용 API 의존성이다. #611/#606의 실제
`rust/crates/process-supervision` diff는0개다. 해당 `launch.rs`에 기존 구현의
두 새 메서드와 소유 session/stdio 회귀만 추가하는 최소 예외를 #630에 기록했다.
기존 API/StreamMode/호출의 동작을 변경하지 않고 새 의존성·lock 변경도 없다.
공용 CI/provider/bootstrap/설치 경로의 격리는 유지한다.

현재 dev의 hosted 고정 CI를 중복 전환하지 않기 위해 별도 검증 controller
브랜치에서 정확 후보 SHA의 집중 검사를 준비한다. controller는 개발 PR에
병합하지 않으며 집중 결과는 필수5개 context와 정상 병합 후 검사를 대신하지 않는다.

## bin-actions2 재개와 첫 package 검증 — 2026-10-29 KST

사용자가 새 `bin-actions2` 러너를 추가하고 진행 재개를 지시했다. 등록 id23,
Linux/X64/online 상태를 확인하고 #630 전용 `support-630` 라벨로 연결했다.
기존 `bin-actions` 실행과 `dev-md-local` 라벨은 이 작업에서 변경하지 않았다.

개발 후보는 `5723dd8fcdd125be0d4b008c6838e89188d89ae9`,
[dev 대상 draft PR #631](https://github.com/bin9208/openpilot-rust/pull/631)이다.
격리 검증 controller `d13fc5587941d8977636a9e8e899a2fba8f3c96c`의
[host 실행40877475304](https://github.com/bin9208/openpilot-rust/actions/runs/40877475304)에서
선정9package의 실제 build·all-targets tests·strict Clippy 및 production LPA/Wi-Fi
할당 probe가 성공했고, 유지된 원본 비교 단계로 진행했다. 숫자로 된 할당/메모리
효과는 해당 source-bound 산출물을 확인한 뒤 기록하며 이 단계로 실기 효과를 주장하지 않는다.

초기 환경·연결 문제는 기능 결함과 구분해 추적했다.

- #633: 호스트 AppImageLauncher PPA의 Noble404를 작업별 공식 Ubuntu
  source/list/archive 경로로 분리했다. 호스트 APT source와 서명 검증은 유지했다.
- #634: Rust 경로가 없는 새 runner에서 고정 기존 `rust-path` action을 sparse Git
  checkout해 재사용했다. 원격 action 전체 archive의 관계없는 agnos 경로 누락도 피했다.
- #636: 실제 child 바이너리를 빌드했지만 package gate의 Wi-Fi 환경 변수가 빠졌다.
  MODEM과 같은 새 `openpilot-process-child` 경로를 연결해 기존3개 회귀를 그대로 통과했다.

위3개 준비 결함은 각자의 정확 실행 기준을 만족한 근거를 이슈에 기록하고 닫았다.
원본 비교·ARM·필수 PR/별도 병합 후 검사 완료를 뜻하지 않는다.

첫 [ARM 실행40876851867](https://github.com/bin9208/openpilot-rust/actions/runs/40876851867)은
기존 고정 helper 빌드 후 FFmpeg 원본 curl28로 중단됐다. 기존 #628의 동일 source
전송 문제와 연결하고, 같은 구현의 `assets::Curl::fetch_with_mirror`를 인자 adapter로
재사용했다. 원래 URL 우선·zero-body 제한 재시도·동일 archive SHA/10,458,600bytes
검증을 유지하며 별도 downloader를 만들지 않는다. 후속
[ARM 실행40878034961](https://github.com/bin9208/openpilot-rust/actions/runs/40878034961)은
같은 개발 후보의 영향9package GNU ARM 빌드이며 실제 결과 확인 전에는 성공으로 계산하지 않는다.

### 정상 PR 검사와 공통 CI 선행 조건

현재 dev의 필수5개 workflow는 hosted 고정이고, 기존 #606은 `dev-md-local`과
`bin-actions` 이름 검사에 묶여 있다. #606의 기존 full gate를 보존하면서 runner2를
수용하도록 정상 반영하는 것이 최소 선행이며, #598 전체 완료가 선행이라는 뜻은 아니다.
집중 controller 결과를 #631의 필수 context로 복사하거나 범위를 줄여 대신하지 않는다.
사용자가 관련 채팅에 조율 메시지를 직접 전달하겠다고 했으므로 이 작업에서 다른
사용자 채팅에 메시지를 보내지 않았다. #630 개발 체크와 원래 실기 조건은 아직 유지한다.

## 정확 후보의 원본 비교·실측 결과 — 2026-10-29 KST

후보 `5723dd8fcdd125be0d4b008c6838e89188d89ae9`의
[host 실행40877475304](https://github.com/bin9208/openpilot-rust/actions/runs/40877475304)
산출물12900446282(SHA256
`f4ac68bcf6a0e367b9e367fc11dc97eacf1011e4c8039d6bcc9fc64a37bf47e7`)을
다운로드해 단계별 exit/JSON과 source 전후 diff/index를 확인했다. 후보 source/index는
변경되지 않았다. 다음은 개별 성공 근거이며 run 전체 성공을 뜻하지 않는다.

| 범위 | 실제 결과 |
|---|---|
| 선정9package | build·all-targets tests107개(실패/ignore0)·strict Clippy·production 할당 probe 성공 |
| web-upload | 원본101개(파일 오류2개 포함), 실제 timeout14개와 EINTR 전후 비교 성공 |
| bootlog | 기존 snapshot10개와 xattr 소유권/메타데이터 회귀 성공 |
| tombstoned | 기존242개 모두 일치·divergence0, safety/SDK/runtime/shutdown 및 owned-child3개 성공 |
| soundd | 6시나리오×141단계, 샘플6,348개 검증. 원본 보존5,235개/의도한 수정1,113개를 분리하고 다른 상태는 원본과 정확히 일치. daemon7개와 playback8개 성공 |
| Bluetooth | 기존14suite와 prompt 소유권3개 성공. 실제60/90초 deadline과 기존 NUL PIN의 명시적 원본 차이 #166을 구분해 유지 |
| Wi-Fi | 정책781case/2,322frame 정확 일치, private bus2개 및 callback7event 순서 일치. 소유 자식3개 성공 |
| modem | 기존14개 비교, process boundary와 helper15개 성공 |
| LPA | 기존 serial/TLS/profile/notification9개 성공 |

64MiB multipart를 기준·후보 각각 새 프로세스로3회 교차 실행했다. 모든 payload
SHA256과 multipart `Content-Length=67,109,136`은 같았다. 기준 peak RSS는
137,956/137,960/138,156KiB, 후보는76,580/76,840/76,704KiB다.
최소 기준−최대 후보는61,116KiB(약59.68MiB)로32MiB 판정 기준을 넘었다.
이 수치는 호스트의 peak RSS이며 차량/CPU 효과가 아니다.

실제 helper 할당 계측도 같은 bytes를 보존했다.

| 계측 | 할당 호출 기준→후보 | 요청 bytes 기준→후보 |
|---|---|---|
| LPA 쿠키8개×64 | 1,664→384 | 68,480→44,352 |
| LPA 쿠키1,024개×64 | 196,736→832 | 8,781,696→5,766,464 |
| Wi-Fi 네트워크8개×8 | 256→144 | 10,856→5,760 |
| Wi-Fi 네트워크1,024개×8 | 54,672→43,720 | 2,574,920→1,930,304 |

Wi-Fi46개 의미 비교와 source immutability도 성공했고 차이는 실제 버려지는
clone 계측과 정확히 같다. 할당 호출/요청량을 CPU·peak/live RAM 개선으로 해석하지 않는다.
독립 읽기 감사에서도 index12,730개 항목과 입력2,519개 hash가 모두 일치했고
추가 후보 불일치는 발견하지 못했다. Bluetooth의 #166 차이를 전체 원본 동등성으로 표현하지 않는다.

run은 마지막 sound old-source RED에서 compiler 성공 후 실제5실패/3성공을
재현했지만 `--nocapture`의 여러 줄 출력을 controller가 한 줄로 요구해 실패했다
(#637). 실패한 run을 성공으로 바꾸지 않는다. 출력 판정만 보완한 controller
`0dd6219197037da83ebd41638b47b5bd77f2de29`의
[baselines 실행40881804452](https://github.com/bin9208/openpilot-rust/actions/runs/40881804452)에서
같은 source의 나머지 sound/Bluetooth/tombstoned/modem 대조군을 이어 확인한다.
전체 원본 비교를 다시 수행한 것처럼 표시하지 않도록 해당 산출물에 범위를 기록한다.

### 정상 workspace 검사에 새 회귀 연결 준비

집중 controller의 성공을 정상 PR gate로 대체하지 않으려면 새 회귀의 prerequisite도
실제 workspace 명령에 연결해야 한다. 현재 dev와 #606 `a5c5267`의 workspace에는
Wi-Fi용 새 `openpilot-process-child` 경로가 없고 modem opt-in15개도 기본 검사에서
선택되지 않는다. 테스트를 skip하거나 fixture fallback으로 바꾸지 않는다.

#606의 runner/guard 변경을 보존한 후 정상 통합 단계에서 workspace의 기존 범위를
유지하며 child를 명시적으로 빌드하고 Cargo metadata의 실제 target 경로로
`WIFI_PROCESS_CHILD_BINARY`/`MODEM_PROCESS_CHILD_BINARY`를 설정한다.
workspace test/strict Clippy에는 `openpilot-modem/helper-lifetime-tests`를 추가한다.
해당 최소 diff의 YAML·apply 구문만 준비·확인했으며 아직 공용 workflow를 수정하지 않았다.
이는 #630 회귀 연결이며 #606의 러너 전환을 다시 구현하는 작업이 아니다.

## 영향 GNU ARM 빌드 성공 — 2026-10-29 KST

동일 후보 `5723dd8fcdd125be0d4b008c6838e89188d89ae9`, controller
`396a02c4f45695b3db128c9f0301ca6331acc274`의
[40878034961](https://github.com/bin9208/openpilot-rust/actions/runs/40878034961)은
`bin-actions2`에서 성공했다. artifact12902400229(629,275bytes, API SHA256
`48c66de7245c15b60f611874a02d478f5e898ee302aa103f9848bc7d1503ed3a`)의
outcome·명령 exit·provider receipt와 ELF 기록을 직접 읽기 확인했다.

- 선정9package 전부에서 release 실행 파일/예제27개를 생성했다.
- 모든 결과는 ELF64/little-endian/AArch64, GNU loader
  `/lib/ld-linux-aarch64.so.1`이며 GLIBC 최대 요구는2.39다.
- 기존 fb33 고정 helper를 수정하지 않았고 Rust1.94.0/GCC13/glibc2.39 ABI를 확인했다.
- FFmpeg6.1.1 archive10,458,600bytes와 기존 SHA256을 확인했다.
  `logger-minimal-static` provider는 completed=true/headers_only=false이며
  avcodec100개·avformat44개·avutil90개 object 모두 AArch64다.
- candidate/helper source 및 index 전후 diff는 비어 있고 candidate index hash도 같다.
- 기존 `Curl::fetch_with_mirror` 호출이 정확 전송량으로 성공했다. 선택 URL은 이
  receipt에 없으므로 실제 fallback 발생 여부까지 주장하지 않는다. #628은 별도 유지한다.

이는 영향9package GNU cross-build 성공이며 ARM 실행·AGNOS ABI·full GNU/musl 필수
gate·기기 검증은 outcome의 false 항목으로 분리했다. 남은 대조군과 정상 PR/병합 후
검사를 통과하기 전에는 #630을 종료하지 않는다.

대조군 [40881804452](https://github.com/bin9208/openpilot-rust/actions/runs/40881804452)은
sound5실패/3성공, Bluetooth1실패/2성공, tombstoned의 실제 `wait4` 후 `kill(-group)`
순서 재현 및 각각의 compiler 성공을 확인했다. #637 출력 판정 보완은 실제 동작했다.
다음 modem checkout은 shallow clone에서 unreferenced 기준 SHA가 빠져 중단됐다
(#638, artifact12901134179/API SHA256
`877ae98c932d9d0629ec2826db3ebc91069929bf0ec0a5e114e798339369845f`).
제품 코드 실패로 기록하지 않고 source/index 불변과 범위 `baselines`를 유지했다.
별도 modem clone에서 동일 저장소의 정확1364e48c 기준 SHA를 직접 fetch하도록 보완한
controller `2a639f3507e5f89c0a305c968d7ceb08e31066ba`의
[40885483934](https://github.com/bin9208/openpilot-rust/actions/runs/40885483934)에서
같은 source의 대조군을 재검증한다. 실제 결과 확인 전에는 #637/#638을 종료하지 않는다.

## 대조군 최종 성공과 남은 정상 통합

[40885483934](https://github.com/bin9208/openpilot-rust/actions/runs/40885483934)은
같은 source/controller2a639f350에서 `bin-actions2` 실행으로 성공했다.
artifact12902593824(4,307,167bytes/API SHA256
`5070db57a259d9797f5263e5cc9ec555d7e7a62f2fdfb7c6105a4805be238d86`)의
outcome returncode0/last_step=completed, source/index 불변과 native ELF hash52개를
직접 확인했다. `validation_scope=baselines`, `maintained_comparisons_executed=false`다.

- sound 기준: compiler 성공 후5개 실패/3개 control 성공 및 overflow control 유지.
- Bluetooth 기준: compiler 성공 후 이전 응답이 새 prompt를 지우는1개 실패/2개 control 성공.
- tombstoned 기준: compiler 성공 후 실제 `wait4` reap→numeric group signal 순서 실패.
- modem 기준: 정확1364e48c fetch/checkout과 원래 파일 bytes 검사·compiler 성공 후
  `must time out, not accept leader's valid inet line` EOF 실패를 재현했다.
- 같은 후보 modem/helper15개·공용 session3개·strict Clippy·기존 modem14개 원본 비교와
  process boundary가 성공했다. 실패한 기준 테스트를 후보 성공 개수에 합산하지 않는다.

#637/#638의 검증 준비·판정 결함은 실제 후속 Actions 기준으로 해소됐다.
선정9개의 현재 후보에 필요한 패키지/원본/회귀·할당 및 영향 GNU ARM 증거는 위의
서로 구분된 정확 source 실행으로 확보했다. 앞선 실패 run 전체를 성공으로 바꾸지 않는다.

2026-10-29 23:04 KST 조회 기준 dev는 여전히e445c9d이며 #606은
`917369a5162c983e2519a0151330aba1790bc3eb`에서 미병합이다.
정상 source PR의 새 회귀 연결·필수5개 검사·dev 병합과 별도 병합 후 검사는 아직 남았다.
#630의 항목별 개발·검증·통합 체크와 원래 실기 조건 이슈는 이 때문에 열어 둔다.

## 정상 PR 검사 재개 — 기존 사용자 승인과 CI 연결

사용자는 #630 완료까지 계속 진행하라고 이미 지시했다. 공통 CI 대기를 작업 종료로
처리한 판단을 바로잡는다. 관련 #598 채팅의 실제 사용자 지시에서 GitHub Actions와
등록 self-hosted3개를 함께 병렬 사용하라는 승인 및 Cargo/test 병렬 제한 해제 지시를
확인했다. 따라서 #606 병합 자체를 #630 정상 PR 검사의 절대 선행으로 두지 않는다.

현재 dev에서 상속한 전체 PR workflow를 그대로 실행한다. #606의 runner/guard/provider
전환을 복사하지 않고 `.github/workflows/rust.yml`에 다음 최소 변경만 포함한다.

- workspace test 전에 fresh `openpilot-process-child`를 빌드하고 Cargo metadata가
  보고한 실제 target 경로로 Wi-Fi/modem 소유 자식 회귀를 연결한다.
- 기존 workspace test와 all-target strict Clippy에 modem helper15개 opt-in을 추가한다.
- 이 workflow의 고정 `CARGO_BUILD_JOBS=2`를 제거해 runner의 기본 병렬도를 사용한다.
  모델 입력/동작을 바꾸는 `CPU_COUNT`/`JIT` 값은 이번 변경에 포함하지 않는다.

기존 job·needs·matrix·수용 명령과 전체 필수 gate 범위는 유지한다. 공용 CI 일괄 제외의
최소 예외이며, workflow와 고정 cap을 검사하는 기존 CI 정책 테스트에 한정한다. 다른 작업 브랜치나 실제
진행 중인 CI를 수정/취소하지 않는다. 정상 PR의 정확 새 SHA 결과와 dev 병합·별도
병합 후 검사까지 계속 진행하며 집중 결과를 필수 status로 복사하지 않는다.

첫 정상 PR head `aae73b2d76db653b91ff90d1e575f7b9ce9be2cb`의
[fast40894426442](https://github.com/bin9208/openpilot-rust/actions/runs/40894426442)는
기존 webcam CI 정책이 `CARGO_BUILD_JOBS=2` 키를 직접 요구해 실패했다. 기존 미디어/
아키텍처/메모리 관련 단언은 보존하고 사용자 지시에 맞게 해당 고정 cap 부재를 검사한다.
독립 읽기 리뷰가 찾은 workspace modem 원본 비교용 `MODEM_SOURCE_PYTHON`도 같은
setup-python의 실제 절대 경로로 연결한다. 테스트를 skip하거나 원본 비교를 제거하지 않는다.

## dev 통합 및 무거운 CI 배치 변경 — 2026-10-30 KST

선정9개는 PR #631로 dev `3fd7fd051b0af237fb8f75a4260e48707e3ae7c8`에 반영됐다.
정확 PR head `875e7a4b6172305dbc6ae4d16c78d5f02ba84fe3`에서 필수5개와
Rust46job/통합16job/전체 ARM이 성공했다. 병합 후 통합16job·fast/docs도 성공했다.
사용자가 무거운 모든 검사를 셀프호스트로, hosted는 가벼운 검사만 남기도록 지시하여
진행 중이던 이 작업의 hosted post Rust40908894073은 취소했다. 취소를 성공으로 표시하지 않는다.

새 dev 기반 후속 CI 변경과 남은 정상 PR·병합 후 검증은
[self-hosted-heavy-ci-630.md](self-hosted-heavy-ci-630.md)와 #630에서 이어서 추적한다.
이 최신 배치 지시가 위 과거 hosted 병렬 실행 설명보다 우선한다.
