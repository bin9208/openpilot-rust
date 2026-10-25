# #560 cold tracer 환경 관측 보완

2026-10-25. source `acc716e47875f352f1521d0211f218499aa34603`의
[Fast40155092407/job121054673203](https://github.com/bin9208/openpilot-rust/actions/runs/40155092407/job/121054673203)는
실제 cold 감사에서 `unrecognized trace line 169`로 실패했다. 원문은
`2560  ???( <detached ...>`이며 바로 앞 PID2559는 실제 `rustc -vV`를 실행했다.
debug에는2559의 CLONE/2560 startup과 두 task의 EXIT0이 있다. syscall-info 실패 errno는
아직 관측하지 못했으며 unknown은 계속 거부한다.

artifact12583359346=589832B/SHA256
`764c74c3095d96401cf92c175c73aa551e4f0832a0389ccd9ca1b46659f73a9f`를
전수 CRC/SHA로 확인했다. 기존 #560의 dying-task 유형과 같지만 원인이 해결됐다는 뜻은 아니다.
실제 kernel release와 dpkg source revision이 원문에 없으므로 다음 최소 관측을 먼저 연결한다.

구현 전 계획은 기존 strace provider 전후 기록에 kernel release/version/machine,
`/proc/version`, strace의 실제 dpkg binary/source package/version/architecture와 조회 provider의
metadata/SHA를 추가하는 것이다. 각 channel은64KiB/file로 제한하고 실행 실패를 전파한다.
전후 raw bytes를 비교하며 collector의 원래6개 통제를 보존하고 새 metadata drift4개도 거부한다.
실제 cold 명령·timeout·stderr cap·자손 회수·source/SDK gate와 unknown parser는 바꾸지 않는다.

이는 관측 후보이며 기능은 Actions UNRUN이다. 배포판 strace 교체, 추가 ptrace 호출,
실패 행 삭제/허용, 성공까지 재실행 또는 원본 제거 credit은0이다. errno/stop 관측과 근거 있는
provider 수정은 별도 후속이며 이 기록만으로 #560을 종료하지 않는다.


## 격리 source 계측 후보 설계 — 2026-10-25 승인 범위

기본안은 공식 Ubuntu6.8 source의 최소 계측이다. 시스템 `/usr/bin/strace`와 기존 Fast/parser는 수정하지 않는다. 새 private ELF는 같은 Ubuntu ELF가 아니며 exact source에 대한 diagnostic provider다. actual errno/root fix는 아직 UNRUN이다.

소유 파일은 `tools/ci/cold_tracer_diagnostic_560.sh`, `.github/workflows/python-rust-523-cold-tracer.yml`, `tools/ci/fixtures/strace-6.8-560-observer.patch`, 이 한국어 기록4개다. shared ci_policy/Focus/pins/원장/SDK helper는 부모가 등록한다. local은 source/patch/data/문법/diff만 수행한다.

공식 orig2457948B/SHA256 `ba6950a96824cdf93a584fa04f0a733896d2a6bc5f0ad9ffe505d9b41e970149`와 기존 dsc/debian tar/deb pins를 재검증한다. 원래 syscall.c/strace.c/filter_seccomp.c의3SHA 및 deb 안 strace bytes의 기존 `28f957...` 연결은 기존 audit의 출처다. CI는 설치하지 않는 deb 원문과 원래 package metadata, 실제 system provider SHA/version/kernel/dpkg source revision을 함께 보존한다. Ubuntu quilt4개는 test-only이고 runtime src 변경이 없다는 근거를 유지하며, fresh build는 Ubuntu rules의 hardening=+all 및 -Wall -g -O2/--enable-mpers=check를 명시한다. configure 결과·compiler·flags·원문patch/license를 보존하고 재현 ELF 동일성을 주장하지 않는다.

patch는 external `src/syscall.c`/`src/strace.c`만 바꾼다. GET_SYSCALL_INFO 기존 호출1회 retval와 직후 errno를 보존하고, 성공 반환과 요청 길이가 op1byte를 포함할 때만 op를 읽는다. 실패 buffer/nr/args 읽기0, ptrace 추가/재시도0이다. get_scno/get_syscall_args 실패 stage와 decode 실패 시 이미 받은 wait status/event/tracee PID/TCB flags/entering을 같은 bounded 전용 log로 남기고 errno를 복원한다. 정상 ENTRY/EXIT/SECCOMP는 각 op의 마지막 필드까지 반환 길이가 유효할 때 counter만 늘린다. sizeof/offsetof는 미평가식이며 nr/args/rval 값은 읽지 않는다. rc<0/short/NONE/unknown op 및 위 decode 실패만 raw event를 쓴다. footer는 sci=entry+exit+seccomp+abnormal, seen=정상+raw event, written=raw event, abnormal=해당 GETINFO event 수를 검증한다. 정상 수천 stop이 anomaly 예산을 소진하지 않으며 anomaly 자체가 넘치면 health 실패다. logger는 root tracer PID만 쓰고 owned O_EXCL/NOFOLLOW/CLOEXEC regular file64KiB에서 reserved footer를 보장하며 누락/overflow/IO실패는 바깥 receipt가 거부한다. 원래 retval/branch/trace text는 바꾸지 않는다.

전용 job은 지정 branch push와 명시 GitHub-hosted Ubuntu24.04 LinuxX64만 받는다. pinned job-temp/owned source checkout/index 봉인/prepare-readonly-ci-rust before-after와 실패 업로드 후 조건부 cleanup 패턴을 재사용한다. 예산은 source2GiB, native+Rust build8GiB, SDK와 Cargo homes 합3GiB, cold diskguard cache1GiB, proof512MiB, apt1GiB, filesystem reserve512MiB로 동일 filesystem일 때 reserve 포함 동시16GiB를 실제 device별 합산한다. 실제 할당/apparent 성장과 free를 df/du로 감시한다. 이 수치는 fit 가정이 아니라 admission 상한이며 초과/측정불능은 실패다.

준비 중 check-disk-space를 호출하지 않는다. helper/SDK/configure/make와 source 다운로드는 df/du admission을 사용한다. 원래 Fast처럼 job의 diskguard cache 부재를 확인하고 새 private tracer가 관측하는 첫 check-disk-space --growth-mib2048에서 현재 source의 guard를 fresh build한다. 그 뒤 기존 native cold auditor만 build/실행하여 unknown을 그대로 strict FAIL한다. parser 예상값/규칙을 바꾸거나 새 tracer의 unknown을 허용하지 않는다. 이미 cold target가 실패한 경우에도 원문 trace/outcome/seal/upload를 보존한다.

기능 phase마다 owned setsid leader/PID-starttime/PGID handshake, timeout, resource watcher와 direct wait 후 non-live group 검사를 적용한다. stale PGID에 신호0이며 adopted 전체 wait는 주장하지 않는다. provider ELF/DSO/SDK/source 전후를 FD+identity/SHA 및 roster로 묶는다. 실패 finalizer는 독립적으로 봉인을 확인하고 source/SDK/owned-process 검증이 실패하면 cleanup하지 않는다. sourcepatch·observer log·exec trace·stderr는 별도 원문이며 sourceExpected나 trace rows를 정규화하지 않는다. 추가 로깅의 scheduling 영향과 비재현 한계를 receipt false flags에 유지한다.

실제 native configure/make/새 tracer/cold guard/native auditor는 모두 Actions 전 UNRUN이다. 이 계획과 코드 작성은 실제 errno 관측이나 #560 해결·전체cohort 수용이 아니다.

### 고정 원문과 Actions 연결

[Ubuntu archive](https://archive.ubuntu.com/ubuntu/pool/main/s/strace/)의 다음4개를 다운로드 크기와 SHA256으로 검증한다. `.deb`는 설치·실행하지 않고 `./usr/bin/strace`만 inert 추출하여 현재 시스템 파일과 byte 비교한다.

| 원문 | bytes | SHA256 |
| --- | ---: | --- |
| `strace_6.8.orig.tar.xz` | 2457948 | `ba6950a96824cdf93a584fa04f0a733896d2a6bc5f0ad9ffe505d9b41e970149` |
| `strace_6.8-0ubuntu2.debian.tar.xz` | 21776 | `6efc6fde478f3beb37dbd246f1e9fe4f13e74eef794b5b66d7589f0933bc351b` |
| `strace_6.8-0ubuntu2.dsc` | 2285 | `330679ec872f5d097809a28fe0bcd4fe3a0ba2a57e639feec53ffc3745aa831b` |
| `strace_6.8-0ubuntu2_amd64.deb` | 584172 | `d588810ae26b06fee6678dc81e5b54f6efcde8e718e4589adb4d11d254b9820b` |

runtime 원문3SHA는 `src/syscall.c=0d8c5449b011a8cd16f0b5d0864f57fa633867876682270fee294390bb0e62b1`, `src/strace.c=21fe0b653612b9354ff783a9ca1643d39682d85fd38bf32a14b09cdea41febd1`, `src/filter_seccomp.c=8073129838f921c5c0286c3c6cc0bebf7832b04f476340114ea25e5c8216c4a1`이다. `.deb` member는2087432B/SHA256 `28f957c227012de0b18d1bd7fff2d396cb693ea60ed8013be68de071e84b5001`이다. 네 quilt patch 원문을 그대로 적용한 뒤 이3SHA를 다시 확인한다. 계측 diff 적용 후2SHA도 helper에 고정한다. orig/debian tar/dsc/deb, observer.patch, `COPYING`/LGPL/Ubuntu copyright/Linux UAPI notice, 실제 flags/config.log/config.h, command argv와 compiler/DSO seals를 proof에 보존한다.

job `cold-tracer`는 prepare16분/apt6분과 구분한 diagnostic shared1200초를 갖는다. 내부 각 명령은 남은 shared 시간으로 잘라 configure180초/make300초/cold420초/auditor build300초/audit60초 이하로 실행한다. 이 숫자의 합을 보장 시간으로 주장하지 않으며 준비·관측·후속 감사가 shared 안에 맞지 않으면 실패한다. proof512MiB와 child file512MiB, observer64KiB를 유지한다. 원래 SDK helper의 cold-tracer job은 Android 회수 allowlist에 없으므로 초기16GiB admission이 충분한지 실제 hosted FS에서 확인해야 한다. 새 회수 정책은 추가하지 않았다.

후속 native 감사는 current `ci_disk_bootstrap` ELF를 Cargo JSONL 전체 parse와 exact package/manifest/target/src/profile/features/executable 선택으로 검증한 뒤 `--ignored --exact prepared_audit::prepared_audit_actual_cold_bootstrap_receipt --nocapture --test-threads=1 --color=never` 하나만 실행한다. 정상 event0개/footer만인 경우도 전체 sci/정상/비정상 counter 합으로 검증한다. missing footer/overflow/IO오류/unknown trace는 실패이며, `outcome.json`에 phase/command_exit/finalizer_exit/observer_complete/strict_cold_audit/immutable_seals/owned_processes_nonlive를 분리한다. `root_fixed`, `ubuntu_elf_identical`, `nonreproduction_is_fix`, retirement credit은 항상 false 또는0이다. DSO는 현재 ELF의 loader/ldd 해석과 FD 봉인 범위이며 전체 동적 로드 폐쇄나 adopted 자손 전체 wait를 주장하지 않는다.

부모가 새 workflow의 정적 정책·count·Focus exact pins·ledger를 등록한 뒤 Actions에서만 build/실행한다. 현재 local 검증은 Bash/YAML 문법, exact archive bytes에 대한 patch context/postimage의 메모리 비교, 추가 ptrace0 및 source diff 검토뿐이다. 실제 C compile/ABI/health footer/cap/timeout/reap/최종 fit/비재현 여부는 모두 UNRUN이다.

## 2026-10-25 338 재발

[Fast40158739511/job121065588455](https://github.com/bin9208/openpilot-rust/actions/runs/40158739511/job/121065588455)의
source `338fcb4cd160da8733bbcadcb932770f85d38d97`에서도 strict cold audit가
raw276행 `2932  ???( <detached ...>`를 거부했다. 새 roster 검사는 아직 미도달이다.
artifact12585324471은590,039B/SHA256
`d7fc11a2065270188a8c2131cc71b5047d84c199808c11743ec8ee9e6f4e18aa`/78CRC다.
strace와 stderr collector의 wait는0이고 회수가 확인됐으며 debug21,766,689B가 보존됐다.
오류 반환과 op의 실제 값은 아직 관측되지 않았다. 원문/strict 거부와 기존 Fast 실패는 유지한다.
별도 patch 진단4파일은 독립 source 검토에서 추가 P1/P2가 없었으며 C build/실제 관측은 미실행이다.

### patch 파일의 서식 경계

Git diff 서식 검사가 unified patch의 원문 context인 공백·탭을 코드 여백으로 판정해,
동일한 두 C postimage를 만드는 zero-context unified patch로 저장했다. 공식 tar의 두 preimage와
모든 기존 context를 읽기 전용으로 대조해 재구성했고, 결과 C SHA256은 이전 계측과 동일하다.
원문 preimage/postimage 전체 SHA 고정과 `--fuzz=0`을 유지한다. C 빌드·실제 patch 도구 적용은
로컬에서 실행하지 않았으며 Actions가 확인한다. 전역 서식 예외는 추가하지 않았다.

## 2026-10-25 392 첫 실행과 stopped-leader 후속 후보

[40159289213/job121067255595](https://github.com/bin9208/openpilot-rust/actions/runs/40159289213/job/121067255595)는
source `392ff232b83a10b89e5485dfdbfbba610fa827e1`에서 실패했다. artifact12585755866은
8,831,269B/SHA256 `a44beafbf77b82f175f40043e6e67d2730b2eb111f442219fbaf7c86237f3858`,
전체334 CRC·중복0·expanded45,637,157B다. 마지막 deb 다운로드는584172B/status0이지만,
release 전에 `command left live/unknown group members`로 중단돼 마지막 SHA 검사는 미도달이다.
C patch 적용·configure/make·새 tracer·cold bootstrap·errno 관측·strict auditor도 모두 미도달이다.
source/index와 외부 SDK final은 성공하고8개 내부 tree와127개 held canonical 파일은 전후 같았다.
그러나 실제 finalizer1/immutable_seals=false/closure 실패를 성공으로 바꾸지 않는다.

기존 leader는 command 완료 후 release를 기다리며 외부 `sleep 0.01` 자식을 만들었다.
parent는 leader만 제외하므로 이 대기 자식과 group 검사가 충돌할 수 있다. 이전 proof는 실패
PID/stat을 저장하지 않아 이번 실제 잔류자가 sleep이었는지는 미확정이다. 커널/strace의
unknown 원인이 이 controller 결함으로 해결됐다는 뜻도 아니다.

승인된 후속 후보는 command status/done 뒤 builtin `kill -STOP "$$"`으로 대기한다.
parent는 PID/PPID/PGID/session/startticks와 T 상태를 최대300회/기존 command deadline 안에서
확인하고 원래 group 검사를 수행한다. 성공하면 release 생성→동일 stopped identity 확인→CONT→
`wait -f`로 원래 종료코드를 받는다. release 없는 CONT는125로 실패한다. 완료 후 release
대기에는 외부 자식이 없으며 실제 다른 live member와 unreadable/malformed proc 항목은 계속 거부한다.
실패 회수는 현재 소유성을 확인한 group만 KILL하고 직접 wait한다. 소유성 확인 실패는 신호·wait를
시도하지 않고 finalizer 실패/nonlive=false로 남기며 adopted 전체 wait를 주장하지 않는다.

`logs/<label>.stopped.stat`, `exit-codes.tsv`, `group-scans.tsv`를 보존한다. group 실패는
`logs/<label>.group-before-release`/`.group-after-wait` 또는 `logs/finalizer.group-failure`에
create-new·64KiB 상한으로 kind/return/PID/raw stat/state/PPID/PGID/session/startticks/가능한 exe를
남긴다. raw stat 읽기 상한은8192B이며 argv/env는 수집하지 않는다. 이미 사라진 proc와 남아 있지만
읽을 수 없거나 형식이 불명확한 항목을 구분하고 후자는 실패한다. `finalizer-stages.tsv`도64KiB로
회수/direct wait/provider/held/tree/source-index/admission의 반환을 구분하며 aggregate 실패를 보존한다.

hosted 전용 protocol 통제는 다음3개다. 일반 오류나 통제 setup 실패를 음성 PASS로 세지 않는다.

- `childless-stop-cont`: 실제 command0와 원래 direct wait0, 잔류 group 부재.
- `original-nonzero-direct-wait`: 실제 command7을 direct wait7로 그대로 전달.
- `genuine-live-member-rejected`: 같은 session/group의 실제 추가 stopped child와 정확한
  `live-member`/return1/해당 PID sidecar를 확인한 뒤 owned group KILL/direct wait137 및 nonlive 확인.
  이 통제에서 KILL한 특정 자식의 종료 전이만 최대300회/기존 shared deadline 안에서 관측한다.
  일반 command의 live/unknown 판정에는 재시도나 유예를 추가하지 않는다.

`protocol-controls.tsv`는 정확3행이며 `outcome.json`의 `protocol_controls_complete`와
`protocol_controls_expected=3`으로 분리된다. 기존1200초 공유deadline·각command timeout·모든
성장 cap·observer64KiB·원문 pins·C postimages·Fast/parser/unknown strict 실패는 그대로다.
로컬은 Bash/YAML 문법·source diff/hash만 확인했다. 새 통제·회수·C build·실제 errno·cold audit는
Actions **UNRUN**이며 이 후보의 기능 합격·root fix·원본 제거 credit은0이다.
