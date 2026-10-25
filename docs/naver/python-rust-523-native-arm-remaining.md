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


## 2026-10-25: native8 actual 감사와 main 이관 source 후보

아래 8개 candidate job의 actual 성공을 artifact 원문과 API로 감사했다. **새 main/reusable 이관의 기능 성공은 아직 UNRUN**이며 이전 source 성공을 새 source로 소급하지 않는다. 물리 기기·실차·전체 SDK/DSO closure 또는 Python retirement credit은0이다.

| family | actual source / run / job | 기능·private artifact |
| --- | --- | --- |
| CarrotMan | `01fcc36e09b39172f4c88954a702fa8353b2d1e8` / [40157685731/121062463077](https://github.com/bin9208/openpilot-rust/actions/runs/40157685731/job/121062463077) | `12585442590` / `12585073136` |
| Planner | 같은01f / [40157685727/121062463154](https://github.com/bin9208/openpilot-rust/actions/runs/40157685727/job/121062463154) | `12584912724` / `12584543485` |
| WebRTC | 같은01f / [40157685727/121062463051](https://github.com/bin9208/openpilot-rust/actions/runs/40157685727/job/121062463051) | `12585497731` / `12585417745` |
| RadarCAN | 같은01f / [40157685763/121062463233](https://github.com/bin9208/openpilot-rust/actions/runs/40157685763/job/121062463233) | `12585408189` / `12585213498` |
| CarrotNavi ARM | 같은01f / [40157685763/121062463084](https://github.com/bin9208/openpilot-rust/actions/runs/40157685763/job/121062463084) | `12584742650` / `12584842556` |
| webcam | 같은01f / [40157685759/121062463491](https://github.com/bin9208/openpilot-rust/actions/runs/40157685759/job/121062463491) | `12584598321` / `12584663185` |
| radard | 같은01f / [40157685759/121062463522](https://github.com/bin9208/openpilot-rust/actions/runs/40157685759/job/121062463522) | `12585194313` / `12585573805` |
| Xiaoge | `338fcb4cd160da8733bbcadcb932770f85d38d97` / [40158739519/121065793973](https://github.com/bin9208/openpilot-rust/actions/runs/40158739519/job/121065793973) | `12584704930` / `12585830706` |

각 ZIP은 API size/digest/source와 전체 CRC를 대조했다. 16 ZIP 합계 packed301,117,817B/expanded2,686,556,719B, members33,774개, ZIP별 중복name0이다. 96 ELF header는 모두 ELF64/little-endian/AArch64다. archived ELF·원본·모델·PKL은 로컬 실행하지 않았다. 01f source/index 전후7쌍과338 Xiaoge source/index 전후가 각각 같고, 각 index는 해당 Git commit의19,462개 경로 원문과 같다. 8개 job의 final seal·기능/private upload·owned source/common cleanup은 API success다. SDK는 원래 private Rust1.94 설치/version과 Python3.12.14 실행 계약을 유지했으며, 이 private artifact에는 별도 full SDK before/after seal이 없다. cleanup success를 새 전체 자손·provider closure 증명으로 확대하지 않는다.

관측 범위는 CarrotMan owned12 scenarios, Planner12 comparison commands/IPC4runs·3비교 differences0/lifecycle13쌍, WebRTC8 frontends+STUN8, RadarCAN reference6군121cases+constructor558 및 IPC23/5/lifecycle15, webcam pixels28/capture7/worker5source+native/error3/Linux CLI source·native exit0, radard 정책6군·IPC6 differences0/lifecycle8/deadline·startup이다. CarrotNavi ARM은 build/ELF inspection이며 runtime traffic 비교가 아니다. Planner sanitizer와 CarrotMan policy의 원래 x64-only skip을 native 성공으로 세지 않는다.

Xiaoge338은 LFS helper의 정확 candidate ARM 예외를 실제 통과했다. `prepared.pass`는2models/22,820,800B/정확338, pull·helper exit0이며 LFS index/other/providers/source/storage-rejected 전후5쌍이 byte 동일하다. 모델 SHA는 lane.onnx `d3761c185daf33897ff3ff5edf28115c5dc829c06c538033accb616d6c67528c`, v_asm_model.onnx `00247ede5159dff9a0768c171095711d40f1ee109ac7b5e24adce344fe4ba6f9`다. 이후 OpenCV/build/test/Clippy, policy436/lane14/inference4frames·24 exact images+tensors/HTTP65/live19/lifecycle10/JPEG16/OpenCV22와 최종 seal/upload/cleanup이 성공했다. live의 원문 publication 수 source38/native37도 보존하며 comparator의19case PASS를 임의 전체 count 동일성으로 바꾸지 않는다. 새 main caller LFS 예외의 실제 성공과는 분리한다. Xiaoge 기능 ZIP은58,664,682B/SHA256 `9e0c7019f44d73e8ae7a8c8044eb6c3338b45d28b4105eae624d1dd3106c6c44`, private ZIP은3,595,986B/SHA256 `689037abe9dfb5702464a904b531bf67b803d465d7962bff545c5a8723eb3342`다.

### 승인된 main/reusable8 이관 구현의 source-only 상태

선행 Xiaoge actual 성공 뒤 승인된 소유8파일만 적용했다. main은489,835B/callee172,849B이며 각각500000B 이내다. main44 keys/root/fast42 needs·env·run·조건 및 target5 외39개 job의 전체 parsed model을 보존했다. 3dual job은 기존 x64 row와 나머지 body/env/timeout을 보존하고 ARM만 callee로 옮겼다. 두 ARM-only 기능 전체는 callee의 같은 key로 옮기고 기존 main name/key는 always+동일-source native-arm success만 인정하는 엄격한 결과 proxy로 남겼다. callee는 기존3 leaf의 model과 source prefix가 동일하며 새5는 첫 main Deny·caller env2개 외 candidate whole-job과 같다. 원래90/75/90/90/60분과 기능/provider/예산/원본/전후 seal/upload/cleanup을 유지한다. 원래 full ARM12GiB/workspace/generic ARM/SAN은 바꾸지 않았다.

main의 공유 `V=local-x64` scalar를 남겼고 제거되는60분 anchor의 외부 참조2개는 같은 literal60으로 풀었다. callee aggregate는8개 result를 각각 success로 요구한다. 두 proxy/aggregate의 skipped/failure/cancelled/빈 값·누락/unknown/continue-on-error/step-skip을 허용하지 않으며 raw 기능 기대값을 바꾸지 않는다.

`tools/ci/native-arm`은 이미 실제 main/reusable caller를 지원하므로 byte 불변이다. LFS helper는 실제 ARM Xiaoge runtime만 기존 candidate 또는 정확 main context(push dev/main, same-repo PR merge/base/head, 기존 dispatch ref)로 허용한다. X64 guard 및 그 뒤의 모든 Git/provider/소유권/pointer/OID/size/cap/후처리 body는 동일하다. 새 helper SHA는 `836c65ed0b09b614a15fe0a1d39290dfee19749e7e3163e562d8d31e3fb95966`이며 **소비자 literal pin 갱신은 부모 통합 대기**다. 기존 strict hash 검사를 우회하지 않는다.

정책/tests는 정확8 roster·6x64 rows·2proxy·main42 및 모든 원래 stage를 검사한다. source/LOCAL_CI_ARCH source roster는74/22, main+callee command-file/finalizer는14이며 이 숫자는 소스 열거 결과다. 기존 Python Planner/Xiaoge matrix assertion도 단순 삭제하지 않고 x64+native/caller/aggregate/proxy 및 음성 case로 보완했다. 현재 source 선언상 Planner pytest24/Xiaoge pytest10이며 이 새 개수의 기능 실행은 UNRUN이다.

로컬 확인은 YAML/token/model·Bash849블록·Python AST2파일·Rustfmt3파일·diff와 inert archive/hash 확인뿐이다. Cargo/build/test/Clippy/원본·프로젝트 실행/CI dispatch·cancel/stage/commit/push는0이다. ignored `native-arm-final-five-candidate/`에 base/candidate와 미적용 patch·source 비교 JSON을, `hosted-01f-native-seven-*`와 `hosted-338-native-xiaoge-*`에 실제 증거 감사를 보존했다. 부모가 독립 리뷰·모든 소비자 pin/Focus/ledger를 통합한 뒤 새 same-source main8/aggregate/strict42 및 양음성·Clippy를 GHA에서 검증해야 한다.
