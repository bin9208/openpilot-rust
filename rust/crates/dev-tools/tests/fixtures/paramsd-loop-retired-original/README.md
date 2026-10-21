# Paramsd loop original observation for the post-deletion gate

The only designated Python deletions are `rust/tools/check_paramsd_loop.py` and
`rust/tools/paramsd_loop_fixture.py`. The retained `paramsd_fixture.py` and
`paramsd_loop_source.py` remain live, pinned source inputs.

These files came from the actual unchanged Python checker lane of [run
39535390950](https://github.com/bin9208/openpilot-rust/actions/runs/39535390950),
job `119010745485`, artifact `12317675555`. Its downloaded ZIP SHA-256 is
`1111f01652e8d9899cd0c1669bc589498487bb6b3b74a064e8c429abc64ffcf4`.
The workflow commit was `07656775bb4d5cca369731565db1dca523fe0318`;
the separately checked-out source was
`5f6104be47db0b3ae9131241726613788eeac747`. The exact ZIP member prefix,
source hashes, raw sizes and raw/stored hashes are in `observation.json`, whose
SHA-256 is pinned in the Rust retirement test.

`source.json.bz2` holds the original C++ oracle's complete 39-case observation,
not a native-generated answer. `input.json.bz2` holds the original Python
request; its raw SHA matches the earlier independent original input fixture.
The original checker stdout, stderr, exit, process log and result receipt are
preserved byte-for-byte. The accepted artifact also contained the original
checker's *candidate* raw output; that candidate-derived file is intentionally
not an expected fixture here. It is regenerated on each cloud run.

The post-deletion test authenticates the compressed and raw bytes, parses the
entire original request, and passes the original source through
`verify_fresh_source`. Against the older independent original source, only
the stack addresses in logged cases 9, 10 and 35 may vary; their prefixes,
stack shape, and every other field must match. The current candidate is
checked by `compare_file` against that complete older original observation,
with the pre-existing scalar comparison rules. A new ELF's hash is sealed
within its own run and is **not** required to equal the historical ELF hash.

After the two Python files are deleted, the mandatory cloud gate must build
the current checker and `paramsd_loop` example, run the same fresh ELF under
the actual checkout root and a copied root lacking those two paths, require
two successful exact native execs per lane under unavailable `PATH`, and run
the generator CLI/library and controller/finalization negatives plus strict
Clippy. It must retain both raw streams, results, source and executable hashes,
exec traces, and the original observation checks. Neither Python file needs
to be copied or executed in the post-deletion gate.
