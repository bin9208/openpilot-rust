# Original upstream-sync numeric semantics

`original-number-semantics.json` was captured on 2026-10-15 by importing the
unchanged `tools/naver_ci/sync_upstream.py` at baseline
`b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a` and calling its `should_update`
function. It made no subprocess/API/network calls or repository mutations.

The 19 cases preserve Python's JSON numeric distinctions: `ahead_by: true`
satisfies `isinstance(value, int)` and positive comparison, while `1.0` does not.
For `behind_by`, false, integer zero, positive floating zero and negative floating
zero compare equal to zero. These expectations come from the original code,
independently of the native implementation.
