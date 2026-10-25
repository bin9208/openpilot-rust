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
