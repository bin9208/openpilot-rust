# Planner memory 원본 관측의 frozen 데이터

2026-10-26. `observations.json`은 기존 Python memory wrapper main과 기존 unittest fixture를
실제로 실행한 source425 Actions 관측이다. native 계산으로 expected를 만들지 않았다.
원문에 포함된 Python snippet·경로는 데이터로만 읽으며 실행/import/replay하지 않는다.

## 원본과 추출 근거

- source: `425f29d5ffa4ae0e603ceddf5dce8c605e49bc35`.
- [run40227415620/job121271640924](https://github.com/bin9208/openpilot-rust/actions/runs/40227415620/job/121271640924).
- artifact `12612286621`: 1,605,293 B, SHA256
  `41abeb085169ae55372ab2cb665175dbf1ac4a7f320915f1f7682e53b83aa6c1`.
- 부모 확인: ZIP56 unique/nosymlink/CRC; native8/original1 및 기존 관련 검사·strict Clippy 성공.
- 추출 원본:
  `.analysis/scratch/2026-10-25-523-completion/ci/12612286621.zip`의
  `planner-memory-original/observations.json`.
- 보존 파일: 24,396 B, SHA256
  `f2f8c972e65638a1eac1d3eb5ede9114502a48547a403e5f163a2291cd8a0780`.

작성자는 ZIP 크기/전체 SHA와 선택 member CRC/크기/SHA를 확인하고 바이트 그대로 복사했다.
JSON 정렬·공백·경로·stdout·코드 문자열·마지막 newline을 변경하지 않았다.
`original_stdout_utf8`을 JSON으로 읽은 값은 `original_results`와 같고, outer returncode는0,
stderr는 빈 값이다. 이는 관측 성공이며 failure case의 실제42와 구분한다.

canonical Python은 `/usr/bin/python3.12`, SHA256
`e50d468e8b0adfb05733f5b87b3cff34829c4a8c1aea50c865aa8bdfe4bb150f`,
version proof는 `Python 3.12.3\n`이다. reader는 역사 metadata를 확인하며 현재 interpreter를
찾거나 실행하지 않는다. GitHub source SHA·Python proof·아래 source13의 SHA가 원문에 있다.

## 역사 source13 증명

아래 값은 모두 source425 Git blob을 읽어 SHA256과 일치시켰다. 현재 checkout의 옛
Python 파일 존재를 요구하지 않는다. reader는 원문 전체 SHA로 source13 metadata까지 묶는다.

| source | SHA256 |
| --- | --- |
| `rust/tools/check_plannerd_native_memory.py` | `4f36c97c584d11cccafa53a327893f0d0155f2da0e988ea774d2615396110782` |
| `rust/tools/tests/test_plannerd_native_memory_controller.py` | `67006fee5e1a81e8350a4b514debd6f7e170af3541afa740c797fe0086016fb4` |
| `rust/crates/dev-tools/src/planner_ci_controller.rs` | `ca36ec936776d6ccc8e84694ecb42d860f377655c59a8db9bd44b888a31aadcc` |
| `rust/crates/dev-tools/src/planner_ci_controller/cli.rs` | `b06b4fed1e59cf63c449c7dbec34b221e9cfba0ec74d8bdaa65544127d147680` |
| `rust/crates/dev-tools/src/planner_ci_controller/memory.rs` | `5420fdab0317a8083306075b503208282db42ee66a358bb0c42caf4a637e4881` |
| `rust/crates/dev-tools/src/planner_ci_controller/plan.rs` | `c1dddd235346b8f415c9b131aba7a371cb67ae80d0db5f13a86b450c4d0a8b44` |
| `rust/crates/dev-tools/src/bin/check_plannerd_ci.rs` | `32d2b777eec4ff960ea70195aa0d7b6ac553aee8621658462033ddcf933985d9` |
| `rust/crates/dev-tools/src/qa_process.rs` | `aa9fda1ed867fe6dcf7befb19fc6857fa419bbaa33fcc518264276abef7c6633` |
| `rust/crates/dev-tools/src/seal.rs` | `61acb2852b7b000b84ab179fcf7bdde4e062bb5055e1759ef0c4fa7523837c54` |
| `rust/crates/dev-tools/src/path_resolve.rs` | `952414a69cb4e0eba3a76279ae2d59605b99ef5b9179a39e4dc8d73aa7eb7b8b` |
| `rust/crates/disk-space/src/lib.rs` | `fa920db8d68ffbe2772c8c60a7e793346427b5d9add0a81184d3fa162ea34bd0` |
| `rust/crates/dev-tools/tests/planner_memory_controller.rs` | `7dc897ad2d4ff28dfc06bc411d5ddd5e94f0a63b0ed2d02e0151e0608bdc0c1d` |
| `tools/ci/planner_native_controller_523.sh` | `32cfc4a9b57010c57bf3bdd112c4339ee22ea2ecd5618c39f6b1aa441de5ea63` |

## 5개 경계와 허용한 치환

reader의 frozen 비교5개는 success의 원본 receipt 전체/hash 값 및 wrapper 원문,
failure의42/후속 미실행/PASS 부재, empty의 누락 record 오류,
incomplete의 부분 subset 거부, unsanitized의 controller 이전 거부를 확인한다.
empty/incomplete는 production receipt 검증 함수를 직접 호출한다. incomplete 입력의
두 행 `abi0/owner0`은 source425 기존 unittest의 `CONTROLLER` fixture가 사용한 고정 입력이다.
원문에 없는 기대 receipt를 native로 계산해 덧붙이지 않는다.

`C`는 각 case 이름이고, `T`는 `/tmp/planner-memory-frozen-<고유 suffix>`다.
기존 공백·작은따옴표·한글 suffix와 각 파일 상대 경로는 그대로 둔다.

| 원본의 필드/범위 | 명시적 치환 |
| --- | --- |
| `calls`의 exact `/usr/bin/python3.12` | `T/original memory ' 한글/C/original-python-fixture` (실행하지 않는 입력 경로) |
| `calls`의 exact `/tmp/openpilot-ci.4qveQz/.tmpboaa2r/original memory ' 한글/C` 또는 `/` component 하위 | `T/original memory ' 한글/C`와 같은 상대 suffix |
| success `result.binary_sha256`의 같은 case root 하위 key3개 | 같은 새 case root와 상대 suffix; hash 값은 원문 그대로 |
| `wrappers` 원문에서 shell quote된 같은 case root 접두사2곳 | 아래 두 경로 접두사만 치환; 나머지 bytes 불변 |

wrapper에서는 원래의 `/tmp/openpilot-ci.4qveQz/.tmpboaa2r/original memory '"'"' 한글/C/`를
`T/original memory '"'"' 한글/C/`로 바꾼다. 각 wrapper에서 정확히2회여야 한다.
LD_PRELOAD와 binary 경로 외의 flags/escape/exec/`"$@"`/newline을 다시 생성하지 않는다.
명령 인자의 순서·예산·2700초와 receipt의 scope/status/checks/해시 값은 치환 대상이 아니다.
원본 Python/Rust source를 현재 checkout에서 읽어 expected를 만들지 않는다.

## 퇴역 경계

부모는 source425 actual host run40227447987의 runtime12+native ASan3 성공과 functional/private
artifact 감사를 완료한 뒤 정확한 두 Python 파일의 삭제를 승인했다. 새 guard는
`memory::run`에서 그 두 경로의 존재·symlink 재등장을 출력/leaf 이전에 거부한다.
다른 원본 controller/IPC/source5는 이 퇴역에 포함하지 않는다.
새 frozen5/guard와 삭제 이후 actual ASan·absence qualification은 부모 Actions 대기다.
관측·부모의 삭제 전 성공과 삭제 후 수용을 구분하며 retirement credit은 부모가 관리한다.
