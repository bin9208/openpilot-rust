# BlueZ prompt policy controller corpus

The native controller preserves the historical Python schedule: seven prompt
kinds, 33 fixed values, every Unicode 15.0.0 Digit or Decimal code point
(808), and every six-character Decimal-zero sequence (68). This is 909 values
per kind and 6,363 responses. The pinned independent property source is
`../../../data/unicode-15.0.0/DerivedNumericType.txt`, SHA-256
`710abf2d581ac9c57f244c0834f9d9969d9781e0396adccd330eaae658ac7d6b`.
The controller never imports production Bluetooth policy tables. It compares
all candidate responses and the complete `--digits` table.

The independently captured Python 3.12 observation from Actions run
`39499806776` is sealed in `original-observation-39499806776/`. It contains
the exact ordered `cases.jsonl`, independent `source.json`, and `result.json`.
The same-ELF original/present/absent comparison subsequently passed in
Actions run `39508434006`, job `118923150264`, before source deletion. Its
public artifact `12304089348` retains the complete raw lanes and exec trace.
These are original observations; no native-generated expected file replaces
them.

Post-retirement, `bluetooth_bluez_policy_reference` compares the sealed
ordered case and source files with a fresh native lane from
`BLUEZ_POLICY_NATIVE_OUTPUT`. It checks fresh stdout/stderr via
`BLUEZ_POLICY_NATIVE_STDOUT` and `BLUEZ_POLICY_NATIVE_STDERR`. A second fresh
lane in a copied root must use the same candidate ELF and exact native
controller ELF. The controller requires its historical Python path absent
under a held `rust/tools` parent; any regular file, symlink, FIFO, replaced
parent, or reappearance before final receipt publication fails. The three
surviving source pins remain mandatory.

The post-deletion Actions gate must seal executable files before and after
both lanes, retain full raw reports and fixture responses, and compare their
ordered streams. Its copied-root `PATH=/nonexistent` exec audit requires
exactly three successful calls: native controller once, `bluez_policy`
without arguments once, and `bluez_policy --digits` once. Python or any extra
exec attempt fails. This post-deletion gate is still pending cloud validation.
