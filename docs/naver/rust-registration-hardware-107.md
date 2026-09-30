# Native hardware identity for registration (#107)

Tracking: [#107](https://github.com/bin9208/openpilot-rust/issues/107), following
hardware information [#98](https://github.com/bin9208/openpilot-rust/issues/98)
and registration [#85](https://github.com/bin9208/openpilot-rust/issues/85).
Source: unchanged `system/athena/registration.py`, `common/api.py` and
`system/hardware/tici/hardware.py`; inherited licensing remains intact.

`NativeHardware::new(&dyn HardwareInfo)` borrows the read-only native hardware
implementation. `Hardware::imei` now returns native `JsonValue`; only JSON null
means unavailable. Empty strings, booleans, numbers and containers retain their
source meaning. A failed second read still discards the first read's result.
The adapter forwards errors to the existing registration retry/logging policy.

Registration preserves Requests' iterable expansion and urllib's second
`doseq` expansion for query values. Null omission occurs at the original
boundary, dictionaries yield their keys, and repeated values retain their
order. The existing `runtime-version::python_str` handles scalar and nested
Python-style formatting for spinner text and query elements. Unencodable
Unicode raises an error instead of silently replacing identifier bytes.
The existing string-based `api_get` and spinner interfaces remain unchanged.

## Focused verification

Twelve new scenarios read real temporary cmdline/modem files through unchanged
Python Tici methods and the Rust adapter, then compare registration outcomes,
Params bytes, hardware/clock/spinner traces, collector records and signed
loopback HTTP queries. They cover normal/null/empty/scalar/container identifiers,
a surrogate query failure, modem shape errors and missing serial fields.
All twelve passed on the host. The adapter's native file test also passed;
its initial build failed because the adapter API did not yet exist. Package
formatting, all-target clippy, example build and focused Python lint passed.

Artifacts and invocations are under the issue worktree's
`.omo/evidence/registration-hardware-107/`; the host checker writes `host/result.json`
and source/native captures for each named scenario. The main workspace receipt
is `.omo/evidence/registration-hardware-107-final-receipt.txt`.

```sh
# Python 3.12; original Params and msgq bindings as in registration-validation.md.
python rust/tools/check_registration.py REGISTRATION_TRACE ORIGINAL_PARAMS_SO OUTPUT --hardware
# Omit --hardware for existing 119 plus the 12 new cases (131 total).
```

Requests 2.34.2 remains the combined CI version. Hardware-info's original
2.32.5 script pin does not require downgrading that environment: a focused
hardware-info source/native smoke check passed 40 observations on 2.34.2.
The original Tici import additionally requires `pyserial==3.5`.

The complete 131-case matrix and generic aarch64 checks are assigned to Actions,
not claimed as local results. Manager adoption, actual spinner rendering,
AGNOS startup, normal runtime upload and device acceptance remain separate.
No user device, modem, private persist key or external registration service was
accessed; keys and requests were synthetic and local.
