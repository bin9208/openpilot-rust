# dev-md의 세 실행 풀과 native ARM 병렬 검사 — #598

## 2026-10-30 — workspace 구형 순서 단언 보완

`c1c78e6f1` [CI40916707849](https://github.com/bin9208/openpilot-rust/actions/runs/40916707849)의
fast job123599508979는110개 중109개 성공·1개 실패였다. 별도 isolation 검사의
workspace `needs: integration-gate` 단언이 남아 있었으므로 새 `check_user_docs` 순서와
기존 route/full·fast 조건을 함께 검사한다. workspace 원래 명령과 최종 gate 단언은
유지한다. 다른 후속 gate의 실패는 이 빠른 검사 실패의 결과이며 runtime 실패로
표시하지 않는다. 로컬은 AST/diff만 확인하고 전체110개는 Actions에서 확인한다.

## 사용자 지시와 범위

사용자가 2026-10-29 GitHub-hosted Actions와 등록 runner `bin-actions`,
`bin-actions2`의 동시 사용을 명시했다. 과거 AGENTS.md의 단일 self-hosted 정책은
이 작업에서 대체된다. 기준은 PR #494 → dev-md의
`28545961a18cd221b40da1369e48560db887f50f`다. 현재 기준 검사
[40871320361](https://github.com/bin9208/openpilot-rust/actions/runs/40871320361)는
보존하며 이 작업에서 취소·재실행·dispatch·push하지 않는다.

GitHub [공식 runner 문서](https://docs.github.com/en/actions/reference/runners/github-hosted-runners#standard-github-hosted-runners-for-public-repositories)는
공개 저장소의 `ubuntu-24.04` X64와 `ubuntu-24.04-arm` ARM64 표준 VM을 지원하며
표준 runner 사용을 무료로 제공한다고 명시한다. ARM64 VM은 AGNOS 기기 검증을
대신하지 않는다. 실제 실행의 source/runner/host/target/native 또는 QEMU를 기록한다.

## 정확한 라우팅

| 실행 풀 | 대상 |
| --- | --- |
| 등록 공통 `[self-hosted, Linux, X64, openpilot-rust]` | workspace/build_release/arm-build, 일반 X64 runtime·memory·빌드, matrix X64 행, 기존 AGNOS prerelease |
| GitHub `ubuntu-24.04` | route/fast-checks/docs/model_web/coordination gate/기존 조건부 delivery 요청 |
| GitHub `ubuntu-24.04-arm` | planner/radard/Xiaoge/CarrotMan/WebRTC/webcam matrix ARM 행, radar-arm, carrot-navi-arm |

등록 이름은 실제 API의 `bin-actions`, `bin-actions2`, `bin-actions3`다. #630이 실행 중인
support-630 전용 runner에 perf를 고정하지 않으며, 공통 label을 만족하는 빈 runner가
받는다. 무거운 X64 검사를 GitHub VM으로 보내던 이전 배분은 이 최신 지시로 대체한다.

matrix는 X64·ARM 두 행을 동시에 허용한다(`max-parallel: 2`). 원래 `fail-fast`
설정과 각 architecture의 시간 제한·환경·compile feature·메모리 검사는 유지한다.
workspace와 release build는 같은 공통 풀의 빈 등록 runner에 분산되고, native ARM과 hosted coordination은 독립 실행한다.

## 의존 관계와 필수 결과

모든 job의 checkout·도구·출력 준비, `needs.*` 참조와 artifact 다운로드를 읽어
검토했다. runtime job 사이에 다운로드나 앞 job 출력 참조가 없고 각 job이 자신의
의존성을 준비한다. 따라서 workspace부터 webcam-runtime까지의 단순 직렬 `needs`를
각각 `[integration-gate]`로 바꾼다. integration 내부의 build_release/model_web/model/
android-core/jetlink-native 역시 각각 docs 성공 후 독립 실행한다. Git update 검사는
그대로 필수다. `check_user_docs`가 route/full/fast 조건을 유지하므로 빠른 branch push에
전체 검사가 새로 열리지 않는다.

`integration-gate`, `fast`(rust checks), `arm64`(rust aarch64 build)의 직접 `needs`,
EXPECTED 목록, 실제 거절 코드와 event/PR 조건은 원문 그대로다. 실패·취소·skip·누락을
성공으로 취급하지 않는다. `arm-build`는 기존대로 `fast` 성공 후 실행하고 `arm64`가
모든 결과와 실제 ARM build를 확인한다. artifact/data 의존을 없앤 변경은 없다.

## 실행 경계와 출력 소유권

`runner_policy.py`는 환경 API `environment_allowed`, `require_environment`,
`job_is_allowed`를 제공한다. self-hosted는 두 등록 이름만, hosted는 실제 Actions의
Linux/X64 또는 ARM64만 허용한다. API job 근거에서는 두 표준 hosted label 또는
등록 self-hosted 이름/기본 label을 확인한다. `self_hosted_only=True`는 hosted를 거절한다.

`local-arm`은 실제 `uname -m`과 명시 target이 같으면 원래 command 파일을 native로
실행한다. native ARM은 prepare 앞의 별도 pinned setup-python/rust-path를 사용하고 QEMU/PRoot/rootfs를
다운로드하지 않는다. X64→ARM만 기존 고정 SHA의 QEMU/PRoot 경로를 유지한다.
`native-apt-shell`은 환경을 확인한 뒤 기존 Bash 옵션과 command bytes로 실행한다.
원래 apt의 명시 architecture와 cross-build의 private APT_CONFIG는 수정하지 않는다.

job-temp의 mktemp/owner marker/BASH_ENV/짧은 Unix socket 경로 및 cleanup 소유권 절차는
그대로다. 각 job의 source/target/artifact는 별도 checkout·임시 디렉터리에 있고 다른
job나 runner의 출력/cache를 삭제하지 않는다. 세 실행 풀이 동시에 쓰더라도 공용
임시 이름이나 동일 artifact 이름에 의존하지 않는다. matrix artifact는 runner 행으로
구분되고 kernel/ASan/Miri/실제 원본 비교 명령은 유지된다.

planner의 CI-only adapter와 host SDK admission도 같은 환경 정책을 사용하도록 한다.
기존 ISA 선택·SDK source·wheel·builder SHA·숫자/허용 오차는 그대로다. LFS 회귀의
Actions-only 조건을 확장해 hosted에서 기존 기능 검사가 skip되지 않게 한다.
실제 runtime 발행 경로는 계속 등록 runner에 두며 기존 dispatch/merge/표시 조건을
보존한다. source-policy에는 새 실행 helper·관련 기록의 정확 경로만 추가한다.
원래 저장소 동기화·publish workflow를 활성화하거나 main599 입력 기능을 이식하지 않는다.

## 정적 확인과 남은 검증

로컬에서는 Python AST, YAML, Bash `-n`, diff 및 기준285와의 구조 비교만 수행한다.
원래 job ID·event·permissions·concurrency·세 gate 목록과 body·모든 원래 run command,
feature/pin/숫자/threshold/검사 순서를 대조한다. 의도한 차이는 runner/직렬 needs/matrix
label과 동시 행 수, native ARM용 추가 준비 단계 및 실행 모드 설명뿐이다.
회귀는 새 라우팅과 실제 환경/API 허용·거절, native/fallback 구분, 모든 필수 gate의
실패 거절을 검사하도록 준비한다. 로컬에서 이 회귀나 기능·빌드·테스트는 실행하지 않는다.
새 정확 SHA의 Actions 성공과 세 풀/native ARM의 실제 증거는 아직 없다. 기준285의
기존 성공을 이 수정본의 성공으로 대신하지 않으며 부모가 통합 후 검증한다.


## 후속: 원래 Python 조건을 보존한 native ARM 사전 준비

부모 검토에 따라 기존 setup-python의 `if: env.LOCAL_CI_ARCH == 'x86_64'`와
기존 run/argv는 기준285 원문으로 유지한다. ARM을 지원하는8개 job의 local-arm
prepare 앞에만 `actions/setup-python@v6`/Python3.12.14와 rust-path를 추가한다.
두 추가 단계의 조건은 모두
`runner.arch == 'ARM64' && env.LOCAL_CI_ARCH == 'aarch64'`다.
따라서 GitHub native ARM이 이미지의 기본 Python에 의존하지 않으며 X64→ARM
fallback은 원래 guest Python pin을 계속 사용한다.

native helper는 `pythonLocation`, PATH, HOME을 덮어쓰지 않고 현재 환경을 Bash에
상속한다. 정책 확인용 `/usr/bin/python3 -I -B -S`는 명시 시스템 Python으로 계속
분리된다. 이는 코드의 정적 확인이며 실제 interpreter/version/실행 metadata를
관측했다는 의미가 아니다. 새16개 uses와 기존 단계의 순서·if·argv를 구조적으로
대조하고 AST/YAML/Bash 문법/diff만 확인했다. 실제 native ARM 검사는 미실행이다.


## 공통 API 확정본 대조

후속으로 `tools/ci/runner_policy.py`를 부모 공통 helper
`14f26458c9321e7a3f905c04946911daef922319`와 정확히 같은 bytes로 맞춘다.
세 함수의 keyword API와 `--self-hosted-only` CLI를 공유하며 malformed mapping/labels와
hosted job의 누락·빈 runner_name도 거절한다. perf의 두 Python adapter는 반환값에
의존하지 않으므로 `require_environment()`의 None 반환과 호환된다.

환경 검사 helper가 실행 성공을 기록하거나 임의 runner 이름을 만들지 않는다.
perf transport의 실행 JSON은 실제 RUNNER_NAME/RUNNER_ENVIRONMENT/uname/source를
자체 job 디렉터리에 기록하고 SDK provenance도 실제 job context를 유지한다.
현재 대조는 코드 읽기이며 실제 hosted/native ARM 실행 metadata를 새로 얻지는 않았다.
공통 API 외 main599의 준비·발행 코드는 이식하지 않는다. 최종 원래 argv/if/feature/
pin/숫자·세 gate 보존은 추가 ARM uses16개와 동등한 matrix label 변경을 제외한
기준285 YAML 전체 구조 대조로 확인했다. 기능·빌드·테스트·원격 CI 조작은 미실행이다.


## 기존 AGNOS prerelease 경로의 두 등록 runner 공유

사용자가 기존 완료 조건의 실제 installer 발행 경로도 두 등록 runner에 배분하도록
명시했다. `.github/workflows/dev-md-runtime.yml`의 verify/package/publish 세 job에서
`runs-on`만 `[self-hosted, Linux, X64]`로 바꾼다. 비어 있는 등록 X64 runner가
받을 수 있고 `package.py`의 실제 실행 admission은 `bin-actions`와 `bin-actions2`
두 이름만 계속 허용한다. workflow event/if/needs/source SHA/필수 CI gate/기존
발행 조건·토큰 권한·모든 step와 argv는 기준 원문 그대로다. 이 기록은 발행 실행이나
후보 완성을 의미하지 않으며 새 workflow를 활성화하는 변경도 아니다.

기존 Carrot/NAS 동기화·publish workflow와 ci.yml의 조건부 delivery 요청은 수정하지
않는다. YAML을 읽어 세 runs-on만 복원하면 기준 workflow 전체가 같음을 대조하고,
관련 guard AST와 Bash 문법/diff만 확인한다. 실제 기능·빌드·테스트·발행·CI 조작은
수행하지 않았다. native ARM 준비 보강 및 공통 runner API 후속 커밋과 함께 통합한다.


## 기존 실행을 보존하는 concurrency 세대 분리

새 사용자 지시에 따라 ci.yml의 workflow-level concurrency를 다음으로 바꾼다.

- group: 기존 event/PR-or-ref 구분 뒤 `-parallel-v2`를 붙인다.
- `cancel-in-progress: false`, `queue: max`.

기존285 등 이전 source는 원래 concurrency group에 남고 현재 수정은 별도 group을
사용한다. 새 hosted/native ARM job이 이전 단일 runner workflow의 concurrency
대기열 때문에 시작하지 못하는 문제를 피한다. 기존 실제 실행이나 queued run을
취소·재시작하지 않는다. 같은 새 group 안에서는 실행을 보존하며 순서대로 기다린다.
이 단락은 앞의 원래 concurrency 보존 기록을 새 명시 정책으로 대체한다.

로컬 구조 대조에서 이 후속 변경은 ci.yml의 concurrency 세 필드뿐이며 모든 job,
gate, 조건, step와 argv는 직전 커밋과 같다. queue 세대 분리 회귀를 추가했지만
실행하지 않았다. AST/YAML/diff만 확인했으며 원격 등록·CI 실행 결과는 미확인이다.


## 정책 추적 #635와 발행 write group 보존

현재 사용자 runner 병렬 정책은 [#635](https://github.com/bin9208/openpilot-rust/issues/635)에서
추적한다. `-parallel-v2` 세대 분리는 ci.yml의 읽기 검증 그룹에만 적용한다.
기존 dev-md-runtime의 발행 보호 그룹 `dev-md-runtime-${{ github.ref }}`와
`cancel-in-progress: false`는 기준285 원문 그대로이며 세대별로 나누지 않는다.
따라서 새 runner 배분이 서로 다른 그룹의 발행 작업을 추가로 열지 않는다.
부모 main의 native-input-publication 전역 write group도 이번 perf 변경 범위가 아니다.
관련 회귀는 두 concurrency 용도를 구분하며 실행은 아직 하지 않았다.


## 최신 #635 정책: 세 등록 runner의 무거운 X64 공통 풀

저장소 runner API를 읽어 `bin-actions`/`bin-actions2`가 online/busy,
`bin-actions3`가 online/idle이며 세 대 모두 self-hosted/Linux/X64 label을 가진 것을
확인했다. 이는 해당 조회 시점의 상태이며 새 job의 실행 또는 성공 증거가 아니다.
`bin-actions3`에는 별도 사용자 label이 없어 특정 dev-md-local/support-630 label로
고정하면 선택할 수 없다. 따라서 무거운 X64 job과 실제 AGNOS 발행 경로는
`[self-hosted, Linux, X64]`로 유지/변경하고 환경·root admission은 등록 이름 세 개만
허용한다. #630의 actual 실행을 취소하거나 runner 등록·label을 수정하지 않는다.

matrix X64 행은 `self-hosted-x64`라는 논리 이름으로 기록하고 `runs-on` 식에서 공통
세 label을 선택한다. ARM 행은 계속 ubuntu-24.04-arm의 native ARM이다. X64 전용
sanitizer 조건도 같은 논리 행에 대응시키며 원래 실행 argv·feature·pin·숫자·허용
오차·원본 Python 조건과 메모리 검사를 보존한다. 세 gate의 필수 목록과 본문,
parallel-v2 validation group 및 기존 발행 write group도 그대로다.

runner_policy의 함수/CLI API는 그대로 유지하고 allowlist에 실제 API 이름
`bin-actions3`만 추가한다. 실제 package root admission 및 관련 회귀도 세 이름을
대조한다. 상속 Carrot/NAS sync/publish와 main599 입력 기능은 변경하지 않는다.
로컬 AST/YAML/Bash 문법/diff 및 원문 구조 비교만 수행했으며 실제 기능·빌드·테스트와
원격 push·dispatch·취소·재시작은 하지 않았다. 새 정확 SHA의 실행 근거는 부모 검증이
필요하다. 이전 두 runner/hosted-X64 우선 기록은 최신 사용자 정책으로 대체된다.


## 최종 공통 label과 최소 네 hosted native ARM 준비 작업

최신 사용자 지시의 공통 label `openpilot-rust`를 실제 CI와 기존 AGNOS delivery의
모든 공통 self-hosted runs-on 및 matrix X64 선택식에 추가한다. 이름 allowlist는
이미 API에서 확인한 세 등록 이름을 유지한다. 부모가 실제 세 runner에 공통 label
적용을 확인한 뒤 push한다. 이 로컬 작업에서는 runner label API를 변경하거나
적용 완료를 주장하지 않는다.

GitHub hosted는 fast/docs/coordination과 native ARM에 분담한다. 현재 그래프에서
planner/radard/Xiaoge/CarrotMan/WebRTC/webcam의 native ARM6행과 radar-arm/
carrot-navi-arm2개, 총8개 hosted 작업은 같은 integration-gate 성공 직후 함께
준비된다. 이들끼리의 직렬 needs는 없고 matrix max-parallel2가 X64와 ARM을 동시에
허용한다. 따라서 integration 이후 최소4개의 hosted ready 작업과 self-hosted3대의
동시 분산이 가능한 구조다. 실제7개 동시 실행을 관측한 결과는 아직 없다.

integration 전의 fast/docs 선행 정책은 이번에 풀지 않는다. 기존 모든 check/gate
목록·검사 조건·원래 명령·pin/threshold·공유 cache 및 job owner/cleanup 절차는
직전 커밋 그대로다. validation의 parallel-v2와 발행의 기존 write group도 보존한다.
정적 YAML 대조·AST·Bash 문법/diff만 확인하고, 최소4개 hosted ready 작업의 그래프
회귀를 추가했다. 기능·빌드·테스트·원격 push/CI 조작은 수행하지 않았다.


## CPU 병렬도: 고정2/1 제거와 실제 affinity 기본값

추가 사용자 지시로 Cargo/libtest의 고정 worker 제한을 제거한다. ci.yml 및 기존
AGNOS delivery·Bosch source reference·media fixture·self-hosted readiness의
`CARGO_BUILD_JOBS: 2`, docs command의 inline `CARGO_BUILD_JOBS=2`, 저장된
CarrotMan job template의2를 제거한다. disk-space bootstrap·실제 AGNOS build·
Xiaoge native memory의 child 환경도 고정2를 덮어쓰지 않고 원래 환경을 상속한다.

등록 runner에 사용자 Cargo config의 `jobs=2`가 남아 있을 수 있으므로 env 제거만으로
끝내지 않는다. 공통 rust-path action은 `os.sched_getaffinity(0)`를 우선 사용하고,
사용 불가 시 `os.cpu_count()`로 사용 가능한 CPU 수를 산출한다. 각 환경 변수가
정의되지 않았을 때만 `CARGO_BUILD_JOBS`/`RUST_TEST_THREADS`에 그 값을 설정해
GITHUB_ENV와 현재 shell export에 반영한다. 명시된 환경 override는 빈 값까지도
임의로 바꾸지 않는다. CPU 수를 판별할 수 없으면 고정1/2로 숨겨 진행하지 않고
실패한다. 로컬 `/home/fma1/.cargo/config.toml`의 `jobs=2`는 수정하지 않는다.

누락된 fast-checks의 간접 disk guard bootstrap, scons/native model build,
readiness ARM transport와 delivery publish에도 기존 rust-path를 연결한다.
모든 명시·간접 Cargo CI 소비자의 common action 사용을 정적으로 대조했다.
기존 action의 Rustup URL/SHA·space admission·cache·job owner·cleanup은 유지한다.

직접 `--test-threads=1`은 Jetlink ASan, planner/radar Miri, radar ARM 실제 test,
Xiaoge OpenCV/JPEG native suite에서 제거한다. 읽기 감사에서 유지해야 할 테스트
순서 의존 근거를 찾지 못했다. Jetlink의50ms deadline과 Rust fixture 내부의
기존 mutex, sanitizer/Miri 옵션·대상·기존 유효성/허용 오차는 바꾸지 않는다.
별도 `RUST_TEST_THREADS` 고정 기본값이나 저장소 .cargo build.jobs는 없었다.
읽은 vendor RTC .cargo 설정은 ARM 암호화 rustflags만 지정하며 그대로다.

perf에는 이번 변경에 영향을 받는 전체 docs command frozen hash 검사가 없었다.
기존 native disk guard의 raylib/reservation/template 인자 hash 입력은 그대로이고
독립 hash pin은 수정하지 않는다. 프로덕션 source guard·acceptance는 변경하지 않는다.
기존 test_rust_path에 actual CPU/default/명시 override와 helper child 환경 상속 회귀를
추가했으나 실행하지 않았다. AST/YAML/Bash 문법/diff와 원문 구조 비교만 수행했으며
기능·빌드·테스트·원격 push/CI 조작과 추가 실패 원인 수정은 수행하지 않았다.


## 실제 unsupported PPA 404: job 소유 공식 Ubuntu APT view

[#635](https://github.com/bin9208/openpilot-rust/issues/635)의 실제
[telemetry job123482820046](https://github.com/bin9208/openpilot-rust/actions/jobs/123482820046)에서
bin-actions2의 기존 appimagelauncher-team/stable Ubuntu noble PPA Release404로
apt update가 exit100인 실패를 사용자와 부모가 확인했다. 공유 /etc/apt 설정은
삭제·수정하지 않고 기본 CI APT만 job 소유 source view를 사용하도록 한다.

native-apt-shell은 root-owned 공식 Ubuntu archive/security/ports source만 읽어
기존 deb822 ubuntu.sources와 legacy sources.list 형식을 그대로 보존한 private
view를 만든다. HTTPS URI와 Signed-By/architecture/suite/component 필드는 원문을
유지하고 signature 우회 source는 거부한다. PPA source는 포함하지 않는다.
source hash와 실제 runner/job/source metadata를 job 임시 경로에 남긴다.

명시 architecture 인자는 원문 그대로 전달하면서 기본 source view를 적용한다.
APT_CONFIG 또는 명시 config/source 설정이 있으면 해당 호출 전체를 원래 방식으로
통과시킨다. 실제 host의 /usr/bin/dpkg architecture와 /usr/bin/uname 결과 일치를
확인한다. PATH의 fixture uname은 호출하지 않는다. header는 qemu-user 모드만
제외하므로 host X64→ARM bootstrap APT도 보호하고, 명시 qemu-user인 guest 내부는
원래 동작을 유지한다. job-temp는 기존 owner/cleanup 절차를 유지하며 이 wrapper를
BASH_ENV에 연결한다. 기존 API/환경/명령/패키지 pin과 공유 설정은 변경하지 않는다.

로컬에서 Python AST·YAML·Bash -n·diff만 확인했다. 현재 parser/재귀 회귀는 작성만
되어 있고 실행하지 않았으며, 로컬 apt·기능·빌드·원격 CI 조작은 하지 않았다.
실제 PPA404 해소와 기존 fake-uname 회귀 성공은 부모의 Actions 검증이 남아 있다.


## APT list cleanup과 package metadata도 job 경로에 격리

부모 검토에 따라 source view에 더해 `view/lists`, `view/cache`를 job 소유
디렉터리로 만들고 기존 경로의 symlink/foreign owner를 먼저 거절한다. 기본 APT에만
`Dir::State::lists`, `Dir::Cache::pkgcache`, `Dir::Cache::srcpkgcache` 세 인자를
추가한다. 따라서 기본 apt update의 List-Cleanup 및 package metadata 작성 대상도
job private 경로이며 공유 host PPA lists/cache를 대상으로 하지 않는다.
명시 source/config 호출의 원문 통과와 architecture 값·parser·서명·mode guard는
유지한다. 로컬 Bash syntax·내장 Python AST·diff만 확인했으며 apt/기능 실행이나
공유 설정 변경·원격 CI 조작은 하지 않았다.


## GitHub-hosted 기본 mirror 보존

실제 hosted navigationwire job123495106545 (`GitHub Actions 1000030444`)에서
`no signed official Ubuntu archive/security/ports source was found`가 발생했다.
GitHub 이미지의 기본 source는 mirror+file 등을 사용할 수 있으므로 self-hosted PPA
격리용 allowlist를 넓히지 않는다. source/lists/cache view 생성은
native-default이면서 RUNNER_ENVIRONMENT가 self-hosted일 때만 적용한다.
GitHub-hosted native-default는 원래 이미지의 source를 보존하고, 명시 architecture가
없을 때만 기존 native architecture 기본 옵션을 사용한다. 명시 config/source 호출과
명시 architecture 값은 원문대로다. parser/URI allowlist와 기존 세 parser/재귀 회귀는
변경하지 않는다. Bash syntax·내장 Python AST·diff만 확인했으며 기능·apt 실행과
원격 push/CI 조작은 하지 않았다. 실제 hosted/등록 runner 성공은 부모 Actions 검증이
필요하다.


## 2026-10-30 — 네 hosted runtime job으로 실제 CPU 추가 사용

사용자가 bin-actions2/3은 동일 CPU16 PC라고 확인했다. 고정2/1 제한은 해제한 상태로
유지하고 support·telemetry·web upload timeout·joystick의 네 독립 X64 검사를 hosted
ubuntu24.04에 배치한다. 대형 workspace/model/memory/AGNOS는 세 self-hosted 공통
pool을 우선 사용하며 native ARM·이 네 검사와 병렬 실행한다. 기존 integration 이후
의존과 전체 최종 gate·argv·허용 기준·target 소유권·publication write group은
그대로다. AST/YAML/diff만 확인하며 실제 속도·성공은 해당 source의 Actions에서 확인한다.

## 2026-10-30 — 독립 일반 X64 runtime 18개를 hosted로 추가 분산

사용자는 bin-actions2/3이 같은 PC여서 세 등록 runner가 실제로는 두 PC이며,
hosted 작업 종료 뒤 일반 X64 작업 약30개가 기다리는 병목을 확인했다.
기준 `070ea919977d0c49283b0e73b9948a5165cf4f95`의 `ci.yml`에서 다음18개 job이
실제로 존재하고 main `rust.yml`의 job ID·이름과 일치함을 읽어 대조했다.
이 job들의 `runs-on`만 `ubuntu-24.04`로 바꾼다.

```text
encoder-runtime, camera-runtime, panda-runtime, selfdrive-runtime,
radar-runtime, navd-runtime, carrot-navi-runtime, startup-runtime,
startup-services, sensor-audio, gnss-runtime, controls-runtime,
estimation-runtime, ui-connectivity, platform-runtime, hardware-runtime,
logger-runtime, athena-runtime
```

기존 hosted4개 support/telemetry/web-upload-timeouts/joystickd는 보존한다.
workspace·ARM build·model·memory·card·UI 및 oracle matrix의 X64·AGNOS/발행은
원래 경로 그대로다. needs·실행 argv·pin·feature·env·원본 command hash/baseline·
필수 gate와 수용 기준은 변경하지 않는다. routing 회귀는22개 hosted runtime을
명시적으로 대조하고 estimation-runtime의 이전 self-hosted 기대만 옮긴다.

로컬에서 Python AST·YAML·181개 Bash 구문 및 기준070 대비 diff 검사를 통과했다.
18개 selector를 원래 값으로 복원한 정적 구조가 기준070 전체 workflow와 일치했다.
기능·빌드·원격 push·CI dispatch/cancel은 하지 않으며 기준070의 실행은 부모가 관리한다.
dev-md 변경을 dev/main에 통합하지 않고 새 정확 source의 결과는 Actions에서 확인한다.

## 2026-10-30 — 독립 runtime38개를 docs 이후 integration 검사와 병렬 시작

기준 `8932dc4aa5ee06e93fb0339ad9905ef1a1a729ff`의 PR CI
[40910714151](https://github.com/bin9208/openpilot-rust/actions/runs/40910714151)는
일반 X64의 hosted 배분 뒤에도 build release 등 self-hosted 선행 작업을 기다렸다.
runtime job38개 모두 `needs: [integration-gate]`여서 이 단계 전에는 시작할 수 없었다.

38개 job과 호출하는 공통 job-temp/rust-path/local-arm action을 읽어 대조했다.
다른 job의 `needs` 출력 참조나 artifact 다운로드 소비는 없고 각 job이 독립 checkout과
자기 tool/provider/fixture 준비를 갖는다. 이38개의 `needs`만 `[check_user_docs]`로 옮겨
기존 Git/model/build release 검사와 병렬로 시작한다. 선행 산출물 때문에 원래
integration 의존을 유지해야 하는 runtime job은 발견하지 못했다.

`check_user_docs`의 `[route, fast-checks]`와 `route.full == 'true'` 조건은 그대로다.
따라서 fast-only feature push, 다른 저장소 fork, dev 대상 차단은 기존 경로로 유지한다.
38개 job의 if·runner·명령·env·feature·원본 pin·수용 기준을 바꾸지 않는다.
최종 integration/fast/arm64 gate의 전체 needs·EXPECTED 목록과 실제 실패 거부 body,
`arm-build: [fast]`, `runtime-delivery: arm64` 및 발행 workflow는 그대로 보존한다.
기존 needs 회귀의 기대값만 이번 순서로 바꾸고 docs 선행 조건도 명시 대조한다.

로컬에서 Python AST·YAML·386개 Bash 구문·기준8932 대비 diff 검사를 통과했다.
38개 needs를 복원한 전체 workflow 구조와 모든 최종 gate가 기준8932와 동일했고,
의존 순환이 없으며 기존 명령·환경·runner가 보존됨을 정적으로 확인했다.
기능 실행·push·CI dispatch/cancel은 하지 않으며 기존8932 실행은 부모가 관리한다.
dev-md/runtime 소스를 dev/main과 섞지 않고 실제 새 source 결과는 Actions에서 확인한다.

## 2026-10-30 — ARM Xiaoge clipping fixture의 strict config 거부 분리

source `61df63fa492219867bc252c4245d51a06a25c8ab`의 PR CI
[40918034918/job123606389743](https://github.com/bin9208/openpilot-rust/actions/runs/40918034918/job/123606389743)는
`composite_bsd_tensor_preserves_padding_chroma_mask_and_letterbox_bits`에서
`Config::normalize`가 `poly_left point is outside the image`를 반환해 실패했다.
원래 `[-2,1]` 및 오른쪽 polygon의 image 밖 좌표는 geometry clipping 검사용이며,
최근 strict config 입구가 geometry 비교 전에 이를 거부한 것이다.

테스트에서 같은 좌표의 typed Config를 직접 구성하고, 별도 단언으로 그 값을
`serde_json::to_value`로 직렬화한 입력이 normalize에서 정확한 기존 오류로 거부됨을
확인하도록 한다. runtime Config 정책·geometry·frozen oracle은 변경하지 않는다.
세 `(width,height,stride,gap)`인 `(20,12,23,5)`, `(12,20,16,8)`, `(20,20,20,0)`과
양쪽 bounds/mask·tensor shape/모든 float bit 비교, padding/chroma/letterbox,
UV 부족·겹침·overflow 거부 단언은 원문 그대로 유지한다.

정상 config는 기존 `rust/tools/check_xiaoge_policy.py`의 default/valid config case가
`examples/xiaoge_policy.rs`의 Config::normalize 경로로 원본과 비교하며,
`check_xiaoge_inference.py`도 정상 config로 inference trace를 실행하는 경로가 있다.
별도 positive fixture를 중복 추가하지 않는다. 이번 로컬에서는 해당 기능을 실행하지 않고
읽기로만 대조했다. 변경 Rust 파일의 Rustfmt 및 기준61df 대비 diff 검사를 통과했고,
runtime 소스와 frozen support 파일에 차이가 없음을 확인했다. 실제 ARM/X64 결과는
새 정확 source의 Actions에서 확인한다. push·CI dispatch/cancel은 부모가 담당한다.

## 2026-10-30 — Webcam X64 source oracle의 선택 Python loader 보존

시작 시 전용 perf checkout과 원격 PR #494의 head가 모두
`90e2f5e58e16c69d217c0da88ba8acb7a2f93038`이며 PR base가 `dev-md`임을 확인했다.
[40922150657/job123623233132](https://github.com/bin9208/openpilot-rust/actions/runs/40922150657/job/123623233132)는
`bin-actions3`에서 2026-10-29 21:15:48 UTC에 실패 종료했다.
완료된 job 로그155,961bytes를 전용 scratch
`.analysis/scratch/2026-10-30-598-perf-webcam/90e-webcam-job123623233132.log`에 보존했다.

실제 실패는 비교 step12의 첫 `webcam_camera_compare.py:12`에서
`ModuleNotFoundError: No module named 'av'`다. pixel/frames/IPC/lifecycle 비교 전에
source oracle import에서 중단됐으므로 픽셀 불일치나 runtime 기능 결함으로 분류하지 않는다.
같은 로그628행은 `av==16.1.0`이 setup-python3.12.14의 x64 site-packages에 이미 있다고
기록한다. provider/IPC 준비 시693행에는 그 Python의 `lib`가 `LD_LIBRARY_PATH`에 있지만,
workflow의 GITHUB_ENV 덮어쓰기 뒤927/1204행에는 OpenCV provider lib만 남는다.
현재 source의 해당 printf와 실제 로그의 명령이 같아 선택 Python library 경로 손실을
확정했다. 실패 process의 실제 loader mapping/sys.path는 원래 로그에 없으므로
어느 대체 libpython을 로드했는지나 이 수정의 실제 성공은 아직 단정하지 않는다.

같은 source-oracle 환경 문제에 적용돼 있던 main의 좁은 CI 단계만 dev-md workflow에
추가한다. `LOCAL_CI_ARCH != aarch64`일 때 setup-python의 `pythonLocation/lib`를
현재 provider library 경로 앞에 보존하고, 그 명시 interpreter의 `av` import·실행 경로·
버전·av 버전을 기록한 뒤 후속 단계에도 같은 loader 경로를 넘긴다.
main runtime·다른 common/캐시 branch·공통 shell helper를 가져오지 않는다.
ARM 행과 원래 dependency pins/requirements·provider source SHA·모든 기존 run body,
픽셀/프레임/IPC/lifecycle/timeout·정확 비교·최종 source gate는 그대로 유지한다.

로컬은 YAML/추가 inline Python AST·Bash 구문·diff만 검사한다. 새 단계 하나를 제거한
전체 workflow 구조가 기준90e와 같은지 정적으로 대조한다. 기능·build·test 실행이나
캐시/다른 CI 정리·push·dispatch/cancel·원격 글은 하지 않는다. 기존90e의 성공 결과를
수정 source의 성공으로 바꾸지 않으며, 실제 비교 통과는 다음 정확 source의 Actions에서
확인해야 한다. 이 perf 실패를 #598 전체 완료나 실기 수용 완료로 표시하지 않는다.

## 2026-10-30 — Xiaoge X64 source oracle의 선택 Python loader 보존 (#614)

시작 시 원격 PR #494 head와 전용 perf checkout이 모두
`ce20f2582dfdc5ca77776e58ba23a52c28f90655`임을 확인했다.
[40957390360/job123744005591](https://github.com/bin9208/openpilot-rust/actions/runs/40957390360/job/123744005591)는
`bin-actions`에서 2026-10-30 00:58:44 UTC에 실패 종료했다. 완료 로그의 실제 checkout은
`8339fd9df14ecb78fb4be9fe49e3049a7ab816a5`이며 두 parent가 PR의 base
`dc95138076836ea3a5c1b6c27262be3f3b912f71`과 위 head임을 API로 확인했다.
완료 로그와 artifact `12946115927` (`rust-xiaoge-self-hosted-x64-evidence`)를
전용 scratch의 `ce20-evidence/`에 보존했다.

직접 확인된 실패는 artifact의 `xiaoge-comparison/policy.log`에 기록된
`check_xiaoge_policy.py:14`의 `ModuleNotFoundError: No module named 'numpy'`다.
`commands.json`은 첫 policy subprocess의 returncode1만 기록한다. 이후 source pixel·
tensor·IPC·lifecycle 및 CPU equivalence 비교를 완료하거나 불일치를 발견한 결과가 아니다.
설치 로그에는 `numpy==2.5.3`가 선택한 toolcache Python3.12.15의 site-packages에
이미 설치돼 있다. provider 준비 전에는 같은 Python의 `lib`가 `LD_LIBRARY_PATH`에
있지만, GITHUB_ENV 덮어쓰기 뒤 실패 단계에는 OpenCV provider lib만 남는다.
Python library 경로 소실은 로그와 현재 workflow에서 확인했고, 그에 따른 loader 선택
변화가 import 실패의 원인이라는 설명은 가설이다. 실제 로드된 libpython mapping과
sys.path는 로그에 없으므로 특정 대체 library를 로드했다고 단정하지 않는다.

기존 [#614](https://github.com/bin9208/openpilot-rust/issues/614)의 같은 loader 회귀로
연결한다. provider 준비 직후 X64에서만 선택 `pythonLocation/lib`와 기존 provider
library 경로를 함께 보존한다. 명시 interpreter로 numpy import·실행 경로·버전을
기록하고 같은 LD_LIBRARY_PATH를 GITHUB_ENV에 넘기는 7줄 단계 하나를 workflow에 추가한다.
기존 dependency pin·provider 경로/OpenCL·source oracle·runtime·ARM/native argv와
다른 job은 변경하지 않는다. #497/#506의 중단 트랙은 재개하지 않는다.

후속 읽기 감사에서 `check_xiaoge_runtime.py`와 `check_xiaoge_lifecycle.py`가 source
Python에도 provider-only 환경을 넘기고, `check_xiaoge_live.py`가 두 Python wrapper에
같은 환경을 넘기는 경계를 확인했다. 이는 직접 실패한 policy import와 구분되는
소스 확인 결과다. 이 후속 peer 경계의 실제 실패를 이번 Actions에서 관측한 것은 아니다.
`c929c0fe1540d005def3469aff4304e4b81d93db`의 해당 세 파일 수정 전례만 읽었으며,
옛 고정25GiB 정책·broad #506 helper/probe 모듈이나 다른 트랙 변경은 가져오지 않았다.

세 caller에서만 짧은 표현식으로 provider 경로를 먼저 두고 기존 LD_LIBRARY_PATH의
비어 있지 않은 tail 항목을 보존한다. runtime/lifecycle은 source peer에만 적용하고
native peer는 원래 provider-only 환경을 유지한다. live의 source/native Python wrapper는
tail을 보존하되 실제 native child를 실행할 때만 명시적으로 provider-only로 복원한다.
source/native argv·timeout·payload·실제 native maps의 libpython 부재 단언과
원래 동등성 비교는 그대로다. 별도 테스트나 공통 환경/probe 모듈은 추가하지 않는다.

로컬에서 수정 Python/추가 inline Python AST·YAML·추가 Bash 구문·diff 검사를 통과했다.
추가 7줄을 제거하면 ce20 workflow와 바이트·전체 YAML 구조가 같고, 세 Python 파일도
승인된 환경 연결 변경만 제거하면 ce20 원문과 일치함을 정적으로 확인했다.
기능·unit·build는 실행하지 않았다.
부모 독립 review 전 commit/push·CI cancel/rerun·병합·새 Actions 실행은 하지 않는다. 기존 ce20의
남은 실행과 다른 세션·공유 cache는 보존한다. 실제 import·원본 비교와 전체 필수 검사의
통과는 부모 diff review 후 승인된 다음 정확 source의 Actions에서 확인해야 한다.

## 2026-10-30 — Xiaoge 준비 단계의 기존 정책 검사 대응 (#614)

source `1c5d326bb2e81d6680343de840b0aed6bff556c2`의 최신 PR CI
[40984000661/job123832713478](https://github.com/bin9208/openpilot-rust/actions/runs/40984000661/job/123832713478)는
fast 검사110개 중1개가 실패했다. 기존 `test_rust_isolation.py:372`의 모든 Xiaoge
run 단계에 대한 `if` 부재 단언이 새 X64-only Python 준비 단계를 거부한 결과다.
완료 로그의 checkout은 `78ddb028343868b987d4b9ae64b60506abb77169`로 위 source와
base `dc95138076836ea3a5c1b6c27262be3f3b912f71`의 검증용 병합이다.
앞선 동일 source PR CI `40983838085`도 같은 오류로 실제 failure 종료했다.
ce20 두 번째 실행 `40957600548`은 snapshot 보존 후 정상 취소해 completed/cancelled를
확인했다. 완료된 두1c5 failure를 cancelled로 바꾸지 않으며 중복 정리는 재동작하지 않는다.

기존 `test_xiaoge_requires_both_architectures_and_instrumented_native_libraries` 안에서만
새 준비 단계의 정확 이름 유일성과 `{name, if, env, run}` 전체 dict를 검증한다.
정확 X64 조건·LD 환경·명시 interpreter numpy import·GITHUB_ENV persist를 확인한
그 객체 하나만 `if` 부재 단언의 예외로 둔다. 원래 source/build/compare/memory run의
`if` 부재와 모든 run의 `continue-on-error` 금지, 양쪽 architecture·필수 명령·artifact
검증은 유지한다. workflow/runtime·다른 테스트는 변경하지 않는다.

로컬 AST·YAML·추가 run body의 Bash 문법·diff 검증을 통과했다. 해당 함수 외 AST와
workflow/runtime이 불변임을 정적으로 확인했으며 unit·기능·build는 실행하지 않았다.
부모 diff review 전 commit/push·PR 본문/제목 edit·Actions cancel/rerun/dispatch는 하지 않는다.
이번 정책 보정과 원래 loader 수정의 실제 성공 및 원본 비교·memory·전체 ARM 결과는
다음 정확 source의 Actions 검증 대기 상태다. 현재 main/S·Genuine·다른 checkout은 변경하지 않는다.
