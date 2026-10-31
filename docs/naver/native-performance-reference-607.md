# #607 원본 전체 상태의 환경별 비교 증거

[실행40358677927](https://github.com/bin9208/openpilot-rust/actions/runs/40358677927)은
source `fb07c98294269e0144d56aeeea02f0cdfa85980e`에서 x64 job121697923570과
ARM job121697923802 모두 성공했다. 모든 job은 등록된 bin-actions에서 실행했다.
비교 원본은 `e8e152dbcfd9574a1791ed12026d1880e87689b3`의 실제 Python이며
파일 hash는 아래와 같다.

- `radar_lead_filter.py`: `55ab0ad700459c3c9b02fcfb8f9dcae54fd8626aee515d4b3e473422c70d5528`
- `hyundai/radar_bosch.py`: `8c4cf75bcf371980cedc77432d22950c5ca490eff8a300a97685f2f84419a2f6`

양쪽 모두 같은 환경의 실제 원본과 candidate의2,544회 결과/debug/전체 filter state를
float bit까지 일치시켰으며 strict Clippy도 통과했다. 허용 오차·비교 필드 축소·candidate로
기대값 재생성을 사용하지 않았다. Python3.12.14, glibc2.39 환경을 기록했다.

| 환경 | 같은 환경 원본 비교 | 원본 대 과거 frozen fixture | 별도 historical Rust 검사 |
| --- | --- | --- | --- |
| x86_64 | 성공 | 8행/8필드 bit 차이 | 실패; 진단 artifact에 보존 |
| aarch64 QEMU/PRoot | 성공 | 차이0 | 성공 |

x64 첫 차이는 `state/190/states/2/lead_filter/response_weight`:
과거 `0.0016548305925441129` (`3f5b1cdcd1a01bdb`), 실제 원본과 Rust
`0.0016548305925441127` (`3f5b1cdcd1a01bda`)다. 나머지도 같은 response_weight 필드다.
이 증거는 과거 고정 수치가 해당 환경 사이에 bit 단위로 이식되지 않음을 보여준다.
libm만을 차이의 원인으로 확정하지 않는다.

x64 artifact12709949965(5,286bytes), ARM artifact12717802440(6,281bytes)에
환경 JSON·원본 비교·과거 비교 로그와 자원 기록이 있다. `/usr/bin/time`의 compile 포함
단일 명령 wall time은 x64 2:30.08, ARM 46:21.89, maximum single-child RSS는
각789,096/950,140KiB다. aggregate process-tree RAM peak, 차량 시간 또는 성능 개선 실측이 아니다.

ARM host는 x86_64, guest aarch64, execution_mode=qemu-user이며 물리 ARM CPU 검증이 아니다.
rootfs SHA256 `a91d5a93010193712d346d761372b7c9db6df093893161c64ca107f05914f2`,
PRoot SHA256 `db185300a9daabbce86aca7632d8c7fdc4249f6f153171954013b70c5f491ec0`를 기록했다.

focused 성공은 전체 workspace·필수 PR·병합 후·차량 성능 수용의 통과가 아니다.
일반 회귀에 같은 환경 원본 검사를 필수로 연결하고 역사 비교를 별도로 보존하는 후속 작업이
남아 있으며, dev-md runtime을 dev/main으로 통합하지 않는다. #598/#601은 계속 열어 둔다.
