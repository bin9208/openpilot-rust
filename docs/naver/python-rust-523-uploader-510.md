# #523 후보의 기존 #510 uploader 수정 연결

2026-10-25 KST. [#510](https://github.com/bin9208/openpilot-rust/issues/510)의 기존 교정
`5b66042b0fd7808c2523df732c2433993d2314dc` 중 uploader runtime의 정확한 diff만 재사용했다.
현재 `rust/crates/uploader/src/runtime.rs` 전체 bytes가 해당 commit의 blob과 같음을 확인했다.
[PR500](https://github.com/bin9208/openpilot-rust/pull/500)의 branch/ref·다른 변경을 수정하거나
전체 source/history를 병합하지 않았다. 이번 후보는 기존 #523/#296 dev 전환의 실제 runtime
결함 연결이며 dev-md 소스는 포함하지 않는다.

## 기존 결함과 원본 계약

[기존 실패39869685862/job120141047822](https://github.com/bin9208/openpilot-rust/actions/runs/39869685862/job/120141047822)는
834 Params string 비교 뒤 missing-id/native uploader에서 예상하지 않은 `errorLogMessage`를
검출했다. artifact12470339923 SHA256은
`b21ea13782d650a199e46a5d5b85596574ed6e2ed5185867a776e3274e57aa08`이다.
실제 메시지는 `failed to set core affinity`, errno22였으며 당시 CPU 허용 mask는 미보존이다.
특정 cpuset·캐시/로그 결함이라고 해석하지 않는다. 이 원문 실패와 기존 unit 성공은 #510의
역사 근거이며 이번 기록에서 새로 실행·재감사한 것으로 표현하지 않는다.

원본 `openpilot/common/realtime.py::set_core_affinity`는 Linux의 `not PC`에서만 syscall을
요청한다. hardware의 TICI는 `/TICI` 파일 판정이고 PC는 그 반대다. 기존 Rust uploader의
무조건0..3 요청을 기존 수정대로 PC에서는 생략하고 기기에서는 정확0..3 mask를 요청한다.
기존 기기 실패 진단·로그/HTTP/업로드·signal 정책은 그대로다. 새 unit을 중복 작성하지 않고
기존 `pc_does_not_request_device_affinity`, `device_affinity_keeps_the_source_little_cores`를 유지한다.

## 현재 수용 상태

한국어/영어 사용자 안내에 같은 host 경계를 반영했다. 로컬 검증은 rustfmt·diff·원본 patch와
postimage byte 동일성 및 source 검토만이다. 현재 HEAD의 기능·실제28조합/전체 support/필수 PR·
정상 dev 통합/별도 post-merge는 아직 미실행이다. 실행 중인0538007 PR 결과에는 이 미커밋 수정을
소급 포함하지 않는다. 다음 현재-source PR 검증에서 별도로 판정하며 #510은 열린 상태를 유지한다.
이 변경의 inventory credit과 실기 수용은0이다.

## #510–#513 현재 acceptance 재판정 — 2026-10-26

네 이슈의 최신 본문과 현재 source, 기존 cached 감사 기록을 읽었다. 기존 archive의
재다운로드·전체 SHA/CRC 재감사, 기능/CI 재실행은0이다. 앞 절과 private의 옛
`issues-510-513-current-acceptance-audit.md`는 당시 상태이며, 특히 #511 미구현/UNRUN을
현재 상태로 재사용하지 않는다. 이번 추가는 좁은 기술 수용과 이슈 본문 전체 완료를 구분한다.

| 이슈 | 정확한 수정 및 현재 source | 재사용하는 실제 수용 근거 | 판정·남은 최소 조건 |
| --- | --- | --- | --- |
| #510 | 기존 `5b66042b0fd7808c2523df732c2433993d2314dc`, #523 반영 `b7ab74bf0979dce2f02ad53bf6a8ef24643c83a3`. 현재 runtime 전체 bytes가 기존 수정과 동일 | [unit2/core](https://github.com/bin9208/openpilot-rust/actions/runs/39884415762/job/120189124006), [native Params14/support](https://github.com/bin9208/openpilot-rust/actions/runs/40187340482/job/121150826873), 아래 source126 support도 성공 | PC no-op/device0..3의 좁은 수정은 수용 가능. 새 runtime patch나 같은 unit/native14 반복 불필요. 본문 전체의 원본/native28 증거 경계·필수 gate·정상 PR500/dev 통합·post-merge는 별도이며 현재 모두 완료했다고 쓰지 않는다 |
| #511 | 기존 Python 교정 `b1db2b0f839a3a2e5c2f8ba8317d97b4b03faccb`; 현재 native receipt 보완 `126fdbea36598566d1040be0a35a365c909be4e4` | [source126 support](https://github.com/bin9208/openpilot-rust/actions/runs/40209691630/job/121222236467)의 native Params14와 새 downstream-failure exact1 실제 PASS | 실패 전 실제 child receipt 보존의 좁은 구현·회귀는 수용 가능. 삭제된 Python을 복원하거나 같은 exact1을 반복할 필요 없음. 본문 전체의28경계·명시된 정상 통합/post-merge는 남음 |
| #512 | `10919538d31b04d28d9a23b383d925441434d579` | [real-sudo5/registration4](https://github.com/bin9208/openpilot-rust/actions/runs/39893953500/job/120219914880), [complete web/archive/upload/cleanup](https://github.com/bin9208/openpilot-rust/actions/runs/39893948836/job/120219900477), 이미 감사한 artifact12480001554 | root0600 archive 결함의 좁은 기술 조건은 이미 수용됨. 새 archive 테스트 불필요. #523에는 옛 caller와 test2개가 없으므로 현재 UI job을 그 caller로 세지 않는다. 본문에 명시된 exact-head gates·정상 통합/post-merge는 남음 |
| #513 | `5a26b3ffda5062039f15c9cc2f649dc33b98a81d`; 공개 최소 child `9e483329b671e908e9a686c54d23407bfb7ac8f1` | [source9e7 native26/영향 image5 PASS](https://github.com/bin9208/openpilot-rust/actions/runs/40080413343/job/120832600964), 기존 artifact12554902305 감사 | corrected branch/commenter·overwrite=false·upload/error 순서는 실제 native 수용됨. 그러나 pre-fix 원본의 wrong-branch prefix 실호출 증거는 확인 못했으므로 현재 본문 전체 완료 불가. full PR 의존은 본문에 없으며 새로 추가하지 않음 |

현재 #510 runtime SHA256은 `bca54dfcf60001fda8da9b5ad34dfb90e3097ac942ef3e22931f49570c71845a`다.
기존 genuine original Params14는 고정 fixture
`30f9e1f21c47cd9bcefcc161456e1df9984338cc2a2e940eabb8dce0a97a6176`의
7case×2 collector 원본 관측이며, native14는 그 기대값을 재사용한다. 기존 source/provider
수용을 보존하되 `fresh_python_runtime=false`인 새 native14를 같은 job에서 원본/native
28회를 새로 실행한 것으로 바꾸지 않는다. 성공한 불변 원본·native 검사를 다시 돌리는
권고는 하지 않는다.

### #511의 새 실제 증거와 원문 보존 범위

source126의 `records.rs`/integration test는 현재와 byte 동일하고 각각 SHA256
`a9feb6e4c9040b7fba28074c06df48e42f245529f72622ee3cc092dae4316711` /
`31b78885f433393d54b230c1106628088e586998248a4b614f4da496d42cc995`다.
`NativeRunner` 반환 직후, 기존 permission 복원·peer.finish·HTTP/log assertion 전에
exclusive0600 bounded `uploader-exit.json`을 남긴다. 실제 code/OS argv와 worker의
읽기 전용 host/TICI/affinity context를 보존하며, unobserved code를 만들지 않는다.
uploader와 collector의 stdout/stderr 경로는 별도다.

원본 job log5240–5248행은 기존 target/feature의
`downstream_records_failure_retains_actual_exit_and_cleans_worker_namespace`가
0.40s에 **1 passed/0 failed/0 ignored/8 filtered out**임을 확인한다.
뒤의 grep 출력은 같은 결과를 한 번 더 인쇄한 것이며 두 번째 실행으로 세지 않는다.
해당 test는 실제 uploader exit1 뒤 정확 downstream 오류, receipt/독립 로그,
불변 원본 records/HTTP, worker-owned IPC/shm의 ENOENT를 요구한다.
이것은 성공 report의 exit_code만 확인한 옛 native14와 다른 경계다.

이번에 캐시에 없던 job log만 읽기 전용으로 받았다. 보관된
`ci/support126-121222236467.log`의 SHA256은
`053a7b7a74238c1259154c244db4fbd48c3968eeea2ad937c7202a5977eaeff0`다.
support artifact12606067030(61,829,349B, API/업로드 digest
`612cfd4e94bff0af10d693d18d26414f34764035cb0caa41007dda45db5df2dc`)와
private12606127008의 업로드 및 job success도 확인했다. 이 두 ZIP을 새로 받아
개별 receipt member를 직접 재감사했다고 주장하지 않는다.

### 남은 조건을 최소 범위로 유지

#513의 현재 Python/native/test3개는 기존 source9e7과 같은 bytes다. native의 기존5개
`images_513_*`는 corrected 동작을 검증하며 pre-fix 원본을 실행하지 않는다.
가장 작은 부족 증거는 pinned pre-fix 원본을 private image/fake transport로 실제 호출해
서로 다른 commenter/branch에서 업로드 뒤 `commits/<commenter>/pulls`가 남는 prefix를
Actions에서 한 번 보존하는 것이다. corrected native5/26의 기존 PASS와 source diff를
재사용하면 된다. 과거 계획의 original5/image-only18이라는 숫자 자체를 새 acceptance로
추가하지 않으며, 전체 PR·실제 계정/token/comment나 native26 재실행을 요구하지 않는다.

#510/#511/#512의 정상 통합/post-merge 의존은 추정이 아니라 각 상세 본문의 명시 조건이다.
이번 단일 API 관측에서 PR500은 head `9b047be51cf092039783824c79b8c7120764ed83`의
OPEN/미병합이며 integration gate FAILURE, rust checks/aarch64 CANCELLED다.
별도 PR343의 source126 integration/support 성공을 PR500 정상 병합·post-merge로
대체하지 않는다. #512의 과거 archive 수용은 보존하고, 현재 caller 대응 여부는 정상 통합
때 정리한다. 폐기된 옛 workflow 전체를 복원하라는 구현 요구는 아니다.

이슈 close/edit, 코드/WF/원장 수정, stage/commit/push는0이다. 이 기록만으로 #523 전체
전환·퇴역·기기 수용을 주장하지 않는다.
