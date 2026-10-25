# #595 SSH recorder의 임시 object 배치 수정

작성: 2026-10-26. 상위 #523. 상태: 원인 확인·한 literal 수정과 source3f532 실제 Focus 수용 완료.

source `7a928cefe22e3c81bcb941f6ed20eaa80eeabc59`의
[Focus40236805939](https://github.com/bin9208/openpilot-rust/actions/runs/40236805939)는
`recorder-build`에서 resource-failure를 기록했다. public/entry Cargo build는 각각 성공했고
runtime entry4는 미실행이다. 최종 봉인과 source/SDK 정리는 별도 결과이며 이 실행을 성공으로
세지 않는다. 원래 failure/finalizer exit1도 보존한다.

artifact12615637199는39033077B/SHA256
`78d0f85f11466b173da8463bbc9ea29a4ce62991aece110f6809da3916b4972d`다.
1148 safe unique regular member/전체 CRC, expanded89681883B를 inert 확인했다.
`driver/resource-failure=recorder-build`, `usage.build.44929.1792965072.954785/first-failure`가
가리킨 raw stderr는 다음 두 rustc 중간 파일의 ENOENT다.

```text
/tmp/openpilot-ci.GflcYj/build/developer-ssh-entry-recorder.exec_recorder.612cef0f0a0b3aea-cgu.1.rcgu.o
/tmp/openpilot-ci.GflcYj/build/developer-ssh-entry-recorder.8q0za86vqb0d6ml788vo1gpnh.rcgu.o
```

`usage_result`는 actual mutable `$build/target` 하위의 실제 사라진 파일만 최대3회 재표본한다.
따라서 build root의 이 오류를 거부한 것은 기존 계약에 맞다. 공간·RAM cap 초과나 runtime SSH
실패의 증거는 없다. 후속 `/proc/<pid>/stat` 부재와 finalizer 실패를 최초 원인으로 바꾸지 않는다.

수정은 `tools/ci/developer_ssh_entry_523.sh`의 recorder 출력 경로 한 곳을
`$build/target/developer-ssh-entry-recorder`로 옮기는 것이다. 이미 생성한 owned mode700 target을
사용하고 같은 rustc source/edition2021/opt1/기한·held ELF·runtime argv/recorder SHA 검증을
유지한다. native test의 recorder 경계는 기존 `$job/build` 하위이므로 그대로 충족한다.
재표본 허용 scope나 partial du 수용, cap/메모리/기한·usage 음성 controls·source 부재/runtime4/
trace/소유 회수 함수는 바꾸지 않았다. 원본 source는 이미 부재이며 실제 원격 SSH 연결은 없다.

로컬은 Bash syntax, 한 literal diff와 경로 소비자 읽기만 확인했다. GHA의 실제 recorder build,
원래 runtime4와 source/index/SDK/held 후검증·업로드/소유 정리를 확인한 뒤 #595를 판정한다.
새 Python 경로·baseline retirement credit·전체 Python 부재 수용은0이다.


## 새 source3f532 실제 수용

[Focus40240059303/job121309736866](https://github.com/bin9208/openpilot-rust/actions/runs/40240059303/job/121309736866)은
source `3f5329dcf76f461bde804d09dfef0c4e5980893b`에서 전체 성공했다.
registry-copy/public-build/entry-build/recorder-build/strace-help와 원래entry0~3은
각각 command0/finalizer0이다. recorder는 실제 `$build/target/developer-ssh-entry-recorder`에서
생성됐고 SHA256은 `574c4754385710cb352a8529fba6b6de24139e66a51ef789e62059ec9e73c0e1`이다.
native summary7개/public summary1개는 complete/reaped/held_verified=true, error=null이다.
실제 원격 SSH 연결은 없고 bounded recorder가 argv를 관측했다.

artifact12616482211은41,558,023B, SHA256
`81d8b43515df22713270ebc7842fdbdcf5d983b46be1ebf863a2f94996dea52c`다.
1,713 safe unique regular member/전체 CRC, expanded98,165,877B를 inert 감사했다.
source/index, SDK file/program/roster, frozen input/roster와 original venv file/link의
전후9쌍이 byte 동일하다. original deletion/부재6통제, Unicode 원본 비교, native regression,
strict auth/SSH Clippy, final source/SDK 봉인·부재closure·업로드 뒤 소유 정리까지 성공했다.
부재closure는 sampled_absence=true이며 전체 Python 부재 주장은 false를 유지한다.

이 결과로 #595 한 경로 배치 수정의 수용 조건을 충족한다. 과거7a928 실패는 그대로 보존하며
전체 runtime exec/mmap 완전 해석, installed provider 전체 closure 또는 #523 완료로 확대하지 않는다.
