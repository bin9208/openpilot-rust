# #559 UI recording pending-close 현재 검증 경계

2026-10-25. 실제 [c00 run40154418322/job121052614014](https://github.com/bin9208/openpilot-rust/actions/runs/40154418322/job/121052614014)은 failure다. #559의 구현 전체 수용, 원본 제거 또는 retirement credit은 아직 없다.

## c00의 실제 결과

source `c00c7a11fbee357cbfb39bee45125d73596bf56c`, helper SHA256 `10cb3d55ba9a655bd1239843c4d5ab6e414695d5fef91bf0fdb52cd39682381a`, workflow SHA256 `b60233dbcded60ae051e986b1bf76b9b3b73bf6670824952f6a89799ee95d397`. actual GitHub-hosted Ubuntu24.04 X64/private Rust1.94.0에서 decoder SDK 준비, fresh baseline RED1(의도한 정확 실패), candidate GREEN1, recording9, strict Clippy까지 성공했다.

최초 실패는 `five_original_modes_preserve_real_graphics_ticks_and_native_encoder`의 첫 profile case다. child exit1에 `application original source changed: openpilot/system/ui/lib/application.py`가 기록됐고 Rust test exit101/helper exit1로 실패했다. source admission 뒤 실제 graphics/PNG/frame/encoder 비교에는 도달하지 못했으며 나머지 네 mode도 실행되지 않았다. shared1200초 및 파일/성장 cap은 변경하지 않는다. 실패 메시지는 deadline/provider가 아니라 source pin 거부다.

[artifact12584138814](https://github.com/bin9208/openpilot-rust/actions/runs/40154418322/artifacts/12584138814)은40,897,338B/SHA256 `f3f7a1b25e5ceeb84beca5fb17f2655e8a462856ecd358f2b5861fe56aec6718`, 전체25,304 member CRC/SHA·expanded206,632,400B 감사 완료. 실패 finalizer0, held372paths identity/SHA 동일, source/SDK 및 baseline/index 전후 동일, watcher27개 모두0/direct leader wait 뒤 group 부재27개였다. 별도 SDK/source after·upload·owned cleanup은 실제 success다. adopted children 전체 wait/완전 FD·DSO closure는 주장하지 않으며 outcome의 관련 false 및 retirement_credit0을 유지한다.

helper 실행 step 시작부터 첫 실패까지 약1158.299초로1200초 소진이 아니며, timeout124/resource-stop이 없다. 실제1,101회 admission은 모두 통과했다. 기존 resource budget을 확대하거나 다음 수정의1200초 성공을 추론하지 않는다.

## 직접 원인과 최소 후속 범위

`ui_application_lifecycle_reference.rs::validate_source`는 provenance 및 source-closure 두 루프에서 각각 application.py의 원문38,593B/SHA256 `d17a3d0e8cb67c2bd37e4f48d61dc79e5ee7c7af984f0d9dc434301121a85b00`만 허용한다. #557의 현재 font staging source는42,048B/SHA256 `c659ce3574276546da11a7bd25e373321b62226411b1eaadda37401fe3b2110d`다. 두 manifest에서 이 파일만 현재와 달랐다(기존 명시된 retired Python2 absent 예외는 별개).

이미 `ui_application_logic_check.rs`에는 정확한 font3구간 길이/hash/출현1/순서와 메모리 역투영 후 old fullSHA를 검증하는 private matcher가 있다. c00 bytes의3구간을 source 데이터로 비교하면 실제 Git 원문 전체 bytes가 복원된다. 이것은 원본/현재 프로그램 실행이나 변경된 기대값 수용이 아니다.

최소 후속안은 기존 matcher를 crate-private로 재사용하고 lifecycle의 두 검사에서 정확한 application 경로 및 기존 d17a expected pin일 때만 같은 source 경계를 적용하는 것이다. 다른 source/font/absence/Params/dependency pin, fixture, 원문5 body, RED/GREEN 및 recording9는 유지해야 한다. 기존 integration target의 새 sibling 회귀에는 현재 source 통과와 font구간 변조·누락·중복·순서/비font영역 변조 거부가 필요하다. runtime mode는 현재 default source-font 범위이며 staged-font env 기능 수용을 의미하지 않는다.

초기 helper/WF/docs 범위에서 source 우회 patch는 하지 않았다. 이후 사용자 승인으로 아래 Rust4파일의 최소 연결 및 회귀를 구현했다. helper/WF의 시간·cap·source/baselineoverlay·원문 expected는 변경하지 않았다. 수정 코드와 신규 회귀의 기능 검증은 Actions 전 UNRUN이다.

## source-only 구현과 회귀 동결

기존 `application_source_matches`의 본문은 변경하지 않고 `pub(crate)`로만 공유했다. lifecycle 내부 `source_matches` closure는 정확 경로 `openpilot/system/ui/lib/application.py` **및** 기존 expected `d17a3d0e8cb67c2bd37e4f48d61dc79e5ee7c7af984f0d9dc434301121a85b00`가 모두 맞을 때만 그 matcher를 사용한다. provenance·closure 두 루프가 동일 closure를 호출한다. 다른 경로/expected에는 원래 SHA 비교를 적용한다. 새 public API, env override, 새 feature/crate는 없다.

기존 matcher의 original38,593B/fullSHA 허용 및 current42,048B의 정확3구간 길이/hash/출현1/순서/최종 old fullSHA 확인은 전부 불변이다. current c659 fullSHA는 새 회귀 입력 출처 검증에만 있으며 production의 blanket 허용이 아니다. 현재 source/fixture를 검사할 뿐 원본 source를 복구·실행하거나 expected를 재캡처하지 않는다.

기존 integration target `ui_application_lifecycle_reference`에 새 sibling `source_binding` module을 연결했다. 기존 파일은 module 등록2줄 외 전체 byte 동일하므로 원문 five-modes와 기존13개 나머지 body를 유지한다. 새4tests는 실제 public `validate_source`와 private 임시 source copy를 사용하며 프로그램 spawn/skip/fixture expected 재생성이 없다.

- `source_binding::current_font_source_passes_both_frozen_pin_boundaries`: 한쪽 연결만 있으면 전체 validator가 실패한다.
- `source_binding::font_blocks_reject_mutation_missing_duplicate_and_reordering`: 구간별 변조/길이 유지 제거/중복 및 같은 길이 순서 변경을 거부한다.
- `source_binding::non_font_application_drift_is_rejected`: FPS/stack/close 영역의 같은 길이 변조를 거부한다.
- `source_binding::projection_does_not_admit_other_paths_fonts_or_params_providers`: 유효 application bytes를 widget/provenance 또는 font/closure-only 경로에 넣어도 각각 정확한 첫/둘째 루프 오류로 거부한다. 새로운 source-font fallback 및 Params provider도 거부한다. 각 임시 변조를 복구한 뒤 validator의 성공을 다시 확인한다.

## 검증·부모 CI 연결

로컬은 Rust1.94 rustfmt/parse·diff·inert source/JSON/TOML 검토만 수행했다. matcher는 visibility 외 전체 byte 동일, 기존 integration 파일은 module 등록 외 전체 byte 동일, validator 밖 runtime 및 기존 fixture/helper/WF 불변을 확인했다. package lock의3개 graphics dependency record/기존 fallback 부재/Params layout은 고정 자료와 일치한다. 신규4tests와 수정 후 original5·RED/GREEN/recording9·Clippy는 로컬 실행하지 않았고 current-source CI 성공을 아직 주장하지 않는다.

부모가 현재 fresh candidate-reference ELF의 기존 owned wrapper 안에 아래 한 검사를 등록하면 된다. 기존 five-modes/RED/GREEN/recording9/Clippy 실행은 보존한다. expected는4passed/0failed/0ignored 및 summary1개이며 0선택을 성공으로 인정하지 않는다. 시간은 같은 shared1200초에서 차감하고 caps/원문 expected를 바꾸지 않는다. helper/WF/pins/원장/Cargo는 부모 소유이며 여기서 편집하지 않았다.

```bash
"$proof/elfs/candidate-reference" source_binding:: --nocapture --test-threads=1 --color=never
```

해당 ELF의 기존 build target은 `cargo test --manifest-path "$source/rust/Cargo.toml" --locked -p openpilot-dev-tools --features ui-application-lifecycle-native --test ui_application_lifecycle_reference --no-run --message-format=json` 그대로다. 필요 시 기존 `transitive_sources_fonts_absence_and_dependency_records_reject_drift` exact selector로 전체32 source/font/fallback/dependency 경계도 검증할 수 있다. 이 추가 검사는 원문5개를 대체하지 않는다.

| 변경 파일 | SHA256 |
| --- | --- |
| `rust/crates/dev-tools/src/ui_application_logic_check.rs` | `672942f302fa7538cbc57d20cb61ba146aeedc4e8a92397061891e933542b9e6` |
| `rust/crates/dev-tools/src/ui_application_lifecycle_reference.rs` | `910d83f44909352ace57d1bf1c59f7e3da00a1878106448eee4a7fa712b2c8ae` |
| `rust/crates/dev-tools/tests/ui_application_lifecycle_reference.rs` | `cd6233cbe31283b47f3d8a4d6dac9c4a11b60739ec5fa54d25cd23630473c458` |
| `rust/crates/dev-tools/tests/ui_application_lifecycle_reference/source_binding.rs` | `9aa47cb3cc2403e24d13d4faf1ae0b9155d980a8b6696f72931432d321a60ce6` |

실제 수용은 새 exact-source Actions 결과가 필요하다. c00의 부분 성공을 이 수정 source에 소급하지 않으며, old source pin/fixture/원본5·RED/GREEN9·1200초·성장 cap·retirement_credit0은 유지한다.

상세 inert 원문은 ignored `.analysis/scratch/2026-10-24-python-rust-523/hosted-c00-ui-12584138814-audit.md`와 동일 prefix `.audit/`에 있다. 감사 raw log/API/전수 member SHA/CRC/outcome/실패 stderr/시간·seal 비교를 함께 보존했다.


부모 CI 연결 후보는 같은 fresh candidate-reference ELF에서 `source_binding::`의4pass/0fail/0ignored를 명시 확인한다. outcome의source_binding4와기존pass조건,WF최종closure에도같이묶었다. 기존5원문mode/RED101/GREEN9/Clippy/1200초·cap은유지하며새controls도기존총예산에포함한다. 기능은아직UNRUN이다.
