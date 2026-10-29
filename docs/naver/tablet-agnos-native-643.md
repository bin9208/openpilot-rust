# ARM 태블릿 AGNOS 빌드 비교 (#643)

2026-10-30 KST 사용자 요청으로 `bin-Arm-tablet`(runner ID25)에 공통
`openpilot-rust` 라벨을 추가하고 API로 확인했다. 등록 라벨은 `self-hosted`,
`Linux`, `ARM64`, `openpilot-rust`다. ARM 작업의 선택자는
`[self-hosted, Linux, ARM64, openpilot-rust]`이며 X64 작업에 ARM을 섞지 않는다.

사용자는 1job 비교 재실행을 금지했다. 실제 프로세스 CPU affinity 수를 Cargo/test
병렬도로 사용한다. CPU가 하나만 허용되면 1job으로 실행하지 않고 원인을 보고한다.

사용자가 지정한 비교 대상은 `bin-actions3`의
[AGNOS 실행40904508393](https://github.com/bin9208/openpilot-rust/actions/runs/40904508393),
job123556239177, source `9b8c6337b56b80b79788ebc389b6622b484fe0c7`이다.
이 실행은 약3시간 후 bootstrap 검증에서 실패했으므로 완료 빌드 시간으로 취급하지 않는다.
#608 logger 개별 시험이나 #642 합성 workspace로 대체하지 않는다.

태블릿은 Android 위 PRoot Ubuntu 환경이다. 같은 AGNOS19.8-carrot-bt1 파일시스템을
QEMU 없이 실행할 수 있는지 먼저 실제 Actions에서 확인한다. 기존 원본의 kernel
resource guard와 ABI 판정을 완화하거나 Ubuntu에서의 일반 Rust 빌드를 AGNOS 성공으로
표시하지 않는다. 원래 소스·실행 중인 다른 runner 작업은 변경하지 않는다.

첫 확인은 외부 Actions 없이 공통 ARM selector로 native PRoot 실행, 실제 CPU affinity,
도구와 kernel/cgroup 상태를 기록한다. #642의 checkout/upload Action 추출 실패가 남아
일반 모든 CI에 즉시 호환된다고 주장하지 않는다. 로컬은 YAML/Bash/Python 문법과 diff만
확인하며 기능·ARM 실행은 Actions에서 수행한다.

이슈: https://github.com/bin9208/openpilot-rust/issues/643

## 네이티브 실행 전제 확인

source `063db70514eb57cb63caeecbc13df36fb1bd72d2`,
[Actions40936714336](https://github.com/bin9208/openpilot-rust/actions/runs/40936714336),
job123669068409가 성공했다. 공통 ARM selector가 실제 `bin-Arm-tablet`를 선택했고
QEMU 없이 PRoot 내부에서 Python ARM 실행이 통과했다. 이번 실제 affinity는0,1,2로
3jobs였다. #642의5CPU와 달라 자동 선택값을 고정5로 표현하지 않는다. 보고된 Ubuntu는
24.04.5, PRoot kernel 문자열6.2.1-PRoot-Distro, 실제 CapEff는0이며 cgroup controller
읽기는 PermissionError였다. `top-app` cpuset 표시만으로 전체8CPU 사용을 주장하지 않는다.

기준 QEMU artifact12924545511(50,179,819bytes)의
`native-agnos/bootstrap-current-evidence/initial.json.stderr`를 읽었다.
실패 원문은 `private native build memory limit reached; normal runtime retained`다.
이 실패는 완료 빌드나 성공 검증이 아니며 제한을 제거해 성공으로 바꾸지 않는다.

후속 source `a48686e1fc841d95eeee844f0c6b92ef142882bb`의
[Actions40937256101](https://github.com/bin9208/openpilot-rust/actions/runs/40937256101)은
정확한 원본 `9b8c6337...` Git 소스와 AGNOS raw SHA를 확인한 뒤 원본의
`openpilot-runtime-build`/`openpilot-runtime-plan` 두 실제 바이너리 컴파일 명령을
실행한다. Rust1.94.0과 Cargo.lock을 유지한다. 다운로드·이미지 해제·의존성 fetch와
컴파일 시간을 별도로 남긴다. ARM ELF·원래13그룹/99바이너리 catalog·compiler feature
판정을 유지하며 실제99개 전체 runtime 컴파일로 표현하지 않는다.

원본 전체 workflow의 privileged cgroup/bootstrap 및 codec/provider 회귀는 태블릿에서
확인되지 않았다. 이 비교는 네이티브 compiler 구간의 증거이며 해당 항목들의 통과가 아니다.
일반 checkout/upload Action 호환성과 다른 workflow의 공통 runner admission 추가는 별도다.

## 실행 중 로그의 사용자 확인

현재 실행의 사용자 제공 원문에서 `rust-setup`은135.354470677초, 종료0이고,
`locked-dependency-fetch`는1068.747040895초, 종료0이다. 이후 `START native-project-build`
까지 확인했다. 이는 의존성 준비에17분49초가 소요된 기록이며 컴파일 완료 시간으로
합산하지 않는다. 완료 후 Actions 원문과 다시 대조한다.

일반 Actions 호환성은 별도
[40940667812](https://github.com/bin9208/openpilot-rust/actions/runs/40940667812)로 대기 중이다.
source `4d0af7c68`의 fast [40940667511](https://github.com/bin9208/openpilot-rust/actions/runs/40940667511)은
성공했다. 이 변경은 실행 중인 AGNOS 작업을 중단·재시작하지 않았다.

사용자 확인에 따라 x86 비교 상태는 다른 실행 중인 CI와 분리한다. 비교 기준인
`bin-actions3`의40904508393은 이미 종료됐다. 그 안의 동일한 두 빌드 도구 컴파일은
213.169038514초에 성공했으며, 뒤의 bootstrap 메모리 제한 실패와 구분한다.

사용자는 태블릿의3코어 제한이 컴파일 지연 원인일 가능성을 지적했다. 사전 확인의
실제 affinity는0,1,2였고 총8논리 CPU가 보이더라도 모두 사용 가능한 것은 아니다.
비교 스크립트는 시작 시점의 affinity로 `CARGO_BUILD_JOBS`를 정하므로 실행 도중
허용 CPU가 증가해도 Cargo 작업 수를 다시 계산하지 않는다. 실제 본 실행의 선택값은
완료 보고서로 확인한다. 초기 다운로드 지연만으로 이후 컴파일 지연을 설명하거나,
미완료 상태에서 ARM과 QEMU의 속도 비율을 확정하지 않는다.

40904508393의 원본 Actions 로그는 호스트 단계에서
`available_cpus=16 cargo_build_jobs=16 rust_test_threads=16`을 기록했다.
태블릿 사전 확인의3CPU와 동일한 병렬 조건이 아니다. 원본 게스트의 별도 환경 구성까지
호스트의 환경 변수16이 그대로 전달됐다고 단정하지 않는다.
