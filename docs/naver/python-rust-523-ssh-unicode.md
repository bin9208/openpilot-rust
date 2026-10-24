# #523 SSH alias Unicode15 lower 회귀 수정

2026-10-24 KST. **실제 CLI RED 확보 후 구현했으며 새 helper의 정상 수용은 UNRUN이다.**
로컬에서는 고정 원문 data 변환, Rust syntax/format 및 diff 검토만 수행했다.
원본/후보/scalar 실행, Rust build/test/Clippy, 실제 auth/SSH/기기 접근은 수행하지 않았다.
전체 auth/SSH 또는 #296 전환 완료를 뜻하지 않는다.

## 실제 실패와 수정 범위

[Focus run40092771379/job120868686487](https://github.com/bin9208/openpilot-rust/actions/runs/40092771379/job/120868686487),
source [fb1228f02e0546b6fe7a9d8d0eec098e650a86bd](https://github.com/bin9208/openpilot-rust/commit/fb1228f02e0546b6fe7a9d8d0eec098e650a86bd)에서
`ssh_cli_original_aliases_prefix_unicode_duplicate_order_and_exec`가 `criteria.kind=exact` 비교에 실패했다.
ignored `focused-fb1228.log:1466`의 실제 assertion JSON도 읽어 확인했다.

- 사례 `unicode16-rams-horn-alias-false-match`: backend alias U+A7CB, query U+0264.
- 실제 CPython3.12.15 원본: `exit_code=1`, `exec=null`, lookup 실패 stderr.
- native: 합성 `0000000000000001`을 선택하고 `comma@comma-0000000000000001`에 대한 잘못된 exec plan을 생성했다.
  harness 관찰값이며 실제 SSH 세션을 실행했다는 뜻은 아니다.
- 부모가 통지한 같은 RED source의 parser13/strict Clippy 성공은 이 새 helper의 검증 결과가 아니다.
  비교는 첫 실패에서 멈췄으므로 뒤의 모든 authored 사례가 개별 실패했다고 추론하지 않는다.

`developer_ssh.rs::normalized()`는 U+0020 제거 후 Rust std의 Unicode17 lower를 호출했다.
CPython3.12.15의 Unicode15에서 미배정인 새 문자를 Rust17이 소문자로 변환해 alias를 잘못 선택했다.
단순 assigned guard로는 Σ 문맥의 버전 차이까지 고칠 수 없다.
이미15에 배정된 U+0295는 17에서 `Cased`가, U+1171E는 `Case_Ignorable`이 제거됐다.

수정은 `developer_ssh/unicode_lower15.rs`의 private `pub(super) fn lower15(&str) -> String`과
부모 모듈의 private `mod` 선언 및 normalized 한 줄이다.
U+0020을 먼저 제거하고 helper를 호출한다. 기존 alias/query 호출 위치, substring 비교, direct ID,
중복 ID 순서, API/출력/exec 정책은 그대로다. 공개 API·crate·Python dependency를 추가하지 않았다.

## 고정 data와 문맥 규칙

생성 입력은 Unicode 공식15 원문 세 파일이며 URL·byte 길이·전체 SHA는 sibling
`unicode_lower15.provenance.json`에 보존했다. 기본 mapping은 입력 유지다.

| 원문 | SHA256 |
| --- | --- |
| [UnicodeData15](https://www.unicode.org/Public/15.0.0/ucd/UnicodeData.txt) | `806e9aed65037197f1ec85e12be6e8cd870fc5608b4de0fffd990f689f376a73` |
| [SpecialCasing15](https://www.unicode.org/Public/15.0.0/ucd/SpecialCasing.txt) | `78b29c64b5840d25c11a9f31b665ee551b8a499eca6c70d770fcad7dd710f494` |
| [DerivedCoreProperties15](https://www.unicode.org/Public/15.0.0/ucd/DerivedCoreProperties.txt) | `d367290bc0867e6b484c68370530bdd1a08b6b32404601b8c7accaf83e05628d` |

UnicodeData의 simple lower에 unconditional SpecialCasing을 우선하는 정수 data 변환으로 identity를 제외한1433개를 얻었다.
단일 scalar mapping1432개와 U+0130 → U+0069 U+0307 확장1개를 고정했다.
`Cased`4526 scalar를157 range, `Case_Ignorable`2707 scalar를437 range로 병합했다. range는 양끝을 포함한다.
앞선 source-table 조사에서는 이 data가
[CPython 생성 table](https://github.com/python/cpython/blob/e848e4b09ca4bc67c0041c9ce7526dae471d4c36/Objects/unicodetype_db.h)의
mapping/두 속성과 전체 항목이 같음을 정수 table로 확인했다. lower 함수를 실행한 증거는 아니다.

Σ는 [CPython의 문맥 분기](https://github.com/python/cpython/blob/e848e4b09ca4bc67c0041c9ce7526dae471d4c36/Objects/unicodeobject.c#L9534-L9571)와 같은 순서로 처리한다.
원래 입력의 양쪽에서 먼저 Case_Ignorable을 건너뛴다. 왼쪽 첫 비무시 문자가 Cased이고,
오른쪽 첫 비무시 문자가 Cased가 아니거나 끝이면ς, 그 외에는σ로 바꾼다.
두 속성을 가진 U+0345도 먼저 건너뛴다. 변환된 출력은 문맥으로 사용하지 않는다.
locale 조건, casefold, Unicode normalization, 허용차나 mask는 추가하지 않는다.

## pin과 라이선스

구현 시점 helper 전체는49834 bytes이며 SHA256은
`f0edde8b8443486b6940dc8f22e7cea65675b8230a46baec11c8ce0c2bab6b40`。
생성 data block SHA도 provenance에 별도로 기록했으며 full source pin을 대체하지 않는다.
원래 UCD raw bytes를 보관하고 SHA를 확인한 뒤 source data를 변환했다.
1432/157/437 선언 내용은 ignored 생성물과 byte가 같음을 확인했다.
빌드 binary는 만들지 않았으며 새로운 binary 전체 SHA는 후속 CI에서 기록한다.

`unicode_lower15.LICENSE-UNICODE-V3.txt`는 [공식 license.txt](https://www.unicode.org/license.txt)의
**Unicode License V3** raw snapshot을 그대로 보관했다. 1995 bytes이며 SHA256은
`e7a93b009565cfce55919a381437ac4db883e9da2126fa28b91d12732bc53d96`이다.
원문의 ©2022 Unicode notice를 module에도 남겼다. 2026-10-24에 얻은 현재 license이며
2022 최초 배포의 license 판본이었다고 소급 주장하지 않는다. 기존 프로젝트의 MIT notice도 유지한다.
CPython/Rust 생성 table 본체를 vendoring하지 않았으며 고정 UCD15에서 만든 data를 쓴다.

## 검증 경계와 부모 인계

고정 경계 native unit6개를 추가했다. 새 scalar identity, U+0130 full lower 확장과 locale/casefold 비적용,
Σ의 Cased 양방향, Unicode15 무시 문자 양방향, U+0345 skip 우선순위, ASCII space 제거 순서를 다룬다.
기대값은 고정 원문에서 설계한 literal이며 native helper로 생성하지 않았다. 모두 실행 UNRUN이다.

부모가 작성하는 sibling `unicode_lower15_original.rs`를 다음 선언으로 연결했다.

```rust
#[cfg(test)]
#[path = "unicode_lower15_original.rs"]
mod original_tests;
```

이 원본 테스트와 `developer_cli_original.rs`, workflow는 부모가 소유한다.
부모가 예정한1112064 scalar stream 길이·bytes SHA와154 context 독립 원본 비교, 추가 exact CLI2개,
strict Clippy의 성공은 아직 확인하지 않았다. 선택 수·context 수·seal을 유지해 CI에서 판정한다.
helper 전체, provenance, license, 부모 원본 테스트를 source 전후 seal에 넣는 작업도 부모가 담당한다.
로컬 syntax/format 성공을 green, 원본 일치나 실기 수용으로 대체하지 않는다.


## 부모의 추가 독립 원본 gate

새 private helper는 원본 CPython3.12.15의 `.lower()`를 직접 실행한 결과와 대조한다.
전체1112064 Unicode scalar의 lower 출력 UTF-8 bytes를 길이 접두사와 함께 SHA256으로
비교하며 Σ 문맥154개는 전체 문자열로 비교한다. locale·casefold·정규화나 선택 mask는 없다.
원본 프로세스는 준비한 절대 interpreter, 명시적 SDK LD_LIBRARY_PATH, `-I -B -S`, 새 pycache
prefix와20초 timeout을 사용한다. mapping/helper/test/provenance/license5개와 실제 test ELF를
열어 보유한 상태에서 전체 비교 전후 파일 identity/내용 SHA를 확인하고 증거 JSON을 출력한다.

실제 CLI는 기존20개와 새 문자2개, 문맥/공백 순서4개를 합쳐 alias26개다.
기존 fb1228 parser13은 production parser/scanner pin과 artifact로 재사용한다.
영향 있는 SSH27개, 새 native unit6개와 위 독립 원본1개, strict Clippy는 fresh Actions 범위다.
이 추가 검사는 모두 **UNRUN**이다. 성공을 확인한 후에만 #532 수용 상태를 갱신한다.
