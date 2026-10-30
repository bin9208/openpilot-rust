# 공개 입력 pin으로 production source_entry 실행 — #613/#598

이 변경은 실제 실행 전의 검증 driver다. 현재 브랜치에는
`rust/native-bootstrap-inputs.json`이 없고 factory sudo admission도 Actions 결과가
필요하므로, 전체 source entry 성공을 주장하지 않는다. 기능·빌드·namespace·probe는
로컬에서 실행하지 않았다. 기기 접속이나 설치는 수행하지 않는다.

## 실행 경로

`native-source-entry.yml`은 `workflow_dispatch`만 받는다. 공통 `openpilot-rust` pool의
등록된 `bin-actions`, `bin-actions2`, `bin-actions3` 중 하나의
독립 Actions checkout에서 정확 HEAD/tree와 clean 상태를 확인하고 **커밋된 Git blob**의
입력 index SHA를 dispatch의 독립 SHA와 비교한다. 후보 artifact, 임시 index commit,
fixture provider, 런타임 release를 대체 입력으로 받지 않는다. index가 없으면 image
준비 전 실패한다. 실제 GitHub 공개 metadata body 17개(seed/SDK/provider15개)를
HTTPS로 받아 SHA/byte와 현재 SDK plan/provider profile을 확인한다. seed의
`current_tools=1`, `request_owner=2`, `generation_boot=1`도 빌드 전에 요구한다.
production entry도 공개 입력을 다시 독립 검증한다. 별도 producer artifact의 공개·pin
검증은 기존 publication workflow의 책임이며 여기서 발행을 수행하지 않는다.

고정 factory raw image를 기존 transport helper로 검증하고 GitHub native ARM에서
에뮬레이터 없이 실행한다. hosted host identity와 factory comma1000을 별도로 결속한다.
`debugfs rdump` 뒤 원래 인증 image inode의 root 소유/mode04755와 sudo body SHA를
확인해 **그 sudo 한 파일의 SUID만** 복원한다. 새 child만 job cgroup으로 이동시키고,
private user/mount/cgroup namespace의 명시적 UID/GID map으로 host runner와 내부 comma를
연결한다. host identity가1001이면 새로 추출한 private factory tree의 대응 소유만
역매핑해 내부 원래 소유가 유지되도록 하며 symlink를 따라가거나 image/source/cache를
변경하지 않는다. native ARM 경로는 QEMU나 binfmt 규칙을 등록하지 않는다.
원래 ARM `/usr/bin/python3 -I -B -S`, UID/GID1000과 `comma`
passwd/group, 실제 `sudo -n -l` 및 원래 admission payload의 root 권한 전환을 요구한다.
원래 sudoers는 편집하지 않는다. 공유 binfmt/controller의 전후 상태도 비교한다.

source는 `/native/source`에 readonly bind하고 job 전용 state를 `/native/state`에
bind한다. 실제 CLI로 다음을 실행한다.

1. build/seal 전체 실행. 원래 seed current-tools, current runtime-inputs,
   native-build-scope의 세 sudo gate를 모두 그대로 호출한다.
2. 같은 source·같은 machine policy·같은 실제 cache 경로로 CLI를 다시 실행한다.
   production 결과의 source/index, build/seal, stores와
   `/native/state/cache/cargo-build` 경로를 확인한다.
3. 실제 `--launch --verify-package --paths /native/state/launch-paths.json` 실행.
   machine의 독립 `boot_seed`는 검증한 공개 index의 seed와 같고 실제 Params 경로도
   launcher의 data/params와 일치시킨다. 최초 startup record가 없으면 이 호출에서
   원래 production 정책대로 한 번 더 build 요청 후 startup record를 보존한다.
   generation ID는 seal에 포함되는 요청별 증거 때문에 같다고 가정하지 않는다.
4. `/native/state/reuse-source`에 원본 P의 owned Git clone을 만들고 `README.md`만
   바꾼 실제 docs-only commit을 생성한다. 공개 index bytes와 원본 P는 그대로 두고,
   derived checkout의 CLI로 같은 state/cache/machine SHA의 전체 build/seal을 실행한다.
5. 같은 owned checkout에서 logger `--help` 문자열만 바꾼 실제 후속 commit을 만든다.
   그 source의 CLI 전체 경로를 실행하고 sealed logger의 실제 `--help` 결과도 확인한다.
   원본 readonly mount는 수정하지 않으며 derived SHA/tree를 P라고 재표기하지 않는다.

`--verify-package`는 원래 launcher의 패키지 검증 후 manager 시작 전에 반환한다.
따라서 manager 정상 부팅, hardware/model/녹화/업로드, 실제 AGNOS kernel과 사용자 기기,
update/recovery는 이 driver가 검증했다고 표시하지 않는다. docs/작은 변경 재사용은
아래 실제 receipt 대조까지 성공한 실행에서만 표시한다. 현재 Actions 실행 증거는 없다.
정상 startup의 20초 deadline과 정책은 바꾸지 않는다. `build_timeout_seconds`는
factory admission과 최초/반복/launch/docs/logger 다섯 실행, Git 준비와 receipt 검사를 합친 **단일 payload 예산**이며
startup deadline이 아니다. root watchdog이 같은 host kernel의 절대 monotonic deadline을
전달하고, admission과 각 CLI는 매번 그 deadline의 남은 시간만 사용한다. deadline
이후의 정상 exit도 성공으로 받지 않는다. image 준비는 이 payload 예산 앞에 있다.

watchdog은 payload cgroup 밖에 남아 전체 예산 만료 시 자기 child/cgroup만 종료한다.
payload 예산과 별도로 cleanup에60초를 두고 pidfd의 종료 대기와 cgroup 제거도 이
여유 안에서 제한한다. workflow330분 제한은 그대로이며, image/도구 준비와 artifact
시간도 그 job 제한 안에 포함된다. payload 최대300분을 각 단계에 중복 배정하지 않는다.

각 실행은 실제 production `seed-current-tools.json`, `prepared-inputs.json`,
`built-generation.json`을 읽는다. 요청 source/index, metadata17개 SHA/byte,
machine/bootstrap/tools/native config pin, seed helper SHA와 실제 current tool bundle,
runtime18/compiler9 역할, generation ID를 독립 대조한다. native build receipt에는
helper SHA 필드가 없으므로 이를 만들어 읽지 않는다. 원래 CLI가 호출하는
`openpilot-native-build-scope`의 SHA는 검증된 current tool bundle의 pin과 연결한다.
launch는 새로 남긴 production 전체 build result를 찾아 세 receipt를 동일하게 검사하고,
startup record의 generation/configuration/path·SHA와 그 launch build를 연결한다.

다섯 실행과 receipt 대조가 모두 성공하면 `execution-summary.json`에 실행 범위만
기록하고 `cleanup_verified=False`를 유지한다. root watchdog은 `cleanup.json`에 자기
cgroup 경로/inode와 제거 및 공유 binfmt/controller 불변 증거를 별도로 남긴다. 바깥 job
scope의 기존 workflow cleanup은 별도 단계임도 표시한다. 실행과 cleanup 모두
성공한 뒤에만 최종 `result.json`을 저장하며 `cleanup_verified=True`와 cleanup 증거의
SHA를 연결한다. timeout·receipt 불일치·cleanup 실패 시 최종 성공 result는 없다.
기존 `state/*.json` artifact 경로를 그대로 쓰므로 workflow 변경은 없다.

## 작업별 admission과 cache identity

dispatch의 `operation_estimates_json`에는 다음 필드가 모두 필요하다. 임의 숫자의
기본값이나 고정25/35GiB 기준은 없다. 실행자가 실제 producer 계측/보수적 상한을
대조하여 이번 전체 source build에 맞는 값을 제공해야 한다. 기기 예산으로 재사용하지 않는다.

- `schema=1`
- `maximum_memory_bytes`, `minimum_available_memory_bytes`
- `maximum_growth_bytes`, `minimum_free_bytes`
- `expected_source_growth_bytes`, `expected_compile_growth_bytes`
- `remaining_object_growth_bytes`, `compiler_growth_bytes`
- `build_timeout_seconds` (120..18000; admission과 다섯 실행 전체 예산, cleanup60초 별도)
- `budgets`: production의 schema1, 현재 catalog의 모든 group별
  `group_growth_bytes`, `expected_source_growth_bytes`, `remaining_object_growth_bytes`,
  `estimate_basis`

image 압축본/sparse/raw/추출본과 도구, owned clone 및 두 추가 source의 보존·seal까지
위 전체 증가량·여유에 포함해 사전 확인한다. clone 전에도 명시적인
`expected_source_growth_bytes + minimum_free_bytes`를 요구하며 예산을 자동 확대하지 않는다.
각 production gate는 자체 원래 resource policy를 계속 검사한다. root setup은 job의
실제 소유/inode/lease를 확인한다. 마지막에는 자기 cgroup만 kill/rmdir하고 artifact를
올린 뒤 기존 job-temp cleanup을 사용한다. 다른 process/cache/worktree는 삭제하지 않는다.

Genuine API 검사에서 사용하는 `native-complete-cargo-cache`를 이름만 바꿔 연결하지
않는다. source/target/툴체인/provider 절대 경로와 build recipe identity가 같다는 증거가
없기 때문이다. 이번 driver는 job 간 Cargo cache를 복원하지 않는다. 같은 namespace의
동일 production cache 경로를 사용하며 아래 실제 전송/compile receipt를 대조한다.
동일 경로라는 사실만으로 전송0이나 재사용 성공을 추론하지 않는다.

## 2026-10-30 공개 pin의 docs-only / logger 변경 연결 검증 준비

이 변경은 `tools/ci/verify_native_source_entry.py`와 기존 회귀 파일,
이 문서에 한정한다. runtime/library/build 코드·inputs/catalog/gates/workflow는
변경하지 않는다. 진행 중 E416/ce20 CI는 부모가 담당하며 재시작하지 않는다.

- Git clone/commit은 factory UID1000의 writable owned state에서 실제 생성한다.
  Git hook/signing/LFS smudge를 끄고 로컬 원본 objects를 읽어 사용한다. origin은 원래
  저장소 URL로 설정한다. 매 commit의 실제 parent·tree·변경 경로 한 개·clean 상태와
  index Git blob을 대조하고, 매 phase 뒤 원본 P 및 machine/startup bytes 불변도 확인한다.
  index나 provider를 임의로 생성하지 않으며 원래 source identity 검사를 우회하지 않는다.
- 각 phase의 세 sudo 경로가 남긴 `seed-current-tools`, `prepared-inputs`,
  `built-generation`을 실제 source/config/helper SHA와 결합한다. 입력 결과의 해시 목록으로
  `operations.json`을 검증하고, sealed generation ID/descriptor·source·protocol·ABI와
  현재 catalog의 99 binaries/13 groups, 실제 immutable object bytes를 확인한다.
- repeated/launch/docs는 전 group fresh, 99개 object 동일, 새 binary 저장0을 요구한다.
  각 group의 cache key/target 경로·recipe·Rust/Cargo/toolchain·검증한 입력 tree도 동일해야 한다.
  작은 logger 변경은 실제 normal/target path dependency의 nonoptional reverse closure인
  `loggerd→bootlog→manager`와 catalog의 `07`·`08`·`09`를 대조한다. 세 binary의 Cargo
  rebuild를 요구하지만 ELF hash 판정은 분리한다. `loggerd/build.rs`의 `src` 추적으로
  새 Git commit stamp가 library에 들어가며 bootlog의 `capture()`가
  `Environment::init_data()`로 사용하므로 logger·bootlog 두 object 변경은 필수다.
  manager는 snapshot 경로를 사용하므로 rebuild 뒤 최종 ELF가 같아도 허용한다.
  세 group 안의 무관 binary까지 포함한 나머지96개는 모두 fresh·object 동일이어야 한다.
  `rebuilt_groups`와 `changed_binaries`를 실제 report/object 비교로 각각 기록한다.
  새 sealed logger의 `--help`에는 `generation::tool`과 같이
  `LD_LIBRARY_PATH=logger.parent/lib`를 명시하여 검증한 sealed library를 우선한다.
  loader의 nonzero 종료나 help 문자열 불일치는 거부한다. 변경 source의 새 tools build는 그 source provenance로
  기록하며 이전 bundle의 검증된 origin이 재사용될 수 있음을 별도로 남긴다.
- seed helper/notices, 두 bootstrap의 Rust/Cargo, capnp/codecs/raylib/15 provider,
  FFmpeg/OpenCV headers+provider, Python 준비, model+wheel 환경, source LFS/eGPU model의
  기존 `transferred_bytes` receipt를 읽는다. model처럼 이미 합산한 값은 자식과 중복 합산하지
  않는다. provider의 원래 전체 runtime archive 다운로드도 false여야 한다.
  **17 metadata body의 전송량은 이 큰 입력 전송량과 별도 필드**다. warm phase는 두 값이
  각각0인 경우만 통과하며, 필드 누락이나 bool/음수 counter를0으로 추정하지 않는다.
- phase별 `cli_elapsed_ms`는 CLI 전체 호출의 실제 경과 시간이며, production의
  `elapsed_ms`도 별도로 남긴다. 세 worker의 실제 `resources.jsonl` 최종 success를
  source/config/policy SHA에 결합하고 kernel peak·limit events0·관측 disk 증가량 및
  최소 free를 확인한다. 이 값은 **각 private worker cgroup 범위**이며 전체 CLI/clone을
  합친 동시 memory peak라고 표현하지 않는다. filesystem 수치도 기존 정책대로
  동시 writer를 포함한 관측 free-space 감소이지 hard quota가 아니다.
- `state/{initial,reused,launch-package,docs-only,small-logger}-proof.json`과
  `state/{docs-only,small-logger}-source.json`에 실제 receipt·생성 신원을 보존한다.
  기존 `state/*.json` 및 resource JSONL artifact 경로를 그대로 사용한다.
  summary의 `warm_zero_download_claimed`, `docs_and_small_change_reuse_tested`는
  위 검사를 모두 마친 뒤에만 true이며, 최종 성공 result는 원래 cleanup 검증 뒤에만 생성한다.

필드 계약은 기존 `bootstrap_tools.rs`의 `resource_evidence/preparations/runtime_inputs.files`,
`native_runtime_inputs.py`의 해시된 `operations.json`, `generation.rs`의
`prepared.reports/binaries/abi_report`, `resources.rs`의 최종 journal이다. native build
receipt에는 helper SHA 필드가 없으므로 새 필드를 가정하지 않고 검증한 tool bundle의 실제
scope helper SHA에 원래 호출을 결합한다. 다른 필드가 실제 실행에서 없거나 계약이 바뀌면
해당 경로에서 실패하고 최종 성공/true summary를 만들지 않는다. 새로운 receipt 필드나
기능을 runtime에 추가해 통과시키지 않는다.

기존 테스트 파일에 실제 owned Git 두 commit/원본 보존, counter 누락·metadata/asset 구분,
모든 crate manifest의 실제 normal/target path 전이 closure·07/08/09 rebuild 누락·manager ELF 동일 수용·
무관 group/artifact rebuild 및 무관 binary 변경,
sealed library 환경·loader 실패, resource source/pin/limit 불일치 및
공유 deadline 만료의 거부 회귀를 작성했다. **로컬에서는 AST와 diff만 확인하며 이 회귀를
실행하지 않는다.** 공개 index·작업 예산·실제 factory admission과 이후 Actions 성공은
아직 필요하다. manager/기기/model 실행·녹화/upload·update/recovery 수용을 주장하지 않는다.

## 최소 공유 helper 확장 제안 / 미확정 blocker

기존 transport helper/workflow는 이 변경에서 편집하지 않았다. 기존 `child`는 payload가
고정되어 있고 `probe`는 admission120초로 제한된다. 그래서 새 driver는 fork/identity-map/
private mount/owned cleanup orchestration을 독립 구현하고 인증 image·SUID·QEMU·command·
controller helper를 읽어 재사용한다. 후속 정리 시 공통 helper에 `binds`, **고정된 신뢰
payload argv**, 작업 timeout, 결과 파일과 owned cleanup context를 받는 작은 API를
추출하면 이 중복을 없앨 수 있다. 지금 admission 코드의 의미나 진행 중 CI를 바꾸는
refactor는 하지 않는다.

실제 namespace/binfmt/cgroup mount와 factory sudo의 지원은 admission Actions 결과로
확인해야 한다. 정책 차단이면 공유 host 설정을 바꾸지 않고 정확 실패를 남긴다. 공개
metadata의 실제 발행/Git pin, 전체 build에 맞는 작업별 예산, admission 통과가 첫
실행의 선행 조건이다. 최초 실행에서 새 실제 실패가 나오면 그 원인만 보정한다.
최종 기기 수용은 [#528](https://github.com/bin9208/openpilot-rust/issues/528)에 남긴다.

## 2026-10-30 자원 계측 보존

전체 source entry의 job 전용 `state/generations/**/resources.jsonl`을 artifact에
포함한다. 기존 production bootstrap/build worker가 남기는 kernel memory peak와
filesystem 증가량을 cleanup 전에 보존하여 요청별 receipt와 대조하기 위한 변경이다.
실행 명령·예산·deadline·판정은 변경하지 않는다. 현재 공개 index가 없어 이 실행의
실제 측정값은 아직 없으며, 예산 상한이나 단일 child RSS를 전체 빌드 측정값으로
대신 표시하지 않는다.

## 이번 로컬 확인

Python AST, workflow YAML 및 포함 Bash 문법, diff만 확인한다. 기능 테스트, build,
namespace/probe, CI dispatch/cancel/push/외부 comment는 실행하지 않는다.


## 후속 정적 보강

기존 `abec6aef8`의 결과 저장 순서·중복 timeout 예산·간접 receipt 검증을 위와 같이
보강했다. 기존 transport와 workflow는 바꾸지 않았다. production 필드 정의를 읽어
대조했고 AST/YAML/Bash 구문 및 diff만 확인한다. receipt stub, 기능 실행, 원격 push,
CI dispatch/cancel/재시작은 수행하지 않는다. 실제 성공 증거는 아직 없다.

## 2026-10-29 admission 수정과 최신 main 기반 통합

`34ae6cb1ea3f4cb1536cf40baab7efd08464c445`에서 별도
`codex/feat-613-full-native-entry` 브랜치를 만들고 기존 구현
`abec6aef80abf075fe5b04d268e598f23464efd3`,
`c88e8c91538892d85ccd8e1ebe5a7a85084e1b76`의 세 파일만 순서대로 가져왔다.
이후 수정은 다음 연결로 제한한다.

- 실제 admission 실패에서 보완한 transport의 `readonly_proc_tree()`를 재사용한다.
  `/proc`의 locked 하위 mount를 `--rbind`로 보존하고 복제 mount 전체에만 readonly를
  추가하며 원본 mount 불변을 확인한다. 별도 namespace 설계 변경은 없다.
- workflow와 host preflight/run은 공통 runner 정책의 `self_hosted_only=True`를 요구한다.
  factory 내부의 최소 환경에는 Actions 값을 주입하지 않고 기존 실제 ARM UID·sudo·
  source/index/receipt 검사를 유지한다.
- 고정 `CARGO_BUILD_JOBS=2`를 제거하고 기존 `rust-path`의 실제 CPU 기반 기본값을
  사용한다. 기존 세 production sudo, 전체 deadline, 독립 cleanup 예산은 변경하지 않는다.
- 기존 `OWNED_WORKFLOWS`에 entry workflow만 추가하여 pool label과 동일 저장소 PR
  안전식을 검사한다. 원래 명령·skip 허용 조건과 고정 입력은 바꾸지 않는다.

공개 index는 아직 커밋되어 있지 않다. fake/candidate index를 만들거나 dispatch하지
않았으며 preflight, 기능·빌드·namespace 실행도 하지 않았다. 로컬 확인은 AST,
workflow YAML, 포함 Bash의 `bash -n`, diff에 한정된다. public metadata의 실제 HTTPS
body·source/profile pin 확인과 production receipt 검증은 원래 실행 조건 그대로다.
