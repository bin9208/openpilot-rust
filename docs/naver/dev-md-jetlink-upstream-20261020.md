# dev-md Jetlink USB 전원·재부팅 수정 (2026-10-20, #402)

사용자가 지정한 [491cf1e](https://github.com/zoompilot/jetlink/commit/491cf1ef8d6786ed51cf995e18fc8721389066c6)는 C3X가 USB-C 상대 기기에서 입력 전류를 끌어올 때 발생한 PMIC UVLO 전원 차단을 다룬다. 부팅부터 제한하고 모드 변경·이전 owner 복원까지 처리한 후속 e3dbebc를 포함해 d24328d의 comma 전원 정책을 Rust로 옮긴다. 사용자 사진 이전 재시작의 동일 원인은 아직 확인되지 않았다. 차량 설치·시험은 사용자가 수행한다.

## 구현과 범위

- 부팅 shell에서 설치기 실행 전 USB_ICL의 고정 경로를 원본 순서로 기록한다. 아직 새 native launcher가 없는 첫 업그레이드도 보호한다. 새 launcher bootstrap에서도 manager 시작 전에 USB_ICL을 0으로 강제한다. Jetlink가 필요한 동안 유지한다. Off 전환 시 케이블이 붙어 있으면 복원을 보류하고 `current_pr=none`일 때 복원한다.
- 원본의 PD 역할 전환(3회, 2초 간격), 이후 hard reset 1회, source hold/release, UDC 재시작(최소 10초), unplug 처리를 native 상태 기계와 고정 경로 helper로 구현한다. eGPU가 감지되면 포트 제어와 Jetlink 소유를 해제한다.
- VM dirty limits와 socket maxima는 owner당 한 번 적용하고 원래 값을 기록한다. 기존 값이 bytes=0이면 ratio를 기록해 올바르게 복원한다. UDC compliance 두 항목과 FunctionFS debug log 제한은 각각 시도한다. 활성 상태에서 owner 재시작이면 전류 제한·tuning을 유지한다.
- VM 기록 파일의 root 소유·일반 파일·쓰기 권한·symlink 여부를 검사하며 기록에 실패하면 값을 변경하지 않는다. 이는 원본 복원 정책을 native 권한 경계에 맞춘 차이이다.
- 다른 gadget(예: ADB)이 컨트롤러를 소유하면 이름을 오류에 남기고 2초 뒤 재시도한다. 강제 unbind하지 않는다. 일반 setup 실패는 60초 간격이다. 실패 여부를 영구 latch하던 경로도 복구한다.

이 작업은 지정된 comma 전원 수정과 인접 USB 정책의 포트이다. Android/iOS/macOS 앱 소스를 차량 안에 복제하지 않는다. 기존 native 모델/RPC·v2/v3 경로는 유지한다. 원본 RPS core6 설정은 이 fork의 controlsd/selfdrived 배치와 충돌하므로 가져오지 않는다. onroad 연결/준비를 허용한 사용자 지시와 기존 실측 CPU 배치를 유지한다. 선택적 iOS 폰 충전은 원본 기본값인 Off로 연결한다. 신규 설정을 추가하지 않아 사용자 설정 가이드/Wiki 변경 대상이 아니다. 파일별 provenance와 차이는 `rust/upstream/jetlink.json`에 기록한다.

## CPU 부하 제약

500ms owner 주기는 유지한다. 전류 제한은 같은 wanted 상태에서 성공·실패 모두 재호출하지 않는다. VM/UDC tuning도 owner/설정 전환 시에만 실행한다. 포트 역할·UDC 파일은 2Hz로 읽고 변경 시에만 로그를 기록한다. 역할 전환과 UDC 재시작은 원본의 횟수·간격을 지킨다. 모델 프레임마다 helper를 실행하지 않으며 모델/제어 deadline, 우선순위, 코어 배치, pose 유효성 기준을 변경하지 않는다. 장치 CPU 감소·재부팅 해결 여부는 실기 시험 전 확정하지 않는다.

## 검증과 발행

사용자의 2026-10-20 지시: 로컬에서는 문법·포맷·diff만 확인한다. 기능 검증, ARM/AGNOS 빌드, seal, 설치 검증과 발행은 GitHub-hosted Actions에서 수행한다. #296의 도구 전환 작업은 별도다.

- 빠른 검사: `Jetlink native fast checks`. 전류 정책 회귀, helper/tuning 파일 fixture, 변경된 launcher 컴파일, 실제 upstream `Port`와 `Owner.steer_draw`를 hash 검증 후 실행하는 상태/명령 differential oracle. 결함 재현 CI [39276328626](https://github.com/bin9208/openpilot-rust/actions/runs/39276328626)는 구현 전 6개 실패, 첫 전류 수정 CI [39277000589](https://github.com/bin9208/openpilot-rust/actions/runs/39277000589)는 7개 통과했다.
- 깊은 검사: PR 및 dev-md 정확한 SHA의 기존 Rust/ARM·integration·mapped-docs gates. 이미 통과한 검사를 별도 로컬에서 반복하지 않는다.
- 패키지: `[runtime-package]`가 있는 최종 기능 브랜치 push만 실제 AGNOS 패키지를 빌드한다. 공개 baseline 3496c4bb의 모델·외부 라이브러리·Panda·QCOM 산출물을 모두 hash 검증하고 producer 입력이 바뀌지 않았는지 변경 경로를 제한한다. 프로젝트 Rust 실행 파일 87개는 현 SHA에서 다시 빌드한다.
- OS image는 AGNOS 19.8-carrot-bt1의 공개 압축/sparse/raw SHA를 검증해 sysroot를 추출한다. OpenCV·ORT·raylib header·FFmpeg·libyuv도 고정 원본과 hash를 사용한다. generic Ubuntu ARM artifact를 배포하지 않는다. 실제 AGNOS ELF dependency/symbol/version closure, seal 및 static ARM installer의 QEMU 설치와 설치된 AGNOS launcher `--verify-package`를 CI에서 확인한다.
- dev-md에는 같은 SHA를 fast-forward한다. 발행 job은 그 SHA의 성공한 package artifact를 가져오고 기존 publisher가 정확한 SHA의 모든 required push gate와 6개 asset을 검증한 뒤 prerelease를 공개한다. 공개 installer의 online 설치도 CI에서 확인한다. 최종 CI/PR/release URL은 #402에 기록하며, 사용자 실기 검증 전 issue를 닫지 않는다.

원본 dev/main 통합, 차량 접속·설치, Athena 별도 수정은 범위 밖이다.
