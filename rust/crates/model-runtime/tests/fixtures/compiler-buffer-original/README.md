# compiler buffer source100 고정 원본 관측

이 디렉터리는 실행 코드가 아닌 data-only fixture다. source
`1000464f57239a1d15bfba3bdb6c01550f12d8a0`의
[run40231559285/job121285061989](https://github.com/bin9208/openpilot-rust/actions/runs/40231559285/job/121285061989),
[artifact12613507032](https://github.com/bin9208/openpilot-rust/actions/runs/40231559285/artifacts/12613507032)의
`compiler-buffer-original/` 7파일을 그대로 보존한다. ZIP1595256B의 SHA256은
`b8eb67c19d759265cb25e5fca54aee1979c1b79fe11dc3435a098e934cdea97e`다.

유일한 이름 mapping은 **archive `oracle.py` → fixture `oracle.txt`**다. bytes와 SHA는
그대로이며 `.txt`는 provenance 자료로만 읽는다. Python/snippet/subprocess 실행은 없다.
`observation.json` 내부의 과거 argv 및 원래 이름은 다시 쓰지 않는다.

| fixture | archive 원래 이름 | bytes | SHA256 |
| --- | --- | ---: | --- |
| `observation.json` | `observation.json` | 39344 | `e8e52475eba99d6e13287ecb957ec650c8e4d64b1e517caea1fd3b08fdddf257` |
| `oracle.txt` | `oracle.py` | 4694 | `2b4747903018e15f3f99eabf62a6fb052f4899bfd0bee1ac44ce7bb5501cf371` |
| `python.sha256` | `python.sha256` | 86 | `74a2bf284a5116d43270762ab1766fe74421da338fa0889fcd2c188202eae773` |
| `python.version` | `python.version` | 14 | `5b3e43dc38ca01e3f5a7854ba50d4330864b0c1e0f646650b2c78cf072acc366` |
| `request.json` | `request.json` | 60636 | `809ff0aba501e93a4bab0d86fd8066a77d72881580cfc72408e1c1ea19fd15fb` |
| `stderr.bin` | `stderr.bin` | 0 | `e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855` |
| `stdout.bin` | `stdout.bin` | 270242 | `fc019a80d96e11a95cc8d1573b94cbbb4ea06f30b56fdf71efa6f29d5e149593` |

독립 기대값은 `stdout.bin`의 descriptor32/trace147이며 입력은 `request.json`이다.
후속 native 출력으로 기대값을 재생성하지 않는다. test는 7파일 SHA/길이, 과거 source22와
Python/owner/proof의 전후 receipt, rc0·signal 없음·opened CPU·179입력·959행과 cap을
확인하고 기존 native6 및32/147 비교를 보존한다. 현재 원본22파일이나 Python 설치는
열지 않는다. 원본 `buffers.py`/`offline_buffers.py`의 production caller 전환·제거와
전체 Python import/DSO closure 수용은 이 fixture의 범위 밖이다.

상세 감사 및 GHA 잔여 조건은 `docs/naver/python-rust-523-buffer-corpus-100-audit.md`다.
