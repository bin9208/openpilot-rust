# #535 나머지 native ARM 후보5개

2026-10-25. 새 두 workflow는 webcam/Xiaoge/radard와 RadarCAN/Carrot Navi ARM의 기존 기능을 기본ARM VM에서 관측한다. 기존 main의5개 ARM 범위를 아직 제거하지 않는다. 새 기능/빌드/플랫폼/봉인 결과는 Actions UNRUN이다.

각각90/75/90/90/60분이며 원래 기능run·source/index·최종upload/cleanup·원본비교·수치기준을 유지한다. QEMU 준비/마감만 수용된native adapter로 바꾸고 실제Python3.12.14 privatevenv를 준비한다. Carrot Navi의venv 준비는 원래guest가공급하던계약을충족하며512MiB 순차성장admission을별도로확인한다. 60GBfit/전체SDK·DSOclosure/실기수용을소급주장하지않는다.

모두정확matrix.include=[{arch:aarch64,runner:hosted-arm64}]단일행이다. radard의두uname/readelfbody에서matrix.machine을matrix.arch로치환했고실제aarch64대입후명령bytes는원문ARM행과같다. 다른기능기준은바꾸지않는다.

helper는정확한두caller경로와같은codexbranchpush만추가허용한다. maincaller·기존candidate경계는그대로다. source-layout정책의논리job5개와ARM5개를추가해66/14→71/19를검사하며기존테스트를줄이지않는다. 새CI의원문Python/venv호출은각각execution-edge pending5개로등록하며 baseline3580 credit0이다.

초기source검토와행렬P2보완뒤YAML/Bash문법을확인했다. 실제provider·apt/wheel·성장량·기능·upload/cleanup은새정확소스Actions에서만판정한다. 먼저모든후보의실제결과를확인하고필수main ARM경로이동은후속으로진행한다.

## 2026-10-25: 01f Xiaoge의 baseline 전 최초 실패

source `01fcc36e09b39172f4c88954a702fa8353b2d1e8`, [run40157685759/job121062463588](https://github.com/bin9208/openpilot-rust/actions/runs/40157685759/job/121062463588)은 실패했다. 실제 runner는 `GitHub Actions 1000027979`, `ubuntu-24.04-arm`/GitHub-hosted Linux ARM64다. 처음 source guard·private root allocation·checkout까지 성공했고, `Admit pinned LFS models before source baseline`에서 즉시 exit1이다.

로그422행에서 `prepare-lfs-source-535`의 고정 SHA `1d6c6c57f313bc73f1a378649c1d4a0a3f0e3eba5b868b064ffb2976cd78b3ec` 검증은 성공했다. 다음423행의 실제 오류는 `private LFS source: actual hosted Linux/X64 required`다. helper의 현재 host guard가 X64/x86_64만 허용하므로 실제 ARM 후보와 충돌한다. Git/git-lfs FD·SHA admission, LFS storage·pointer/OID/size 검사, payload 다운로드에는 아직 도달하지 않았다. provider 미설치·SHA 오류·모델 수치·용량 초과를 원인으로 확정할 증거는 없다. checkout이 기록한 `/usr/bin/git version`은2.55.0이지만 이것만으로 git-lfs provider의 설치·identity를 수용하지 않는다.

뒤 source final의 exit2는530행의 `index-before.bin: No such file or directory`다. 최초 실패로 baseline admission 자체가 skipped였기 때문이며 새로운 source drift의 증거가 아니다. 이 검사를 생략하거나 누락 baseline을 새 expected로 만들지 않는다. native adapter prepare/finish, Python·Rust·OpenCV·원본 bindings 준비, 타입/테스트/Clippy 및 원본 pixel/model/socket/IPC/lifecycle 비교는 모두 미도달이다.

단독 다운로드한 private artifact `12585402158`은144,971B/SHA256 `965b6a9d2017c7a603fe7ec1250a148573c970d5b2ffab0305da3cfc93548f62`, 6개 member/expanded1,171,875B다. API digest 일치, 전체 CRC 통과, 중복0을 확인했다. member는 `disk-before.txt`, `index-after.bin`, `memory-before.txt`, `ownership.txt`, `runner.txt`, `source-growth-before-checkout.json`이다. ownership의 source SHA/job/alias가 실제 run과 같고, index-after의19,462경로 원문은 해당 commit의 Git tree 목록과 같다. index-before/source-before/SDK/LFS 결과가 없으므로 전후 봉인 성공은 주장하지 않는다. private proof 업로드는 success지만 owned source/common cleanup은 source seal 미성공으로 skipped다. 기능 artifact는 생성되지 않았다.

원래 source admission2GiB는 유지됐고 초기 free 기록은115,294,015,488B다. 이 기록은 LFS 또는 후속 OpenCV build의 실제 peak·fit 증거가 아니다. 다른 webcam/radard/RadarCAN/Carrot Navi의 별도 결과를 Xiaoge에 합치지 않는다.

### 필요한 최소 수정 범위와 현재 상태

최초 조사에 승인된 쓰기 범위는 이 기록과 native-remaining workflow의 Xiaoge 준비 stage였다. 실제 원인인 `tools/ci/prepare-lfs-source-535`는 그 밖에 있어 당시에는 수정하지 않고 패치 후보를 먼저 보고했다. workflow의 helper SHA만 바꾸거나 `RUNNER_ARCH`/uname을 위장해 기존 guard를 통과시키는 수정은 하지 않는다.

확정한 최소 수정은 공통 helper에서 기존 X64 경계를 그대로 두고 **실제 hosted ARM64/aarch64 + native intent + Xiaoge selection/job + 정확 candidate workflow ref + 지정 branch push**의 한 경계만 추가한 뒤 Xiaoge stage의 literal helper SHA를 갱신하는 것이다. 소유 root/source/HEAD/index, root-owned Git/git-lfs held FD·전후 provider, 두 고정 blob/OID/size, 22,820,800B payload, private storage 및2GiB cap, 기존 bounded manifest/pull와 모든 final 비교는 변경하지 않는다. 새 APT/SDK/runtime 설치는 실제 필요가 확인되기 전에는 추가하지 않는다. 최초 미적용 후보는 ignored `xiaoge-lfs-arm-host-boundary.candidate.patch`에 보존했다.

로컬 확인은 source/YAML/Bash 문법·JSON/ZIP CRC·hash·Git metadata 읽기뿐이다. 프로젝트/원본/모델·빌드·테스트·Clippy·CI dispatch, stage/commit/push는0이다. 실제 수정 후 LFS provider admission·hydration·source seal·기능·cleanup은 새 GHA에서 검증해야 하며 이 실패의 retirement credit은0이다. ignored `hosted-01f-xiaoge-audit.json` 및 `hosted-01f-xiaoge.log`에 member metadata/API와 최초 오류를 보존했다.

### 승인 후 source-only 적용 동결

사용자가 shared helper의 정확 ARM Xiaoge 예외를 추가 승인해 위 후보를 적용했다. ARM 분기는 `GITHUB_ACTIONS=true`, 저장소, 실제 `github-hosted/Linux/ARM64`와 uname `Linux/aarch64`, `CI296_NATIVE_ARM=1`, `LOCAL_CI_ARCH=aarch64`, `selection=xiaoge`, `GITHUB_JOB=xiaoge-runtime`, 지정 branch의 push 및 정확 `.github/workflows/python-rust-523-native-arm-remaining.yml` caller ref를 모두 요구한다. generic ARM/main caller는 추가 허용하지 않는다. X64는 원래 host guard를 그대로 실행한다.

| 적용 파일 | SHA256 |
| --- | --- |
| `tools/ci/prepare-lfs-source-535` | `be7ba0db79e941674ea2adf31dfbf0e5c3dfa63e68b42e01e51f51da3ff68082` |
| `.github/workflows/python-rust-523-native-arm-remaining.yml` | `1bce0d7df28903d6280235f5c94bedf1c6619f9d2ecfcd350df8a51e96bda2ec` |

helper의 host guard 외 본문과 기존 X64 guard가 동일하고 workflow는 Xiaoge 준비 stage의 SHA literal1개만 바뀌었음을 source bytes로 확인했다. 세 job의 기능 step·조건·90/75/90분, LFS roster/pins·2GiB cap·기존 provider/소유권/봉인/cleanup을 유지한다. YAML parse, Bash46블록 및 helper1개 문법, 해당 diff 검사를 통과했다. branch의 양성/음성 실행 및 LFS hydration 등 기능은 전부 GHA 대기다.

다른 current caller의 helper pin은 부모 global 검사 소유로 남긴다. 확인 시 `.github/workflows/rust.yml`2곳과 `.github/workflows/python-rust-523.yml`1곳에 이전 literal이 있으며 이 작업에서 수정하지 않았다. 새 APT·설치·환경 위장, 기대값/원본 payload 변경 및 retirement credit은0이다. Fast01f cold collector의 실제 PASS는 별도 증거이며 Xiaoge 실패나 수정 후 성공으로 합치지 않는다.
