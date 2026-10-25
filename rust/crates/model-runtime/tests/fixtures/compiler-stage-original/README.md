# compiler stage source7a928 고정 원본 관측

이 디렉터리는 data-only fixture다. source
`7a928cefe22e3c81bcb941f6ed20eaa80eeabc59`의
[run40236805936/job121300012716](https://github.com/bin9208/openpilot-rust/actions/runs/40236805936/job/121300012716),
[artifact12615238889](https://github.com/bin9208/openpilot-rust/actions/runs/40236805936/artifacts/12615238889)의
`compiler-stage-original/` raw7을 보존한다. ZIP은 **2,069,028B**, SHA256
`8dd0c45777c45b71ffc70012a966f4ef4402fdd746fab72b64066834a2b679ab`다.

유일한 이름 mapping은 **archive `oracle.py` → fixture `oracle.txt`**다. bytes와 SHA는
그대로이며 text를 compile/import/exec하지 않는다. observation 내부의 과거 argv·원래 이름·
source 경로를 다시 쓰지 않는다. raw JSON key 순서·공백·개행도 재직렬화하지 않는다.

| fixture | archive 원래 이름 | bytes | SHA256 |
| --- | --- | ---: | --- |
| `observation.json` | `observation.json` | 19411 | `e128237987e14b4ed20d3514fb25f2f243736815d8e784b0ff99fd4fd5e2d39c` |
| `oracle.txt` | `oracle.py` | 2870 | `f47b29f458a1f143926fd16e00d5861cfd92c97bce0c3bb1d3536b9c4e992d5f` |
| `python.sha256` | `python.sha256` | 86 | `74a2bf284a5116d43270762ab1766fe74421da338fa0889fcd2c188202eae773` |
| `python.version` | `python.version` | 14 | `5b3e43dc38ca01e3f5a7854ba50d4330864b0c1e0f646650b2c78cf072acc366` |
| `request.json` | `request.json` | 4444067 | `e00ad79756d2e51bc63eb87b13b12832177faf7bdab4cda92526e1c044e5bf85` |
| `stderr.bin` | `stderr.bin` | 0 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `stdout.bin` | `stdout.bin` | 3079930 | `67596fc5e6cde23b1df9b08d3d93dc932e17aefa512a4aa2805f9c21731ede32` |

raw 합계는 **7,546,378B**다. 당시 record/stream16MiB·total64MiB 상한 안에 있고,
상한 도달·truncation·stdin 오류·signal 종료가 없다. SHA/길이는 reader가7개 모두 고정한다.

## 실제 원본 결과와 독립 기대값

당시 native-exit-codes는 `model-units=0`, `original-boundaries=0`, `model-clippy=0`이다.
일반 실행은 launch9 + stage7/ignored1, 원본 exact selector는1 passed/7 filtered out,
Clippy는 정상 종료했다. 두 stage 호출 로그의 ELF 경로는
`compiler_schedule-78099b66c3fbfe8c`로 같다. 별도 ELF full-SHA/전체 exec trace를 고정한
자료로 확대하지 않는다.
원본 ignored 실행의4.40s는 관측/검사를 포함한 시간으로 알고리즘 성능 측정치가 아니다.

독립 expected는 원본 프로세스가 쓴 `stdout.bin`뿐이며 입력은 `request.json`이다.
request/result는 각각73개다. 성공68·거부5이고, 거부 index는 **66/67/68/69/72**다.
index는0부터 센다. index71은65,536개 named empty stage를 수용하여65,536개 entrypoint를 반환하며,
index72의65,537개 stage는 원래대로 거부한다. 빈 stage/no-stage, 반복 identity,
pruning subset, expanded prefix, Unicode, invalid name/partition/overrun을 유지한다.
native 출력으로 expected를 재생성하거나 거부 case를 성공으로 바꾸지 않는다.

원본 stdout은 `schema.py`와 `offline_schedule.py` 두 SHA를 보고한다. 각각
`720abcc396852a395689c4d37f9487822f951166a36b5efe8586dce5f656c6a9`,
`bc1b7b572c8dd2f84f3ea0ca38b1744e4c7120155c33f3dabb633eabaf7f0144`다.
원본 subprocess의 rc0·signal null·stderr0과 source SHA binding이 관측에 남아 있다.
argv는 `/usr/bin/python3.12 -I -c <oracle 원문> <원래 root 인자>`다.
root 인자의 `rust/crates/model-runtime/../../..` 표기를 그대로 보존한다.

## source/Python 및 외부 봉인 감사

부모가 받은 완성 ZIP 한 개를 중복 다운로드 없이 inert 감사했다. 47 safe/unique member,
symlink/특수 파일 없음, CRC 정상, expanded **14,468,053B**다. 각 member의 CRC와 전체 SHA를
읽었으며 project/oracle/CLI는 실행하지 않았다. raw7만 지정 이름으로 새 파일에 복사했다.
archive mode는7개 모두0100600이다. ZIP에 directory entry가 없어 directory mode를
별도 ZIP 증거로 확대하지 않는다.

observation의13개 witness가 실행 전후 완전히 같다. Python 원본2개, native core/test/
manifest/lock6개, Python executable, job owner, SOURCE_COMMIT과 Python proof2개다.
선택 source8개의 실제 historical Git blob bytes/SHA가 source7a928와 모두 일치한다.
source root는 `.diag296-source-40236805936-1.iO4Nek/source`, job은
`/tmp/openpilot-ci.oTOw0T`, owner 원문은 개행 없는 `40236805936:1:native-units`다.

canonical Python은 `/usr/bin/python3.12`, 8,020,928B, SHA256
`e50d468e8b0adfb05733f5b87b3cff34829c4a8c1aea50c865aa8bdfe4bb150f`다.
같은 admission proof는 `Python 3.12.3\n`과 SHA/정확한 path를 기록한다. raw proof 두 개는
archive 최상위 stage-oracle proof와도 같다. mode의 executable bit를 포함해 file metadata/
dev/inode/uid/gid/nlink/mtime/ctime의 전후 결합을 고정한다. 현재 Python 설치는 열지 않는다.

archive의 source/index, index-stage 및 SDK file/program/roster before/after가 byte 동일하다.
index와 index-stage는 historical Git tree의 **19,571개** tracked entry와 정확히 같다.
source tar 봉인값은 `c88e97d5e9590bb627009091436544c85aa02716c1647917f9a304747c86780d`이며
전후 같다. source preflight0, outcomes의 source/SDK 전후 success·SDK install0도 확인했다.
이 외부 봉인들은 전체 ZIP 감사 근거이며 raw7 fixture에 별도 중복하지 않는다.
현재 source tree/SDK 전체를 다시 실행·설치·다운로드해 검증한 것이 아니며, 전체 Python
stdlib/import/DSO closure·모델·GPU·F02 오류 우선순위 수용으로 확대하지 않는다.

## frozen reader와 새 GHA

`compiler_schedule.rs::frozen_original_pipeline_boundaries_match_native`가 원래73건 생성과
native 비교 본문을 byte 그대로 유지한다. 입력은 raw SHA를 고정한 뒤 JSON Value로
구조 비교해 `serde_json/preserve_order`의 object key 순서 차이를 허용한다. array 순서·값·
숫자/boolean 종류를 바꾸는 정규화는 하지 않는다.

reader는7개 SHA/길이, historical source/root/Python·13 witness·rc0·상한, 원래5개 오류와
65,536/65,537 경계를 확인한다. 원본 Python subprocess/현재 source AST 조회/ignored gate는
없다. 같은 `native-skip-miri` 조건에서 일반stage7 + frozen1 = **8 tests/ignored0**다.
새 reader의 실제8개 GHA는 아직 UNRUN이다. 부모는 옛 ignored selector와 Python version/
proof 준비를 stage 경계에서 제거하고 새 normal8 수용 후에만 원장 edge를 닫는다.

F02 production v2 patch와 오류 순서 pending defect는 별도다. 자세한 연결 상태는
`docs/naver/python-rust-523-compiler-native-emission.md`에 기록한다.


## 2026-10-26 stage73의 실제 native 수용

source `a62109fa706012b91b24206e7d8c875653746aec`의
[run40241338985/job121313583472](https://github.com/bin9208/openpilot-rust/actions/runs/40241338985/job/121313583472)은
launch9·stage8/0ignored, preserve_order union1, 원본launch4411 corpus1과 strict Clippy를 모두
성공했다. 새 Python subprocess/현재 원본 AST 조회가 없는 고정 reader다. 원본73 raw 입력·
기대값·SHA와 과거13 witness를 유지했으며 native 결과로 expected를 만들지 않았다.
artifact12616208518은1,583,513B/SHA256
`ece5ed6e6be36855b4180fe3400b97bbf9aed2d9bb55eb0a72a5b42c0018acc4`,
40 safe unique regular member/전체 CRC, expanded6,929,918B다. command4개 exit0,
source/index/index-stage와 whole SDK file/program/roster 전후 동일, SDK install0,
업로드 뒤 source/owned temp cleanup까지 실제 성공했다.

추가 실행 원장은 **162개 =32교체/126대기/1기존native/2비실행/1제거**다.
기준3580은 **299교체/3부분/3278대기**, production offline emitter와 전체 F02 수용은 그대로
미완료다. 이 좁은 비교 실행경계의 제거만 인정하며 source-file baseline credit은0이다.
