# dev-md offline startup clock floor

Issue #338, source `e8e152dbcfd9574a1791ed12026d1880e87689b3`:
`openpilot/common/build_time.py` and its tests.

The native launcher checks the checked-out HEAD committer timestamp after AGNOS
initialization and before selected-model delivery. It does not use an upstream
reference, author date or network clock. An already-current clock is unchanged.
A second wall-clock read avoids moving it backwards when NTP has caught up. An
older clock advances to HEAD plus one second through the original noninteractive
`sudo date` command, then the launcher verifies the resulting clock. Invalid Git
metadata, failed commands, timeout or an unsuccessful correction stop startup and
retain the recovery surface. This path is enabled only on AGNOS.

Git and date commands use the native process-child error handshake and a ten-second
capture timeout. No system clock was changed during validation: policy tests inject
clock/command providers, and integration tests use an owned fake Git/sudo provider.

Host and ARM gates pass:

- Original current/future, old clock, NTP catch-up, invalid commit, failed command
  and ineffective correction scenarios preserve exact command ordering.
- 759 UTC conversion cases executed against the pinned original match exactly,
  including negative times, sub-microsecond ties, calendar limits and invalid values.
  The first native attempt exposed negative-fraction rounding; matching the source
  truncation and signed fractional rounding fixed it without relaxing comparisons.
- All 13 actual launcher scenarios pass, including clock inspection before eGPU
  preparation on the owned AGNOS fixture and no date-setting command for a current
  clock. Existing cancellation, manifest, update and fallback assertions remain.
- Runtime-launcher all-target Clippy passes with warnings denied.

These checks establish policy and native startup integration, not physical clock
correction or vehicle acceptance.
