# 작업 브랜치의 전체 AGNOS workflow ARM 측정 (#643)

2026-10-30 KST 사용자가 빌드 도구 두 개의 짧은 비교를 넘어 실제 AGNOS 작업 브랜치의
빌드를 ARM에 배정하고 시간을 측정하도록 요청했다.

선택한 작업 브랜치는 `codex/feat-599-complete-runtime-inputs`, 기준 SHA는
`dff230060a2ccec0cc58c3d09822b9a026beff1f`이다. 이 커밋에서 별도 실험 브랜치
`codex/feat-643-agnos-arm-full`을 만들었다. 선택한 브랜치와 실행 중인 x86 CI는 보존한다.

새 `Full AGNOS build on GitHub ARM` workflow는 원본
`.github/workflows/runtime-native-environment.yml`의 준비·컴파일·검증·입력 구성·증거
수집·정리 단계를 모두 유지한다. GitHub `ubuntu-24.04-arm`만 선택한다. 별도 Fast checks
push 실행은 사용자 정책대로 제거한다.

## ARM 실행에 필요한 변경

- 호스트 planner 생성기를 `aarch64` 대상으로 실행한다.
- 원본과 같은 PRoot5.5.0 소스를 ARM에서 빌드하고 동일 AGNOS 이미지에 사용한다.
- ARM에서는 QEMU를 넣지 않는다. 기존 x86 실행 경로는 보존한다.
- 명시한 ARM 실험 환경에 한해 원본 self-hosted 전용 입장 조건을 GitHub ARM에도
  허용한다. 실제 커널 cgroup, 비특권 실행, 소유권·SHA·자원 검증은 유지한다.
- 실제 허용 CPU 수로 병렬도를 정한다. 1job 강제나 fallback을 사용하지 않는다.
- 전체 job 시간, 준비부터 증거 수집까지의 시간, 원본 AGNOS 내부 명령 구간 시간을
  기록한다. 실패 시 실제 실패 단계와 그때까지의 시간으로 보고한다.

런타임 구현과 의존성 pin을 바꾸지 않고 CI 실행 환경만 조정한다. 정확한 측정 커밋 SHA는
새 workflow의 실행 SHA이며 위 작업 브랜치 기준 커밋과 함께 기록한다. 사용자 설치·장치
부팅·주행 수용의 증거로 취급하지 않는다. 입력 파일의 publication 후보와 artifact를
원본처럼 보존하지만 릴리스 발행이나 설치는 실행하지 않는다.

로컬은 Python/YAML/Bash 문법과 diff만 확인하고 실제 실행은 Actions에서 수행한다.
결과가 나올 때까지 이슈를 완료 처리하지 않는다.

이슈: https://github.com/bin9208/openpilot-rust/issues/643

사용자는 이 전체 비교가 끝나고 GitHub ARM이 기존 QEMU보다 훨씬 빠른 것으로 확인되면
ARM 대상 빌드를 모두 GitHub ARM으로 전환하도록 추가 지시했다. 측정 중에는 ARM 작업
배정과 QEMU 사용 경로를 읽기 감사하고, 조건이 확인되면 별도 CI 변경과 실제 Actions
검증을 진행한다. 비교가 끝나기 전 전환 완료로 표시하지 않는다.

## 조건부 전환의 읽기 감사

`origin/dev`는 `abea63adf5140cb670a1880fa2c350727a0feda9`, 선택한 작업 브랜치는 위
`dff23006...` 기준으로 확인했다. 일반 webcam/planner/CarrotMan/WebRTC/Xiaoge/radard
matrix의 ARM 항목, RadarCAN/Carrot Navi, messaging allocation과 offline Cargo reuse는
이미 GitHub ARM 선택지가 있다. 선택한 작업 브랜치의 X64 matrix 항목은 기존 private
러너를 사용하므로 ARM 항목과 구분한다.

남은 ARM/QEMU 경로는 원래 AGNOS workflow, logger source-reuse의 aarch64 matrix,
generic aarch64 전체 빌드, X64 workspace 내부 QCOM ARM frontend 빌드/실행,
factory ARM sudo transport와 전체 source-entry다. factory 경로는 기존 x86 정적 QEMU
ELF와 binfmt를 직접 검사하고 source-entry는 uid1000을 전제로 하므로, GitHub ARM에서
동일한 소유권·sudo·커널 검증을 유지하도록 실제 실행 경로를 조정하고 검증해야 한다.
X64 전용 커널 자원 검증 자체는 ARM 빌드로 오인하지 않는다.

QCOM의 실제 컴파일러는 `rust/tools/tests/test_offline_qcom_compiler.py`와
`test_offline_qcom_schedule.py`에서 `QCOM_COMPILER_COMMAND`로 사용한다. 나머지 X64
workspace 검증을 보존하면서 이 실제 ARM 컴파일 검증을 전용 ARM job으로 옮기는
구성을 검토한다. 현재 `stage-qcom-compiler`는 QEMU 인자가 필수이므로 네이티브 실행을
명시하는 입력과 동일한 compiler SHA/ELF 의존성 검증이 필요하다.

## 실행 기록

실행 커밋은 `f62e361d59afa2e12125d416f372a1042bcbdfb9`,
[Actions40947309362](https://github.com/bin9208/openpilot-rust/actions/runs/40947309362),
job123705956835다. 원본의69개 단계와 시간 기록/업로드2개를 유지한 구성을 로컬 문법·
diff 읽기 점검으로 확인했다. 실제 빌드·검증은 위 ARM Actions에서 진행한다.

같은 기준 작업 브랜치의 기존 x86 실행
[40937558092](https://github.com/bin9208/openpilot-rust/actions/runs/40937558092),
job123673055883은 새로 배정한 것이 아니며 그대로 유지했다. x86의 AGNOS 내부 단계는
2026-10-30 KST05:02:43에 시작했다. 비교 시 두 실행의 실제 종료/실패 상태와 캐시,
준비 시간·AGNOS 내부 실행 시간을 함께 확인한다.

중간 완료 단계에서 기존 x86의 Rust 구성 준비는80초, locked registry 준비는149초였고,
ARM은 각각189초,297초였다. 이들은 호스트 준비 단계이므로 두 도구의 앞선10.7배
컴파일 차이를 전체 workflow에 적용하지 않는다. 전체 종료와 AGNOS 내부 구간을
확인한 뒤 조건부 전환을 판단한다.

### 기존 x86 실행의 취소 종료

기존40937558092는 KST06:02:20에 `cancelled`로 종료됐다. 이 세션에서 취소하거나
재실행한 작업이 아니다. AGNOS 내부 단계는05:02:43부터06:01:36까지 진행됐으며,
완료된 원본 명령64개의 시간 기록을 확보했다. 전체 빌드 완료 시간으로 취급하지 않고
ARM의 동일 명령과 비교한다. 예를 들어 빌드 도구 첫 컴파일은217.964초,
고정 Rust prefix를 쓰는 두 프로그램 첫 빌드는743.551초, 같은 prefix의 재실행은
455.258초였다. Cargo source 검증/재사용 명령은913.493초였다.

### 조건부 전환 초안

측정 중인f62e361d5는 유지하고 로컬 작업 트리에만 전환 초안을 작성했다. 원래 AGNOS,
generic ARM, logger 재사용, QCOM, factory transport/source-entry의 배정을 수정하는
초안이다. QCOM 원래 테스트 두 파일은 전용 ARM job에서 모두 실행하고 그 성공을
workspace 수용 조건에 연결한다. factory 검증은 원래 comma uid/gid1000과 이미지
sudo·실제 cgroup 검증을 유지하도록 private user namespace의 ID mapping을 조정한다.
아직 push/적용하거나 기능 실행하지 않았으며, 로컬 문법·서식·diff 검토만 수행했다.

준비 시간이 길어져 이후 초안을 별도 실험 브랜치에서 검증하기로 했다. 이 결정과 범위는
[조건부 전환 실험 기록](arm-routing-draft-643.md)에 남긴다. 실제 배정 전환은 비교 결과의
조건 확인 이후이며, 원래 전체 측정 실행은 그대로 진행한다.

### 가장 긴 호스트 준비 구간의 완료

수치계산·geometry·compression·acados 등8개 외부 component의 내보내기/무결성/재사용
준비 단계는 ARM에서 UTC20:51:36부터21:40:06까지2910초, 기존 x86에서는1222초였다.
ARM이 약2.38배 오래 걸렸다. 이 구간은 새 컴파일을 하지 않는 component 준비이므로,
컴파일 도구의 앞선10.7배 단축과 구분한다. 전체 결과와 동일 AGNOS 명령의 실측이
나오기 전 전체 workflow가 빨라졌다고 판단하지 않는다.

시간 기록은 원본 증거 artifact 업로드 이후 실행하므로 `elapsed_seconds`에는 원본 증거
업로드까지 포함한다. 초기 실행의 summary에 있는 “증거 수집 직전” 표현은 정확하지
않으며, 초안에서 표현을 바로잡았다. 측정 중인 f62 소스나 실행을 바꾸지는 않았다.

### 첫 전체 실행 종료와 권한 처리 수정 후 재측정

f62의40947309362는 준비부터 증거 업로드까지7714.026초, AGNOS 내부752.699초를
기록하고 seed export의 원본 `ldd`에서 실패했다. 전체99개 바이너리 빌드에 도달하지
않았으며, 이를 전체 성공 시간으로 계산하지 않는다. 호스트 준비의 job 시작 기준은
ARM1시간55분42초 대 기존 x8649분37초였다.

기존 dff의 완료 명령과59개를 대응시킨 합산은 QEMU3009.542초 대 ARM523.747초로
약5.75배 단축됐다. 빌드 도구 첫 컴파일은217.964초 대22.446초였다. 이 수치를 전체
workflow 시간으로 확대하지 않는다. 처음 이미지1010655092bytes와 모델776634338bytes를
전송했고 각각 반복 전송은0이었다. 최초 cache 조건도 구분한다.

짧은 진단 [40966055910](https://github.com/bin9208/openpilot-rust/actions/runs/40966055910)은
같은 f62 소스의 seed가 실행되고 기본 loader 검사도 통과하지만, PRoot 안의 `ldd`에서
`test -r`가 실패하고 실제 chroot의 원본 `ldd`는 통과함을 재현했다. 고정 PRoot source의
ARM64 syscall table에 Linux439번 `faccessat2` 매핑이 빠져 있었다.
해당 한 줄을 추가한 [40966682467](https://github.com/bin9208/openpilot-rust/actions/runs/40966682467),
진단 SHA `8713b4cd1fab0f223b8194bdd08600df98327065`에서22개 진단이 모두0으로
종료했다. 원래 `bootstrap_seed.inspect_abi`와 원본 `ldd`를 유지하고, 권한 없는 파일과
없는 파일의 읽기 거부도 유지했다. 원본 AGNOS 파일이나 ABI 규칙은 바꾸지 않았다.

전체 재측정은 `78fe1dd503f9b3a9727c78bcb407b491290d9d87`의
[40967438168](https://github.com/bin9208/openpilot-rust/actions/runs/40967438168)이다.
같은 준비·빌드 profile을 유지하며 깊은 준비 전에 실제 bind 파일의 읽기·거부 검사를
추가했다. 그 조기 검사는 통과했다. PRoot base source SHA, 패치 SHA256과 실제 실행
파일 SHA를 함께 기록한다. 패치 SHA256은
`bb510391bb9a57ca6249e9046dc2247f2c5e131698881f1578e10074861aaae2`다.
이 재측정도 전체 결과가 나오기 전 전환 완료로 표시하지 않는다.
