# #523 최초 native ARM 3개 main 통합 후보

2026-10-25. #523/#535의 GitHub-hosted 실행 정책에 따라 Planner·CarrotMan·WebRTC의 기존 ARM 명령을 main Rust gate의 동일-source reusable workflow로 연결했다. 새 통합의 기능·빌드·정책 테스트는 **UNRUN**이다. 부모가 감사한 CarrotMan `40151446363`, Planner/WebRTC `40153172365`의 actual 후보 결과를 새 source의 PASS로 소급하지 않는다.

## 호출과 필수 결과

main `.github/workflows/rust.yml`은 `native-arm` job에서 `./.github/workflows/rust-native-arm.yml`을 호출한다. caller에는 조건·continue-on-error·선택 input이 없다. 같은 commit의 callee는 고정 `ubuntu-24.04-arm`에서 다음 3개 leaf를 각각 실행한다.

| family | main의 기존 x64 경로 | callee native 경로 |
| --- | --- | --- |
| Planner | local-x64, 기존 45분 expression 유지 | 원래 90분 |
| CarrotMan | x86_64/local-x64, 기존 25분 expression 유지 | 원래 60분 |
| WebRTC | local-x64, 기존 45분 expression 유지 | 원래 90분 |

main `jobs.fast`의 기존 41개 needs/env/test-success는 그대로이며 `NATIVE_ARM`을 42번째 필수 결과로 추가했다. callee의 별도 `native-arm-checks`는 `always()`로 3개 leaf의 결과를 각각 success로 요구한다. failure/cancelled/skipped/빈 값/누락을 PASS로 바꾸지 않는다. 두 aggregate의 조건과 결과 결합을 native policy와 mutation tests가 검사한다.

callee는 `on.workflow_call`만 가지며 `contents: read`, Rust `1.94.0`을 직접 선언한다. caller env가 전파된다고 가정하지 않는다. callee concurrency는 `rust-native-arm-...`로 main과 구별하고 cancel false/queue max를 유지한다. 원래 `rust checks` 이름과 main의 push/PR dev·main 및 workflow_dispatch triggers는 바꾸지 않았다.

## source와 호스트 경계

첫 Deny는 조건 없이 fork PR을 명시 실패시킨다. 그 뒤 모든 source 단계의 trusted predicate와 기존 always/outcome/cleanup 조건을 유지한다. 실제 GitHub-hosted Linux ARM64·uname Linux/aarch64·정확 repository·native intent를 검사한다. runner label은 literal `ubuntu-24.04-arm`이며 동적 runner나 x64 fallback이 없다.

main 경로는 `GITHUB_WORKFLOW_REF`가 실제 `bin9208/openpilot-rust/.github/workflows/rust.yml@GITHUB_REF`인 경우에만 사용한다. push는 dev/main, PR은 `refs/pull/<양의번호>/merge`와 base dev/main 및 event의 head repository 동일성을 요구한다. `CI296_PR_HEAD_REPOSITORY`는 `${{ github.event.pull_request.head.repo.full_name }}`에 직접 바인딩한다. 일반 `trusted=true` 값만으로 통과할 수 없다. workflow_dispatch는 기존 repository의 선택 branch/tag ref를 유지한다.

`tools/ci/native-arm`은 prepare·command·finish 모두에서 같은 main context를 검사한다. 기존 candidate 2개 workflow는 수정하지 않았으며 해당 caller와 codex branch push 제한을 유지한다. 새 pilot workflow를 후속 등록할 때에는 그 별도 caller 경로를 명시적으로 검토·추가해야 한다. 이번 변경이 임의 candidate workflow를 허용하지 않는다.

공식 근거: [reusable github context는 caller에 연결](https://docs.github.com/en/actions/reference/workflows-and-actions/reusing-workflow-configurations#github-context), [GITHUB_WORKFLOW_REF의 실제 workflow ref](https://docs.github.com/en/actions/reference/workflows-and-actions/variables), [상대 uses의 동일 commit 선택](https://docs.github.com/en/actions/how-tos/reuse-automations/reuse-workflows). env 전달 및 caller 지원 키 제한도 해당 공식 문서를 기준으로 했다.

## 기능 및 다른 범위 보존

native 3개 job은 이미 수용된 candidate의 전체 job을 가져오고 첫 context guard와 job env 2개만 변경했다. 기존 run/uses/with/env/조건/timeout/원본 비교/수치/FD·DSO/source·index 봉인/업로드/cleanup은 그대로다. Planner ASan/UBSan과 CarrotMan policy 비교의 원래 x64-only 조건도 유지한다. 해당 조건이 native에서 skip되는 기존 범위를 새 native 수용으로 세지 않는다.

main에서는 3개 strategy의 ARM row만 이동했다. x64 run/조건/env/timeout은 그대로다. Planner/WebRTC strategy anchor가 webcam/Xiaoge와 공유되므로 두 job에만 독립 strategy 객체를 배정했다. CarrotMan의 제거된 ARM row가 정의하던 `aarch64` scalar anchor의 후속 4개 사용은 같은 literal로 풀었다. 다른 job의 parsed model을 바꾸지 않는다.

선택 span 편집 및 YAML model 비교 결과:

- main baseline: 498181 bytes, SHA256 `8ac74a45f12f9e85139d9d05a6f1f53867f5e3e78fe17c55f299aa3b809904cd`.
- main 후보: 498466 bytes. callee: 64195 bytes. 두 파일 모두 500000-byte 상한 이내다.
- 선택된 4개 job 외 기존 39개 job과 root trigger/permissions/env/concurrency의 parsed model은 동일하다.
- x64 3개는 strategy를 제외한 전체 parsed job이 동일하다. native 3개는 위의 guard/env 변경을 제외하고 candidate의 전체 parsed job과 동일하다.
- 기존 candidate 2개 파일은 byte-identical이다. 나머지 ARM 5개, workspace, 일반 aarch64 build는 이동·생략하지 않았다.

WebRTC의 네 필수 stage가 모두 성공해야 cleanup하는 조건은 x64/native 양쪽에서 유지한다. 실패 후 source/index after와 upload를 success-only로 바꾸지 않았다. native VM 결과의 `device_validated=false`, `bare_metal_verified=false`도 그대로다. 이번 등록으로 물리 기기·실차 검증을 주장하지 않는다.

## 정책 및 검사 인계

policy는 같은-source caller, callee 정확한 3-leaf+aggregate roster, 단일 native matrix, 실제 호스트/이벤트 경계, 원래 timeout, 기능 stage의 누락·중복·skip, 42개 main 결과 및 3개 callee 결과 결합을 요구한다. source/private-layout mutation은 기존 63→66개, ARM routing 11→14개로 범위를 늘렸다. 명시 command-file shell/finalizer 검사는 main 8개에 새 native 3개를 더한 11개를 검사하며 기존 scope를 줄이지 않는다. WebRTC cleanup mutation도 양쪽에 적용한다.

새 테스트는 production 변경 전에 작성했다. 로컬에서 RED/GREEN을 실행하지 않았으며 아래 수는 소스에 선언된 개수다.

- `ci_workflow_policy`: 기존 33개 보존 + 5개 = 38개. 새 prefix는 `native_arm_`이며 caller/leaf/aggregate, 실제 caller-context 모형, host·candidate 경계, 모든 non-success 결과, callee 파일 부재를 검사한다.
- `ci_rust_isolation`: 기존 84개 보존 + 2개 = 86개. candidate 전체 job 보존/공유 strategy 보존과 새 42번째 결과의 needs/env/test 누락을 검사한다.

Actions에서 부모가 실행할 최소 대상:

```bash
cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --test ci_workflow_policy --test ci_rust_isolation
cargo clippy --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked --lib --test ci_workflow_policy --test ci_rust_isolation -- -D warnings
```

이후 새 same-source main PR/dispatch에서 x64 기존 기능과 native 3개, 두 aggregate, 전후 seals/upload/cleanup을 확인해야 한다. 과거 QEMU Planner 90분 timeout은 부모가 annotation으로 확인한 기존 결과이며 새 후보의 성능 성공을 뜻하지 않는다. 다른 ARM 5개는 별도 실제 수용 전까지 기존 경로를 유지한다.

로컬 수행은 Rustfmt edition2021, YAML/BaseLoader model 비교, native adapter Bash 문법, callee embedded Bash 42개 문법 및 diff 검사뿐이다. Cargo·빌드·기능·원본 실행·Actions 시작·stage·commit·push는 수행하지 않았다. Focus pins/원장 갱신과 실제 CI는 부모 소유다.
