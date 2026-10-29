# 실제 source entry 권한 전환의 실행 조건 — #613/#598

현재 `verify_native_runtime_inputs.py`는 전체 `source_entry`의 실제 build/sudo 실행을
검증했다고 표시하지 않는다. #611의 진행 중 검사는 유지하고 이 남은 조건의 transport
검사를 후속 브랜치에서 준비한다. 이는 새 runtime 기능이나 추가 언어 전환이 아니다.

`native-source-transport.yml`은 등록된 `bin-actions`에서 원래 factory image의 전체
SHA/크기를 검증하고 해당 job 폴더에만 추출한다. root의 실제 파일 소유권과 SUID를
유지한 원래 ARM `/usr/bin/sudo`를 사용한다. 가짜 root/PRoot나 host sudo 대체로
factory의 권한 상승을 통과시켰다고 처리하지 않는다.

`debugfs rdump`는 UID/GID와 rwx만 복원하며 SUID는 보존하지 않는다.
[e2fsprogs 원문](https://github.com/tytso/e2fsprogs/blob/master/debugfs/dump.c)의
`mode_xlate`/`fix_perms`를 읽어 확인했다. 추출 후 이미 전체 SHA를 검증한 raw image의
`stat /usr/bin/sudo`에서 regular inode와 원래 UID/GID0, mode04755를 읽는다.
이 값과 추출한 regular 파일의 소유권을 확인한 뒤 그 sudo 파일에만 원래 mode를
복원한다. body를 바꾸거나 다른 실행 파일에 SUID를 부여하지 않으며, 실제 ARM
실행에서 읽은 sudo SHA와 복원 시 SHA도 같아야 한다. 이는 추출기 보완이며
실제 sudo/namespace 지원의 성공 근거는 여전히 Actions 실행에 남아 있다.

새 child만 자기 job의 cgroup으로 이동한 뒤 private user/mount/cgroup namespace와
그 user namespace의 binfmt registry를 만든다. 고정된 static QEMU와 credential(C)
규칙으로 실제 ARM Python→factory sudo→root Python의 UID를 검사한다. root Python이
자기 namespace의 cgroup을 만들고 memory limit·소유권을 설정한 뒤 제거해야 한다.
기존 process는 옮기지 않으며 공유 binfmt와 전역 controller의 전후 상태가 같아야 한다.
mount는 mtab을 갱신하지 않는다. root 추출을 사용하므로 전역 loop device도 만들지 않는다.

준비에는 image와 sparse/raw/추출본 및 host 도구의 실제 예상 증가량을 검사한다.
새 child의 준비30초·sudo admission120초와 job30분을 제한하며 자기 cgroup/PID fd로만
정리한다. 기기에 접속·설치·부팅하지 않는다. 이 결과는 X64 host kernel의 ARM user
emulation이며 실제 ARM CPU나 AGNOS kernel의 증거가 아니다.

현재 로컬은 Python AST/YAML/diff 읽기만 수행한다. namespace·static interpreter·factory
sudo·cgroup chown의 실제 지원 여부는 Actions 실행 전에는 확인된 상태가 아니다.
성공하더라도 `source_entry_full_build_executed=False`를 유지한다. #611의 정상 병합과
입력의 실제 발행/검증/Git pin 뒤에 전체 실제 `source_entry` build, 반복·docs·작은 수정,
`--launch --verify-package` 및 source/update/복구 경로를 연결해야 #598의 최종 조건을
완료할 수 있다. 최종 기기 시험은 #528에서 사용자가 수행한다.
