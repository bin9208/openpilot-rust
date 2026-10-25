# #560 Fast native용 private patched strace preparation

아래 본문은 초기 preparation 납품과 launcher 추출 기록이다. 후속 Fast 실제 caller/native
admission 및 현재 pin/API는 [Fast 연결 기록](python-rust-523-fast-native-cold-560.md)이 최신이다.

작성: 2026-10-26. 추적: [#560](https://github.com/bin9208/openpilot-rust/issues/560).
기준 source: `126fdbea36598566d1040be0a35a365c909be4e4`.
상태: 새 Bash helper와 인계 문서만 작성, 기능·build·GHA **UNRUN**.
기존 Fast/감사 parser/diagnostic helper/patch/SDK helper/compiler buffers는 수정하지 않았다.

## 먼저 확인한 실제 실패와 producer 범위

[Fast40209583336/job121217757300](https://github.com/bin9208/openpilot-rust/actions/runs/40209583336/job/121217757300)의
artifact12605218513을 로컬에서 inert 확인했다.

- 경로: `.analysis/scratch/2026-10-25-523-completion/ci/12605218513.zip`.
- 626,538B, safe unique 66개, CRC 오류 없음.
- SHA256: `434a226361dd588f9e7ba1acad539284617cf44d3b6009e01c2ad51d2f85f79a`.
- source126, 실제 `bin-actions/self-hosted/Linux/X64`, kernel
  `7.0.2-6-pve #1 SMP PREEMPT_DYNAMIC PMX 7.0.2-6`와 system strace6.8이 기록되었다.
- raw157은 **`19116 ???( <detached ...>`**다. 바로 앞 PID19115는 실제 SDK의
  `rustc -vV` exec0다. PID19116 debug에는 새 TCB/startup, 다수 syscall stop,
  EVENT_EXIT와 exit0가 있다. tracer 및 stderr 수집기 wait0, raw stderr22,128,070B다.
  cold 관측 시작·끝은 `16:06:15.439278274Z`~`16:06:35.706460725Z`다.

이 자료로 unknown을 성공·무해한 thread 종료·exec 부재로 해석하지 않는다.
이 system strace 실행에는 GET_SYSCALL_INFO 반환/errno 계측이 없으므로,
PID19116에서 ESRCH였다고 확정하지 않는다. 커널이 달라져도 같은 거부가 발생했으며
hosted-only 원인이라는 가정은 사용할 수 없다. strict parser의 정확 unknown 거부를 유지한다.

검토한 수정 대상은 strace의 producer state다. 기존 closed-launch patch는 실제
root SECCOMP를 본 뒤 새 TCB에 inherited `TCB_SECCOMP_FILTER`를 부여하고,
nonleader exec로 current TCB가 교체된 경우 새 TCB의 filter 상태에서 restart operator를
다시 선택한다. 이를 일반적인 모든 strace birth/attach 동작에 확대하지 않는다.
unknown 행을 재작성하거나 syscall-info query를 추가하는 patch가 아니다.

## 기존 18통제와 patch의 의미

현재 `cold_tracer_diagnostic_560.sh`의 실제 source를 읽었다. 일곱 양성
`fork/vfork/thread/nonleader/execve/execveat/root-first`, binding 음성4개,
context 음성7개로 **18개**이며, vfork는 실제 EVENT_VFORK를 요구한다.
fixture의 semantic verifier는 unknown/`= ?`/잘린 EOF·잘못된 opcode/argv/env/
PID 교체·개수·결과를 계속 거절한다.

사용자가 전달한 최신 상태는
[diagnostic40209583364](https://github.com/bin9208/openpilot-rust/actions/runs/40209583364)의
기존 patched18 및 actual strict cold 성공이다. 이 sidecar에서는 해당 최신 diagnostic
artifact를 새로 다운로드하거나 그 결과를 재실행하지 않았다. source18의 구성·수정 의미를
검토했고, 기존 기록의 [40199544139](https://github.com/bin9208/openpilot-rust/actions/runs/40199544139)
18통제/actual cold 수용과도 구분한다. **최신 diagnostic 성공은 이 새 preparation helper나
local Fast 연결의 실행 성공을 대신하지 않는다.**

두 patch의 경계를 유지한다.

- observer patch는 기존 ptrace 호출의 반환/errno/이미 받은 stop 정보를 관측한다.
  실패 buffer나 syscall 인자 값을 읽거나 ptrace query를 추가하지 않는다.
- inherited-filter patch는 임의 attach/daemon/no-bpf/no-follow/NOMMU/옛 seccomp 순서 등
  지원하지 않는 문맥을 거절한다. source/helper/provider SHA, 실제 tracer와 timeout
  parent의 PID/start ticks·ELF identity·정확 argv를 STOP 중 확인하고 O_EXCL 승인 후
  CONT해야 켜진다. parent linkage를 관측했다고 주장하지 않는다.

따라서 **`/usr/bin/strace` 경로를 새 ELF로 치환하는 것만으로는 이 수정이 활성화되지 않는다.**
`COLD560_FILTER_INHERIT=1`만 붙이고 승인기를 생략하면 target startup 전에 실패해야 한다.
이 준비 helper는 launch 승인을 흉내 내거나 private ELF를 system provider로 위장하지 않는다.

## 새 helper의 계약

새 파일: `tools/ci/prepare-cold-strace-560`, executable Bash, `build|verify` 두 mode다.
작성 SHA256: `4c45123c13ef4e455d1b31f99fcd503fbd4f03ee827c9c116a51a46a5353fb51`.
호출자는 검토한 SHA를 `COLD560_PREPARE_HELPER_SHA256`에 고정한다.

`build`는 같은 소유 job의 `$CI_JOB_TEMP/cold-strace-560`에만 준비한다.
공유 preparation을 위한 `COLD560_PREPARE_ROOT` 대안 경로는
`$CI_JOB_TEMP/build/cold-strace-560` 하나다. `provider.env`는 선택한 root도 반환하여
후속 verify에 같은 경로를 전달한다.
실제 Actions/repo/event·runner·아키텍처·소유 source namespace/inode/marker·HEAD를
검증한다. self-hosted는 정확 `bin-actions`와 실제 전역 SDK 경로,
hosted는 실제 private SDK 경로를 사용한다. 환경·kernel·CPU·trace를 바꾸지 않는다.

원래 diagnostic preparation의 다음 부분만 좁게 옮겼다. 958행 controller 전체를
복제하지 않았고, collector/18통제/marker/PGID launcher/parser 코드는 새 helper에 없다.

1. 공식 Ubuntu orig/debian/dsc/deb 4개를 기존 size/SHA로 다운로드한다.
   deb는 설치하지 않고 original ELF를 inert 추출해 기존 SHA로 확인한다.
   현재 system strace는 별도 actual executable로 봉인하며 서로 같다고 표시하지 않는다.
2. 기존 quilt4개를 fuzz0으로 적용하고 runtime preimage3개를 확인한다.
   observer와 inherited-filter patch를 기존 고정 SHA로 적용한 뒤 모든 기존
   중간·최종 C postimage를 확인한다. filter_seccomp.c의 원래 SHA도 유지한다.
3. 원래 Ubuntu hardening=+all build flags, `-Wall -g -O2`,
   `--enable-mpers=check --disable-dependency-tracking --build=x86_64-linux-gnu`를
   유지한다. configure/config.status와 compiler/DSO·header/GCC/Perl input 원문을
   보존한다. `make -j1`로만 빌드하고, `make install`은 하지 않는다.
4. 결과는 private `bin/strace`의 실제 ELF다. source·입력 archive·patch·license·
   out-of-tree build·selected ELF, 원래 전체 SDK의 hash/metadata roster와 공식
   Rust program4개 pin, tracked source/index/stage, 사용 실행파일/DSO·ld cache를
   봉인한다. SDK 원본을 복사·chmod·수리·설치하지 않는다.
5. source/provider/전체 SDK 후검사가 모두 성공해야 `provider.env`와 `build-complete`를
   발행한다. `provider.env`에는 실제 root/path/SHA만 담는다. fresh cold와 18통제는
   실행하지 않았다고 receipt에 명시한다. 이후 caller가 가진 path/SHA를 넘겨
   `verify`를 한 번 실행하면 실제 cold 뒤의 동일성을 별도 `after-fast` proof로 남긴다.

시스템 `/usr/bin`, apt, 공유 toolchain/cache는 수정하지 않는다. 준비 중 Cargo/rustc나
`check-disk-space`를 실행하지 않아 첫 cold cache를 warm하지 않는다. 의존 tool이
없거나 provider/source/SDK가 바뀌면 실패한다. 자동 설치·fallback·성공까지 재시도는 없다.
실패 root는 이 helper가 삭제하지 않는다. 부모가 artifact 업로드와 소유성·봉인 상태를
확인한 뒤 기존 private source/common-temp 회수 구조에서 처리해야 한다.

## 7분·worker1·6 GiB 안에서의 예상

Fast native의 기존 **7분**, `CARGO_BUILD_JOBS=1`, 6 GiB routing 성장 예산을 늘리지 않는다.
새 helper는 남은 원래 job deadline 안에서 preparation 전체를 최대120초로 줄여 받으며,
개별 command timeout의 KILL5초 유예도 남은 시간에서 미리 뺀다. 모든 metadata/seal을
포함한 완료 시점도 deadline 안이어야 한다. 실제 job timeout7분이 최종 상한이다.

공식 압축 입력 합계는 약3.07 MB다. source+generated C+object+private ELF+추가 proof의
예상 증가량을 **512 MiB 상한**으로 나눈다. 실제 allocated와 apparent 합계는 `du`로
확인하고 초과·측정 오류를 거절한다. 전체 SDK와 headers/GCC/Perl은 hash만 읽으므로
원본을 private root에 복제하는 증가량은 없다. 사용되는 경로는 같은 admitted job
파일시스템이어야 하며, 다른 source/target filesystem의 기존 admission도 부모가 유지한다.

| 기존 6 GiB 내부 배분 | 예측 |
| --- | ---: |
| 새 provider source/build/bin 및 추가 proof | 최대512 MiB |
| 나머지 cold cache·Cargo registry·routing target·기존 proof | 최대5.5 GiB |
| 합계 | 6 GiB |

호출자는 **현재 남은 작업 예측**을 `COLD560_NONPROVIDER_GROWTH_BYTES`에 명시한다.
이는 실측 peak가 아니며 helper는 예측+512 MiB가 6 GiB를 넘으면 거절한다.
각 명령 전 실제 provider/proof 사용량과 파일시스템 free를 다시 확인한다.
고정25/35 GiB floor를 넣거나 기존 6 GiB 위에 provider 예산을 얹지 않는다.

디스크 fit는 위 상한 안에서 가능할 것으로 예상한다. **시간 fit는 미확정**이다.
기존 진단의 configure180초/make300초 허용치나 hosted18 성공만으로 local `-j1` 준비가
120초 안에 끝난다고 주장할 수 없다. local raw cold의 약20.27초 역시 전체 routing/
Clippy/test 시간을 뜻하지 않는다. 새 helper의 full source/SDK/provider 봉인 비용도 있다.
실제 GHA에서 준비+나머지 worker가7분에 들어오지 않으면 기간·parser를 완화하지 않는다.

그 경우의 별도 구성은 **provider 전용 선행 job에서 build·18통제를 수행하고, exact-source
artifact의 ID/digest·source/patch/flags/provenance를 고정해 native가 private ELF만 가져오는
방식**이다. 전용 job 시간은 Fast7분과 분리하고 필수 dependency로 연결한다.
native는 실제 다운로드한 ELF/DSO를 새로 봉인하고 원래 cold/strict audit를 실행한다.
다른 runner의 headers/SDK/CPU/ELF identity가 native와 같다고 하지 않으며 builder provenance와
consumer actual provenance를 각각 보존한다. 이 artifact-consumer 기능은 현재 새 helper에
추가하지 않았다. 우선 exact-source artifact acceptance 계약을 부모가 등록할 후속 구성이다.

## 부모가 먼저 검토할 기존 helper 수정 범위 제안

이번에는 기존 helper를 수정하지 않았다. 다음 단계의 공유 추출 범위는 다음처럼 제한한다.

1. diagnostic의 공식 source 다운로드~patch~configure/make preparation 부분
   (현재841–909행 주변)만 새 helper 호출과 `provider.env` 읽기로 대체한다.
   18통제/approval/held-FD/observer/strict auditor/최종 회수 함수는 그대로 둔다.
   diagnostic은 `$CI_JOB_TEMP/build/cold-strace-560` 경로로 기존 build cap에 포함한다.
   helper SHA·새 namespace와 원래 diagnostic deadline 안의 더 짧은 prep cap만 연결한다.
2. 별도로 현재 source의 **검증된 closed-launch 승인기**를 부작용 없이 호출할 공통
   경계로 추출하고 Fast에서 사용한다. diagnostic shell 전체를 `source`하면 안 된다.
   파일 첫 줄부터 admission/install/control 실행 부작용이 있다. 기존 승인기를 Fast에
   통째 복제하는 대신 어떤 함수와 globals를 공유할지 부모가 먼저 범위를 고정한다.
3. maintained Fast는 selected private ELF의 실제 path/SHA 및 launch receipt를 보존한다.
   기존 cold 감사의 system-path 전제는 parent가 actual private-provider admission으로
   명시적으로 연결해야 한다. `/usr/bin/strace` 이름이나 과거 PID·CPU·trace·SDK 경로를
   흉내 내거나 unknown을 skip하여 통과시키지 않는다. 파서 의미는 그대로 유지한다.

새 preparation helper만으로 Fast 교체가 완료되었다고 하지 않는다. 특히 STOP 승인기
추출·Fast 연결·감사 입구의 실제 provider binding이 아직 부모 작업이다.

## GHA 실행 인계

아래는 **부모 Actions job에서만** 실행할 준비 명령이다. 첫 trusted guard에서 원래
job 시작 기준420초 deadline을 한 번 기록하고, helper 호출 때 새420초로 재설정하지 않는다.
기존 source/SDK before gate 뒤, 첫 `check-disk-space` 이전에 호출한다.

```sh
# 첫 trusted step에서 한 번, 기존 timeout-minutes: 7은 유지한다.
printf 'COLD560_JOB_DEADLINE_EPOCH=%s\n' "$((EPOCHSECONDS+420))" >> "$GITHUB_ENV"

# 부모가 현재 source 기준으로 pin/작업 예측을 검토하여 전달한다.
export COLD560_PREPARE_HELPER_SHA256=4c45123c13ef4e455d1b31f99fcd503fbd4f03ee827c9c116a51a46a5353fb51
export COLD560_NONPROVIDER_GROWTH_BYTES=5905580032
/usr/bin/timeout --signal=TERM --kill-after=5s 120s \
  /usr/bin/bash "$DIAG_SOURCE/tools/ci/prepare-cold-strace-560" build
set -a
source "$CI_JOB_TEMP/cold-strace-560/provider.env"
set +a
```

그 다음 단계는 기존 diagnostic `run-inherited`의 7+4+7 통제, 실제 source-bound
launch approval 및 selected ELF의 inherited cold 호출을 **공유 경계 추출 후** 연결하는
것이다. 이 문서에는 승인기를 생략한 direct-strace 명령을 대체 실행안으로 쓰지 않았다.
18통제·actual cold를 새 provider/build flags에 대해 수용해야 하며, 이름/개수·원문
trace와 실패 EOF도 검증한다. 그 후 기존 native 감사 selector는 유지한다.

```sh
CI_PREPARED_RUST_PROOF="$CI_JOB_TEMP/proof" \
cargo test --manifest-path rust/Cargo.toml -p openpilot-dev-tools --locked \
  --test ci_disk_bootstrap prepared_audit::prepared_audit_actual_cold_bootstrap_receipt \
  -- --ignored --exact --nocapture

# 성공/실패와 무관한 후속 step에서 caller가 보유한 실제 path/SHA로 한 번 실행한다.
/usr/bin/bash "$DIAG_SOURCE/tools/ci/prepare-cold-strace-560" verify
```

위 selector가 실제1개 성공해야 한다. 기존 routing selectors/strict parser/trace bounds/
원래 SDK after와 source/index after/실행파일 provider after/artifact/소유 회수도 필수다.
Fast/공유감사/Focus pins·원장·이슈·commit/push/GHA 조작은 부모 담당이며 이 sidecar는
실행 중 CI를 취소·재실행하지 않았다.

## 로컬 확인

새 helper의 `bash -n`과 source/diff 검토 및 보존 ZIP의 안전 경로·CRC/SHA·raw 읽기만
수행했다. 새 helper build/verify, source 다운로드·patch 적용·C configure/make·Cargo,
ptrace/18통제/actual cold·GHA 조작은 모두0회다. 모든 새 기능 수용은 **GHA UNRUN**이며
unknown 원인 해결·Fast 정상화·baseline 제거 credit는0이다.

## 후속 공유 launcher 추출 — 2026-10-26

사용자 후속 승인으로 closed-launch와 필수 FD/PID/STOP 함수12개를 원문 byte 그대로
`tools/ci/cold_tracer_launch_560.sh`로 추출하고 diagnostic에 FD/SHA 검증 loader를 연결했다.
정확 API/globals·원본 역투영·새 pins는
[공유 launcher 기록](python-rust-523-cold-tracer-launch-560.md)에 있다.
새 preparation helper 자체는 수정하지 않았다. 위 provider preparation 공유 제안은
아직 미완료이며 diagnostic180/300초와 Fast120초를 혼합하지 않았다. 새 extraction의
18통제·actual cold·Fast fit도 GHA UNRUN이다.

## 후속 Fast 연결 — 2026-10-26

`fast_native_cold_560.sh cold|after`와 native prepared audit의 private provider 입구를
구현했다. preparation은 unchanged std-only approver를 같은120초/512MiB 안에서
공식 SDK absolute rustc로 컴파일하고 실제 ELF/DSO를 봉인한다. Cargo warmup은 없다.
`verify`의 기본 after-fast 계약은 유지하고, 별도 1회
`COLD560_VERIFY_PHASE=after-cold`/`verify-after-cold-complete`를 추가했다.
현재 helper SHA256은
`6c7d99e6f5c1ee7fc4929c105a8bbb80334a47ec75e7e990ca5ea47c3d81bd58`다.
초기 본문의 `4c45123c...`는 이전 납품 pin이며 새 caller에 사용하지 않는다.
diagnostic preparation 공유는 여전히 미완료로, 원래180/300초 body를 변경하지 않았다.
정확 WF snippet/worker1 등록/실제 자원 가정/미실행 검증은 위 Fast 연결 기록을 따른다.
