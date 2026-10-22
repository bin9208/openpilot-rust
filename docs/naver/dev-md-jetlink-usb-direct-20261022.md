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
