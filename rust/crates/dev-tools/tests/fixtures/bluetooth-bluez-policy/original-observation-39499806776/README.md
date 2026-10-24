# Original BlueZ policy observation, run 39499806776

These are raw files written by the **unchanged Python 3.12**
`rust/tools/check_bluetooth_bluez_policy.py` in GitHub Actions job
`118894126663`. The workflow run head was
`6acd3681a39ee0602c34f72688f7685a50b5fe4a`, but this job explicitly
checked out and executed source commit
`0d1700e5a3ad0181ecfdd7f7f25e4fc9d088e1d2`.
They came from artifact `12302620012` (`rust-ui-connectivity-evidence`),
whose downloaded ZIP SHA-256 is
`7363d70dc5fab935242f796a2cd8608e7a70f214b3a21141275bee4747bb46d7`.

`cases.jsonl` is the original controller's ordered input corpus;
`source.json` is its independent `Bluez.respond` result for each case;
`result.json` is its original receipt. They are copied byte-for-byte from
`ui-connectivity/bluetooth-bluez-policy/original/` in that artifact, with no
native-generated expected values. The unchanged checker source SHA-256 was
`fa1e764393c064f3aa95386a221eaf1690cf14d5cb932b027304124ceaf1c3af`;
the checked `bluez_policy` ELF SHA-256 was
`cc350aa1c41fd1e537e46cb4cfc9a99448e28d0a9e2b31fd7ec6d7c7196b1316`.

| File | SHA-256 |
| --- | --- |
| `cases.jsonl` | `db2d480a8bace324c8854bf837b1d2cc4270d2c058d481609de1532c4e0c9fbc` |
| `source.json` | `fedaa7902e27d97cc508b8acfcf0995edd6117ea55725fa3c658ebd54eb18e5d` |
| `result.json` | `923ec492db1924b994824dab7d09e505e39d0acd856cf049eecbc591ba11dbe5` |

The corpus has 6,363 ordered cases, of which 3,598 were accepted, covering
all 808 Python Unicode 15.0.0 digit points. The live original/native pair
passed in this job. This observation alone does **not** prove the Python
checker can be removed; a reviewed absent-source and interpreter-free native
execution gate remains required.
