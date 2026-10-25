# #557: DBC·font 생성 출력과 실소비 경계

2026-10-25. [#557](https://github.com/bin9208/openpilot-rust/issues/557)의 **소스 검토용 구현 후보**다. 구현 기준 HEAD는 `f55b3b6d34ca709b924c68d93a02061186fc27b9`이고 아래 소유14파일만 다룬다. 기능·프로젝트 import·generator·컴파일·테스트·Clippy·Actions·stage/commit/push는 로컬에서 실행하지 않았다. **빌드/실소비/자원 fit은 모두 UNRUN**이며 기존 성공을 이 후보에 소급하지 않는다.

## 실제 실패 근거

[Integration40140991803/job121012510478](https://github.com/bin9208/openpilot-rust/actions/runs/40140991803/job/121012510478), merge `e882cd1dd2f709572c94fe35bcdb4cfc85997ab8`/head `419244279fd42ca7a8d83272b8e659bd4136c3ec`에서 SCons·native_models·camera216assertions/4cases·exposure972transitions·pytest570은 성공했으나 source-final이 DBC2/font4 변경으로 실패했다. source/index 불변 성공이나 cleanup 성공으로 바꾸어 기록하지 않는다.

기존 artifact12578757722는 5,518,830B/SHA256 `bf1397b681106b14a801ae82125abc8782da70b9ad2f40564dbbfa6689064af1`,47CRC이다. 원인 조사에 기존 로컬 원문만 사용했다. DBC180/235의 before는 card97, after는 RadarCAN103의 기존 SHA와 같다. font after 본문은 이 artifact에 없어 committed226 glyph와 실제 로그227/234의 정확한 차이는 미확정이다. provider를 원인으로 단정하지 않는다.

## 소유 파일과 동결 SHA

기존10 + 신규 C++ header/leaf/Python3 + 이 기록1 = **14파일**이다. 처음12파일에서 허용된 회귀 leaf와 기록이 추가됐다. 이전 scratch의9파일/5selector 안보다 아래 목록과6selector가 우선한다. workflow/공유Cargo/lib/Focus/원장 및 collision8 동결3파일은 소유하지 않는다.

| 경로 | 책임 | SHA256 |
|---|---|---|
| `SConstruct` | SCons 전용 output root export | `b285aaaaedec301715e7de1af929f5b81389e7ca59f288bed9f1c0ba621a9e0a` |
| `opendbc_repo/opendbc/__init__.py` | source/consumer root 분리와 private stage admission | `20ac3f3bd0942e48a28c7a82e4e6b011f915825281809930ffff7979f26656d0` |
| `opendbc_repo/opendbc/can/dbc.py` | 실제 named parser import의 staged admission | `5db064bb957f96706f7009255756f38271a9d14374ecf8e70628981abff2ae02` |
| `opendbc_repo/opendbc/dbc/SConscript` | 작은 generator 사본과 완전103 출력 publish | `8fa9e182b8a0fc845828fe7263076ac1018aec9cb48d2ad6ce68730ae6bb1655` |
| `openpilot/selfdrive/assets/fonts/process.py` | 출력 위치 분리와 glyph/provider provenance | `c71bd7b0c5f309481b3d2d759493cea894c0931ba1ca465c70af63bad96f91f8` |
| `openpilot/selfdrive/ui/SConscript` | font output targets/argv/입력 dependency | `f297e932f32e7a2f65aac6f7ab831690fcebd3b0d7cd6065ee7995a53da25632` |
| `openpilot/system/ui/lib/application.py` | 실제 GUI font root 및 명시 mode fallback 거부 | `c659ce3574276546da11a7bd25e373321b62226411b1eaadda37401fe3b2110d` |
| `openpilot/tools/jotpluggler/SConscript` | 동일 stage를 실제 C++ consumer에 연결 | `679d792bdc1d92ff68d98ab61c774791dbedcb5ae59365074ac4f028bab405d7` |
| `openpilot/tools/jotpluggler/layout.cc` | editor factory/사용자 편집 경로 선택 | `23950a7f01945cc88ce9a3294075add10d7ef433d06adf6477008e02aa1dfde9` |
| `openpilot/tools/jotpluggler/sketch_layout.cc` | 목록/named resolver의 factory root 연결 | `cab0d2883ed7f63bf541c04eb857e6e55b44e2438ccad6e4eee69301552a5aef` |
| `openpilot/tools/jotpluggler/dbc_paths.h` | 신규 private C++ 공통 경로 경계 | `89d67af70cccdf0411ab4f5f6fa2f171e4b188ab27287c86c046cd4b817e0595` |
| `openpilot/tools/jotpluggler/tests/dbc_paths_557.cc` | 신규 actual C++ 소비 leaf/4거부 통제 | `e860bf826791dfc63710365a5416a272c9e99d52eb00e6394bf2c4d24528ca74` |
| `tools/ci/tests/test_generated_assets_557.py` | 신규 Actions-only 실소비 회귀6/pending Python1 | `18ebd11d7d87111116d813c50db1ffd75c130dbb35d0597aa6afdeacec51a0f3` |
| `docs/naver/python-rust-523-generated-assets-557.md` | 이 기록, 자기 SHA는 인계 출력에서 별도 기록 | 해당 없음 |

## 경로/역할 계약

- `OPENPILOT_BUILD_ASSET_ROOT=BASE`는 **명시 opt-in**이다. 절대 canonical/동일 UID/0700의 source 외부 디렉터리여야 하며 source와 동일·상하위·symlink/확인한 inode alias를 거부한다. Jot macro/shell 인자에 위험한 문자는 admission에서 거부한다. mount namespace 전체의 별도 alias를 탐지하는 보안 경계라고 주장하지 않는다.
- producer는 `BASE/dbc`와 `BASE/fonts`를 만든다. consumer는 생성 후 `OPENPILOT_DBC_ROOT=BASE/dbc`, `OPENPILOT_FONT_ROOT=BASE/fonts`를 정확히 지정한다. producer만 지정하고 consumer 값을 누락한 실행은 새 실소비 gate를 통과하지 못한다. 세 env가 없는 기존 runtime/source 모드는 원래 경로와 fallback을 유지한다. 자동 기본 build-root 전환을 도입하지 않았다.
- DBC는 source generator 트리만 private 복사하고 raw65를 출력에 복사한 뒤 기존 `create-all OUTPUT --source COPIED_GENERATOR --all-programs`를 실행한다. generator는 procedural 입력에도 쓰므로 OUTPUT만 옮기는 것으로는 부족하다. 완전103 catalog의 두 variant는 기존 RadarCAN map과 비교한다. native card97/RadarCAN103 API·renderer·신호·golden은 변경하지 않는다.
- DBC manifest는 current source/native generator source·실제 generator 파일 SHA before/after, 실제 argv/private 입력 사본 SHA, 출력 전체 SHA를 기록한다. 출력 전체가 만들어진 후 `.ready`를 publish한다. 소비는 current source/ELF/role/output을 검증하고 실제 named parser는 그 root에서 파일을 연다. SCons cache에는 새 stage를 넣지 않는다.
- 기존 stage를 교체할 때는 소유 mode/이전 manifest/정확 roster/실제 bytes를 먼저 확인한다. `.ready`를 먼저 제거하여 실패·부분 생성을 ready로 수용하지 않는다. DBC는 private 임시 출력 후 확인된 이전 output만 교체한다. font는 같은 owned output을 갱신하며 실패 뒤 partial root를 자동 수리/소스로 fallback하지 않는다. 실패 proof 보존 뒤 새 job/root로 처리해야 한다.
- font는 TTF/OTF/translations를 source에서 읽고 atlas/metrics 출력만 외부로 보낸다. glyph 집합·size/padding·atlas algorithm·metrics·page basename은 유지한다. manifest에 요청/반환 codepoint·metrics SHA·출력 SHA·실제 Python version 및 관찰 provider 파일 SHA before/after를 기록한다. `/proc/self/maps`는 **sampling**이며 닫힌 전체 DSO trace가 아니다. consumer마다 실제 provider 전부를 이 manifest가 대신 검증한다는 주장도 하지 않는다.
- `ASSETS_DIR`/icons/BASEDIR는 유지하고 `FONT_DIR`만 명시 선택한다. 명시 staged mode에서 누락 atlas를 source TTF로 fallback하지 않는다. 실제 UI loader가 `rl.load_font`를 호출하며 관측 wrapper는 원래 호출/결과에 위임한다.
- Jot의 두 실제 C++ 소비 파일은 같은 private `dbc_paths.h`를 쓴다. SCons macro가 고정한 factory103과 기존 사용자 편집 디렉터리의 선택 순서를 유지하고 사용자 파일을 삭제하지 않는다. header는 canonical/UID/mode/ready 역할을 확인한다. **C++ 단독 manifest 전체 SHA verifier는 아니다.** 현재 fixture103/source/ELF 검증 및 전후 봉인을 연결한 gate로 수용해야 한다. 명시 stage를 사용한 Jot ELF에는 private job 절대경로가 들어가므로 일반 배포 후보로 세지 않는다.

## 부모 Actions 연결 — 기존 필수 명령은 유지

아래는 실행 계획이며 여기서는 실행하지 않았다. 실제 prepared interpreter/SDK/compiler absolute 경로·FD·provider와 operation-specific admission은 부모 workflow가 검증한다.

1. source baseline/SDK 준비 뒤 기존 build filesystem 안에 canonical `BASE=$CI_JOB_TEMP/build/generated-assets`를 만들고0700을 검증한다. `OPENPILOT_BUILD_ASSET_ROOT`를 producer 현재 process에 export하고 DBC_ROOT/FONT_ROOT는 제거한다. 현재 `OPENPILOT_DBC_GENERATOR`도 기존 준비 경계의 actual ELF로 고정한다. **GITHUB_ENV만 쓰면 같은 step에는 적용되지 않는다.**
2. 기존 `scons`와 `scons native_models`, camera/exposure/기존 pytest 명령·selectors를 유지한다. SCons의 기존 `ENV` whitelist를 통째 상속으로 바꾸지 않는다. font action도 그 whitelist로 같은 `python3`를 실행한다. 비-larch64의 기존 SCons가 실제 Jot 두 translation unit도 빌드하므로 full build를 좁은 header leaf만으로 대체하지 않는다.
3. producer 성공 뒤 consumer 두 root를 현재 process와 후속 GITHUB_ENV에 정확 전달한다. manifest/ready/roster와 실제 interpreter version/provider를 proof에 보존한다. 설치 요청 버전만으로 actual version을 단정하지 않는다.
4. 기존 bounded compiler/provider 경계에서 아래 leaf를 fresh build하고 그 실제 ELF path를 `GENERATED_ASSETS_JOT_LEAF`에 전달한다. 새 Cargo bin/크레이트는 없다. 아래 `c++`는 등록 시 확인한 absolute current compiler로 바인딩한다.

```bash
c++ -std=c++17 -Wall -Wextra -Werror -I "$CI296_SOURCE" \
  "$CI296_SOURCE/openpilot/tools/jotpluggler/tests/dbc_paths_557.cc" \
  -o "$CI_JOB_TEMP/build/dbc-paths-557"
export GENERATED_ASSETS_JOT_LEAF="$CI_JOB_TEMP/build/dbc-paths-557"
```

5. 현재 준비한 프로젝트 venv의 absolute Python으로 다음 **정확6 selector**를 실행한다. pytest tmp도 감시 build filesystem 아래에 둔다. 실제 Xvfb/xauth/GL 준비는 아직 확인 전이며 없으면 bounded 준비 또는 명시 FAIL이다. OFFSCREEN만으로 headless provider가 준비됐다고 세거나 skip하지 않는다.

```bash
OFFSCREEN=1 xvfb-run -a "$CI296_SOURCE/.venv/bin/python" -m pytest -o addopts='' -q \
  --basetemp="$CI_JOB_TEMP/build/generated-assets-557-tests" \
  tools/ci/tests/test_generated_assets_557.py::test_scons_dbc_catalog_matches_frozen_reference \
  tools/ci/tests/test_generated_assets_557.py::test_named_dbc_parser_reads_published_output \
  tools/ci/tests/test_generated_assets_557.py::test_font_output_directory_preserves_bytes_and_glyphs \
  tools/ci/tests/test_generated_assets_557.py::test_explicit_roots_and_partial_publication_are_rejected \
  tools/ci/tests/test_generated_assets_557.py::test_gui_font_loader_uses_published_atlas \
  tools/ci/tests/test_generated_assets_557.py::test_jotpluggler_shared_resolver_reads_current_variants
```

- catalog103과 generated45는 기존 frozen JSON의 name/bytes/SHA를 비교한다. fixture SHA는 각각 `57278d6bf03b3cd7ea0c00238c6082657c37512056adc423079abe80562ea6cc`, `ef1951344a2be6d41be51568432559a55d4271def9249dfa840ba4237f624736`로 고정했고 수정하지 않았다.
- named parser는180/235/comma_body를 **이름**으로 읽고 stdlib audit-hook의 실제 open3개를 확인한다. 직접 absolute 파일 전달로 wiring을 우회하지 않는다.
- font 재생성 비교는 같은 current 입력/provider에서 다른 private output의 모든 atlas/metrics bytes와 glyph 목록을 비교한다. `historical_font_equivalence:false`이며 역사 font expected 재capture가 아니다. clean producer와 같은 PATH/PYTHONPATH로 자식 interpreter를 실행한다.
- 거부 통제는 올바른 공통 context에서 wrong/missing/partial root·source 중첩·symlink·role/source pin·extra DBC를 거부한다. source TTF가 있는 private control에서도 staged missing atlas의 fallback 거부를 검사한다. 각 오류 메시지까지 확인하고 copied valid DBC context의 양성부터 요구한다.
- actual GUI/font loader와 공통 C++ resolver를 실행한다. C++ leaf는 actual variant2·거부4·사용자 foreign bytes 보존을 확인하고 GUI/decoder/traffic을 실행하지 않는다. C++ leaf 성공만으로 Jot 전체 GUI 또는 차량 정상 동작을 주장하지 않는다.

관련 기존 Rust4 selector는 부모 소유 gate에서 영향 범위를 확인한다. 이 작업은 해당 원문 검사/body/fixture를 수정하지 않았으며 새 leaf에서 중복 fresh Rust build를 요구하지 않는다.

```text
--features dbc-generator-native --test dbc_generator full_original_45_names_sizes_hashes_order_and_source_nonmutation -- --exact
--features dbc-generator-native --test dbc_assets card_original_97_assets_subset_and_source_isolation_are_preserved -- --exact
--features dbc-generator-native --test dbc_assets radar_original_103_assets_and_live_space_input_metadata_are_preserved -- --exact
--features dbc-generator-native --test dbc_assets immutable_original_consumers_and_catalog_distinctions_remain_explicit -- --exact
```

## 자원·봉인·등록 잔여

source2GiB/기존 dependency-native setup12GiB 및 원래 phase deadline을 올리지 않는다. 작은 generator 입력 약0.52MB와 rawDBC 약4.29MB만 사본이다. font 입력 약35.7MB는 source에서 읽는다. stage 출력/두 번째 font 출력/GL·Xvfb/실제 C++ 빌드의 동시 크기·메모리·시간·60GB fit은 **실제 CI 전 미확정**이다. 같은 filesystem의 기존 감시/admission에 이 경로들과 proof를 합산해야 하며 추정치를 PASS로 바꾸지 않는다.

새 Python `tools/ci/tests/test_generated_assets_557.py`는 **Actions CI 진단/회귀 pending1**이다. 고정 baseline3580 밖에 이 후보가 신규 Python1을 추가한다. 별도 실행-edge에 이 파일과 workflow의 새 manifest 검증을 pending2로 등록했으며 baseline 교체 credit으로 합산하지 않는다. 실행 위치는 위6 selector이고 interpreter는 기존 prepared project venv(Python3.12 계열/pytest/pyray 및 UI 의존), Xvfb/GL은 실제 준비 필요다. 새 Python 의존 설치를 로컬에서 수행하지 않았고 원본 삭제/retirement credit은0이다. 기존 `.py` 파일은 유지·수정이며 신규로 중복 계상하지 않는다.

`$CI_JOB_TEMP/proof/generated-assets-557`의 소비 reports, stage manifest/ready와 전체 roster/SHA, 특히 Pretendard4 원문 출력, compiler argv/ELF·actual SDK/provider/source/index 전후 증거를 기존 bounded proof/upload에 연결해야 한다. fixture가 확인하는 일부 source hash는 전체 tracked source/index/SDK/FD/DSO seal의 대체물이 아니다. command 실패에서도 final source/SDK 검사와 upload, 기존 성공 조건의 owned cleanup을 유지한다.

## 로컬 소스 검사와 미완료 경계

허용된 문법 검사에서 Python/SCons9 source와 embedded child4의 AST를 파싱했다. 프로젝트 import/compile/positive·negative 기능 검사는0이다. AST 대조로 DBC parser/checksum class/function, native generator lookup/bootstrap/call, font `_languages/_char_sets/_glyph_metrics/_write_bmfont` 본문이 동일함을 확인했다. font raster 차이는 두 출력 경로와 반환 metadata이며 GUI class의 차이는 명시 staged missing atlas 거부뿐이다. scoped diff whitespace 검사도 수행했다. C++ compiler/clang-format은 실행하지 않았고 C++ syntax/빌드·6 regression의 실동작은 UNRUN이다.

`release/pack.py`는 이 실제 Integration job에서 호출되지 않으며 staged font를 package 안으로 overlay하는 후속이 **미구현**이다. 그 연결/zip member bytes·page 확인 전에는 package 전달 완료라고 하지 않는다. native runtime DBC role·radar semantics/NAS scope는 변경0이다. 일반 source mode에서 opt-in 없이 기존 source 생성이 계속되는 것은 명시한 호환 계약이며, 이번 CI는 producer와 consumer의 명시 연결을 모두 등록해야 한다. Actions 성공 및 artifact/실소비/봉인 감사 전 #557·#523·#296 완료나 설치/실기 수용을 주장하지 않는다.


## 부모 등록 및 독립 검토 보완

독립 소스 검토에서 named parser 검사 hook이 import/admission의 전체DBC 읽기까지 세고
기존 `DBC: ...` stdout을 JSON으로 바로 해석하는 문제가 발견됐다. hook을 import 뒤에
설치하고 표식 JSON 정확1행을 파싱하며 전체 stdout/stderr를 먼저 보존하도록 수정했다.
open3/실제 stage parent 검사는 그대로이며 수정 재검토에서 추가 P1/P2는 없었다.

부모는 기존 Integration build_release의 source 외부 producer root와 생성 후 consumer root를
명시 연결하고 원래 scons/native_models/camera/exposure/570 회귀 명령을 유지한다.
추가6검사와 C++ leaf·Xvfb, 실패 후 manifest/roster/현재 Pretendard4 bytes 보존을 등록 중이다.
실제 op setup은 venv bin을 GITHUB_PATH에 넣으므로 interpreter는 VIRTUAL_ENV 추측 대신
현재 `$CI296_SOURCE/.venv/bin/python`으로 확인한다. stage/비교/GL 준비의 추가 예상 증가량은
1.5GiB로 사전 검사하지만 실제 peak/fit이나 기능 성공은 아직 확인하지 않았다.

## 202a actual: 외부 font default target 누락

[tests40149194155/job121036927945](https://github.com/bin9208/openpilot-rust/actions/runs/40149194155/job/121036927945)의 기존 scons는 성공했으나 consumer 연결이 fonts/.ready 부재로 실패했다. artifact12582183272=4575380B/SHA `cd17d3851cfd12263ca274e54c52003a3e64dca6676fbc65ca836b6416ddc31b`/46CRC·해제15463160B다. source/index seal은 실제 동일하고 staged DBC103/ready/manifest는 존재하지만 fonts는 빈 directory뿐이었다. 새6consumer/원래 후속 native_models/camera/pytest는 미실행이다.

[SCons 공식 FAQ](https://scons.org/faq.html)의 external target 규칙에 따라 root 밖의 font node는 암묵적 기본root 순회에 포함되지 않는다. staged mode에서만 Default(Dir("#"), font_nodes)를 추가해 원래 root의 모든 target과 외부 font를 함께 기본 선택한다. glyph/provider/출력 알고리즘·명시 CLI target은 바꾸지 않는다. 수정 후 실제 생성/실소비는 다음 Actions 전까지 UNRUN이다.
