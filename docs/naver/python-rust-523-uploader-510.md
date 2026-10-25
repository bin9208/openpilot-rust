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
