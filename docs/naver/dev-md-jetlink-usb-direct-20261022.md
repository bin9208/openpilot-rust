# dev-md Jetlink USB 연결과 실제 모델 선택 (#487)

## 사용자 요청과 원인

[이슈 #487](https://github.com/bin9208/openpilot-rust/issues/487). USB-C 양단으로 폰을 연결했을 때 사진의 호스트 IP 미확인보다 먼저 `USB gadget 'g1' already holds the device controller (a600000.dwc3)`가 발생한다. 기존 #402는 다른 gadget 해제를 기다릴 뿐 기본 ADB 소유권을 전환하지 않았다. AGNOS 원본 `set_adb.sh`는 `g1`, VID:PID 04d8:1234, comma.ai 제조사, NCM+ADB 구성의 ncm.0/ffs.adb를 생성하고 같은 UDC를 바인딩한다. 확인 원본: [ajouatom/agnos-builder 61073bdd](https://github.com/ajouatom/agnos-builder/blob/61073bdd9d92c3695aa54eef4d2c33707a82687f/userspace/root/usr/comma/set_adb.sh).

사용자는 별도 Shadow 단계 없이 외부 모델 선택을 실제 모델에 반영하도록 요청했다. Xiaoge의 GPU+CPU 비교 모드와 별개인 Jetlink 전환 문제다.

## 구현 계획과 검증 경계

1. 기본 AGNOS g1의 속성과 함수 연결을 확인하고, root 소유의 복원 기록을 남긴 뒤 ADB watcher/서비스를 정지하고 g1 UDC만 해제한다. 알 수 없는 gadget과 eGPU는 기존대로 거부한다. 저장된 AdbEnabled는 바꾸지 않는다.
2. Jetlink 해제/준비 실패 시 소유권과 현재 ADB 설정을 확인해 복원한다. 재시작 때 기록을 이어받는다. 전체 gadget 삭제, kernel function rmdir, 임의 장치 unbind를 하지 않는다.
3. 사용자 Shadow 옵션과 Shadow→활성 모드 edge를 제거하고 이전 1/2 값을 호환한다. 실제 전환은 준비된 동일 모델의 정차·제어 해제 시점이다. 현재 프레임 계약, deadline, recurrent state, 손실 latch와 eGPU 배타성을 유지한다.
4. #460 봉인 archive mode 복원 결함을 일반 producer에서 수정해 새 SHA 발행에 별도 복구 driver가 필요 없게 한다.
5. 로컬은 문법·포맷·diff만 확인한다. 집중 회귀/필수 5개 검사 및 정확한 병합 SHA의 실제 AGNOS 빌드·seal·공개 설치 검증은 기존 Actions 경로로 실행한다. dev/main 통합과 실기 접속은 하지 않는다. 사용자 연결·주행 확인 전 이슈는 열어 둔다.

구현/PR/Actions/최종 runtime SHA와 공개 설치 근거는 진행에 따라 아래에 기록한다. 합성 파일 fixture와 QEMU 검증은 실물 USB 연결·폰 속도·주행 검증을 대신하지 않는다.

## 실행 전 독립 검토

- GPT-6.1 Sol USB 검토에서 FunctionFS close가 g1을 이미 unbind한 뒤 다시 빈 UDC를 쓰면 AGNOS 커널이 ENODEV를 반환할 수 있음을 확인해, 실제 보유값이 있을 때만 해제한다. [고정 커널 FunctionFS close](https://github.com/commaai/agnos-kernel-sdm845/blob/c368754c26c7b9659de187addc6cccedc6cfb0a0/drivers/usb/gadget/function/f_fs.c#L4461), [configfs unregister](https://github.com/commaai/agnos-kernel-sdm845/blob/c368754c26c7b9659de187addc6cccedc6cfb0a0/drivers/usb/gadget/configfs.c#L283).
- g1이 미생성/해제 상태라도 ADB watcher가 나중에 소유권을 빼앗지 않도록 기록하고 정지한다. 복원 때 watcher를 먼저 시작하고 원본 service로 현재 AdbEnabled를 재조정한다. 이중 해제, 미생성/해제된 g1, owner 재시작, 설정 변경, 외부 소유자, root 기록 검증을 파일/서비스 fixture에 추가했다. 아직 Actions 실행 전이며 통과를 주장하지 않는다.
- 일반 archive 복원기는 data_filter의 경로·link 검증을 유지하고 일반 파일 fd로만 정확한 권한을 복원한다. 0664/0444/0555/0644/0755, symlink, escaping/기존 경로 link, 특수 권한·타입, 설치 cwd 검증을 CI에 연결했다. 원래 Rust publisher의 archive/identity gate는 유지한다.

## 빠른 Actions 결과와 생성물

- 최초 소스 `64a56f885ed607cb1d37305943c2da903985a04f`: [39752504852](https://github.com/bin9208/openpilot-rust/actions/runs/39752504852)에서 archive 복원 13개 회귀가 통과했으나 신규 웹 빌드 디스크 예약의 검사 개수 기대값이 기존 26개에 머물러 실패했다. 기존 179개 baseline 예약은 보존하며 웹 512MiB 예약을 별도로 검증하도록 수정했다.
- 빠른 검사 `084b8096ef1b4089278f4fca91835d559e2d5a51`: [39753010962 / fast checks](https://github.com/bin9208/openpilot-rust/actions/runs/39753010962/job/119738045677) 성공. Python 모델/계약/전환, Android core/runtime, 웹 선택/레거시 값 표시/USB 진단과 일반 archive 복원 검사를 수행했다. 아직 Rust 실행/ARM/AGNOS 검증은 아니다.
- 같은 Actions 실행의 artifact `12413619026`에서 생성 patch를 받았다. 생성된 `tools.js`와 asset manifest 두 파일만 바뀌었으며 manifest의 tools SHA-256은 `a52d66fb22e8777d483f7e918abce2b0db61fe77a6dc0a3f2c2288f21a4f264e`다. 로컬 빌드는 수행하지 않았다. PR와 보호 브랜치 검사에서는 동일 생성물 재현 후 차이가 없어야 통과한다.

## 네이티브 실행 결과와 해제 경로 후속 (#489)

[PR #488](https://github.com/bin9208/openpilot-rust/pull/488) 소스 `6133c589c71d765e9a27a5961fc26c8dce099b67`의 [네이티브 Jetlink 검사](https://github.com/bin9208/openpilot-rust/actions/runs/39753576074/job/119749134075)가 통과했다. gadget 10개, 전환 4개, 실제 RPC runtime 7개를 포함하며 승인 기록 없이 현재 외부 결과를 실제 선택하고, 잘못된 실행 파일은 거부하는 동작을 확인했다. 이는 파일/프로세스 fixture이며 물리 USB 검증이 아니다.

추가 검토에서 [#489](https://github.com/bin9208/openpilot-rust/issues/489)의 기존 FunctionFS `Session::teardown`도 이미 해제된 UDC를 다시 쓰는 것을 확인했다. watchdog/백엔드가 먼저 해제하면 owner 정리가 실패해 ADB 복원까지 막을 수 있다. 완료된 ep0 소유자는 재호출 시 새 세션의 UDC를 건드리지 않으며, 아직 정리할 ep0가 있을 때만 현재 UDC를 확인하고 해제한다. 실제 오류는 기록을 유지해 다시 정리할 수 있게 한다.

Actions는 고정된 이전 `6133c589c`의 ffs.rs SHA-256 `c8e32605ed535bed7523bf55ecbb840c7ac680e981364a7a15051d6b7e4ad930`를 검증한 뒤, 새 재바인딩/빈 UDC 회귀 두 개가 이전 소스에서 실제 실패함을 확인한다. 반드시 현재 소스 원본 바이트를 복원하고 Git diff가 없는지 확인한 다음 새 소스의 전체 FunctionFS·기존 긍정 검사를 실행한다. 원본 실패를 새 후보의 성공으로 오인하지 않도록 별도 RED artifact에 소스 식별자를 기록한다. 로컬 기능 실행은 하지 않는다. 이 추가 수정으로 전체 PR 검사를 새 소스에서 다시 실행한다.
