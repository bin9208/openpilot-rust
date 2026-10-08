# CarrotMan owned comparison input freshness (#243)

Issue: [#243](https://github.com/bin9208/openpilot-rust/issues/243).
The post-camera dev run at `b03901c782094390ba5c9e4baa8e1f294276db2e`
[failed](https://github.com/bin9208/openpilot-rust/actions/runs/37793526299/job/113366878030)
at `selected[6].data.szPosRoadName`: native `route=300.0`, original empty.
The previous PR success does not replace that failed post-merge result.

The owned fixture published carState and selfdriveState only inside `pump()`.
The same caller also blocked on HTTP/TCP/ZMQ and drained non-conflated outputs.
Both services declare 100 Hz and both SubMaster implementations mark them stale
after 100 ms without reception. Original `CarrotServ.update_navi` clears its
debug text on each update and appends `route=...` only when both inputs are alive.
The Rust bus and tick use the same gate. An emitted empty road/debug field in
this fixture therefore identifies the original stale-input branch. Original
oracle Event timestamps are fixed at zero; they were not used to infer input ages.

A controlled 4.3-second caller block across the four-second TMAP owner lease
reproduced the exact selected-field mismatch with unchanged runtime bodies.
An external observation wrapper recorded original receive ages, alive flags and
debug text. After the input producer change, the identical caller block passed:
all seven selected source/native publications agreed under the existing age-field
normalization, with no stale control-input ticks after initial reception.

The fixture now gives its unchanged input values to one periodic msgq producer.
`pump()` consumes outputs and checks producer failures. Cleanup stops and joins
the producer, saves publication timing and its failure result, then closes the
daemon, sockets and namespace even if producer cleanup raises. Runtime freshness
thresholds, navigation selection and comparison fields remain unchanged.

## Verification

All local paths below are relative to the isolated issue checkout's
`.omo/evidence/carrot-man-243/`, except the coordinated build receipt.

| Scenario | Invocation record | Binary observable | Captured artifact |
| --- | --- | --- | --- |
| Caller blocks through owner expiry before the fix | `controlled-expiry-gap-red-invocation.json` | Exact `selected[6].data.szPosRoadName` failure; original stale receive ages and empty debug text | `controlled-expiry-gap-red/{source,native}/selected-probe.json`, `source/input-gates.jsonl`, `source/fixture-timeline.jsonl` |
| Same 4.3-second caller block after the fix | `controlled-expiry-gap-green-invocation.json` | Complete seven-publication comparison passes; original inputs remain alive after first reception | `controlled-expiry-gap-green/{source,native}/selected-probe.json`, `source/input-gates.jsonl`, `{source,native}/input-publications.json` |
| Actual msgq inputs continue during a 250 ms caller block | `pytest-literal-green-invocation.json` | Received carState was published more than 150 ms after blocking began | `pytest-literal-green/test_inputs_continue_while_the0/input-publications.json` |
| Regression sensitivity | `pytest-one-batch-red-invocation.json` | The same wire regression fails when the producer emits only its first batch | `pytest-one-batch-red/test_inputs_continue_while_the0/input-publications.json`, `one-batch-mutation/carrot_man_fixture_inputs.py` |
| Producer wire-construction failure | `pytest-literal-green-invocation.json` | Invalid cereal network enum reaches the caller as a chained failure after the thread is joined | `pytest-literal-green/test_publisher_failure_reaches0/input-publications.json` |
| Existing twelve owned process boundaries | `final-owned-current-invocation.json` | Full original/native output, side-effect and upload comparison passes | `final-owned-current/owned-summary.json`, `{source,native}/selected.json`, `publications.json`, raw cereal streams, upload/HTTP JSON and input timing |

The final full comparison passed in 25.391 seconds using the current native
example SHA256 `0db80b5c9f90b5a99850e79ef2c146067fb49b636ef79f9e110b187edf908135`.
The coordinated primary build took 4.982 seconds with two jobs, incremental
compilation disabled and debug information disabled; its disk guard and exact
source hashes are in the primary checkout's `.omo/evidence/243-primary-owner-build/`.
The retained prior #232 ELF was used only for the focused gate diagnosis.

After that full run, Ruff required a syntax-only conversion of input `dict()`
calls to literals. `input-value-equivalence.json` verifies identical values for
all four network/CAN combinations used by the owned scenario; the focused wire
tests were rerun. New helper/test files pass project Ruff. The existing harness's
intentional early msgq provider import has the same pre-existing F401 finding as
the base revision; syntax checks and diff checks pass.

The focused wire tests can be run after the original msgq binding is staged:

```sh
PYTHONPATH="$CARROT_MSGQ_BINDING:$PWD/rust/tools:$PWD:$PYTHONPATH" \
  python -P -m pytest -c /dev/null --noconftest -p no:cacheprovider \
  rust/tools/test_carrot_man_fixture_inputs.py -q \
  --basetemp "$RUNNER_TEMP/carrot-man-input-tests"
```

PR #245 head `53fbb4df` passed both actual CarrotMan jobs, but its
[workspace job](https://github.com/bin9208/openpilot-rust/actions/runs/37812693529/job/113433537528)
also collected this native-only test from the generic `tools/tests` directory
and failed because `msgq.ipc_pyx` is unavailable there. The unchanged test now
lives beside the other native runtime checks in `rust/tools`; both CarrotMan
jobs still invoke it explicitly. Generic collection without the native binding
collects 99 tests successfully; the relocated two actual-msgq tests and 22
CI-policy checks pass. No dependency, runtime behavior or assertion was removed.
The corrected candidate requires fresh exact-head CI.

The existing `carrot-man-runtime` Actions job runs these tests before the owned
comparison and preserves their JSON artifacts. Exact-head branch and post-merge
CI remain separate gates to inspect. This fixture correction is host evidence;
it does not establish complete manager startup, log-upload integration, CPU
savings, or device/drive acceptance. No vehicle or NAS endpoint was contacted.
