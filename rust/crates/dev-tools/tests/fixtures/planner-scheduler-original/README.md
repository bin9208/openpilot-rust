# #523 원본 ASCII float parse 단계의 고정 관측35

`observations.json`은 실제 원본 `ModelScheduler.target`의 ASCII decode→float→finite check→return
AST 단계에서 얻은 결과다. ZIP member를 **바이트 그대로 복사**했다. native 계산이나 Rust parser의
출력에서 expected를 생성하지 않았고 JSON을 재직렬화하거나 수정하지 않았다.

| 원본 증거 | 값 |
| --- | --- |
| source commit | `aa79c7cd21a4b9bbb6bfb4c68ded9e857511f1cf` |
| Actions run/job | 40237263418 / 121303387188 |
| artifact ID | 12615269988 |
| 보존 ZIP | 1,590,262 B, SHA256 `15cc43a62f8d68718f6678aad96ad9b77a4ef221946a19f667192b5e2135f42e` |
| 실제 member | `planner-scheduler-original-parse/observations.json` |
| 원문 JSON | 9160 B, SHA256 `afe5ea47497bf9ea1af07231400055f348c3c8e555020c1f747c97eb51168acd` |
| actual original source | `rust/tools/plannerd_ipc.py`, SHA256 `a4aec1991a6751af60f5616698e75e0861b0a1f8b1c478e5382889681598c8dc` |
| 원래 ModelScheduler class | SHA256 `818e06d38749c45bd18692538dfc6f2815c4052a820bc3d9449f151e56b7f124` |
| 정확한 추출 stage AST | SHA256 `dcaa9f8ca1c73b6a3e3e32f46edc8c1dc2b614a1268144d53dc5807698e0d63f` |
| 원본 adapter test source | SHA256 `bc9e2beb046308aab5c017950e90fabc627bf11a175bdda0799341be8349f32c` |
| 원본 Python | `/usr/bin/python3`, 3.12.3 (JSON의 전체 version 문자열 보존) |
| 원본 Python 실행물 | SHA256 `e50d468e8b0adfb05733f5b87b3cff34829c4a8c1aea50c865aa8bdfe4bb150f` |

로컬 cached ZIP은 `.analysis/scratch/2026-10-25-523-completion/ci/12615269988.zip`이다.
이 세션에서는 보존 ZIP SHA와 선택 member의 CRC/bytes/SHA·JSON 데이터만 읽었다.
전체45 safe-member/CRC와6956085 B expanded, 여섯 기존 gate exit0 및 source/SDK/install0은
부모가 제공한 감사 결과다. 이를 새 미등록 Rust client의 실행 성공으로 바꾸어 기록하지 않는다.

35개 중 finite 결과17개는 원래 f64 big-endian bits를, 나머지18개는 원래 exception type/message를
기록한다. 거부는 RuntimeError7(nonfinite/overflow), ValueError9, UnicodeDecodeError2다.
공백/tab/VT(`0x0B`)/FF(`0x0C`)/CR, underscore, 지수, signed zero, overflow/underflow,
invalid/nonASCII를 포함한다. 입력은 실제 원본 단계와 같은 bytearray였으며 line_hex로 보존했다.

Rust parser는 ASCII 여부를 먼저 검사한다. 바깥 공백은 명시적인 `09/0A/0B/0C/0D/20`만
제거하고, sign/decimal digits/dot/exponent와 digit 사이의 단일 underscore를 직접 검사한다.
Rust `trim_ascii`/`is_ascii_whitespace`나 f64 parser의 허용 문법을 Python과 같다고 가정하지 않는다.
검사 후 underscore를 제거해 수치 변환만 std f64 parser에 맡기고 finite 결과만 반환한다.
부호가 있는 대소문자 nan/inf/infinity와 수치 overflow는 NonFinite로 거부한다.

`planner_scheduler_client`의 새 순수 회귀는 이 원문의35개 결과 bits/오류 종류를 비교한다.
NonFinite 진단은 원본 RuntimeError 메시지를 유지한다. InvalidFloat/NonAscii의 Rust 진단 문자열은
Python repr/codec 예외 메시지의 바이트 복제까지 주장하지 않는다. 원래 메시지는 JSON에 그대로 남긴다.
별도 stdio 회귀는 finite17개 원문을 한 shell peer session에서 그대로 응답해 public client가
VT/underscore 등을 실제 parser 경로에서 받는지 검사한다. shell fixture는 원본 관측을 생성하지 않는다.

이 원문은 **parse 단계 관측**이다. framing/256-byte limit/복수 행/latch/deadline/close/reap 또는
전체 Python client의35회 actual session 관측이 아니다. 그 의미는 기존13개 Rust 회귀에서
계속 검사하며 real-native4×40=160 검사의 입력·expected bits·호출 수를 변경하지 않았다.
새 Rust target은 일반15/ignored0으로 준비했으며 실행·Clippy 검증은 부모 GHA에서 해야 한다.
원래 Python adapter/oracle 삭제와 전체 native IPC 수용/retirement credit의 근거로 단독 사용하지 않는다.
