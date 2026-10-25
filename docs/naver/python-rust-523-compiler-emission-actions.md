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
