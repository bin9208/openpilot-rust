# Modem original-source oracle

The 13 `source/*.json` files contain only the original Python implementation's
side of the already-audited platform-runtime differential artifact 12389225409
(run 39691148023, source commit `d39b1b08c2079e9b3356be06ddddf9db49551be6`).
`process-boundary-source.json` retains only its original startup error and
external-command observation. The archived native outputs were deliberately
excluded. The capture manifest binds the artifact ZIP and the six original
source files; the Rust checker reconstructs each fixed input scenario from
`check_modem.py` and compares the native trace against these independent source
outputs. `request-plans.json` preserves the independently captured 26 source
and native request records from run 39715283281; the provider/source before and
after receipts and capture summary preserve its Python 3.12.15 / pyserial 3.5
provenance. Those records bind each reconstructed scenario, response table,
fixture flags and scalar configuration to the unchanged Python source inputs.
`plan-capture-provenance.json` binds those request plans to the successful hosted
capture and source commit; it explicitly records that capture as engineering
evidence rather than retirement acceptance.
The original helper-lifetime reference remains at the modem crate's fixture path
and is separately pinned by this manifest.

Lifecycle, process-boundary and helper-ownership checks remain live assertions.
Their archived native records are diagnostic evidence, not expected behavior.
