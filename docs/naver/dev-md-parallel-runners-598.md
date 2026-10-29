# dev-md의 세 실행 풀과 native ARM 병렬 검사 — #598

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
| GitHub `ubuntu-24.04` | workspace, 일반 X64 runtime·memory·정책 검사와 gate |
| GitHub `ubuntu-24.04-arm` | planner/radard/Xiaoge/CarrotMan/WebRTC/webcam matrix의 ARM 행, radar-arm, carrot-navi-arm |
| `bin-actions2`: self-hosted/Linux/X64/support-630 | 원래 `build_release` 전체 setup/scons/모델·기존 Python 통합 검사 |
| `bin-actions`: self-hosted/Linux/X64/dev-md-local | 원래 private ARM sysroot를 사용하는 `arm-build`; 기존 조건부 runtime-delivery 요청 |

matrix는 X64·ARM 두 행을 동시에 허용한다(`max-parallel: 2`). 원래 `fail-fast`
설정과 각 architecture의 시간 제한·환경·compile feature·메모리 검사는 유지한다.
workspace는 별도 GitHub VM에서 실행하므로 긴 release build 및 ARM QEMU에 막히지 않는다.

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
