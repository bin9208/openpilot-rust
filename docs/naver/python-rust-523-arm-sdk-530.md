# #530 두 native ARM role의 새 prepared SDK 후보

2026-10-25. [#530](https://github.com/bin9208/openpilot-rust/issues/530)의 재발을 대상으로 한 **source 후보**다. 설치·guard·기능·Clippy·실제 성장량은 모두 Actions UNRUN이며, 이 변경으로 ARM 또는 #523 수용/삭제 credit을 추가하지 않는다. 로컬은 소스, 공식 원문 inert 확인, Bash/YAML/AST 문법, 신규 Rust 코드 서식과 diff만 확인했다.

## 실제 실패와 적용 범위

Rust run `40161295597`의 Webcam `121073229641`, CarrotNavi `121073229694`는 첫 disk guard의 일반 bootstrap에서 Rustup1.94 동기화 → `recovering from partially installed toolchain` → component 제거/재다운로드 → `clippy-preview-aarch64-unknown-linux-gnu`의 `bin/cargo-clippy` conflict/rollback으로 실패했다. API head는 `593186e6a66b036c8817f0184723f839867d6856`, 실제 checkout/callee source는 PR merge `40562f2d8068878a00b18e12f355799b4fe0405d`다. LFS/호출자 거부나 프로젝트 기능 실패로 분류하지 않는다. 과거 pilot의 같은 partial-recovery 메시지 뒤 성공도 있어 **최초 partial 상태를 만든 원인은 아직 미확정**이다.

대상은 `.github/workflows/rust-native-arm.yml`의 `webcam-runtime`, `carrot-navi-arm` 둘뿐이다. 나머지6개 ARM leaf, aggregate, root 설정과 기존 기능 run/selector/feature/원래 job timeout을 유지한다. X64 prepared 경로, 기존 cold/warm 경계 및 일반 Rustup bootstrap은 변경하지 않는다.

## 새 prefix와 엄격한 준비 순서

새 `tools/ci/prepare-readonly-arm-rust-530`은 `before|verify|after`를 받는다. `before`는 `$CI_JOB_TEMP/arm530-sdk`와 `$CI_JOB_TEMP/proof/arm530-sdk`가 모두 absent일 때만 시작한다. 기존 `$job/rustup-home`과 전역 tool을 수리·덮어쓰기·삭제하거나 prepared 입력으로 채택하지 않는다.

1. 실제 `github-hosted:Linux:ARM64`, `Linux:aarch64`, `LOCAL_CI_ARCH=aarch64`, repo, source SHA/소유 marker/identity를 확인한다. main caller인 `rust.yml`, 위2개 `GITHUB_JOB`, 기존 push dev/main·신뢰 PR merge/base/head repository·명시 dispatch 경계가 모두 필요하다. generic ARM 또는 별도 candidate caller를 추가하지 않는다.
2. 설치 전 실제 rustup/cargo 경로와 알려진 충돌 파일/component metadata를 bounded FD/hash/identity로 관측한다. old SDK 전체 closure 관측은 false다. tool은64MiB/file·합256MiB, control metadata는2MiB/file·합4MiB, 기존 home의 얕은 roster는4096entry/1MiB 상한이다. 원문 channel manifest 자체가845,032B이므로 metadata 상한은 이를 포함한다. credential/config 내용·전체 argv/env는 읽지 않는다. 관측한 tool/metadata는 이후 다시 검사하되 step 사이 동일 FD 보유를 주장하지 않는다.
3. 새 prefix에 공식 Rustup1.29.1 client와 Rust1.94 ARM의 rustc/cargo/rust-std/rustfmt/clippy만 준비한다. installer 환경의 HOME/임시 경로와 설치 전용 CARGO_HOME/RUSTUP_HOME/PATH를 명시하고 ambient toolchain/LD/dist override/token을 전달하지 않는다. runtime Cargo cache는 기존 `$job/cargo-home`를 유지한다.
4. 아래 archive literal size/SHA를 확인하고 component `manifest.in`에서 전체 expected payload를 만든다. 설치된 모든 payload SHA·파일/dir 명단·정확5component·generated control metadata·tool7개·std61개를 검증한다. 기존 SDK를 지우거나 component conflict를 overwrite하여 통과시키지 않는다. 모든 검사가 끝난 뒤에만 새 binding receipt를 게시한다.
5. SDK, whole 새 RUSTUP_HOME, 공식 input, client, shim tree의 파일 SHA와 identity/mode/UID/GID/nlink/mtime/ctime/종류/경로 전체 roster를 봉인한다. SDK에는 symlink를 허용하지 않으며 shim symlink는 pinned rustup으로만 해석되어야 한다. 외부 mount/특수 파일/다른 UID/그룹·전체 쓰기 권한은 거부한다. owner 쓰기 권한을 없앤 파일시스템 immutable 설정을 주장하는 것은 아니다.
6. mode `OPENPILOT_CI_PREPARED_RUST_MODE=native-arm-530-v1`, 정확 root/manifest와 source/role/owner/tree baseline을 묶은 receipt가 모두 필요하다. root는 `$job/arm530-sdk/rustup-home/toolchains/1.94.0-aarch64-unknown-linux-gnu` 하나다. `RUSTUP_AUTO_INSTALL=0`, 명시 toolchain 및 절대 `RUSTC`/`RUSTDOC`를 전달한다.
7. `check-disk-space`의 새 ARM mode는 전체 binding을 검증한 뒤 기존 cold guard를 **절대 SDK cargo/rustc**로 build한다. ARM verify/after는 Rustup/Python을 실행하지 않는다. 원래 X64 validator 본문과 일반 bootstrap의 설치 경로는 그대로다. 두 role의 후속 installer/동일 host target-add만 재검증으로 대체하며 기능 Cargo 명령은 유지한다.

Rustup이 생성하는 TOML은 `before`에만 기존 admitted private Python3.12.14의 표준 `tomllib`로 검사한다. 이 초기 reader는 실제 ELF/kernel/DSO 전체 provider closure를 새로 입증하는 단계가 아니다. 전체 SDK payload와 별개로 generated metadata의 schema/component/공식 manifest 선택 항목도 확인한다.

## 공식 literal pins

공식 manifest는 `https://static.rust-lang.org/dist/channel-rust-1.94.0.toml`,845,032B/SHA256 `aaa177def36e01d539bee6bde95295230b9ce378f81057845db8d0ebe97898ee`, date2026-03-05다. 같은 official origin의 SHA sidecar와 일치했으며 독립 서명 검증을 주장하지 않는다. 아래 component URL prefix는 `https://static.rust-lang.org/dist/2026-03-05/`, 이름 suffix는 `-1.94.0-aarch64-unknown-linux-gnu.tar.xz`다.

| provider | bytes | SHA256 |
|---|---:|---|
| rustc | 61,188,172 | `7261b2fcdb88aef8b21f4b9f915d9a75c959674193f5b38e13fab4569684fc5b` |
| cargo | 10,398,516 | `d57ab7b23b52cba46e58c78327904d0c7db19892988677b0d59495c8af4bb790` |
| rust-std | 28,485,880 | `c781b3ef4fefa5508fbe05820eddc95e46351d905a30921cc020febd9c596a2e` |
| rustfmt | 1,999,864 | `1ac17dbe7f8f6c98db5b62cb149fcd10c7565d624a59471115fc40055db9af28` |
| clippy | 3,673,924 | `f7450c414e394450b53be602d37c374e2a2c5a2f51be2b54faf3d1e3272160b1` |
| Rustup1.29.1 ARM client | 19,856,640 | `15f6e4ce9f583b929c996c91562bad6d4454f3281de858b02cdfdef615fac433` |

client URL은 `https://static.rust-lang.org/rustup/archive/1.29.1/aarch64-unknown-linux-gnu/rustup-init`이다. 원문6개 합125,602,996B, component archive regular bytes 합539,704,460B를 inert 확인했다. archive member의 traversal/중복/특수 파일/link는 없었다. 고정 tool7개와 client의 ELF header는 ELF64/little/EM_AARCH64/`/lib/ld-linux-aarch64.so.1`이며 실행 결과가 아니다. std는61파일/155,661,490B이고 전체 payload에 포함된다.

근거는 공식 [설치 문서](https://rust-lang.github.io/rustup/installation/index.html), [profile](https://rust-lang.github.io/rustup/concepts/profiles.html), [환경 변수](https://rust-lang.github.io/rustup/environment-variables.html), [pinned Rustup1.29.1 source](https://github.com/rust-lang/rustup/tree/1.29.1)다. Rustup의 generated component/config/installer-version 형식은 해당 source를 읽어 고정했다. 로컬 SDK 설치나 version 실행으로 기대값을 만들지 않았다.

## 시간·성장·실패 후 봉인

새 prefix 전체에3GiB planned cap을 둔다. official inputs/expected payload/설치 SDK/임시 작업을 모두 포함한다. 기존 build8GiB, cold guard cache1GiB, Cargo cache1GiB, source2GiB, proof512MiB 및 reserve512MiB의 잔여 성장량을 실제 filesystem별로 더한다. source-owner 집계에서는 별도 build cap의 `source/rust/target`만 제외해 중복 계산을 피한다. 기존 partial 파일의 현재 사용량은 df에 이미 포함되며 추가 SDK로 채택하지 않는다. allocated/apparent 어느 쪽의 초과/unknown도 실패한다. 실제 peak 보장은 아니다.

prepare는 공유1200초 deadline 아래 개별 client180초/component 설치720초와 bounded download를 사용하고 Actions step도20분으로 제한한다. command별512MiB file limit, 소유 PGID/session/starttime 확인, 정지한 직접 leader의 status/release/wait 및 그룹 nonlive 검사를 기록한다. 모든 adopted child를 회수했다고 주장하지 않는다. prefix 안 삭제 경합은 정확 ENOENT와 partial cap 확인 후3회만 다시 측정한다. 다른 오류를 무시하지 않는다.

trusted AND always의 after step은10분 상한이며 준비 중 실패해도 독립 시도한다. readiness/baseline이 없거나 payload/roster/old 관측 파일이 달라지면 실패한다. source/index seal과 업로드는 기존 그대로 시도하고, 두 role cleanup은 SDK after 성공도 요구한다. 실패 뒤 SDK를 없애 성공으로 보이게 하지 않는다. proof는 기존 private artifact 경로에 포함되어 upload 이후에만 기존 소유 cleanup을 평가한다.

## 검증 대기와 부모 통합

기존 `ci_disk_bootstrap` target에 새 음성3개를 추가했다. 기존 test body/기대값과 `prepared_audit.rs`는 그대로다.

- `prepared_arm_mode_rejects_missing_root_and_unknown_modes_before_installation`
- `prepared_arm_rejects_other_hosts_before_installation`
- `prepared_arm_rejects_root_manifest_cross_binding_before_installation`

모두 거부 뒤 installer/cold cache에 도달하지 않아야 한다. fake uname은 음성 입력일 뿐 fake SDK를 수용하지 않는다. 정확한 ARM 설치 양성·whole payload/metadata·guard cold/warm·후속 tool 실행·after/failure cleanup·cap 적합성과 기존 gate/Clippy는 Actions에서 검증해야 한다. 기존 성공6ARM을 이 변경만으로 모두 반복하라는 뜻이 아니다.

부모가 Focus/ledger/pin 및 `ci_policy::validate_private_job_layout`의 exact cleanup 조건을 통합한다. 필요한 정책 조건은 **native ARM이고 alias가 위2개 role일 때만** `steps.arm530_sdk_final.outcome == 'success'`를 기존 source/common cleanup 조건에 추가하는 것이다. SDK stage의 정확 순서·ID·trusted/always·literal helper/source binding도 유지해야 한다. 별도 radard trusted-always 수정과 광범위 runner/role 허용 확대는 이 구현 범위가 아니다. 정책 통합 전 현재 전체 policy positive가 통과한다고 주장하지 않는다.


부모는 main rust.yml caller이고 native ARM이며 정확 Webcam/Navi alias인 두 role에만 SDK step
policy를 연결한다. 예전 candidate workflow와 main X64 role은 이 조건 밖이다. SDK before/after의
유일 stage ID·순서·trusted/always·20/10분·source directory·직접 Bash·고정 helper SHA/명령을
대조한다. 뒤 dependencies는 봉인 verify를 요구하고 Rustup 재설치/target-add는 거부한다.
source/common cleanup 모두 기존 upload/source 성공에 SDK-final 성공을 정확히 더한다.
두 실제 양성과 각14가지 누락/skip/선택실패·pin/명령/순서/cleanup/재설치 변조를 거부하는
새 정책회귀1개를 추가한다. 원래6 ARM 및 X64의 행동은 유지하며 기능 수용은 Actions 대기다.


### a483 실제 planner 정책 검사의 source expectation 후속

[a483 pilot40166180655/job121087586875](https://github.com/bin9208/openpilot-rust/actions/runs/40166180655/job/121087586875)의 planner는 기존 Python CI fixture가 두 role의 dependency 전체 원문을 옛 `rustup toolchain install`과 비교해 실패했다. 같은 실행의 WebRTC는 성공했다. 기능·ARM compiler 실패로 오인하지 않는다.

`test_plannerd_ci.py`의 x64/pilot reference는 보존하고, 정확 Webcam/Carrot Navi 두 role에만 확인된 원래 설치 문자열1개를 새 SDK verify1개로 치환해 전체 functional step 동등 비교를 유지한다. before/after의 유일한 이름·id·20/10분·고정 helper SHA·명령·trusted/always 조건과 venv/기능/finish 사이 순서도 명시 검증한다. 기존 나머지6 role·전체 명령과 음성 통제는 보존한다. 이 source 보완의 Python AST만 확인했으며 기능은 다음 Actions에서 판정한다.
