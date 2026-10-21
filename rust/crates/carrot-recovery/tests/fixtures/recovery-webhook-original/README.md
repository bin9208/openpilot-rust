# Original recovery webhook observations

These four data files are copied byte-for-byte from the unchanged Python `server.py::_support_message` capture in [Actions run 39660686811](https://github.com/bin9208/openpilot-rust/actions/runs/39660686811), artifact `12372898539` (`rust-recovery-family-frozen-candidate-296`). The workflow head was `f688c0d8a66518f78ccb5d019717ec953b1c6b5e`; its explicit checked-out source was `a6bab7a7906da0a21d361567430c7d12c0c8516a`. The downloaded ZIP SHA256 was `ddbfe61dda9516197b884984f7c6aa572949c342cbe689c902f8fc143c57ebcb` (47 members, all CRCs valid).

The original `server.py` SHA256 was `7e1e79b5459f0b85b61cf91b6227e35681790fe06acece3a31ee923ac24e942d` before and after capture; held filesystem identity was `2049 4506563 98259 81a4` both times. Each original invocation reported `_support_message`, exit code 0, and empty stderr. The capture ran before the later Cargo command failed on an unrelated `--exact` argument placement. The artifact also retains the original `source-proof.json`, original stdout/stderr and native output for audit; no candidate output is used as the expected data here.

| File | Bytes | SHA256 |
| --- | ---: | --- |
| `message-0/payload.json` | 5507 | `3536849b534370835d23aeb479927464aa5eced8689d2aec35b7972c351632e8` |
| `message-0/source.txt` | 2054 | `2652090ce6a4888cc4c0096b83f84bc052f0bdf1c11b63dadef6fc9620ad879b` |
| `message-1/payload.json` | 24109 | `7719b245aa440d3108cdf1b49d31de31b1e9c7344b65d0e7437021eb08849600` |
| `message-1/source.txt` | 7016 | `576109588a98527fbcc9136caacd6cd28665e87335313d3db5b10c0c6799143a` |

The first case covers a Korean/emoji note and unlimited TTL; the second covers final message truncation with a long emoji car name, unknown commit and the nonstandard permission value. `source.txt` includes the newline from the original `print`. The unit retains its fixed input construction and requires its encoded bytes to match these original payloads before comparing the entire output and the 1,900-scalar limit. No Python implementation bytes are stored in this fixture.

The capture's `SHA256SUMS` accidentally includes itself with a pre-write digest;
that self-entry is invalid and is not used as evidence. All eight actual
source-only member entries match. Acceptance here uses direct hashes of the
four committed original files, unchanged source identity/hash and independent
parent/reviewer archive audits; the historical capture is preserved unchanged.

Native acceptance: the unchanged payload constructions and both full source
outputs pass in Actions39662187794 at a2c9bf7b8da4e907e0e135e449dec90d223210e2;
the recovery library also passes strict Clippy. Artifact12376375401, ZIP
SHA256005a27edbd7956ec420113f42e101cf510e79e9e1530221aa66db0f83286763f,
contains19 CRC-checked members. The overall continuation fails separate new
dev-tools lint findings before full controller traffic; this is acceptance of
the two-case embedded-oracle retirement only.
