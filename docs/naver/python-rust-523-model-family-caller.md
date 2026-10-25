# #476 실제 fresh4 성공 뒤 원래 전체 six-source 검사

2026-10-26. `python-rust-523-model-family-caller.yml`은 main의
`model-family-retirement` job/env를 그대로 복사하고 기존 다른 exact-copy와 같은
main 전체 SHA 확인 step 하나만 추가한다. 나머지 모든 job field·명령·원본 manifest·
기한·성장량·실패/후속 진단·source/provider 봉인·upload/cleanup은 main과 object 동일하다.
원래 `same_elf_replays_all_three_native_clis_in_both_absent_roots` 전체를 실행하며
작은 fresh4 성공을 fullsix 성공으로 바꾸거나 failure-only 진단 guard를 완화하지 않는다.

main source126의 [job121222236470](https://github.com/bin9208/openpilot-rust/actions/runs/40209691630/job/121222236470)은
첫 driver collector/ordinary에서 실제6.24초 뒤 원래 #476과 같은 차이로 실패했다.
`faceOrientation[0]` expected=-0.08113645017147064, observed=-0.08065624535083771,
기존 tolerance1e-6, 차이0.00048020482063293457다. 이후 source425의 별도 fresh driver
fourcase 성공은 다음 전체 검사에 진행할 새 근거다. 해결 원인은 아직 단정하지 않는다.
source425의 guarded prestate API, 전역 serde_json float_roundtrip 추가와 host/provider 차이는
원인 분석 후보이며 어느 하나의 효과라고 수치 증거 없이 결론내리지 않는다.

fullsix는 같은 held native CLI/daemon/model-run/C++peer, 독립 driver 두 dataset,
현재 호스트에서 별도로 만든 driver4/family5 catalog, genuine USB45assets/3licenses를
유지한다. checkout/private 두 원본6개 부재 루트에서 collector/no-collector와 ordinary/raw
경로를 모두 실행하고 startup8/usbgpu6·복원/외부경로 등 기존 negative도 검사한다.
정확 모델·원본 expected·raw/float 허용 오차·source와 source-absence 정책을 보존한다.

실행은 기존대로 `ubuntu-24.04`, jobs2/debug0,120분 상한이다. 로컬4GB 러너로 옮기지 않는다.
기존 작업별 admission과 비덮어쓰기/source sealing이 계속 용량·소유권을 판정한다.
로컬은 YAML/Bash 구문·복사본 전체 의미 비교·diff만 확인했으며 실제 GHA는 미실행이다.
이 새 진입점에는 기존 Python compiler/frontend가 남으므로 추가 실행 원장에 pending1개를
기록한다. #476 및 #523, baseline source6 수용 credit은 실제 결과를 감사하기 전까지0이다.
