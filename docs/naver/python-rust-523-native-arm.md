# #535 표준 ARM VM의 원래 CarrotMan 검사 실행 후보

2026-10-25. #523의 기본 GitHub runner 전환과 #548의 QEMU cold timeout 후속이다. 부모가 표준 ARM VM에서 원래 기능 명령을 직접 실행하는 작은 후보를 선택했다. 사용자에게 새로운 호스트/증설·유료 runner를 요구하지 않는다. main Rust workflow의 기존 job·원본·기대값·기능 deadline은 이번 후보에서 바꾸지 않는다.

## 근거와 선택

- e882의 planner/WebRTC는 실제90분, Xiaoge는75분 한도를 초과해 취소됐다. 기존25→60분 변경만으로 이 다른 cold 구간의 완료를 보장할 수 없다.
- [공식 표준 runner 문서](https://docs.github.com/en/actions/reference/runners/github-hosted-runners)는 공개 저장소의 ubuntu-24.04-arm을 제공한다. bin9208/openpilot-rust는 실제 public이다.
- [inventory40149186521/job121036901177](https://github.com/bin9208/openpilot-rust/actions/runs/40149186521/job/121036901177), source202a7f9dcbf71ed1c6367d1cd156e4e7ebc280f8에서 실제 hosted/Linux/ARM64/uname aarch64 경계를 통과했다. Neoverse-N2 VM의4CPU, MemTotal16330124kB/MemAvailable15204492kB, 관측 free115306577920B다. 이후 job의 여유 공간 보장이나 물리 bare-metal/device 수용이 아니다.
- artifact12581667083은4054B/SHA256 `43ee04225c076b0151a3ce5f491956a63af6a1de8eb891aecdde896adfeb3c18`,11CRC/해제7433B다. metadata만 읽었고 checkout/프로젝트/원본 실행은0이며 upload와 owned cleanup이 성공했다.

QEMU의 한도를 계속 늘리는 안은 CPU 명령 에뮬레이션 비용을 남긴다. 전체 required job을 한꺼번에 바꾸는 안은 문제를 분리하기 어렵다. 먼저 원래 CarrotMan aarch64 job 하나를 새 별도 workflow에 정확히 복사하고 hosted ARM VM만 선택한다. 기존 기능 명령은 그대로이며 native transport/Python 준비/architecture guard만 바꾼다. 실제 결과를 읽은 뒤에만 다른 job 전환을 검토한다.

## 파일과 실행 계획

- `.github/workflows/python-rust-523-native-arm.yml`: 현재 main Rust workflow의 carrot-man-runtime job을 source로 사용한다. 원문 기능 run 명령·선택·최종 source/index/upload/owner cleanup을 보존한다. native ARM 단일 matrix와 strict branch/repo/hosted/ARM64 guard를 둔다. x64/QEMU adapter 두 step만 현재 native helper의 prepare/finish로 바꾸고 Python3.12.14/setup 및 private venv를 ARM에서도 준비한다. 원래 ARM job60분과 내부 기능 timeout은 유지한다.
- `tools/ci/native-arm`: hosted ARM 전용의 작은 shell adapter다. 기존 owned job/source/HEAD/private HOME·Cargo·Rustup 경계를 확인하고 canonical alias를 만든다. 실제 ISA/native-host stamp를 기존 artifact의 local-arm/execution.json 위치에 남긴다. QEMU/proot/rootfs를 설치하지 않으며 각 원래 command file을 현재 native Bash에서 실행한다. 사용자/global 설정과 기존 local-arm helper는 수정하지 않는다.
- 이 기록과 Focus의 exact path/SHA를 갱신한다. 새 Python 소스 파일은0이다. 다만 새 workflow의 원래 Python 비교 호출과 adapter의 prepared Python version CLI 의존성은 execution-edge pending2로 별도 등록하며 baseline 파일 교체 credit은0이다.

로컬은 YAML/Bash syntax·data 수준의 원래 기능 명령 동일성·diff만 확인한다. actual SDK/provider 설치·Cargo/원본 비교·IPC/회수/봉인·artifact 감사는 Actions에서 수행한다. 새 job이 과거 ARM PASS를 상속하거나 현재 inventory를 기능 성공으로 수용하면 안 된다. 기능·실제 시간·디스크 fit은 아직 UNRUN이다. 실패도 원래 증거 upload와 엄격한 source/소유 정리 조건을 그대로 따른다.

## workflow 정책의 후속 수용 경계

202a Fast40149186502는 새 inventory의 queue:max 누락과 build_release의 강화된 cleanup 조건을 기존 exact validator가 모르는 차이로 실패했다. queue:max를 명시하고 표준 ARM은 CI296_NATIVE_ARM=1 + 정확label + 순서가 고정된 source/ref/hosted/ARM64/uname guard를 가진 경우만 인정한다. bare ARM label/동적 runner 허용은 계속 거부한다. native helper의 정확 current-source prepare/finish, 원래 private source·명시 shell·업로드 조건도 검증한다. generated_assets_check의 실제 success/고유ID/순서/정상 trusted gate를 추가로 요구하고 기존 seal/receipt 조건은 보존한다.

기존 Actions의 실패4건이 기준 RED다. 새 actual guard 누락·변조, generated consumer cleanup bypass, unbound native wrapper의 음성3개를 먼저 작성했다. 로컬 Rustfmt/문법·diff만 확인하며 새로운 제어의 실제 RED/GREEN 및 기존 회귀 성공은 아직 UNRUN이다. native job1개가 추가돼 private roster는61/그중 explicit target-shell9이며 원래60/8을 삭제하지 않았다.

별도 동시 PR 실패 보정으로 main rust.yml의 model-pipelines Python entry env 한 필드는 수정 중이다. 이 native 후보가 복제한 carrot-man-runtime job과 기존 local-arm helper는 바꾸지 않았다.

## 첫 실제 CarrotMan ARM VM 수용

[40151446363/job121043654047](https://github.com/bin9208/openpilot-rust/actions/runs/40151446363/job/121043654047),
source `6a231dc938d35e9da80e0615cfb2570e64f23911`은 실제 ARM VM에서4분36초에 성공했다.
원래 ARM에서 제외하던 x64 policy 단계는 그대로 skipped이며 전체 CarrotMan policy 수용이 아니다.
실제 source/native Params·IPC·소켓·업로드 수신처의12개 시나리오는 owned-summary의
passed/compared true와 원래 source 파일3개 pin으로 확인했다. native identity는 현재 빌드
carrot_man_owned/libpython false, source identity는 Python3.12.14/arm64/libpython true다.

- owner artifact12581409954=9,879,619B/SHA `f664b29657b0eabdfa29464c17e9d893f0c85e4320ad38ee564c338359d66f85`,134CRC/해제32,231,224B.
- private artifact12581444903=289,396B/SHA `a553a18024c9d0699bc454c96468881fa4194997eaf810efce34e89b3e665d47`,12CRC/해제2,341,391B.
- execution.json의 exact source/native-host/aarch64/VM과 private SOURCE_COMMIT이 일치하고 source/index 전후가 동일하다. 원래 upload2개·source seal·owned cleanup이 모두 성공했다.

이는 원래12개 실제 owner 비교의 ARM VM 수용이다. 전체 SDK/DSO closure, 물리 bare-metal,
차량·운전, 원본 Python 제거 또는 required PR 전체 수용을 확대해 주장하지 않는다.

## 다음 planner/WebRTC 후보 계획

첫 native transport 수용 뒤 같은 표준 ARM VM에 원래 planner-runtime과 webrtc-runtime의
ARM 기능 명령을 각각 별도 job으로 연결한다. 기존 main required job은 아직 수정하지 않는다.
새 `python-rust-523-native-arm-more.yml`만 추가하고 같은 native helper를 사용한다.
원래 run 본문·비교/Clippy·기능 timeout·원문/예상값·source/index·upload/cleanup은 보존한다.
기존 전체 job90분도 유지하며 x64 전용 ASan 단계는 원래 ARM처럼 skipped다.

변경은 단일 native ARM matrix/LOCAL_CI_ARCH, 동일 즉시 trust guard, 기존 QEMU prepare/finish의
native helper 연결, host Python 준비뿐이다. 고정 selector의 runner 이름은 hosted-arm64로
기록한다. 모든 기존 run 본문을 data로 비교하고 정책 roster를63/private transport11로 갱신한다.
추가 Python CI 호출 진입2개는 execution-edge pending으로 기록하며 baseline credit0이다.
로컬은 YAML/Bash/Rustfmt/diff만 확인하고 실제 설치·원본·기능·회수·시간/공간은 새 Actions에서
판정한다. 성공하기 전 다른 required job을 대체하거나 QEMU 실패를 성공으로 덮어쓰지 않는다.
