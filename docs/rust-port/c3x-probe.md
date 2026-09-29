# C3X 첫 Rust 실행 검사

추적: [전체 전환 #1](https://github.com/bin9208/openpilot-rust/issues/1),
[IPC/Params/procLog #5](https://github.com/bin9208/openpilot-rust/issues/5).

이 단계는 사용자가 `dev`에서 직접 실행하는 첫 AGNOS ABI/IPC 검사다.
Rust procLog 진단 프로그램은 준비됐지만 기본 manager는 기존 Python
프로세스를 실행한다. 이 검사는 전체 런타임 전환, 프로세스 교체, 실주행,
CPU 절감 또는 모델 실행 검증을 의미하지 않는다.

## 준비

1. 주차·오프로드 상태에서 진행한다. 실행 중인 프로세스를 종료하거나 설정을
   바꾸는 작업은 필요 없다.
2. `bin9208/openpilot-rust`의 검사가 통과한 `dev` 커밋을 적용한다.
3. 같은 **병합 후 dev SHA**의 GitHub Actions **Rust port** 실행에서
   `rust-c3x-static-probe-pending-device-test` 아티팩트를 내려받는다.
   `rust-linux-aarch64-not-device-validated`는 이 검사에 쓰는 파일이 아니다.
4. 압축을 풀어 `openpilot-proclogd`, `SOURCE_COMMIT`, `SHA256SUMS`,
   `aarch64-probe.json`을 기기의 `/data/rust-probe/` 같은 임시 폴더에 옮긴다.
   Actions 다운로드는 실행 권한을 보존하지 않을 수 있다.

아티팩트의 `aarch64-probe.json`은 CI의 에뮬레이터 결과이며 기기 결과가 아니다.
실제 기기 접속·복사·실행은 사용자가 한다.

## 실행

아래 경로는 `/data/openpilot`에 해당 dev 소스가 있고 아티팩트를
`/data/rust-probe`에 풀었다고 가정한다.

```sh
cd /data/rust-probe
sha256sum -c SHA256SUMS
chmod +x openpilot-proclogd
cat SOURCE_COMMIT
git -C /data/openpilot rev-parse HEAD

cd /data/openpilot
PYTHONPATH=/data/openpilot python rust/tools/device_probe.py \
  --binary /data/rust-probe/openpilot-proclogd \
  --report /data/rust-probe/c3x-probe.json
```

두 SHA가 같아야 한다. 스크립트도 아티팩트 SHA와 소스의 일치를 확인한다.
기존 openpilot Python 환경의 cereal/msgq 확장을 사용하므로 별도 Rust나
컴파일러 설치는 필요 없다. `ModuleNotFoundError`가 나면 기존 런타임의 Python
환경에서 실행했는지 확인하고 오류 내용을 전달한다. 검증 조건을 끄지 않는다.

실행은 임시 Params의 바이너리 저장·삭제와 격리 msgq 자체 검사를 한 뒤,
기존 Python 소비자로 0.5 Hz procLog 메시지 10개를 받는다. 보통 약 20초다.
공유메모리는 임의의 `rust-probe-...` 공간을 사용하고 종료 시 정리한다.
실제 차량 Params, 생산 procLog 채널, manager 선택은 바꾸지 않는다.

성공 시 `"result": "pass"`, `"frames": 10`과 바이너리 SHA256이 출력된다.
실패 시 종료 코드는 1이고 가능한 오류 내용이 JSON에 남는다. 결과 파일은
덮어쓰지 않으므로 재실행할 때 `c3x-probe-2.json`처럼 새 파일명을 쓴다.

## 피드백과 다음 단계

다음을 전달한다.

- `c3x-probe.json`의 내용, dev SHA, 실행 중 출력된 오류가 있으면 그 내용
- C3X 및 AGNOS 버전, 정상 종료 여부

JSON에는 프로세스 이름·명령행·차량 설정 원문을 저장하지 않는다. 별도로
`--stdout`이나 `--output-dir`로 얻은 실제 procLog 원문은 공개 이슈에 올리지 않는다.

통과하면 이후 단계에서 기기 ABI와 기존 소비자 호환이 확인된 것으로 기록한다.
프로덕션 교체와 비교 측정은 그 피드백을 받은 후 별도 변경으로 진행한다.
원복은 진단 프로그램이 종료된 상태에서 임시 아티팩트·결과 폴더를 지우면 된다.
manager 설정을 바꾸지 않았으므로 프로세스 선택 원복 작업은 없다.
