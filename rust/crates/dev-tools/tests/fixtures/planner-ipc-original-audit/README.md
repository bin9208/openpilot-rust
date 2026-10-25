# #523 실제 Planner IPC 원문의 외부 감사 고정값

2026-10-26. 두 JSON은 원본을 새로 실행하거나 input/expected를 생성하는 코드가 아니다.
sourceaa79 실제 host/ARM Actions의 private capture ZIP과 functional receipt ZIP을
독립적으로 감사한 뒤 기존 native reader의 `Audit` schema에 맞춰 metadata만 고정했다.
현재 candidate의 출력/파일 hash에서 원래 기대값을 계산하지 않았다.

| domain | 원래 run | capture / receipt artifact | audit JSON SHA256 |
| --- | --- | --- | --- |
| x86_64 | 40237263524 | 12615278417 /12615802090 | `384bcef1bafc962019557d7b89ef5dac4c85ee2d1683906c1942458b04118e54` |
| aarch64 | 40237263421 | 12615287431 /12614614983 | `7dd3abad75e472a7241d16143b155dd3ad6e8bb826359bd0afedca1360689efc` |

원래 index/manifest/packet/Params/CarParams와 receipt의 bytes는 다시 쓰지 않는다.
각 domain의9-source·4×401event·4×184Params·CarParams4·원본121publications×4/비교차이0을
실제 ZIP/JSON으로 확인했다. ZIP API metadata와 원래 run/source 결합·safe unique path/전체CRC,
직접 rehash 가능한 binary/solver와 receipt에만 남은 binding `.so` SHA의 차이도 기록했다.
상세 원문 SHA/직접 확인 범위는 `docs/naver/python-rust-523-planner-ipc-reader-next.md`를 따른다.

미래 stager는 고정 API ID/run/source/size/digest를 먼저 확인하고 정확한 ZIP bytes/SHA를
받아 `capture.zip`/`receipt.zip`으로 보존한다. original member 경로를 유지한 `capture`/
`receipt` private 디렉터리에 필요한 자료만 놓고, CLI에는 이 감사 JSON의 **고정 SHA**를
외부에서 전달해야 한다. 미검토 JSON에서 새로 계산한 SHA를 신뢰하는 경로는 수용하지 않는다.
reader의 감사 root는 canonical absolute owned 디렉터리여야 하며 domain을 교차 사용하지 않는다.

현재 이 data 계약은 source 검토 단계다. full native IPC feature/CLI 등록과 실제 host/ARM
원본 비교·실패/후봉인/소유 회수 및 Python fixture 퇴역은 후속 actual Actions 수용 대상이다.
