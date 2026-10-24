# setup SSH 원본21개 기대값

이 JSON은 `a134b32c5d50d346994d671c0882b147175021dc`의 기존 원본 실행 결과다.
현재 Rust 결과나 새 expected를 생성한 파일이 아니다. [#523](https://github.com/bin9208/openpilot-rust/issues/523)의
한정 원본 제거 후보에 쓰며 전체 auth/SSH·외부 네트워크·실기 수용으로 확장하지 않는다.
기존 저장소 MIT 라이선스와 Comma.ai 원본 provenance를 유지한다.

- 원본 artifact: `prior-a134-focused.zip`, SHA256 `5090a51e593cd6220307593041552ff1ccb96019df6a2ea5ad5b6d5ca6bf3605`.
- 원본 member: `developer-cli-original.log`,43,585B/SHA256 `bf1e0ab9b42fdb4382df46d639c1748ffb4bb67c7addaf172b2e9695c449e682`.
- `DEVELOPER_CLI_ORIGINAL_JSON ` 중 group `setup-effects`의 JSON payload만 byte 그대로 복사했다. marker와 LF는 파일에 없다. 재직렬화·기대값 수정은0이다.
- 현재 파일:13,800B/SHA256 `444a47ecba9579d4042f26dd89f8438908b56d244a0f2c98c8b0dd3e1ef3a6ef`.
- 원본 Python3.12.15/requests2.34.2,21records 모두 `exact` criteria다. 실제 원본 main AST를 제어된 transport/Params에 연결한 기대값이며 실제 GitHub/계정·기기 호출이 아니다.
- 원본 setup source SHA256 `ab157e4943734c969ffc1e8113dc9d09ded355f3463da34c2b9132fc24181f3f`, 원본 SSH source SHA256 `f4fb8dff60a0bab57643172093a6f437acb0d001929103f081d872280710eeff`를 과거 metadata 그대로 보존한다.
- 당시 metadata에 없었던 `native_sources`나 현재 provider 정보를 추가하지 않는다. 현재 source/SDK/ELF/실제 entry 증거는 별도 gate에서 기록한다.
- a134의 전체 테스트 log는2 PASS/1 FAIL이다. setup21의 개별 성공만 재사용하며 parser 실패는 이후 별도 검증으로 관리한다.

현재는 frozen 원문 수입 후보이며, 실제 current replay·원본 부재·공개 helper 실행과 inventory 반영은 미수용이다.
