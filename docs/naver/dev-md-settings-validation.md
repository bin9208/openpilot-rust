# dev-md settings service integration

Source `e8e152dbcfd9574a1791ed12026d1880e87689b3`, issue #338, branch dev-md.
No device, stable branch or deployed web service was changed.

The native server rejects external writes to DriverMonitoringSessionDisabled
and JetsonLegacyUpdatePending, both at HTTP (403) and direct service boundaries.
Backup/export/QR strips these keys and DriverMonitoringEnabled. QR schema/key
codes remain available for historical decoding. Preview and restore reject
internal state; only explicit reset_defaults may reset the persistent DM enable.
Turning DM off clears the uncertainty warning only while offroad. Search-only
settings remain searchable but are excluded from group counts and profile
capture/cleaning. These are source contracts, not parameter-name heuristics.

Backup downloads now parse and filter JSON and return no-store responses. The
existing native refusal to open FIFOs is retained, including a nonblocking open
and descriptor type check to cover replacement races. Final upstream's ordinary
JSON open would block on a FIFO; the HTTP comparison records this native guard
separately instead of running a blocking source read or claiming equivalence.

All four new native tests pass. Regressions were first observed for forbidden
writes, backup leakage, restore classification and profile inclusion, then fixed.
Exact original/native checks on 2026-10-15:

- 78 Params cases, no returned-value or raw-storage mismatches.
- 174 QR cases, including 222 exported keys, exact payload/metadata/binary/error
  comparisons; protected-state export case included.
- 77 profile/history/restore cases, exact returned/coded errors, persisted and
  temporary files, and Params bytes; explicit restore/reset cases included.
- 41 catalog/coercion cases, exact ordered JSON bytes, including search-only
  counts and vehicle filtering.
- 106 real HTTP observations for settings/actions plus graceful shutdown.
- 36 real restore/download HTTP observations plus graceful shutdown; native FIFO
  rejection additionally verified as 403 and recorded separately.

Retained source services params.py/settings.py/setting_profiles.py and the
features/params.py handler match the pinned final source; they are differential
references, not runtime dependencies. Native runtime owns their behavior.
Local detailed logs: `.analysis/scratch/2026-10-15-dev-md-resume/server-*`.

Settings catalog/front-end/native UI and paired guide/Wiki integration remain
pending. This record does not claim complete DM UI or full-runtime acceptance.
