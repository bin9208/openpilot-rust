# #594 Fast mutable build의 du ENOENT 재표본 — 2026-10-26

상위 #523, cold 진단 #560, [문제 #594](https://github.com/bin9208/openpilot-rust/issues/594).
부모가 workflow/pin/이슈/다음 실제 Fast를 통합한다. 이 담당은 Fast helper의 측정 함수,
독립 native fixture8개와 이 문서만 수정했다. 로컬 기능/Cargo/build/test/Clippy/provider
실행 및 GHA dispatch는0이다.

## 실제 실패와 분류

source `ee74a75ffa4762397e3498d597e78135ef015d90`의
[run40232519167/job121287077204](https://github.com/bin9208/openpilot-rust/actions/runs/40232519167/job/121287077204)
부모 보존 `scratch/ci/fast-nativeee74.log`에서20:53:33.717Z의 최초 오류는 다음 두 파일이다.

```text
/tmp/openpilot-ci.NSsBwC/cold-strace-560/build/tmp/cclc4GVX
/tmp/openpilot-ci.NSsBwC/cold-strace-560/build/tmp/ccuIe0mz.mk
```

GNU du는 두 파일에 `No such file or directory`를 기록했고 바로 `resource guard failed`가
이어졌다. 기존 launcher가 owned compiler group을 kill/wait한 뒤 provider 미완료/after 실패가
따랐다. 이 로그는 메모리 또는 공간 한도 초과의 증거가 아니다. ENOENT를 관측하지 못한
provider 실패 전반을 재시도하는 변경도 하지 않는다.

기존 `tools/ci/cold_tracer_diagnostic_560.sh::measure/admission_failure`의 최대3회 샘플,
부분합 cap 즉시 거부, 실제 사라진 mutable path 확인, 첫 실패 보존 패턴을 읽고 Fast에 좁게
이식했다. 참고 당시 tracer helper SHA256은
`8a7383587589d01404f7e6573db8fd1ad458c22fbc6ff2755298b5a4a35ddc4b`다. 그 파일은 수정하지 않았다.

## 변경 범위와 fail-closed 계약

`tools/ci/fast_native_cold_560.sh`의 새 private `measure(cap, kind, roots...)`를 기존 owner/job
allocated·apparent 측정과 provider aggregate 두 곳에서 호출한다. 새 CLI나 registry는 없다.

- 완전한 root별 stdout 행, canonical decimal bytes, 정확한 root 순서/개수/개행을 검증한다.
  NUL 등 Bash read가 건너뛸 수 있는 bytes도 실제 stream byte count와 대조해 거부한다.
- 매 측정 전·후 job/owner의 원래 device/inode와 canonical owned mode700 조건을 확인하고,
  각 measured root의 device/inode도 유지해야 한다. 교체·symlink·권한/identity 오류는 거부한다.
- allocated와 apparent 모두 총합을 cap과 비교한다. 실패한 du의 부분합도 cap을 넘으면
  즉시 실패하며 재시도하지 않는다. 큰 decimal의 산술 overflow도 문자열 길이로 먼저 막는다.
- exit0 + 빈 stderr + 완전한 stdout + unchanged roots인 sample만 값으로 반환한다.
- 재표본은 exit1의 모든 stderr 행이 GNU du의 정확한 ENOENT 형식이고, 경로가 해당 measured
  root 안이며 아래 세 mutable scope에 속할 때만 허용한다. 경로는 현재 absent이며 symlink가
  아니어야 하고 `readlink -m`도 같은 경로여야 한다. `..`/symlink ancestor 우회는 거부한다.
  scope는 `$root/build`, `$source/rust/target`, `$build/tmp` 및 그 하위뿐이다.
- permission/unknown/mixed/성공 exit의 stderr, 다른 exit status, malformed stdout,
  immutable 또는 현재 존재하는 경로, symlink, owner/job/root 교체는 첫 sample에서 거부한다.
  연속 ENOENT3회 뒤에도 성공값을 만들지 않는다. sleep이나 별도 deadline 연장은 없다.
- du stdout/stderr는 각각 child file-size limit16KiB 아래 저장한다. 재시도 sample의 raw 두
  stream과 phase/kind/attempt/status/root를 `du-failed-samples.log`에64KiB까지 보존한다.
  evidence bound가 차면 더 쓰거나 조용히 버리지 않고 실패한다. 최초 fatal reason TSV와
  그 시점의 bounded stdout/stderr도 noclobber로 보존해 후속 admission이 덮어쓰지 못한다.

provider root와 `$job/proof/cold-strace-560`은 allocated/apparent 각각 계속 **한 GNU du 호출의
다중 인자**다. 전체 행을 검증하고 합치므로 같은 호출 안에서 적용되는 hardlink 중복 제거와
aggregate512MiB의 기존 의미를 유지한다. 각 root를 별도 du로 재서 합치지 않았다.

owner2GiB + job4GiB의6GiB, provider aggregate512MiB, 최초7분 deadline, initial memory2GiB,
이후64MiB pressure, free-space growth 계산, provider120초, first Rust cold bootstrap/trace,
owned launch/kill/wait/finalizer/source/SDK/provider 검증은 그대로다. default cold functional18
정의와 기존 policy 테스트는 수정하지 않았다.

## 작성된 GHA 전용 회귀

새 `rust/crates/dev-tools/tests/fast_cold_measure.rs`는 실제 helper에서 `owned_dir`,
`admission_failure`, `measure` 함수 원문만 추출한다. 새로 쓴 대체 측정 알고리즘을 검사하지
않는다. private 임시 root와 fake GNU du를 쓴 Bash fixture를 기존 bounded NativeRunner의
5초 제한 아래 실행하도록 작성했으며 Python/SDK/Cargo/compiler/provider는 호출하지 않는다.

| test | 주요 경계 |
| --- | --- |
| `complete_single_and_aggregate_samples_preserve_gnu_du_arguments` | allocated/apparent 전달, 완전 sample, provider 두 인자의 단일 호출·hardlink aggregate 모사 |
| `actually_removed_mutable_files_retry_to_a_complete_clean_sample` | 세 scope 및 provider pair에서 실제 임시파일 삭제 후 ENOENT→완전 재측정, partial 값 미수용 |
| `only_a_complete_third_sample_can_succeed_and_three_enoents_fail` | 두 번 실패 뒤 완전3번째만 성공, 연속3회 거부, raw 보존·evidence cap 거부 |
| `complete_or_partial_cap_excess_never_retries_including_aggregate` | complete/partial/aggregate cap 초과, 큰 decimal 즉시 거부 |
| `malformed_stdout_never_becomes_an_accepted_partial_sample` | 잘못된 root/행/숫자/개행/NUL 거부 |
| `permission_unknown_status_and_immutable_scope_errors_fail_closed` | permission·unknown/mixed stderr·status·immutable/out-of-root 거부 |
| `existing_symlink_and_noncanonical_mutable_paths_are_not_retried` | 현재 존재·dangling symlink·symlink ancestor·dotdot 거부 |
| `replacing_job_owner_or_a_measured_provider_root_is_rejected` | du 도중 job/owner/provider root inode 교체 거부 |

native test 선언8개 안의 Bash fixture 호출은42개다. 이 숫자는 작성된 source roster이며 실제
통과 개수가 아니다. 다음은 부모 GHA용 command이며 로컬에서는 실행하지 않았다.

```sh
cargo test --manifest-path rust/Cargo.toml --locked -p openpilot-dev-tools --no-default-features --test fast_cold_measure -- --nocapture
```

기존18개 default cold functional test는 별도 유지한다. 부모가 새8개와 strict Clippy 및 helper
pin을 통합하고 실제 Fast에서 provider 완료→원래 first Rust exec→독립 after를 검증해야 한다.

## 로컬 source/syntax/format 확인과 남은 수용

- `bash -n tools/ci/fast_native_cold_560.sh` 통과. fake du와 실제 추출 함수도 `bash -n`만 했다.
- 설치된 Rust1.94 rustfmt로 새 Rust 파일의 구문·서식 확인 통과.
- 변경 파일의 `git diff --check` 통과.
- 새 측정 block과 호출 치환만 역투영하면 기존 helper 전체 SHA256
  `645a23e3eed36c6663093cabd71f3618ba3f98af9ccf9848098659b25da1feec`가 정확히 복원된다.
  측정 외 cold flow와 기준의 source 불변 근거이며 기능 성공을 뜻하지 않는다.

현재 구현 SHA256은 Fast helper
`35ce057f1d033daa7705a43bff47b382d0601a53ace00cd02acb899481650561`,
새 fixture `e5305859c2da1b53bf1b0163151ce442865eab5a75113d4d93930f4a7f045a99`다.

미완료는 새8개 실제 PASS/Clippy와 새 source Fast 실제 수용이다. memory/space cap을 늘리거나
provider after를 우회하지 않았다. workflow/pins/Focus/ledger/Cargo/provider/helper 다른 파일의
수정 및 commit/push/CI/이슈 변경은 부모 소유로 남겼다.


## 부모 통합 source pin

별도 recovery474의 명시 hosted context를 추가한 preparer SHA를 Fast의 원래 한 literal에
갱신했다. default Fast의 context6GiB/512MiB/기한·소유권은 그대로이며, 앞의 측정 역투영에
이 literal도 이전 값으로 되돌리면 원래 helper 전체 SHA가 복원된다. Fast WF의 두 helper
환경 pin만 갱신했고 다른7개 worker object와 기존 raw Cargo 명령/indentation은 그대로다.
새 tools 단위 검사에8개/42 Bash fixture와 strict Clippy를 연결하며 실제 Fast 수용은 대기다.

최종 preparer SHA256 `051332a53de80a8debc38c7ef1fcaf0fa43a113d03300b33ea7e6660687f0c5c`, Fast helper SHA256 `bb9f23ef5aaf179e444533e3804d09c2eb275a7df1efa161980c8fe7c47db2f8`.

## 첫 원격 컴파일과 CI 정책 보완

source7a928의 tools job121300012517은 새 fixture의 `env_remove`가 `Vec<OsString>`인데
`Vec<String>`을 만든 E0308로 실패했다. map을 `OsString::from`으로 고쳤으며 실제8개는
그 실행에서 미도달이다. 기존 policy/cold audit 검사는 성공했지만 이를 새8개 성공으로 세지 않는다.
같은 source의 Fast routing job121300012801은 기존 cap 계약의 옛 literal과 recovery 단계2개의
조건 괄호 때문에39성공/3실패다. cap 계약은 여전히512MiB를 한 GNU du 호출로 측정하는 정확한
새 literal로 연결하고, recovery 조건은 기존 source-trust 괄호 형식 안에 모두 넣었다.
신뢰 policy 함수를 완화하거나 cap·상한·기한을 바꾸지 않았다. 이 보완의 실제8개/Clippy/정책42와
Fast provider/first Rust/after 수용은 다음 exact-source Actions에서 확인한다.


## 실제 원격 수용 — 51fd 회귀와 3f532 Fast

[새 회귀40237673607/job121305748955](https://github.com/bin9208/openpilot-rust/actions/runs/40237673607/job/121305748955)의
source는 `51fd430f76dd6bd28776ac1fbd04c16955bd977e`다. 새8개/0실패/0ignored와
strict Clippy가 실제 성공했다. 원래42 Bash fixture roster는 그대로이며 native-exit-codes의
7개 명령이 모두0이다. source/index, SDK file/roster/program before/after는 byte 동일하고
SDK installation0이다. artifact12616591265는1,589,989B, SHA256
`a2f0166dcb49af6360e3348a9932a132635d9a8797061a99faa2758765d0475d`,
46 safe unique regular member/전체 CRC, expanded6,948,613B다.

[Fast40240059299](https://github.com/bin9208/openpilot-rust/actions/runs/40240059299)의
source는 `3f5329dcf76f461bde804d09dfef0c4e5980893b`다. 8개 worker와 aggregate가
모두 성공했다. native job121309737347은 private provider prepare0, cold original0/finalizer0,
after original0/finalizer0와 두 phase의 owned_groups_nonlive=true를 기록했다.
first actual trace181 exec attempts 중 absolute Cargo1/rustc37이 성공했고
Python/rustup exec attempt는 각각0이다. strict native cold audit1/0실패/0ignored도 성공했다.

artifact12616217004는13,279,909B, SHA256
`3a0672a60be56f39110a75ff7ce9c04a74a2bcd8478fbb9723f2a5fb5c842170`,
1,408 safe unique regular member/전체 CRC, expanded71,268,466B다.
source/index와 SDK file/roster/program before/after는 byte 동일하다. provider의 build 완료
기준에서 after-cold/after-fast까지 file SHA/roster/stable548쌍도 byte 동일하다.
provider ELF SHA256은 `3169f3b03845d655b79b29dae8e06093f9f92fead9fee87d20925f3e2f57b15d`다.
raw trace SHA256은 `3efae74b67fe4aa29601c393d90702001f5478753a7a59ee5150a544c711a678`다.
업로드 뒤 source/owned temporary cleanup까지 두 job의 모든 step이 성공했다.

51fd→3f532의 새8개 source, Fast helper/preparer는 byte 동일하다. 이전 절의 대기 상태는
이 두 actual run으로 충족했다. 원래 owner6GiB/provider512MiB·메모리·기한·hardlink 합계와
strict audit/회수 계약을 유지한 #594 범위 수용이며, #560 전체 진단이나 #523 전체 완료는 아니다.
