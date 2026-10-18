# Encoder CI original evidence

The unchanged `rust/tools/check_encoder_ci.py` produced these fixtures before the
Rust policy module existed. Each file pins the checker, pytest file and shared
CI helper source SHA256.

- `dependency-original.json`: 17 actual Cargo JSONL selections/rejections,
  identical duplicate output deduplication, missing/ambiguous/failed builds,
  artifact files and malformed records.
- `main-original.json`: 16 complete original orchestration histories, controlling
  external imports, available disk and subprocess results. The original main
  function is unmodified. Calls retain disk-check order, Git/pkg-config queries,
  staging/build/checker arguments, stage failure short circuit, all three receipt
  counts and final receipt artifacts.

- `process-original.json`: nine real owned shell stager/builder subprocesses,
  preserving command/stdout/stderr/exit artifacts, bypass and schema errors.

The native transition replaces only the Python runtime-checker command prefix
with `check-encoder-runtime`, and replaces Python import/package identity with
native checker executable and complete Rust/cereal/IPC/source identity. Original
host codec versions, Cargo message hash/current dependency paths, source commit,
all build/child phases and 5/32/10 receipt gates remain required.

The original outside-Python-binding negative is intentionally superseded by a
missing-native-runtime-checker fingerprint negative; it is not treated as a
native execution test. The original source is retained pending integration.
The pytest file also contains Params-builder tests, outside this CI module's
ownership; do not delete that file until their native coverage is reconciled.

Raw capture scripts/logs stay in ignored scratch. Synthetic fixtures contain no
credentials, vehicle settings, raw device captures or build outputs.

After actual native 5/32/10 proof, the CI/runtime/outcomes Python tools and their
encoder-only tests are retired. Four unrelated Params builder contracts remain
unchanged in `rust/tools/tests/test_params_native_stager.py`, required by the
Xiaoge source job until that Python builder's consumers are also migrated. This
partial test-family retirement is explicit in the baseline inventory.
