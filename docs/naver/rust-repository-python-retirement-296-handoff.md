# 저장소 전체 Python 전환 작업 인수인계

## 현재 감독·통합 상태: b43ac10d, 2026-10-21

부모6.1 Sol이 #296을 감독·검토·통합하고 기존6 Sol 작업자를 사용한다.
작업트리 native-build-tools-296, branch codex/feat-296-repository-python-retirement,
PR343→dev이다. dev-md와 주 작업폴더, `.omo/`는 보존한다. 콤마가 없으므로
사용자가 최종 pull 후 정상부팅·녹화·업로드·주행을 확인한다.

200ba3fd의 paramsd native controller는 독립 원본1210출력 대조를 phase/top
성공 기록 전에 수행한다. 실제 원본/native IPC·83lib·2controller·전체reference·
Clippy가 run39519319264/job118959001528에서 통과했다. 부모가 실제 packet/journal,
입력봉인, GPS, 저장·재시작, 종료 및 성공 기록을 확인했다. 원본Python은 유지한다.
39loop 입력 및7scenario/daemon 패킷은 run39519153139/job118958465021에서
원본 모든바이트 대조와 실제CLI exec audit를 통과했다.

19fa19c1은39사례2420행의 독립 원본 loop comparator와 native CLI를 추가했다.
입력정확성, host ELF, 취소,32MB출력, 원본수치, 출력·입력·소스봉인을 요구한다.
실제새검사는 sourceb43ac10d의 run39521408106/job118965858671에서 진행 중이다.
앞선 진단workflow는 Eigen준비가 빠져 빌드 전제에서 실패했고 native action으로 보완했다.

Bluetooth 세Python은 d21944e6의 전체14suite/config253/journal721/gesture37
preproof 이후61f4f275에서 제거했다. 삭제 후61f4검사는 journal unit2개에서
JSON키순서와 무효mutation으로 실패했다. b43ac10d는 실제 독립 원본 raw721행을
봉인해 단위를 수정했다. 새 postdelete run39521331522/job118965606861이 진행 중이고
정책job118965606642는 통과했다. f3f0725e의 원자성 대체 lagd정책도
run39520095117/job118961553990에서13disk/67source/2lagd 검사를 통과했다.
인벤토리218replaced-local /3partial /3359pending.

이미 통과한 soundd 삭제후7모드 두native lane/Clippy와 #435 실제5IPC 및
생명주기 증거는 관련 문서에 있다. opt-in fused Conv/Gelu/Mean은 원본3840바이트
일치와138영향검사·Clippy 후4ce6b169에 통합했고 기본선택은 유지한다.
전체compiler 전환이나 기기CPU 절감으로 해석하지 않는다.

원격 주작업 branch는 아직62e8c235이며 전체PR run39514415599의 LLVM20/ARM
결과를 보존한다. 다른45완료job 중5실패는 후속수정/집중검사로 처리 중이다.
calibration25원본/native 검사는 그run의 workspace실패 뒤에 있어 skipped였다.
완료로 오인하지 않고 별도 원본선행검사를 준비한다. logmessaged startup은 실제
성공한 support job118943760348 증거를 확인하고 sourceabsence proof를 준비한다.
전체Python전환·최신필수전체CI·dev통합·소스일치패키지·사용자기기확인은 미완료다.
아래는 과거 기록이다.

## 과거 감독·통합 상태: aace5f64, 2026-10-21

부모6.1 Sol이 #296의 검토·통합을 맡으며 기존6 Sol 작업자를 사용한다.
작업 HEAD는 `aace5f64`이고 PR343→dev에 후속 게시한다. dev-md, 주 작업폴더와
`.omo/`는 건드리지 않는다. 콤마가 없으므로 사용자가 pull 후 정상부팅·녹화·업로드와
주행을 확인한다. 이 실기 결과는 cloud 검사와 구분한다.

soundd #434는 source`f19ef5ad4cb56b9a7bace81fdeb34b6325e704eb`의
[run39512282992/job118935976457](https://github.com/bin9208/openpilot-rust/actions/runs/39512282992/job/118935976457)에서
구파서 RED, 수정본의 원본 소수 bit 회귀와7모드 원본·native 두lane 대조, strictClippy가
통과했다. 부모가 artifact12306223105의 실제7모드와 lane별22exec 및4ELF 봉인을
확인했다. aace5f64에서 Python 검사기를 제거하고 삭제 후 native 두root 증명으로
gate를 전환했다. 삭제 후 정확한 새 소스 검증은 진행해야 한다. 인벤토리는
214replaced-local /3partial /3363pending이다.

Bluetooth journal/config/gesture의 Python 부재 proof 및 호출자 전환을 준비했다.
아직 이 세 원본을 제거하지 않았다. journal proof의 step 간 fixture 환경변수 누락을
수정했고 Enum의 include_bytes 이중참조도 수정했다. 실제 새 cloud 결과를 받아야 한다.
기존 a993 전체 run39510866633의 포맷, lagdClippy, Panda의 러너 미설치rg 사용 실패도
수정했다. 이들 수정의 최신 정확한 SHA 검사는 남아 있다.

모델 fusedMean 후보는 통합하지 않았다. 실제 cloud에서 closure가 선택됐지만 최종
960값이 모두0으로 나왔다. 보존된 LLVM IR에서 서로 다른 누산값의 semantic key가
충돌해 Gelu 결과를 재사용한 결함을 확인했다. 별도 후보에서 수정·검증을 계속하며
원본 최종 출력과 정확히 일치하고 회귀 검사를 통과하기 전에는 production에 반영하지 않는다.
전체 Python 전환·전체 모듈 점검·최신 필수 CI·dev 통합·소스 일치 패키지는 미완료다.

## 최신 통합 상태: a9933ed7, 2026-10-21

부모6.1 Sol이 PR343→dev의 감독·검토·통합을 담당한다. 게시한 코드는
`a9933ed72fc1ca8c91cf5728d0345f1e4181104d`이고 전체 PR 검사
[39510866633](https://github.com/bin9208/openpilot-rust/actions/runs/39510866633)가 진행 중이다.
아래341aa2b5 기록은 이전 checkpoint다. dev-md와 주 작업폴더, `.omo/`는 보존한다.
콤마는 없고 사용자가 최종 후보를 pull해 정상부팅·녹화·업로드·주행을 확인한다.

BlueZ/Enum 원본·동일ELF 부재 proof는 c7005ce1에서 통과해 두Python을 제거했다.
삭제 후 ba5def0a의 BlueZ6363사례·실행audit도 통과했다. Enum은 새reference의
불필요한 이중참조로 컴파일 실패해 from_slice(old_raw)로 수정했다. 최신 exact-source
post-deletion gate는 계속 필요하다. 인벤토리는213replaced-local /3partial /3364pending이다.

ba5def0a의 [paramsd full job118930155954](https://github.com/bin9208/openpilot-rust/actions/runs/39510538880/job/118930155954)는
시작게이트, 원본1210패킷과 native 전체비교, 저장·재시작/GPS/종료, strictClippy를 통과했다.
부모가 artifact12305212845(archive a9a6fc6dcbf15af20696d832169ba81cf5cebe4364546466ccaa1b54b2a15ce2)의
실제 두lane 패킷 수·gate·보고서를 확인했다. 원본Python과 입력생성기는 남아 있다.

soundd f60e4ac0 전체audio GREEN은 #426/#433 문서에 기록했다. 추가부재proof는
원본7모드와 native두lane 실행 후, JSON 소수 재파싱의1bit 차이에서 새reference가 실패했다.
[#434](https://github.com/bin9208/openpilot-rust/issues/434)로 분리했고 sounddfeature의
float_roundtrip과 원본 raw timestamp/summary 회귀를 준비했다. 허용오차는 늘리지 않는다.
구설정RED/수정GREEN 및 전체7모드proof를 받아야 sounddPython을 제거할 수 있다.

Jetlink18f4372d 집중gate는5baseline RED/6current GREEN/build/Clippy를 통과했다.
Panda#432는 추가16진단에서 재현하지 못해 원인 미확정이며10timestamp 조건을 유지한다.
모델 fusedMean 후보는 아직 source-selection 단계에서 거부되어 미통합이다.
세부 rejection 사유를 남기는 한정 진단을 추가해 다음cloud에서 확인한다.
완성되지 않은 compiler/개발도구 전환을 완료로 세거나 기기검증으로 표현하지 않는다.

## 이전 감독·조합 상태: 341aa2b5, 2026-10-21

부모6.1 Sol이 #296 전용 작업트리와 PR343→dev를 통합한다. 로컬 구현 HEAD는
`341aa2b5ace9a49277c53896ad27137dd09178cd`, PR 게시 HEAD는 아직
`2d8d027932dc880bd0acd00288b256ac8d7f1b89`다. 오래 실행 중인 기존 workspace와
ARM 결과를 보존하면서 새 변경은 명시적 source SHA를 checkout하는 별도 Actions로
검증한다. 진단 workflow/원본 Python은 dev에 합치거나 전환으로 세지 않는다.
dev-md와 주 작업폴더 변경은 건드리지 않는다. 콤마는 없으며 사용자가 최종 커밋을
pull한 뒤 정상부팅·녹화·업로드·주행을 직접 확인한다.

기존 Rust run39494388960에서 LLVM20 전체 대조와 cold node280 네 검사가 통과했다.
workspace 전체 cargo test도 통과하고 후속 실제 모델 대조 중이며 ARM은 C3X static
probe 빌드 중이다. Panda 원본 타임스탬프9/10, 오디오 관측 시간초과, BlueZ 컴파일
오류로 집계는 성공이 아니다. Fast와 Integration은 통과했다. lagd #429와 messaging
#431 실제 할당 절감 영수증은 개별 문서에 기록했고 CPU/기기 절감으로 해석하지 않는다.

실제 sourcea0690dd9인 run39503456244에서 Eigen action x86_64/aarch64 아카이브의
690개 헤더가 원본과 일치했다. 부모가 두 실제 action map을 다시 내려받아 봉인된 원본과
바이트 일치까지 확인했다. native paramsd7검사와 보존한 Python5검사도 통과했다.
뒤이어 lagd CI 정책의 옛 Eigen pip/import 요구, audio 정책의 옛 연속 명령 문자열,
audio RED 판독의 러너 미설치 rg가 실패했다. f60e4ac0은 이 조건들을 현재 native
경로에 맞게 수정한다. run39505980261의 sourcef60 정책·디스크·lagd75검사는 통과했다.
오디오 구버전의 정확한 mutation 실패(exit101)는 이미 관측됐으며 수정본 전체 검사는
같은 run의 audio job118914882870에서 진행 중이다.

Bridge는 삭제 후 source0d1700e5의 실제 트래픽·17exec gate도 통과했다. 부모가
artifact12299956453(archive cb5ed1b8…)의 두 보고서·provenance·봉인을 독립 확인했다.
BlueZ policy6363사례와 enumeration40노드의 원본 관측을 고정했고, 원본이 없는
private root에서 같은 ELF로 비교하는 새 gate를 f60에 추가했다. 원본 Python 두 파일은
그 새 cloud 결과 전까지 유지한다. sudo fixture의10원본 대조를 보존하며 두 fixture
호출자가 실제 hash-bound Rust ELF를 설치하도록 바꿨다. 현재 인벤토리는 그대로
211 replaced-local / 3 partial / 3,366 pending이다.

sourcea0690의 UI는 BlueZ·enumeration·sudo 대조를 통과한 뒤 제스처 도구 Clippy의
중복 camera_random 모듈 로드에서 실패했다. 341aa2b5는 기존 공용 Random을 참조한다.
새 집중 Bluetooth run39506338118은 source341aa2b5를 명시적으로 checkout하여
제스처·config·files·journal·input과 변경된 sudo 호출 경로를 검사한다. 큐 등록은
통과가 아니며 결과와 원본 artifact 확인을 계속해야 한다.

prefix93의 원본 차이는 LLVM18 Gelu 다항식의 contraction에 따른 중간 반올림으로
좁혔고, 부모가 역사적 object와 trace를 독립 확인했다. cold 최종 출력 후보의 미구현
실패를 별도 source7626b358/run39505980261/job118914882990에서 확인했다. 생산
fusion fence는 유지하며 generic closure 구현은 작업 중이다. 이 실패 검사는 구현 전
진단 source에만 있으며 main feature branch에 실패 검사를 그대로 넣지 않는다.
Panda run39501353390의8+8회 모두10timestamp 성공은 최초 누락 원인을 입증하지
않는다. 다른 Python PUSH의 readiness는 실제 Panda logger socket의 delivery 증명이
아니며, startup C++9로그도 이미 있었으므로 단순 시작 지연 원인으로 단정하지 않는다.

로컬은 구문·형식·diff·자료 검토만 한다. `.omo/`는 미추적 보존한다. 현재 별도 작업 중인
model-runtime/paramsd leaf 변경은 부모의 검토·명시적 staging 전까지 커밋하지 않는다.
전체 Python 전환·전체 모듈 점검·정확한 새 SHA 필수 CI·dev 통합·소스 일치 패키지는
계속 진행해야 하며 전체/기기 완료를 주장하지 않는다.

## 최신 조합 상태: 7709c2e3, 2026-10-21

#296 전용 브랜치에 검토한 후속 다섯 커밋을 준비했다. HEAD는 `7709c2e3`이고
원격은 아직 `edb42bf0`다. 기존 ARM job 118816130250의 실행 결과를 보존하기
위해 새 PR 푸시를 기다리는 동안 소스 전환을 계속한다. 이 job은 static C3X
musl workspace 빌드 단계이며 전체 한도는 시작 시각 18:48:42 UTC부터 120분이다.
성공·실패·시간 만료 결과를 확인한 뒤 후속 묶음을 push하고 정확한 SHA의
필수 CI 결과를 확인해야 한다. 큐 등록만으로 작업을 종료하지 않는다.

후속 묶음은 #424~428/430의 런처·QA 수명·관측 수정, Bluetooth files 원본
하나 제거, config/BlueZ policy/enumerate native 검사기, 37사례 제스처 입력
생성기, #429 lagd 할당 개선과 cold node280 두 입력의 독립 대조를 포함한다.
원본 대조·회귀·Clippy·할당 결과는 새 Actions에서 확인 전이다. 제스처 검토에서
정수 0/10 시각과 정확한 decimal→f64 파싱 차이를 수정했고 전체 JSONL
바이트 대조를 필수로 추가했다. 생성기와 checker Python 두 원본은 유지한다.
인벤토리는 210 replaced-local / 3 partial / 3,367 pending이다.

prefix93 원본 cloud 진단은 run 39490770391에서 최종 원본 출력이 동일한
계측을 통과했다. Gelu 92,160값 중 46개가 materialized 원본과 다르고 최종
차이 세 채널에도 각각 하나씩 있다. 부모가 payload hash와 raw word 비교를
독립 확인했다. ReduceMean 이전 차이까지 좁혔으며 Conv/Gelu의 정확한 원인은
아직 미확정이다. 별도 capture 브랜치에서 다음 진단을 진행하며 native fence는
유지한다. temporary Python 진단을 dev에 합치거나 Rust 전환으로 세지 않는다.

로컬은 구문·포맷·diff·자료 검토만 수행한다. `.omo/`는 미추적 상태로 보존한다.
dev-md는 건드리지 않으며 실기 정상부팅·녹화·업로드·주행 확인은 사용자가 맡는다.
전체 Python 전환, 필수 CI, dev 통합과 최종 소스 일치 패키지는 아직 미완료다.


## 현재 조합 중인 후속 변경: 2026-10-21

이번 후속 조합의 기준인 직전 게시 HEAD는
`edb42bf095d6a3e3b38cc0a4546976dfb710363e`다.
Integration과 Fast는 통과했고 Rust 집계는 실패다. 개별 platform은 모뎀 #398의
이전 EOF 실패와 수정 15검사·Clippy·원본 대조를 통과했다. telemetry도 SDK #421의
실제 crash-I/O 비교 및 전체 job을 통과했다. 이 결과는 dev 통합이나 기기 검증이 아니다.

실패한 후속 경계는 Panda 새 검사기의 `-hfoo` 처리와 C++ QA probe packed 필드
참조의 UBSan(#425), 오디오 검증의 읽을 수 없는 `/proc/PID/wchan`(#426),
UI 이전 소스 빌드의 CMake 절대 경로 캐시 충돌이다. workspace는 cluster
종료 직후 `/proc` task 관측 단정(#428)에서 실패했다. 실제 소유 thread join은
완료하며, 기존 close와 정확한 최종 집합 조건을 보존하고 커널 관측만 최대 1초
기다리도록 보완했다. 각 수정과 회귀 검사를 준비했다.
부팅 런처 실패 시 TERM 무시 자식 때문에 종료가 멈추는 #424도 수정·회귀·cloud
이전 소스 실패 검사를 연결했다. 로컬은 형식·구문·diff만 검사한다.

Bluetooth files는 edb에서 7검사·Clippy와 같은 ELF의 Python 존재/부재 전체
34파일·21,734,088바이트 일치를 통과해 Python 원본 하나를 삭제했다. 삭제 후
새 빌드·검사·실제 exec 감사는 아직 실행 전이다. 인벤토리는 소스 기준
210 replaced-local / 3 partial / 3,367 pending이다. Bluetooth config는 253사례
원본 대조를 준비했고, journal의 실제 원본 raw trace와 logmessaged startup의
전체 원본 disk rows를 다음 cloud artifact에 보존하도록 추가했다. 두 원본은 유지한다.
Bridge는 원본 관측 고정과 제거 준비를 진행하며 Python 원본을 유지한다.
독립 6 Sol 검토 두 건은 추가 확정 blocker를 찾지 못했고, launcher의 KILL 이후
background reaper 생성 실패 시 reap 보장 한계는 #424 문서에 명시했다.
모델 컴파일러는 독립 prefix279 증거와 prefix93 수치 차이가 별개로 남아 있다.
다음 node280 `policy/linear_2` 원본 캡처는 별도 임시 cloud 진단으로 진행한다.
전체 전환·전체 CI·dev 반영·최종 패키지는 미완료이고, 콤마 정상부팅·녹화·업로드는
사용자가 git pull 후 직접 확인한다. dev-md 및 별도 CI/CD 최적화 작업은 변경하지 않는다.

## 재개 및 CI 복구 준비: 2026-10-21

사용자는 dev-md 완료 대기를 해제하고 #296 진행을 지시했다. 부모는 6.1 Sol로
감독·통합하며 복잡한 구현·검토는 6 Sol, 단순 목록 취합은 6 Luna에 배정한다.
작업 폴더·dev 대상 PR343은 동일하고 dev-md와 CI/CD 최적화 별도 작업은 섞지 않는다.

`11f3dec4`의 Integration은 통과했지만 Fast와 Rust 집계는 실패했다. 실제 실패는
디스크 가드 분류, 모뎀 PPP 종료 상태, soundd 선택지 진단, Panda padding 임시값
수명, SDK 로그 수집 순서 및 UI writer 종료 검사다. startup prerequisites는 등록
검사 통과 후 45분 작업 한도로 취소됐다. 후속 코드에 각 수정과 검증을 연결한다.
SDK #421은 서로 다른 PUSH 소켓의 barrier가 crash 로그보다 먼저 도착할 수 있어
필수 로그 수를 명시한다. UI #423은 native buffered write를 완성한 뒤 frame을 세고,
실제 short-reading child로 이전 소스 실패를 요구한다. 이전 `/bin/true` 실패는
그대로 유지하며 이 변경만으로 원인이 입증됐다고 하지 않는다. Jetlink #399는
외부 소켓 보존과 막힌 stderr에서도 종료를 검증한다.

Panda device 도구는 원본 1,746사례·2,490연산을 대조하는 새 Rust 검사기와 9개
검사를 등록했다. Bluetooth files는 독립 원본 관측을 고정하고 같은 ELF로 Python
존재/부재 전후 전체 출력과 결과를 비교하도록 준비했다. 원본 파일은 아직 유지한다.
로컬은 형식·구문·diff만 확인했고 기능 검증은 후속 정확한 SHA의 Actions에서 한다.
현재 인벤토리209/3/3368, 전체 전환·dev 반영·패키지·사용자 실기 확인은 미완료다.

## 최신 우선 기록: 11f3dec4, 2026-10-19

현재 원격 HEAD는 `11f3dec411662baa961733b363c0b38d7082fcfa`다. PR #343은
dev 대상이며 dev-md는 변경하지 않았다. Rust run39192427750과 Integration
run39192427739가 진행 중이다. 이전 1364의 Fast 경로 보존 실패와 bridge 파일
소유권, logging Clippy, soundd 스키마 import, journal 미사용 import를 수정했다.
calibration float/Panda padding 도구와 modem #398의 14개 회귀 및 이전 SHA의
실제 EOF 실패 검증도 연결했다. 로컬 실행 검증은 하지 않았고 형식·구문만 확인했다.

1364에서는 Bluetooth files의 6검사·Clippy·실제 15사례가 통과해 그 도구 하나의
원본 제거 준비를 시작했다. 아직 삭제하지 않았으므로 인벤토리는209/3/3368이다.
Wi-Fi 46의미 검사와 할당 측정은 통과했으나 전체 connectivity job은 journal
Clippy에서 실패했다. 숫자와 제한은 module-audit/evidence 및 #397에 기록했다.
전체 전환·전체 CI·dev 통합·패키지와 사용자 실기기 확인은 여전히 남아 있다.

## 최신 우선 기록: 1364e48c, 2026-10-19

아래의 과거 SHA·수치보다 이 기록을 우선한다. #296 전용 브랜치의 원격 HEAD는
`1364e48c96e888fa5d178c5eb2132d135cf30765`이며 PR #343의 대상은 dev다.
dev-md/#338은 변경하지 않았다. 현재 기준 인벤토리는 209 replaced-local,
3 partial, 3,368 pending이다. 새 bridge/soundd/logmessaged-startup/Bluetooth
files/journal 검사기 5개는 소스 구현·CI 연결만 완료했으므로 이 수치에 더하지
않는다. 원본 Python은 독립 비교를 위해 유지한다.

2176c4a5의 Fast/Integration/docs는 통과했지만 Rust 전체 검사는 실패했다.
실제 실패는 포맷, 통계 충돌 입력의 파일 모드, navigation의 registration 산출물
환경 누락 3건이고 집계 job도 실패했다. 새 HEAD에 수정했으며 Rust
[run39190411681](https://github.com/bin9208/openpilot-rust/actions/runs/39190411681)
검증은 진행 중이다. 이전 실행의 미완료 job은 새 푸시로 취소되어 통과로 세지 않는다.

로컬은 빠른 형식·구문·diff 검사만 수행한다. 빌드·회귀·원본 대조·측정은
Actions에서 한다. 콤마는 없으며 사용자가 나중에 정상부팅·녹화·업로드를 직접
확인한다. 이 기록은 전체 전환, dev 통합, 최종 패키지 또는 실기기 완료를 뜻하지 않는다.
모뎀 외부 명령 수명 문제는 #398로 분리해 소스 기반 재현·수정을 진행 중이다.

## 2026-10-19 #296 전용 작업의 최신 상태

통계 데몬 검사기 전환 `bf463310`에 이어 사운드 정책 검사기와 WAV 생성 도구를 전환했다.
아래의 이전 원격·로컬 상태보다 이 문단을 우선한다.
`ecfb7774`의 Rust run39159654100은 workspace150분 제한으로 취소됐고,
전체 결과는 실패다. 전체 workspace 테스트는 통과했지만 뒤따른 LLVM20 검사가
시간 한도에 걸렸으며 이후 단계는 실행되지 않았다. LLVM20의 동일 검사를 별도
필수 job으로 분리한 수정은 `c7ea57e0`에 있다. 원격 `74d211ba`의
Rust run39174296648에서 완료된39개 job은 통과했으나 Card CI 목록 검사가
새 LLVM20 job을 누락해 실패했다. 기대 목록만 보완한 `0049df74`는 같은
실패 재현 후 관련39검사를 통과했다. 진행 중인 나머지 CI와 후속 SHA의 CI는
아직 완료되지 않았으며 전체 통과가 아니다.

업로드 완료 시 실제 전송한 파일의 inode에 표시를 남기고, 전송 중 변경된 폴더
파일을 거부하는 #388/#389 수정은 `6716df5e`다. 패키지47검사·Clippy와 기존
원본 대조(파일212/백오프10,000/HTTP32/폴더99)를 통과했다.
등록 시간 검사기 및 Version/tombstone/SDK 공용 도구4개도 검증 후 제거했다.
공용 도구 삭제 전후 같은15실행 파일로9개 도구를 각각 통과했고, 각1,444실행
시도에 Python이 없었다. 삭제 후 재빌드·86검사·관리 진입2비교·Clippy도 통과했다.
현재 인벤토리는203개 전환 완료(로컬),3개 일부 전환,3,374개 대기다.
통계 수신 중단 검사기와 공유 원본 도구2개는 삭제 전후 같은8개 산출물로
각15사례·50실행·Python0회 및 정규화한4결과의 바이트 일치를 확인했다.
삭제 후 새 빌드·72검사·Clippy도 통과했다. 출력 정체 중 마지막 SIGINT에
성공 결과가 남는 버그와 원본 CLI 도움말/인수 처리 차이를 재현해 수정했다.
procLog·ARM 검사기2개는 삭제 전후 동일 실행 파일의44주기·QEMU3프레임
대조 후 제거했다. 삭제 후77검사·Clippy·새 musl 빌드와 실제 대조도 통과했다.
이는 호스트 검증이며 실기기 확인은 수행하지 않았다.
사운드 정책 검사기1개는 같은5개 산출물의 삭제 전후 정책6·데몬7사례와
정책24파일 일치로 검증했다. 삭제 후16검사·Clippy·새 빌드도 통과했다.
데몬 검사용 Python은 남아 있으며 음원 생성만 Rust 도구로 교체했다.
통계 데몬 검사기3개도 원본10사례, 삭제 전후 같은5개 ELF·34회 실행·Python0회,
삭제 후8회귀검사·CI57검사·Clippy로 검증해 제거했다. 공유 stats_source.py는
후속 수신 중단 검사기 전환에서 제거했다. 원본 statsd.py는 유지한다.
통계 전송 검사기와 원본 실행 보조 도구2개는 원본8시나리오, 동일 ELF의
삭제 전후4시나리오·9산출물 일치·Python 실행0회로 검증 후 제거했다.
삭제 후 재빌드·6회귀검사·CI56검사·Clippy·실제4시나리오도 통과했다.
남은 통계 원본 소비자는 별도이며 전체 통계 도구 전환 완료가 아니다.
Bluetooth 입력 디코더 검사기1개는 원본10,049사례·5개 산출물 바이트 대조,
같은 ELF의 삭제 전후 Python 없는 실행, 삭제 후8검사·CI56검사·Clippy와
실제10,049사례를 통과해 제거했다. 나머지 Bluetooth13사례와 등록 도구는
별도 후속 범위다. loggerd #391은 실제 AAC 종료 회귀4개, 패키지16검사와
새 원본/네이티브 비교를 통과해 `74d211ba`에 반영했다. soundd #392/#393은
음원 끝 처리와 로그 정체 시 콜백 중단을 각각 재현·수정했고, 패키지8검사,
정책6시나리오 및 런타임7시나리오를 통과했다. 실기기 음성 검증은 아니다.
전체 전환, 최종 필수 CI, dev 통합 후 검사 및 정확한 소스의 AGNOS 릴리스는 남았다.

작업은 `/home/fma1/.codex/worktrees/native-build-tools-296/openpilot-rust`의
`codex/feat-296-repository-python-retirement`에서만 진행한다. 기본 작업 폴더의
dev-md/#338 변경은 별개다. 콤마는 현재 없고, 사용자가 git pull 후
정상부팅·녹화·업로드를 직접 확인한다. 다시 연결을 요청하지 않는다.

원격 HEAD ecfb7774eea9577d4c3c396213a3ecbd0b7d73c9의 Fast checks 두 실행,
Integration, 문서 검사와 ARM 빌드는 통과했다. Rust run39159654100의 workspace
검사는 진행 중이며 텔레메트리 Version 시작 대기(#382)와 UI strict-mode(#384)가
실패했다. 전체 CI 통과나 dev 반영 완료가 아니다. UI는 실제9/10FPS 경계를
유지한 로컬 수정 검증을 통과했다. Version13검사는 로컬 통과했으나 원격 원인은
미확정이다. 별도의 부분 PID 승인 오류는 실제 실패 재현 후 수정했고,
기존 제한을 유지한 Version14검사·Clippy가 통과했다.

로컬 ec8f90d9까지 tombstone live Python 도구3개 제거, UI strict-mode 검사,
늦은 취소 후 false PASS 제거를 커밋했다. 그 뒤 관리 런타임 검사기1개와
managed-entry 관련3개를 제거했다. managed-entry의 동일 실행 파일로 삭제 전후
44개 대조와 readiness를 각각 통과했고, 양쪽 모두269번의 execve/execveat 시도에
Python 실행이 없었다. 삭제 후 재빌드와21통합 검사(전체44비교 포함),
CI 정책60검사, Clippy·전체 포맷 검사도 통과했다.
관리 런타임 검사기는 원본/네이티브14사례 대조와10통합/1단위 검사를 통과했으나
독립 검토에서 FIFO 사전 해시 정지(#387)와 소켓 inode 재사용(#386)을 재현했다.
후속 수정은12통합/2소유권 단위 검사와 엄격한 Clippy를 통과했고, 새 검사기에서도
원본과 같은14사례/44exec/0Python을 확인했다. 정확한 커밋의 CI는 별도다.

인벤토리는188개 전환 완료(로컬),7개 일부 전환,3,385개 대기다. 이는 도구별
전환 기록이며 저장소 전체 전환이나 최신 변경의 전체 CI 성공을 뜻하지 않는다.
모델 node279의 새 검사와 영향 범위 검사는 LLVM18/20 합계42회 통과했다.
Version/crash/managed-entry 수집기 소켓 소유권(#386) 수정은 실제 재현과
회귀 검사를 통과했다. 아직 커밋되지 않은 변경과 새 등록 시간 도구의 원본 캡처는
별도로 보존한다. 최종 정확한 SHA의 필수 CI, dev 통합 후 검사 및 소스가 일치하는
AGNOS 런타임 릴리스 검증은 남아 있다.

## 2026-10-19 원격 검사 후속

`2a489716a0dc61eaa590e7add3509f8d90053ee2`의
[Rust run39133951820](https://github.com/bin9208/openpilot-rust/actions/runs/39133951820)은
workspace(101분16초)와 ARM(108분16초) 검사를 통과했지만 전체 성공은 아니다.
UI/connectivity는 첫 UI 비교10개 대상·62테스트를 모두 통과한 뒤 수명주기
검사를 컴파일하던 중35분 제한으로 취소됐다. 그 뒤 연결 검사는 실행되지 않았고
필수 집계 작업도 실패했다. [#377](https://github.com/bin9208/openpilot-rust/issues/377)에
원본 로그와 실행 경로를 기록했다. 모든 검사를 유지하면서 작업 한도를75분으로
조정하며, 수정된 정확한 커밋에서 전체 작업을 다시 통과해야 한다.

locationd 센서 입력 순서 수정은 `5f3fea6f`에 커밋했다. 패키지17테스트와
엄격한 Clippy, 새 실행 파일의 실제 IPC 연속3회 발행을 통과했다.
카메라/calibration 후속은 `d9b937fc`에 커밋했다. 원본63사례, 전체23테스트와
엄격한 Clippy, 새 실행 파일의 실제 IPC 연속7회 발행을 통과했다.
사용자의 기기 부팅·녹화·업로드 확인은 여전히 사용자 소유이며 미확인이다.

`17db98663f2d8309740569d057e10847f15d5249`의
[Rust run39143208959](https://github.com/bin9208/openpilot-rust/actions/runs/39143208959)은
workspace 포맷 검사에서 실패했다. lagd 모듈 선언 순서와 process-limits의
클로저 포맷을 `6abc945b`에서 수정했고, 로컬 전체 `cargo fmt --all --check`가
통과했다. 원격 전체 성공으로 보고하지 않으며 후속 정확한 커밋 검사가 필요하다.
모델 node278까지의 누적 LLVM18/20 검증과 ARM A630 오프라인 산출물 검증은
`32cffd87`에 커밋했다. 전체 모델 컴파일러 전환을 뜻하지 않는다.
DBC 생성/패키징 Python17개를 제거했다. 원본45출력/Card97/Radar103 일치,
삭제 후111테스트와 엄격한 Clippy, 실제 기본 SCons bootstrap 및 Card8개
fixture 생성이 통과했다. FIFO 입력 대기, Python 경로 구성요소 정렬,
Chrysler4,301자리 주소 오류 차이를 각각 실제 실패 후 수정했다.
인벤토리는178local/3partial/3399pending이다. 부모 Python facade/SCons 및
Card 비교 컨트롤러는 그대로 남아 있으며 제거 수에 포함하지 않는다.
macOS ARM64 호스트 빌드/해당 플랫폼15테스트도 필수 CI에 추가했으며,
실제 원격 결과는 아직 미확인이다. Linux17개 생성기 테스트는 모두 유지한다.
Version 원본 정책 기반 구현은65bdce63에 커밋했고8테스트/Clippy를 통과했다.
이는 Python 제거가 아니다. 실제 Version 비교·collector 컨트롤러와 공유
load_source 소비자인 tombstoned/managed-entry 도구 전환이 남아 있다.

caa3082d의 Fast checks·Integration·문서 검사는 통과했다.
[Rust run39144897864](https://github.com/bin9208/openpilot-rust/actions/runs/39144897864)은
확인 시 실행 중이었으며 전체 성공으로 보고하지 않는다. 후속 변경은 다시
정확한 SHA의 PR/merged-dev 검사와 source-matched AGNOS release가 필요하다.

## 2026-10-18 재개 상태 — 아래 과거 중단 기록보다 우선

작업 폴더와 #296 브랜치는 동일하다. dev-md/#338은 별도 작업으로 유지한다.
사용자의 최신 지시는 부모 Astra, 별도 워커는 모두 GPT-6.1 Sol이다.
현재 콤마는 없으며 사용자가 git pull 후 정상부팅·녹화·업로드를 직접 확인한다.
연결을 다시 요청하거나 기기 접근을 기다리지 않는다. 검증된 변경을 커밋하고
필수 PR/통합 검사를 거쳐 반영하며, 실기 결과는 사용자 확인 전까지 미확인이다.

원격 a8189aec까지 JSON 접근 할당 최적화와 UI 진단 CI 명령 수정이 반영됐다.
후속 fast timeout은5035bd54에서 알림 관리자 검사를 별도 필수 작업으로
분리했다. Params 검사기는 실제 원본47사례/오류15경계와 현재 후보를 비교하고
Python 삭제 후에도 통과했다. selfdrived 원본 비교의 끊어진 import를 복구하고
planner 입력 주기 차이도 수정했다. 후속 정확한 커밋의 원격 검사는 필수다.
모델 컴파일러의 검증된 범위는 a75b694c에 별도 커밋했다. LLVM18 전체
테스트 대상의 누적 검증과 LLVM20 그래프·기본 연산 검증, 엄격한 Clippy를
통과했다. 전체 모델 컴파일러 전환이나 기기 실행 검증을 뜻하지 않는다.
Forms 참조/검사기는208 추가 사례와 원본 삭제 후18장면/388프레임 일치,
원본·의존성 변경 거부 검사까지 통과했다. 두 Python 도구는 제거했다.
카메라 프레임·패킷 C++ 원본 생성기2개도 Rust로 전환했다. 원본 출력 바이트,
실제 호출부,512시나리오/72,493단계 및3,281패킷 비교를 통과했다.
후속 카메라 생성기10개와 UI 렌더2개도 호출부 전환 및 삭제 후 검증을 통과했다.
센서 상대 경로 검색 차이와 Toggle 연속 release 처리 차이를 원본 사례로 수정했다.
UI framework 오케스트레이터의 실제13단계 검사도 모두 통과했다.
X11 실제 입력·종료, UI 녹화3프레임/로그/EGL을 포함한다. 기기 검증은 아니다.
카메라 전체 수명주기 비교도32+8사례와오류·경로·환경회귀를 통과하여
CLI/공유헬퍼/이전테스트4파일을 추가 제거했다.
카메라 상태·연속 스트레스 검사도 Rust 호출 경로로 전환했다. 원본 스키마와
프로세스 오류 처리·상대 경로·errno 회귀를 보존했다. 일반 stress 검사도 별도의
엄격한 stderr 계약을 유지해 전환했다.
상태 검사기의 마지막 Python 의존이던 센서 비교·노출 quoting 헬퍼도 제거했다.
삭제 후19개 관련 테스트와 실제1,080프레임 비교를 다시 통과했다.
연속 카메라 런타임·cleanup·CI 오케스트레이터3파일도 Rust로 전환했다.
원본60개 검증 계약과25개 제어 흐름 사례를 보존했고, 실제58단계 호스트 검사가
통과했다. 삭제 후49개 테스트·Clippy와 새로 빌드한 카메라의8개 실행 사례도 통과했다.
인벤토리는148 replaced-local /3 partially-replaced /3,429 pending이다.
체크아웃 도구7파일은 원본9단계·오류12단계·Unix경로/환경 변수 비교 후 제거했다.
삭제 후10개 테스트·Clippy와 전체 실행 비교가 통과했다.
UI 수명주기, 인코더 호스트 및 ARM 대상 검증 도구가 추가 전환되었다.
원본·Rust ARM 비교는 AGNOS 라이브러리를 사용하는 호스트 QEMU에서
5사례·112패킷을 통과했다. UI 테스트의 ETXTBSY 경합도 재현·수정했다.
모델 컴파일러와 전체 전환 및 정확한 최종 커밋의 원격 검증은 아직 진행 중이다.
부팅 로그와 journald 검증 도구 7파일도 Rust로 전환해 제거했다. 삭제 후67개
테스트·엄격한 Clippy, 부팅 로그10사례와 journald155메시지/13오류/19치명적입력/
4종료사례 비교가 통과했다. 실제 실행 추적에서도 Python 호출이 없었다.
부분 전환 항목의 공용 원본 로더는 추가 파일 selfdrived_helpers_source.py에
남아 있으며 이동을 Rust 전환으로 세지 않는다. 전체 전환은 미완료다.
부팅 아티팩트 검사기도 Rust로 전환했다. 실제20사례와 원본 결과40개를
삭제 후 다시 비교해 일치했고, 9개 회귀 테스트와 Python 없는 실행을 확인했다.
명령 전환 전에 출력 버퍼가 유실되던 테스트 도구 결함도 독립 원본 비교로 수정했다.
시간 동기화 도구13파일은 Rust로 전환하고 제거했다. 삭제 전·후 각각493사례가
원본과 일치했고 각1,025실행 시도에 Python이 없었다. 삭제 후118테스트와
Clippy도 통과했다. 모델 컴파일러는 계속 진행 중이다. da803825 원격 UI 검사의
X11 연결 실패는 #354로 조사 중이며, 전체 CI 통과·dev 통합으로 보고하지 않는다.
실제 msgq 확장 모듈이 준비되기 전에 원본 검사를 실행하던 CI 순서 오류도
#356으로 재현·수정했다. 원본 검사 4개 및 CI·디스크 검사 48개를 통과했다.
서비스 검사기는 독립 원본의 88개 정의와 전체 Event 필드 154개를 유지해
Rust로 전환했다. 인자 오류 문구의 따옴표·줄바꿈 차이는 추가 원본 18사례로
수정했고, Python 검사기 삭제 후 관련 56개 테스트·Clippy·독립 리뷰와
Python 없는 실행을 통과했다. 공용 services.py는 아직 남아 있다.
최신 검증 결과는 evidence 및 runtime-module-audit 문서를 따른다.
공유 primary/dev-md 캐시 대신 아래의 cargo-target-retirement-296을 사용한다.

아래 2026-10-16 중단 기록의 로컬 모델 워커 지시, 재연결 요구 및 중간 실패 상태는
역사 기록이다. 현재 워커 상태와 후속 소스/검증 기록을 먼저 확인한다.

2026년 10월 16일 03:26 KST 기준. 사용자가 사용량 한도로 부모 Sol 또는 Astra와 로컬 모델 워커로 전환하려고 요청하여 구현을 중단했다. 모든 워커가 중단을 확인했고 로컬 Cargo·캡처 작업은 종료됐다. 소스와 검증 자료는 보존되어 있다.

**전체 전환은 미완료다.** 기준 인벤토리 3,580개 중 51개 소스를 검증 후 제거했고 3,529개는 pending이다. 별도로 6개 내장 Python 명령을 네이티브로 대체했다. 현재 미커밋 모델 컴파일러에는 알려진 수치 불일치가 있으며, UI 도구에도 재검증할 변경이 있다. 이 상태를 일괄 커밋하거나 장치에 배포하면 안 된다.

## 1 시작 위치와 브랜치

실제 작업 디렉터리:

```text
/home/fma1/.codex/worktrees/native-build-tools-296/openpilot-rust
```

- 저장소: `bin9208/openpilot-rust`
- 작업 브랜치: `codex/feat-296-repository-python-retirement`
- 기반 dev: `b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`
- 이슈: https://github.com/bin9208/openpilot-rust/issues/296 및 #1
- Draft PR #343: https://github.com/bin9208/openpilot-rust/pull/343 — 대상은 **dev**.
- `/home/fma1/rust/openpilot-rust`는 별도 **dev-md / #338** 작업이다. 그 폴더에서 이 작업을 구현·스테이징·리셋하거나 dev로 통합하지 않는다.
- `bin9208/openpilot`, `ajouatom/openpilot`으로 push하지 않는다. main 병합·장치 배포는 이번 인수인계 범위에 없다.

현재 폴더를 열었던 다른 에이전트는 먼저 아래처럼 실제 worktree로 이동한다.

```bash
cd /home/fma1/.codex/worktrees/native-build-tools-296/openpilot-rust
git branch --show-current
git status --short
git log -5 --oneline
```

기존 파일을 유지한다. `git reset --hard`, `git clean`, worktree 삭제, 현재 scratch 일괄 삭제를 하지 않는다. 특히 untracked Rust 파일과 ignored scratch 안의 디코더 프로토타입도 진행 중인 구현이다.

## 2 커밋과 원격 검증 상태

인수인계 문서 작성 직전 구현 HEAD는 `2fbf969fad61e973d54af213cabed518c518947e`다. 이후 문서 보존 커밋만 추가될 수 있다.

| 커밋 | 내용 | 원격 상태 |
| --- | --- | --- |
| `a077b0d246863865d943010104b857ed9fe1a5c9` | EGL 참조·검사, 두 명령 검사, Params 검사 등 5개 Python 소스 추가 제거. 누적 51개. | push 완료, 현재 PR head |
| `464309e054fb66ef5d8145b65deb1ac082c53130` | Miri libcore 해시 수집 내장 Python을 `hash-miri-core`로 교체 | 로컬만 |
| `178230571c26e43be6ccbc7b2459104785f9855a` | 검증된 카탈로그 ID를 `model-run --catalog-generation`으로 읽음 | 로컬만 |
| `2fbf969fad61e973d54af213cabed518c518947e` | NumPy 이미지 정수 배열 비교 연산을 Rust로 구현 | 로컬만 |

`a077b0d2`의 Fast checks(PR), user docs, Integration은 통과했다. push 이벤트 Fast checks 하나는 concurrency로 cancelled되었으며 성공으로 세지 않는다.

Rust CI: https://github.com/bin9208/openpilot-rust/actions/runs/38777900468

마지막 조회에서 43개 job 성공, `rust workspace checks`와 `rust aarch64 build` 실행 중, 확인된 실패는 없었다. 집계 job까지 포함한 최종 성공은 아직 확인하지 않았다. 외부 CI는 취소하지 않았으므로 재개 시 정확한 SHA의 최종 결과를 먼저 확인한다.

```bash
gh run view 38777900468 --json headSha,status,conclusion,jobs
gh pr view 343 --repo bin9208/openpilot-rust --json headRefOid,baseRefName,isDraft
```

이전 `38a5abc6a1e3bb0d7e3ffe7729b7fc62cc92d75c`는 46개 Rust job 전체 통과:
https://github.com/bin9208/openpilot-rust/actions/runs/38759226239
이 결과는 후속 커밋의 검증을 대신하지 않는다.

## 3 완료되어 커밋된 최근 작업

- `a077b0d2`: 실제 fast routing으로 dev-tools 310개 + service-tools 16개, 선택적 EGL 6개 테스트 통과. 기본/EGL strict Clippy, fmt, Python 호출부 10개 테스트와 Ruff 통과. Params는 현재 C++ 원본을 동적으로 빌드하여 367개 키와 잠금·복사·정리 동작을 비교했다. 기존 Python 잠금 시간초과의 무한 대기만 명시적으로 소유 자식 종료·회수로 보완했다.
- `464309e0`: 원본 명령 31개 사례, 관련 테스트 47개, strict Clippy와 실제 Miri sysroot 비교 통과. 읽기 불가능한 경로 접두부와 삭제된 현재 디렉터리의 원본 동작도 검토 후 보존했다. 실제 출력 SHA는 `1d88caa42ab506cdb3c5d1d7137837d5f70b4b35cd5f5a5115e2de2dc1f7e528`.
- `17823057`: catalog 테스트 4개 + workflow isolation 32개와 strict Clippy 통과. x86_64/aarch64 원본 메타데이터 캡처를 보존했다. `Catalog::load` 검증 후 ID를 출력하며, 기존 `--check-catalog` 출력은 유지한다. 검증 뒤 파일이 변조되면 재검증에서 거부한다.
- `2fbf969f`: `image_array_math::compare`의 독립 NumPy 2.5.3 캡처 97개(성공 91, 오류 6), strict Clippy, 독립 검토 통과. broadcasting, 마지막 축의 any/sum, i64 wrapping subtraction/abs 및 `abs(MIN)==MIN`을 보존한다. 이 함수만으로 이미지 디코더나 Python 검사기가 대체된 것은 아니다.

상세 결과는 `docs/naver/rust-repository-python-retirement-296-evidence.md`, 작업 범위는 `...-plan.md`, 전체 목록은 `...-inventory.tsv`, 내장·동적 실행 경로는 `...-execution-edges.tsv`에 있다.

## 4 부모와 로컬 워커 운영 규칙

이번 사용자의 최신 선호는 **부모 Sol 또는 Astra + 로컬 모델 워커**다. 이전 지침의 모든 워커 GPT-6.1 Sol 요구는 이 재개 방식에 대해 최신 사용자 요청으로 대체한다. 실제 로컬 모델 이름·API 주소·에이전트 프로그램 설정은 아직 지정하거나 변경하지 않았다.

- 부모: 범위·브랜치·통합·검토·Cargo 소유권·Git/PR/CI·문서 관리.
- 워커 A: 모델 LLVM provider 및 IMAGE 배치/스케줄.
- 워커 B: polygon 원본 참조 도구의 정책·raw raylib ABI·오류/경로 검증.
- 워커 C: PIL→NumPy 이미지 배열 디코더. 먼저 작은 작업 단위로 로컬 워커의 검증 품질을 확인한다.
- 워커들은 지정 파일만 수정한다. 공유 Cargo.toml/lock/lib.rs, workflow, 파일 삭제, 스테이징·커밋·push는 부모가 관리한다.
- **Cargo 빌드는 한 번에 한 워커만** 실행한다. 공용 target cache를 쓰므로 실행 전 소유권을 요청하고 완료 후 넘긴다. 부모는 대형 작업을 중복 실행하지 않는다.
- 리뷰 결과와 원본 캡처가 기대값이다. 교체 구현으로 기대값을 다시 만들어 통과시키거나 기존 양성 테스트·허용 오차를 줄이지 않는다.
- fixed-input golden 검사는 독립 원본·실제 dependency drift guard·고정 입력 한계를 명시한다. generic helper를 canned response로 바꾸지 않는다.
- Python을 실행하는 Rust 래퍼, 옛 소스 다운로드 실행, 알려진 지원 형식 삭제는 전환 완료가 아니다.

### 새 부모에게 붙여 넣을 시작 지시문

```text
/home/fma1/.codex/worktrees/native-build-tools-296/openpilot-rust 에서 작업한다.
docs/naver/rust-repository-python-retirement-296-handoff.md를 먼저 읽고 AGENTS.md,
개발 workflow, plan/evidence/inventory/execution-edges를 확인하라.
사용자는 저장소 전체 개발·빌드·검증·모델·vendor 기능의 Rust 전환과 검증 후
대체 Python 제거를 요청했다. dev-md 폴더와 작업은 분리한다.
기존 미커밋/미추적 소스와 ignored scratch를 보존하고 재개한다.
부모는 Sol 또는 Astra, 워커는 사용자가 설정한 로컬 모델을 사용한다.
한 번에 하나의 Cargo 빌드만 실행하며 부모만 공유 파일/Git/CI를 통합한다.
먼저 현재 git 상태와 a077b0d2 CI 최종 결과를 확인하고, 알려진 실패/미검증
변경부터 작은 단위로 재현·수정하라. 모델 IMAGE init은 현재 잘못된 부분이
있으므로 일괄 커밋하지 않는다. 부분 검증을 전체 완료로 보고하지 않는다.
```

### 로컬 워커에게 붙여 넣을 공통 지시문

```text
지정 worktree와 지정 파일에서만 일한다. dev-md 및 다른 워커 파일은 수정하지 않는다.
착수 전에 담당 파일, 원본 근거, 현재 실패, 완료 판정을 짧게 보고한다.
Cargo를 실행하려면 부모에게 소유권을 받아라. 모든 빌드 전에 디스크 여유를 확인한다.
원본 캡처와 기존 테스트를 보존하고 검증을 약화하지 않는다.
Git stage/commit/push, 원본 Python 삭제, workflow/공유 manifest 수정은 부모에게 맡긴다.
보고에는 실제 명령·종료 결과·로그 경로·아직 검증하지 않은 변경을 구분한다.
중단 요청을 받으면 새 작업을 시작하지 말고 파일과 현재 상태를 보존한다.
```

## 5 미커밋 워커 A 모델 컴파일러

담당 파일은 `rust/crates/model-runtime/` 아래:

- `src/compiler_backend_cpu.rs`, `src/compiler_backend_cpu/{contraction,llvm,image}.rs`
- `src/compiler_onnx/{lower,render}.rs`, `src/compiler_onnx/render/{math,reduction}.rs`
- `src/bin/offline-llvm-compile.rs`, 부모가 추가한 Cargo bin 등록
- `tests/compiler_{contraction,norm,llvm}.rs`, 기존 `compiler_storage_alias.rs`의 사례명 진단 개선
- `tests/fixtures/compiler/README.md`, `original-{mulacc-f32,norm,driving-contraction21,image-muladd-barrier,image-muladd-wide-pending,image-packed,llvm-provider}.json`
- `native-rendered-a630-proof.json` 수정도 미커밋이며 최종 source-matched 재검증이 필요하다.

### 이미 얻은 제한된 결과

직접 입력 scalar norm, raw Sqrt, 일부 Mul/Add contraction을 구현하고 독립 원본 비교를 확장했다. 일반 contraction 290개, IMAGE 160개, norm 435개 기록이 있다. 기존 storage alias 양성 36개를 삭제하지 않았다. 이전에 허용되던 driving prefix 109/115의 불일치를 수정한 중간 결과와 실제 prefix21 비교도 보존되어 있다. 이 결과는 아래 후속 발견 때문에 최종 전체 통과로 간주할 수 없다. 일반 normalization과 전체 compiler parity는 미완료다.

### 현재 핵심 실패와 올바른 재개 방향

`render/reduction.rs`의 aligned Conv first-product 초기화 규칙은 일부 크기에서 signed zero가 다르다. 이 미커밋 규칙을 그대로 확정하지 않는다.

원본 LLVM IR은 +0 초기 accumulator와 `nsz arcp contract afn`을 사용한다. 실제 원인은 IMAGE 메모리 배치·stride에 따른 reduction 축 병합 및 LLVM unroll/SLP다.

- `cin_last`(iy=ix=1) weight layout `[cout/4,cin/4,outlane4,inlane4]`에서는 stride가 달라 reduction 축이 합쳐지지 않는다. inner4만 unroll하고 cin/4 loop를 남긴다.
- 다른 layout `[cout/4,cin/4,inlane4,outlane4]`에서는 축 병합 후 원본 unroll 규칙이 적용된다.
- CPU/features와 LLVM 버전이 바뀌면 zero/NaN 결과도 달라질 수 있다. 크기별 예외 목록으로 맞추지 말고 원본 배치·축 병합·루프 생성 알고리즘을 포팅한다.

### LLVM provider

`compiler_backend_cpu::llvm`와 `offline-llvm-compile`은 기존 libloading으로 별도 프로세스에서 컴파일한다. library SHA, triple, CPU, features, passes와 unroll/vector/SLP/verify 옵션을 명시한다. private output, 실패 시 소유 파일 정리, receipt/hash 검증, timeout/crash containment를 구현했다.

- LLVM20.1과 LLVM18 각각 provider 테스트 5개 통과. 각 버전의 원본 object/optimized IR 3개와 정확히 일치했다.
- strict provider/library/bin/test Clippy와 독립 read-only 검토 통과.
- 테스트 터미널 출력은 파일로 저장되지 않았다. 재개 시 다시 실행해 로그를 남긴다.
- provider는 아직 IMAGE 스케줄 생성이나 모델 admission을 완성하지 않는다.
- **가장 마지막 변경**: `image.rs`의 fail-closed `Image1x1::render` stub과 모듈 등록, `original-image-packed.json`을 추가했다. 이 추가 뒤 빌드·테스트는 하지 않았다. packed-layout TDD는 시작하지 않았다.
- packed fixture는 두 LLVM 버전의 materialized-input 72개 기록과 scheduler 관측 12개다. whole-export 검증으로 해석하지 않는다.

LLVM18 재현 정보:

```text
Ubuntu package libllvm18:amd64 = 1:18.1.3-1ubuntu1
pool/main/l/llvm-toolchain-18/libllvm18_18.1.3-1ubuntu1_amd64.deb
package bytes: 27518500
package SHA256: af6197096ce3b89aab86de59f01c1b5feb008b2ae1514a03906c0664edb8b0b1
library: /usr/lib/llvm-18/lib/libLLVM.so.1
library SHA256: 5c3a63957e28c15cdf1503892074f032d29b72e36a9824a77dfa87e48a0dafb0
```

필수 CI 연결은 아직 안 됐다. 현재 workflow의 LLVM_PATH는 LLVM18이다. exact pinned package를 provision하고 테스트를 필수로 실행해야 한다. 라이브러리가 없을 때 조용히 skip하지 않는다. znver4 object 비교는 다른 CPU에서 실행하지 않고 object-only로 검증한다. 실제 추론은 해당 host/target에 맞는 명시적 설정을 사용한다.

자료: `.analysis/scratch/2026-10-15-native-compiler-launch/prefix-contexts/`

- `image-source-schedules.json`
- `image-optimized-context.json`, `image-optimized-context18.json`
- `original-image-packed-generic18.json`, `original-image-packed-generic20.json`
- `ImageMulAdd-wide-8-1-False-negativezero.ll` 및 C12/W1, C12/W3의 `.ll/.opt.ll/.asm/.o/.jit.bin`

다음 순서: 마지막 stub 상태 확인 → native packed layout/stride merging/LLVM emitter TDD → 동일 버전·동일 target의 원본 비교 → 기존 양성/actual21/109/115 유지 → 전체 model suite/Clippy와 새 A630 proof → 독립 검토 → CI 연결. 그 전 모델 Python을 제거하지 않는다.

## 6 미커밋 워커 B polygon 참조 도구

주요 파일:

- `rust/crates/dev-tools/src/ui_polygon_reference.rs`, `src/bin/ui_polygon_reference.rs`
- `rust/crates/dev-tools/tests/ui_polygon_reference.rs`, `tests/fixtures/ui-polygon/`
- `rust/crates/startup-ui/native/polygon_reference.{h,cc}`, `src/polygon_reference_abi.rs`
- startup-ui `build.rs`, 기존 audited `bridge.rs`와 `lib.rs`의 제한된 추가
- 부모가 dev-tools Cargo/lib에 `polygon-reference` feature와 gated bin을 등록했다.

후보 renderer/정책을 재사용하지 않는 독립 Rust 참조이며 C++는 raw raylib ABI다. 추가 blanket unsafe lint 허용은 없다.

검증 이력과 현재 상태를 구별한다.

- 초기 50개 실제 원본/네이티브 프로세스의 PNG·JSON state·exit/artifact 결과 일치. PATH=/nonexistent 확인. 당시 feature build/Clippy 통과.
- **그 50개 proof는 현재 최종 소스 proof가 아니다.** 이후 검토 수정과 사례가 추가됐다.
- 마지막 feature 테스트에서 live helper/shader/lock pin, raw ABI 음성, PYTHONWARNINGS 12개 검사는 통과했다.
- 76개 scene 검사 중 `overflow-then-bad-point`만 실패했다. NumPy는 뒤의 정수 변환이 실패해도 앞의 overflow warning을 출력한다.
- 마지막 소스 변경은 shape를 먼저 검증하고, 순서대로 숫자를 변환하며 오류가 나도 overflow flag를 전달하도록 고쳤다. **rustfmt만 했고 아직 빌드/테스트/Clippy하지 않았다.**

남은 문제:

1. 위 마지막 수정 재검증.
2. 실패 시 Gradient stdout warning의 순서: 원본 Python은 버퍼링하여 traceback 뒤에 출력한다. 네이티브 CLI는 현재 render 중 먼저 출력한다. 실패 시 진단 뒤에 buffer를 출력하도록 수정하되 가짜 Python traceback을 만들지 않는다.
3. 원본 JSON recursion limit과 NumPy 64차원 경계의 독립 캡처/guard 추가.
4. 최신 binary로 실제 76개, raw/alias/trailing-slash 등 경로 14개, warning 12개 비교를 다시 수행.

numeric whitespace/Unicode decimal/underscore, 큰 정수와 string Infinity 구별, 중첩 좌표와 unknown mode, stop None 거부, lexical Path 정규화, helper live pin 등은 마지막 테스트 빌드까지 반영되어 있었다. 오류 메시지는 native 진단이며 Python traceback 전체 바이트 동일성을 주장하지 않는다.

자료: `.analysis/scratch/2026-10-16-ui-polygon/`

- `capture.py`, `reference-original.json` — 현재 76개, 성공 46개
- `capture_traces.py`, `trace_source.py`; 로그의 `PASS30` 문구는 오래됐고 실제 fixture는 76개
- `review-green.log` — 이름과 달리 마지막 scene 하나 실패한 실행
- `actual_reference.py`, `actual-reference-proof.json` — 이전 50개 snapshot만 검증
- `path_capture.py`, `path-original.json` 14개; 최종 native path proof 미완료
- `warning-original.json`, `warning-red.json`, `raw-source-original.json`
- `checker-original.json`, `checker-blobs/`, `checker_capture.py` — 원본 전체 검사기 32개
- private Xvfb와 필요한 라이브러리는 `host/`, `debs/` 및 실행 스크립트를 확인한다. 소유 Xvfb PID1806479는 중단 시 종료했다. 재개하면 새 소유 display를 띄운다.

`rust/tools/check_ui_polygon.py`, `rust/tools/ui_qa/polygon_source.py`, runtime `shader_polygon.py`는 아직 제거하지 않았다. native checker 자체는 아직 없다. 원본 framework는 13개 gate(7 native +6 Python)이며 orchestration만 Rust로 감싸는 작업은 하지 않았다.

## 7 워커 C 이미지 디코더 프로토타입

**저장소 production decoder 파일이나 dependency edge는 아직 없다.** 구현은 ignored scratch에 있다.

```text
.analysis/scratch/2026-10-16-image-array/
  prototype/Cargo.toml, Cargo.lock, src/main.rs, src/legacy.rs
  capture.py, original.json, inputs/
  compare.py, prototype-comparison.json
  capture_headers.py, header-original.json
  jpeg_decode.c, jpeg-decode, jpeg-comparison.json
  capture_jpeg_errors.py, jpeg-errors-original.json
  libjpeg-release.json, libjpeg-turbo-3.1.4.1.tar.gz, jpeg-headers/
```

원본은 Pillow12.3.0 / NumPy2.5.3 / Python3.12.15다. 182 mode×format 시도 중 decode 가능한 93개 배열의 mode·shape·i64값을 캡처했다.

- 마지막 빌드된 Rust prototype: **88/93** 일치. 4 JPEG와 1 RGB-ICNS가 남았다.
- 독립 native libjpeg probe는 유효 JPEG 6개 모두 일치(CMYK 포함). 아직 Rust 통합 전이므로 이를 합쳐 93/93이라고 하지 않는다. 통합하면 예상 92/93이지만 미검증이다.
- 마지막 **미빌드 수정**: ICNS의 initial RGBA→loaded RGB `asarray` byte-layout 특이 동작, JPEG EOF warning을 원본처럼 실패 처리하는 수정.
- JPEG malformed/truncated 12개 원본 캡처가 있다. 이전 native probe가 원본 오류 5개를 수용했으므로 EOF 수정은 반드시 재검증한다.
- source checker는 확장자가 .png여도 TIFF/TGA/DDS/SGI/IM 등을 수용한다. palette GIF와 16-bit PNG broadcasting도 성공한다. RGBA 변환/PNG 전용 decoder로 바꾸면 기능이 달라진다.

API 방향: `ImageArray { mode, shape: Vec<usize>, values: Vec<i64> }`; 수학 비교는 이미 커밋된 `image_array_math::compare`에 위임한다.

scratch dependency: png0.17.16; gif0.14.2(std, default off); tiff0.11.3(deflate/jpeg/lzw, default off); image0.25.10(bmp/dds/tga/jpeg/pnm/webp/ico, default off); serde_json1.0.145. isolated lock 43개이며 AVIF/rayon/EXR은 없다. image/ico가 png0.18.1을 추가로 가져온다. 필요한 dependency가 검증되면 부모가 production manifest를 변경한다.

Pillow pixel threshold의 header-only 7개 캡처: 89,478,485까지 warning 없음, 89,478,486~178,956,970 warning, 그 초과 error. 0 dimension 거부. 임의로 더 작은 codec 기본 제한을 유지하지 말고 원본 경고/거부와 fallible allocation을 맞춘다. 큰 이미지를 실제 할당하지 않았다.

JPEG는 zune/image decoder 픽셀과 달라 원본과 맞는 native libjpeg가 필요하다.

- 기존 read-only native library: `/home/fma1/rust/openpilot-rust/.venv/lib/python3.12/site-packages/pillow.libs/libjpeg-31e2ca52.so.62.4.0`
- ABI62 / libjpeg-turbo3.1.4.1. library SHA는 아직 별도 기록하지 않았다.
- 공식 source archive SHA256: `ecae8008e2cc9ade2f2c1bb9d5e6d4fb73e7c433866a056bd82980741571a022`, GitHub asset digest와 대조했다.
- 같은 source headers와 생성한 ABI62/8-bit/MEM_SRCDST jconfig를 사용한 C probe가 `-Wall -Wextra -Werror`로 빌드되었다.
- setjmp는 C 경계 안에 두고 cleanup state는 heap 소유. Rust/C++ RAII 경계를 longjmp로 넘지 않는다. mode 정책은 Rust에 둔다.
- native source/dependency 작업은 사용자 전체 전환 범위 안에서 승인되었다. primary venv를 수정하지 말고 별도 cache에서 pin·license·빌드 재현성을 보존한다.

다음 순서: 미빌드 patch 재빌드 → 93개 배열 + JPEG 오류12개 재비교 → JPEG 통합 → malformed/planar TIFF/palette/SGI RLE/IM header 등 음성 검증 → source pin/license/threshold/경로·오류 정책 → production module/검사기 통합 → 독립 검토. 전체 PIL plugin 동등성과 polygon checker 제거는 아직 미완료다.

## 8 실행 환경과 빌드 명령

매번 실제 worktree에서 실행한다. env.sh가 primary root와 다른 Cargo cache를 설정하므로 아래 override가 필수다.

```bash
cd /home/fma1/.codex/worktrees/native-build-tools-296/openpilot-rust
source /home/fma1/.config/openpilot-rust/env.sh
export OPENPILOT_RUST_ROOT="$PWD"
export PYTHONDONTWRITEBYTECODE=1
export PYTHONPATH="$PWD:$PWD/opendbc_repo:$PWD/tinygrad_repo:$PWD/rust/tools"
export CARGO_TARGET_DIR=/home/fma1/rust/cache/cargo-target-retirement-296
export CARGO_INCREMENTAL=0 CARGO_BUILD_JOBS=2
```

빌드·dependency 설치·큰 복사 전에 25GiB + 예상 증가분을 확보한다. 부족하면 새 빌드를 멈추고 35GiB 이상 복구 후 재개한다. 중단 시 약270GiB 여유였다.

```bash
/home/fma1/.codex/worktrees/runtime-retirement-296/openpilot-rust/.analysis/scratch/2026-10-14-agnos296/host-tools-430ec72b/check-disk-space --path /home/fma1/rust/cache --growth-mib 512
```

필요한 작업만, Cargo 소유권을 받은 뒤 실행한다.

```bash
LLVM_PATH=/usr/lib/llvm-18/lib/libLLVM.so.1 cargo test --manifest-path rust/Cargo.toml -p openpilot-model-runtime --test compiler_llvm --locked
cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --features polygon-reference --test ui_polygon_reference --locked
cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --test image_array_math --locked
```

원본 캡처용 Python은 `/home/fma1/rust/openpilot-rust/.venv/bin/python -B`를 read-only로 사용한다. primary venv에 설치·수정하지 않는다. 최종 native 실행 검증은 Python 없는 PATH로 한다. 장치 테스트/실주행 결과와 Linux host 결과를 혼동하지 않는다.

## 9 파일 보존과 재개 체크리스트

복구용 snapshot은 active worktree의 아래 ignored 폴더에 있다.

```text
.analysis/handoff/2026-10-16-repository-python-retirement/
  git-status.txt, git-log.txt, untracked-paths.nul
  tracked-wip.patch
  untracked-source.tar.gz
  current-ui-and-codec-scratch.tar.gz
  SHA256SUMS
```

현재 파일이 원본이며 snapshot은 실수로 잃었을 때의 복구용이다. 현재 WIP 위에 자동으로 patch/archive를 덮어쓰지 않는다. 모델의 큰 scratch 전체는 archive하지 않았으므로 `.analysis/scratch/2026-10-15-native-compiler-launch/`도 그대로 유지한다. 다른 컴퓨터로 옮길 때는 local-only 커밋·untracked 소스·필수 scratch까지 함께 이전해야 한다. GitHub clone만으로는 현재 상태를 복구할 수 없다.

재개 순서:

1. 경로·브랜치·Git 상태와 이 문서의 snapshot을 확인한다.
2. 원격 `a077b0d2` CI의 최종 상태를 확인하고 exact SHA로 기록한다.
3. 전체 uncommitted 상태가 GREEN이라고 가정하지 않는다. polygon 마지막 warning 수정과 모델 image stub을 먼저 확인한다.
4. 워커별 파일 소유권과 단일 Cargo 소유권을 설정한다.
5. 각 작은 단위의 원본 비교·음성·최신 binary proof·review가 끝나면 부모가 통합한다.
6. Python 제거는 실제 호출부 이관과 검증 후 수행한다. 인벤토리와 실행 경로를 함께 갱신한다.
7. push 전에 선택한 변경만 stage하고 dev-md 혼입을 확인한다. PR #343과 #296에 exact SHA, CI URL, 실패·남은 작업을 기록한다.
8. 저장소 전체 기능과 Python 실행 경로 제거가 끝나기 전에는 이슈를 닫거나 전체 완료라고 보고하지 않는다.

이 문서는 인수인계를 위한 개발 기록이다. 사용자 설정·사용자 가이드 변경은 없다.

## #296 local continuation checkpoint (2026-10-19)

`4e98435a` commits the three tombstone live-tool retirements, their native CI
callers, artifact-mutation rejection and late-cancellation receipt invalidation.
The final affected suites pass8 runtime/3 shutdown tests with strict Clippy;
the unchanged policy/safety gates make75 tests across that retirement family.
It is a local commit pending publication with the subsequent managed-entry
batch. Original ecfb7774 workspace/ARM jobs were allowed to finish rather than
being cancelled by a premature new push.
