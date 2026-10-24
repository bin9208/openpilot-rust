# Original managed-entry field contracts

`original-fields.json` selects only the **original source child** from all44
successful comparisons in CI run39153248437, artifact12146936832. It contains
22scenarios through each of the original and native collectors. Both source-side
sets agree on the selected fields; no native candidate response supplies an
expected value.

The tested merge and tree match source2106ad4791d488a6361f93b85a8b43eb3d2de1fe;
identities, original manifest hash, per-result/request hashes and source pins are
embedded. The source SDK is2.55.0 and setproctitle1.3.7. The extraction follows the
unchanged `check_managed_entry.compare` contract for steps and SDK calls, preserves
outcome kind/stage, real Params result, record message/level/context/exception
presence and selected external SDK fields. Ephemeral request root, IPC namespace,
endpoint and loopbackDSN are omitted; full original results remain in the CI
artifact. Disk/wire identity, actual IPC, process name, signal response, traceback
and readiness checks still require live native checker implementation. These
captures alone do not retire any managed-entry source or test.

SHA256: `6b0ca0e35bbc9bb51b2ff78e9560aefbed5bc40e8d65a072346cad69d1251271`.

A separately reproduced original defect is tracked in issue381: an unknown
`--case` returns PASS with zero comparisons. The native CLI must reject that typo
while preserving all22 valid names and the44 unfiltered comparisons. This is an
intentional false-PASS correction; it does not authorize reducing the valid
scenario or collector coverage.
