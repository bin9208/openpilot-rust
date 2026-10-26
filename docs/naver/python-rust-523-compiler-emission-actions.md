# #523 실제 offline emitter의 genuine compiler 집중 검증

2026-10-26. 새 `python-rust-523-compiler-emission.yml`은 작업 브랜치의 해당5개 source/WF push와 manual dispatch로 실행하며 아직 UNRUN이다.
일반 native 단위는 사용자 제공4core/4GiB/80GiB 러너에서 jobs1/debug0으로 계속 실행한다.
여기서는 genuine ARM A630 compiler와 cross SDK가 필요해 ubuntu-24.04의 별도 집중 job을 쓴다.

main `rust.yml` SHA `b51def7342359305c818f15ec59e9002d6b7c1ee73752dc08e91442444cfface`의
기존 owned source·local action alias·private Python/Rust homes와 upload/cleanup 계약을 재사용한다.
main 전체 workspace gate와 같다는 주장은 하지 않는다. 원래 QCOM compiler stager·고정 AGNOS
`ec1cf237a84565a056dbbc6f433b1b1c20c07a2c`의 libllvm-qcom.so·QEMU/sysroot argv를 유지한다.
동일한 ARM std target 설치를 SDK 봉인 이전으로 옮겼다. SDK는 job의 private1.94 host/ARM
파일·roster 전후를 비교하고 source seal 성공에 결합해 업로드 뒤 소유 경로만 정리한다.

source는 기존2GiB 사전 admission을 유지한다. Python/host·ARM SDK 설치보다 앞서 source/job/
root의 실제 filesystem 각각에 남은 SDK/target/registry/apt/provider/proof 성장12GiB를
admit한다. 프로젝트 Cargo bootstrap을 실행해야 admission하는 순환을 피하려고 Bash df만 쓴다. 이후 기존 source/genuine provider와 Cargo 작업의 실제 파일시스템에
1/2GiB 증가량을 각각 다시 확인한다. 과거25/35GiB 고정 중단 기준은 쓰지 않는다.
Cargo jobs2/incremental0/debug0, 각 validation12분/step35분/job60분이다. 모델·GPU inference를
실행하지 않는 작은16-float 혼합 schedule의 검증이며 peak RSS/전체 모델 fit 측정은 아니다.

일반 v1/v2·ordered stream 계약10개·ignored2개 중 COPY1과 genuine 혼합1을 각각 정확 실행하고 strict Clippy를
요구한다. input/output 캡처는 기존 source test의1MiB/record·8MiB 합계다. baseline 원본
`offline_schedule.py`는 source7a928의 Git file을 별도 text로 읽고 SHA
`bc1b7b572c8dd2f84f3ea0ca38b1744e4c7120155c33f3dabb633eabaf7f0144`와 대조한다.
원본 AST prefix가 먼저 graph/weights/kernel을 계산하고 candidate native ordered stream 결과와 비교한다.
새 native 값으로 expected를 만들지 않는다. 실제 canonical Python path/SHA/version도 결합한다.

첫 원격 실행 전 source test의 최종 개수/환경·frozen stage73 연결·launch9/독립4411 경계·원래
Python launch unittest 퇴역과 source 부재 검사를 닫아야 한다. 전체 original/native 모델 수치,
QCOM 모델 전체 export/runtime, 설치·장치·대단원 수용은 이 집중 gate와 별도다.
로컬은 YAML/Bash syntax와 source/diff만 확인하며 실제 build·test·compiler·GHA는 미실행이다.


부모 source 통합은 기존 private source alias roster를 새 실제 job 하나만큼78→79로,
ARM24는 그대로 유지한다. source/alias 정책 함수나 finalizer 조건을 완화하지 않는다.
기존 native-units의 선택 가능한 compiler-export도 일반10/ignored2로 맞추고 COPY exact1만
작은 러너에서 실행한다. genuine mixed/order7은 이 hosted job의 exact selector에서만 실행한다.


## 첫 source581 실행과 provider pathname 보완

[40242309747/job121316531016](https://github.com/bin9208/openpilot-rust/actions/runs/40242309747/job/121316531016)은
source581c84520에서 dependency 준비 단계가 실패했다. apt와 NumPy2.5.3 설치는 성공한 뒤
`readlink -e /usr/lib/x86_64-linux-gnu/libLLVM-18.so.1`가 존재하지 않는 pathname을 거부했다.
기존 실제 currenttriplet WF는 같은 package의 `libLLVM-18.so`를 canonicalize한다. 이 정확한
기존 경로로 literal 하나를 맞추고 resolved regular file의 SHA/후봉인 조건은 그대로 둔다.

ARM provider staging·native10/COPY/mixed/order7은 모두 미도달이다. 후속 functional upload의
파일 없음과 SDK baseline 미생성에 따른 source seal 실패는 최초 원인이 아니며 성공으로
바꾸지 않는다. private artifact12616099750은292540B/SHA256
`ea18b599054d939c03b2041c06da481d3e15d61be4de86e0f38623142cff11bd`다.
이 최초 raw log/Actions 증거를 보존하며 새 source actual 수용을 계속 확인한다.


## 두 번째 source398의 ARM linker 개발 파일 누락

[40242668812/job121317637421](https://github.com/bin9208/openpilot-rust/actions/runs/40242668812/job/121317637421)은
source39800e064에서 LLVM 확인/SDK baseline 뒤 ARM cross-link가
`cannot find Scrt1.o`, `cannot find crti.o`, `cannot find -ldl`로 exit101 종료됐다.
`--no-install-recommends`에서 libc6-dev-arm64-cross가 추천 목록에만 남은 것이 실제 원인이다.
APT 목록에 이 개발 패키지만 명시 추가하고 원래 provider/기한·SDK 봉인·strict 컴파일 조건을
유지한다. native10/COPY/mixed/order7/Clippy는 이 실행에서도 미도달이다.

privateartifact12617945264는327,587B/SHA256
`dc18528dca3015120c6e98112944663afeaa189925943f4ab88b3d1bcc375b0d`,
20 safe unique regular member/전체CRC, expanded2,556,115B다. source/index전후동일,
SDK227파일/roster252 전후 byte 동일과 LLVM후hash가 성공했다. 비교artifact 미생성과
cleanup skipped는 그대로 보존한다. 이 수정의 실제 linker/후속검사는 다음 source로 확인한다.


## source602의 실제 native/genuine 비교와 마지막 lint

[40244158549/job121322096387](https://github.com/bin9208/openpilot-rust/actions/runs/40244158549/job/121322096387),
source `602040b066e37487feb0595a09c8460023bf755e`는 ARM compiler build/stage와
native10/ignored2·COPY exact1·genuine mixed exact1이 모두 성공했다. mixed/order7의
원본/후보 lowering·compiler count는 각각0,0,1,1,0,0,0이며 음성5건의 오류 우선순위·
destination 부재와 lazy 양성2건이 일치한다. mixed의88.64초는 전체 작은 검사의 시간이며
모델 성능 측정이 아니다. 원본prefix native 호출0, 미래compiler진입=false를 원문에서 확인했다.

마지막 strict Clippy가 CLI의 match arm에 남은 명시적 `()`3개를 `clippy::unused_unit`으로
거부해 전체 job은 failure다. 동일 세 경고가 current-model602의 선행 Clippy도 막았으므로
그 모델 run의15/14/A/B/C는 미실행이다. 세 trailing unit만 빈 block으로 정리하며 frame/
기한·compiler·정책·기대값·수치 조건을 바꾸지 않는다. 최종 Clippy 수용은 다음 source로 확인한다.

functionalartifact12618236836은16,493B/SHA256
`5f0837cc8c8f6cc8beabe4473a87aaa58047e26dec1a2c243d12b8e3a72a4866`,
22 safe unique regular member/전체CRC다. privateartifact12617413226은337,041B/SHA256
`01f5c413dc9972ded5983b696620d9a5fc204a94789f09a01764750d408caac6`,
32 safe unique regular member/전체CRC다. source/index는 historical19,596개와 일치하고
SDK227파일/roster252 전후 byte 동일, 두 upload/source/common cleanup도 실제 성공했다.
원본 SHA·historical bytes와 weights64B·Plan의 allocations/views/entrypoints를 직접 대조했다.
whole_model_accepted=false/retirement_credit0을 유지하며 Python file 전체 퇴역으로 세지 않는다.


## sourceef172 첫 시도: APT 인프라 timeout

[run40245597111/job121326368969](https://github.com/bin9208/openpilot-rust/actions/runs/40245597111/job/121326368969)은
source `ef17256300d8caf51d5b0a9b3a900fd2c9615d01`에서 host dependency 설치의12분 timeout으로
실패했다. 마지막 관측은 Azure HTTP mirror의 `gcc-13-aarch64-linux-gnu`21.1MB 다운로드다.
native10/COPY/mixed/order7/Clippy는 모두 미실행이며 비교 artifact는 없다. SDK 변수가 아직
등록되지 않아 후검사도 실패했고 cleanup 두 단계는 skipped다. source/index 전후와 historical
index19,596개는 같았다. 이 실패를 emitter 기능 실패나 수정 Clippy 성공으로 바꾸지 않는다.

private artifact12619240040은292547B, SHA256
`e9e0c8a1949c5717e87df40f230a27ab682de085f1b73b55be0c97bd9401844f`,
13 safe unique regular entry/CRC 정상, expanded2359107B다. SDK/order 실행 증거는 없다.
source602와 비교해 Python/core/test/WF는 byte 동일하며 CLI는 세`()→빈 arm`과 rustfmt 외
차이가 없다. 새 CLI SHA는 `6e3e00ae77a3d275c3aa0e3a664a5baa2a0516d430bdb41911587daf88ce4ec6`다.
source602의 기능 성공은 유지하되 수정 source의 Clippy 포함 최종 수용은 pending이다.

인프라 실패에 한정해 같은 run의 실패 job만 attempt2로 재시도했다. 새 job은121329475106이며
기능 결과를 미리 수용하지 않는다. 이미 성공한 source602 기능을 로컬에서 반복하지 않았다.


## sourceef172 attempt2: 실제 native ordered emitter 경계 수용

같은 source의 [run40245597111/attempt2/job121329475106](https://github.com/bin9208/openpilot-rust/actions/runs/40245597111/job/121329475106)이
최종 SUCCESS다. APT/provider 준비 뒤 native v1/v2 일반10/ignored2, 실제 COPY1,
genuine mixed1/order7 및 strict Clippy가 모두 exit0이다. mixed의47.68초는 테스트 실행 시간이며
모델 성능값이 아니다. 실제 Python은3.12.15로 실행물 SHA
`53dab24bf9e0e9c490088a53d71770b6f027b057678b313648e75f7fc87a675e`에 결합했다.
앞선 attempt1의 APT 실패와 이 실제 성공을 구분한다.

| 실제 artifact | bytes·SHA256 | 비실행 archive 감사 |
| --- | --- | --- |
| native compiler comparison12618734936 |16141B / `7cb79be1b73e37e070b9eac3307c18416b22f592df3552d9839efb833f17549d` |22 safe unique regular/CRC·expanded41324B |
| private source/SDK/proof12618889731 |336677B / `27a195daaaee8a418fae2aad68da0c2586a5ebefb53f5daed0f16f76c7987f96` |32 safe unique regular/CRC·expanded2576927B |

API source/run/attempt2 name·expected bytes/digest와 실제 ZIP을 대조했다. 경로 이탈·중복·
symlink·special·암호화가 없고 전체 CRC가 정상이다. 추출·native/oracle/모델 실행은 하지 않았다.
comparison의 passed=true/independent_python_prefix=true/original_native_calls0이며 원래
16463B source SHA `bc1b7b572c8dd2f84f3ea0ca38b1744e4c7120155c33f3dabb633eabaf7f0144`를 확인했다.
원본을 native Plan에서 복원하지 않았다.

original-graph와 candidate-plan의 allocations3/views3/entrypoints4를 직접 대조해 같았다.
원본 weights64B도 candidate의 hex bytes 및 양쪽 SHA
`58dda328598e2f7fe472621bfc54935aaa354d1a6ebcaf9562cd743fd575eb19`와 일치했다.
graph calls3/kernels1과 native launches2를 실제 확인했다. 원본-first7개 case의 lowering/compiler
횟수는 양쪽 모두 `[0,0,1,1,0,0,0]`, 앞의 실패5개는 destination 부재다. unused/write-covered의
원래 prefix는 파일을 쓰지 않고 실제 후보만 destination을 생성한다는 차이도 그대로다.
future_compiler_after_failure=false/whole_model_accepted=false/retirement_credit0이다.

실제 pinned A630 QCOM compiler SHA `fb7e6390cc25700d6935b2eef3acad85a43f247206bdf84cb4d37bd48a60b093`,
AArch64 frontend SHA `a8f1654e93f2bcaf4a5fa17edd601aadf2f9d7dda50a0a975a24e4a7d4096cc1`,
loader/DT_NEEDED closure·qemu SHA가 provenance에 결합됐고 GPU execution은 false다.
원래 ARM cross startup 객체를 포함한 준비도 성공했다. 이 증거는 실제 source→compile/export
경계이며 GPU/기기 실행이나 전체 모델의 자원 적합성 수용이 아니다.

index19596개와 source SHA, complete host+ARM SDK의227파일/252-roster가 전후 byte 동일하다.
단일 exact-source job API에서 비교 artifact upload, source/index 후봉인, private evidence upload,
owned source/alias/target 정리 및 common temp 정리가 모두 SUCCESS임을 확인했다.
comparison.json SHA는 `00cfaf07d864c9ee54eeafffd0b189e34a5e308152b15b08fdffea4aba6adde0`,
error-order.json은 `39e67c7a598032604008f03ee2be99952026c57c6668eacc1041b993ce52a153`다.

현재 Python caller는 Tensor/UOp lowering·constant preparation·memory planner/render/metadata/
최종 writer를 계속 담당한다. 이번 수용은 native ordered allocation/launch/stage 경계에 한정하고
production Python 파일 삭제나 전체 F02 credit을 추가하지 않는다. 기준3580의300/3/3277은 유지한다.
