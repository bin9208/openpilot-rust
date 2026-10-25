# #523 원본 launch unittest31의 native 대체

2026-10-26. 상태: source581에서 Python 파일 삭제 후 exact-source Actions 수용 완료.
원본 `rust/tools/tests/test_offline_qcom_launch.py`의31개 parameterized case를
`compiler_launch.rs` 일반9개가 그대로 포함한다. 기본 geometry5·u32 경계2·invalid9·
group-sensitive6·explicit2·최대 register1·unsupported3·truncated3이다. native는 추가로
`__local`, Unicode combining mark/word boundary, explicit float/bool pass-through를 검사한다.
원본6,287B의 SHA256은
`83ae3537f1c6ec6f72d27810f40b9fb4dc834a4d5945152c529b4a239cffac72`다.
원본 파일은 기존 Git blob `4fa9bebbff3f4ef5f5fb262fe7a909c5be8ef851`에 보존돼 있다.

독립 원본 expected4411의 `compiler_corpus`도 현재 native policy와 비교한다.
sourcea621/run40241338985의 실제 launch9/원본4411/strict Clippy는 성공했다.
원래 pytest 파일명을 소비하는 workflow/다른 Python·Rust caller는 검색에서 없었다.
source-file 삭제 후에는 같은9/4411과 stage8/union1/strict Clippy를 다시 실제 Actions에서
검증하고 working tree+tracked index의 정확한 파일 부재를 명시 확인한다. 해당 native-units
model cohort 외 다른 원본 테스트·compiler runtime·production offline Python은 유지한다.

원본31개를 단순 삭제한 것이 아니라 기존 원본 수집값과 실제 native 실행을 먼저 확인했다.
다만 삭제 후 새 source gate 전에는 기준 인벤토리 상태를 pending으로 남긴다.
성공 뒤에만 baseline1개를 교체로 인정하며 전체 모델/컴파일러/Python 부재 주장은 하지 않는다.
로컬은 source/caller 대조와 Bash syntax/diff만 확인하고 Cargo/pytest/원본 실행은 하지 않는다.


## 2026-10-26 원본 launch unittest31 삭제 후 실제 수용

source `581c84520fc9d3c014ad155bf0eff13f96289db4`의
[run40242309674/job121316568836](https://github.com/bin9208/openpilot-rust/actions/runs/40242309674/job/121316568836)은
삭제된 `rust/tools/tests/test_offline_qcom_launch.py`의 working/index 부재를 검사 전후 모두
확인했다. 원래31개 parameterized case에 대응하는 native launch9, 독립 원본4411 corpus1,
stage8/ignored0·preserve_order union1·strict Clippy와 source/index-stage/wholeSDK 봉인·업로드/
소유 source cleanup이 모두 성공했다. 기존 Python file을 forwarding/skip으로 남기지 않았다.

artifact12618110662는1,586,311B/SHA256
`b05efdb316aaba463e68f47087c455e80268aefe81c26b8b27aac4af868bdf45`,
46 safe unique regular member/전체 CRC, expanded6,935,713B다. 명령4개 exit0,
source/index/index-stage·SDK file/program/roster6쌍 전후 byte 동일, SDK install0이다.

기준3580은 **300교체/3부분/3277대기**, routing3306은 **26교체/3부분/3277대기**다.
새 genuine compiler 비교 실행경계1개를 pending으로 추가했으므로 추가 원장163은
**32교체/127대기/1기존native/2비실행/1제거**다. production ordered emitter의 실제 genuine
검증은 별도 진행 중이며 이 테스트 파일1개 수용으로 전체 compiler/모델/Python 제거를
완료 처리하지 않는다.
