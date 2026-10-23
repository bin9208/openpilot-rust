# #519: retain Hyundai source provenance through fixture staging

Issue: [#519](https://github.com/bin9208/openpilot-rust/issues/519), under
[PR #500 → dev](https://github.com/bin9208/openpilot-rust/pull/500).
Preparation HEAD: `ad5b3bf5be7b6e712a1a17ff783960aa4cae2131`.
Preparation checkpoint: the focused candidate and eight real-input regression
cases are implemented, registered and independently reviewed. Candidate Actions
execution and remaining Card acceptance are pending; exact later results are
tracked in the issue above. Local work is source/evidence reads, syntax and diff.

## Actual failure, authenticated evidence and case counts

At source `10919538d31b04d28d9a23b383d925441434d579`,
[Card job 120220823008](https://github.com/bin9208/openpilot-rust/actions/runs/39893953434/job/120220823008)
passed fixture generation, native builds/tests and strict lint. In the following
postcheck, the complete source/native CarParams loop and KIA-presence assertion
precede the failed read of `provenance.json["source_parameter_errors"]`.
The log ends that checker with `KeyError: 'source_parameter_errors'`.
The remaining state/controller postchecks, family reference, actual Card IPC and
shutdown steps did not execute after that failure. This is not a demonstrated
parameter inequality, and it is not a complete Card runtime success.

The [actual artifact 12486490206](https://github.com/bin9208/openpilot-rust/actions/runs/39893953434/artifacts/12486490206)
is `rust-card-can-host-evidence`, 47,193,104 bytes. Its downloaded ZIP matches
the API and upload-log SHA-256:
`34ecbe1f2bb9b45f720dafeb0ec69ae382233ea1c7d099cca4dc5d6bbf508e49`.
Before the read-only download, real disk usage reported 77,678,596,096 available
bytes against a 48,241,680-byte download/metadata growth allowance. No fixed free
reserve was used. ZIP members were inspected as bounded text/JSON; nothing was
extracted or executed. Retained private archive/logs are in the parent's ignored
`.analysis/scratch/2026-10-23-issue-audit/`.

The artifact establishes:

- `fixtures/commands/source.command.json` invokes the real producer with the
  source bootstrap and the same fixture root used by staging.
- `source.exit.json` is returncode 0; `source.stdout` reports **1,284** capability
  cases. `detection.json` contains 1,284 cases across **107** candidate identities,
  including **12** `KIA_K5_DL3_24_HEV` cases. There are 1,284 source-params binaries.
- The retained root `fixtures/provenance.json` has only `source_sha256`,
  `fixtures`, `dbc`, `generators`; `source_parameter_errors` is absent.
  Its generator list begins with `source` and includes all eight generators.
- Its producer and stager SHA-256 values match the actual source109 Git blobs:
  `469d08aa21f1bcf971d0f6621c244e924d8790780d88152c9080104633e3700c`
  and `627f6da23dfc7722b62713f269c5c2f9e96b569404f0663d408f808ce9ec3397`.
- The **separate** `kia-key/comparison/report.json` contains 64 complete-schema
  comparisons, all equal. That count must not be relabeled as the full Hyundai
  postcheck count. No `fixtures/parameters-result.json` was published.

The initial issue body's 64-case description conflated these separate checks;
its update now distinguishes the artifact's actual counts.

## Root cause and minimal repair

`stage_card_fixtures.stage` runs `card_hyundai_source.py` as its first generator.
The producer writes detection count, source hashes, runtime-Python policy and
source-parameter errors to the root `provenance.json`. After all generators
finish, the stager writes its generic source hashes, fixture/DBC paths and
generator list to that same filename. The generic write destroys the earlier
record. The strict postcheck then reads the generic record as Hyundai evidence.

Only the producer's output filename and its one actual reader change to
`hyundai-source-provenance.json`. The generic staging API and `provenance.json`
remain unchanged. The whole fixture-root artifact already includes both files.
The source record's contents and hash computation are unchanged. The reader
still requires the file, valid JSON, mandatory `source_parameter_errors` key and
an exactly empty list. There is no generic fallback, default empty list, caught
metadata error or relaxed full-schema comparison.

Consumer search found that the parameter postcheck is the only reader of this
Hyundai-specific error field. State/controller postchecks consume their own
binary/JSON records and do not read it. No other existing consumer requires a
filename update. This does not change CAN, torque calibration, fixture input,
native serialization, deadlines, source equivalence or physical acceptance.

## Regression definitions and Actions registration handoff

`rust/tools/tests/test_card_hyundai_provenance.py` was written before the two
filename edits. It requires the **real staged fixture root and native parameter
outputs** from the existing Card job. It does not mock the producer/stager,
regenerate a reduced fake fixture, or replace schema
readers. Three tests expand to eight pytest cases:

1. Require both identities after actual staging, authenticate their source file
   hashes against the checked-out source, require the real detection count/KIA
   presence and empty source error list, then run the unchanged complete-schema
   checker and verify its result count. Both provenance files must remain intact.
2. Six private metadata copies retain the real source/native binary inputs.
   Remove the source file/key, corrupt JSON, or supply nonempty/null/object error
   values. Even an empty error field deliberately placed in generic provenance
   cannot substitute. The actual checker must reject each with its specific
   metadata/assertion error and publish no successful result.
3. Change wheelbase in the final actual native CarParams record, keeping valid
   provenance and every other real input. The complete-schema reader must reject
   the mismatch and publish no result. This guards against bypassing parameter
   equality while repairing the evidence contract.

The existing Card test/postcheck pytest invocation now includes this selector,
with its original oracle dependencies, PYTHONPATH, HYUNDAI_FIXTURE_DIR and native
outputs. The added selector, from that step's `rust` working directory, is:

```sh
python -m pytest -c /dev/null -p no:cacheprovider --confcutdir=tools/tests \
  tools/tests/test_card_hyundai_provenance.py -q
```

The existing full `stage_card_fixtures.py` → native parameter tests →
`check_card_hyundai_params.py` sequence is the actual failed-source regression
and candidate acceptance path. Preserve the complete producer and all existing
postchecks. The authenticated109 failure supplies the old collision evidence;
a setup/import error or missing native fixture is not a behavioral substitute.
The new pytest file deliberately fails when its required real fixture is absent.

## Changed paths and remaining acceptance

- `rust/tools/card_hyundai_source.py`: distinct source-provenance output.
- `rust/tools/check_card_hyundai_params.py`: strict matching reader.
- `rust/tools/tests/test_card_hyundai_provenance.py`: real-chain and negative controls.
- `docs/naver/issue-519-hyundai-provenance.md`: this record.
- `.github/workflows/rust.yml`: append the selector to the existing Card pytest invocation.

`stage_card_fixtures.py`, native/primary runtime and Panda files are untouched.
The existing workflow commands and checks remain; only the new selector is added.
No tests, project imports, fixture generation, compiled artifacts, builds or
sudo were executed locally; local verification is AST syntax and scoped diff.
Preparation does not constitute a functional or device acceptance result.

Require actual candidate source/parameter/staging identities, successful complete
Hyundai parameter/state/controller checks, all eight new pytest cases, the full
other-family comparisons and real Card IPC/shutdown. Then retain all exact-head
required gates, normal dev merge and separate merged-SHA post-merge validation.
All functional execution stays in registered Actions on bin-actions; ARM work
retains its existing emulation label. No physical CAN/device success, guessed
GMC torque mapping, FCA checksum correction or #296 conversion is claimed.
