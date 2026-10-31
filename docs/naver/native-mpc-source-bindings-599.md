# 생성 MPC 라이브러리의 소스 결속·재사용 — #599 (검증 대기)

상위 [#598](https://github.com/bin9208/openpilot-rust/issues/598),
환경 [#599](https://github.com/bin9208/openpilot-rust/issues/599),
캐시·세대 [#600](https://github.com/bin9208/openpilot-rust/issues/600).
모델 준비 Python과 기존 C/C++ 라이브러리를 유지하는 우선 범위다.

## 재사용 조건

기존 [foreign acados core component](native-runtime-companions-599.md)는 생성 solver와
실행 manifest를 포함하지 않는다. 새 `rust/native-mpc-provider.json`은 별도
`acados022-runtime` profile이며 생성 C solver2개, core SO5개, 원래 manifest/provenance/
notice와 전체 runtime manifest15개 member, 합계4647168 bytes를 고정한다.
원본 source는 `a3d0e3a497b4ae4d23c7cf5ee2275177600bbb31`, 원본 archive1155969168
bytes의 SHA는 `9c9c16a72bad163226388ef84327b0db1fa64bbe4828e64388e0a7e92d364291`다.
검증한 origin cache에서 작은 component를 내보내며 소비자는 별도 descriptor SHA와
component SHA/크기/전체 Tree를 검증한다. 소비자는 전체 origin을 내려받지 않는다.
공개 component 배포와 실제 공개 다운로드 검증은 아직 완료되지 않았다.

`rust/native-mpc-source-contract.json`은 프로젝트/의존성/import alias, schema,
uv.lock/pyproject.toml, 원래 생성 helper3개와 notice의19개 root를 포함한다.
현재 변경의 기반 `9f00e077f298a544a8c95859d75ebbb3c639fe84`와 원본에서 선택한4662개
Git entry(mode/type/OID/path)가 모두 같다. 경로 바이트 순 정렬 후 각 원래 entry와
NUL을 해시한 SHA는 `298082a7f6b0a246c82e261a26a98687f52830eea255f6a5a0c9f5563990ca77`다.
frontend의 generated/js/src/tests4개 root는 생성과 무관한 비 Python 파일만 제외하며,
그 아래 Python/typing/schema는 계속 포함한다. 추가·삭제·mode/OID 변경도 거부한다.
보수적 범위이므로 무관한 Python 수정도 새 검증 provider를 요구할 수 있다.

contract 원문 SHA `6dd5b809d92e716f6654cfb218d7d6cbc0547b473e009c9e0578c04fc9930e41`와
원본 output9개 SHA를 profile/준비 코드에 고정하고 contract sidecar를 component에 넣는다.
`--source-root`가 없으면 export/prepare가 cache/download 전에 실패한다. clean source와
계약을 전후 확인한다. source_binding에는 현재 SHA/tree와 원래 source SHA를 구분하고
regenerated=false를 기록한다. 원래 compiler·입력·산출물 provenance를 보존한다.
원래 provenance에 없는 Python interpreter 정보를 새로 측정한 것처럼 채우지 않는다.

`openpilot-plannerd`를 포함한 native generation은 빌드 전/봉인 전 Layout.files의
solver/core/manifest/provenance/sidecar를 모두 검증한다. 실제 복사 후와 시작·fallback
검증에서도 retained source 및 실제 runtime output을 다시 확인한다. provider/compiler
cache 없이 retained source와 immutable 결과로 확인한다. 달라진 생성 입력에는 새 검증
생성물/계약을 요구한다. 전체 Layout/99개 native build와 기본 시작 연결은 계속 남아 있다.

## 테스트 방법

로컬은 scoped Rustfmt/Python AST/YAML/JSON/hash·diff 읽기만 수행한다. 기능·빌드는
Actions의 등록된 bin-actions에서 실행한다. 변경 branch push가 두 workflow에 자동 연결한다.
기존 실행/다른 세션/cache를 취소·삭제하지 않는다.

```sh
gh run list --repo bin9208/openpilot-rust --workflow runtime-source-build.yml --branch BRANCH
gh run list --repo bin9208/openpilot-rust --workflow runtime-native-environment.yml --branch BRANCH
gh run view RUN_ID --repo bin9208/openpilot-rust --log-failed
```

- foundation x64/ARM: 실제 소유 Git fixture에서 문서/Rust/frontend JS 변경 재사용,
  Python 변경·추가·dirty source·범위 축소·누락/경로 이탈/잘못된 output 거부와 provider
  cache 제거 후 runtime 재검증. dev-tools는 unbound export의 다운로드0/출력 없음,
  실제 checkout의 production contract 결속과 foreign profile 분리를 확인한다.
- genuine AGNOS: host origin cache에서 작은 component를 export/초기·반복 준비하고,
  ARM 준비기가 별도 prefix에서 같은 Tree/source binding을 검증한다. 실제 output9개와
  sidecar를 확인하고 반복 HTTP body/복사/offline seed0을 요구한다.
- 같은 guest에서 현재 openpilot-plannerd의 실제 plannerd_solver_trace 예제를 locked/
  offline native release로 빌드한다. Python stdlib ctypes의 직접 C 호출과 lateral6/
  longitudinal6 solve의 status/x/u/cost f64 비트를 정확 비교한다. 유한값·정상 status,
  실제 provider maps, strict AArch64 ELF/의존성, 반복 Cargo fresh/같은 binary SHA와
  provider 재준비0을 확인한다. 기존 C/C++ provider를 소비하는 ABI 증거다.
- 전체13그룹/99개 계획의 runtime_inputs10개 Tree에는 partial acados core 대신 complete
  source-bound MPC Tree를 넣는다. 기존 partial component/API 증거는 별도로 보존한다.
  추가 SDL·TurboJPEG2개 Tree는 [display 입력 안내](native-display-inputs-599.md)를 따른다.
  추가 도구·글꼴2개 Tree는 [media 입력 안내](native-media-tools-font-599.md)를 따른다.
  추가 current Git LFS Tree1개는 [LFS 입력 안내](native-source-lfs-599.md)를 따른다.
  추가 독립 firmware Tree1개는 [Panda 입력 안내](native-panda-inputs-599.md)를 따른다.

artifact의 native-mpc JSON/build JSONL/ELF audit/ABI cases, native-runtime-inputs.json과
commands log를 정확 source SHA로 확인한다. host 준비는 실행·재생성 증거가 아니다.
ctypes 비교는 원래 Python/Cython 전체 planner 회귀, 새 solver 생성 또는 Python 모델
준비 환경 완료를 대신하지 않는다. 원래 planner-runtime CI의 생성/회귀 명령은 보존한다.
host component4배+16MiB, guest 실제 예제 빌드2GiB의 작업별 예상 증가량으로 확인하며
전체 빌드 자원 실측으로 표현하지 않는다.

host는 x64 bin-actions, guest는 고정 AGNOS image의 QEMU/PRoot ARM user space다.
물리 ARM CPU·AGNOS kernel 부팅·comma3X 성능/수용은 아니다. 정확 변경 SHA의 CI 성공,
필수 PR/병합 후 검사, 전체 runtime·업데이트·복구와 사용자 comma3X 자체 빌드/부팅/
녹화/기존 업로드/주행 수용까지 #598을 열어 둔다.
