# Original paramsd loop oracle

These are byte-for-byte original-only observations from public GitHub Actions
artifact `12306774260` (run `39514415599`, job `118943760823`; merged source
`4a3336f564e5766b07627633a1b9d72038693d92`, source branch `62e8`). The
downloaded ZIP SHA-256 was
`2bda6d0a38a141cda798395d741a67c436240a62011fd56c81b4a48dcc61514f`.
Only `estimation/paramsd/loop/source.json` and `input.json` were extracted and
bzip2-compressed without changing their raw bytes. The archived native result
is not used as an expected output.

| Original member | Raw bytes | Raw SHA-256 | Stored SHA-256 |
| --- | ---: | --- | --- |
| `source.json` | 7,763,306 | `23d374139b2430bd8da0eb800c0a8a91900c8d08c0df90373ba2f4d7419ae22a` | `96b1a6dae1169fbdb39bd0cee6cce2fc67b40ea81fb9af7b2b7f392a3f99a85d` |
| `input.json` | 4,430,072 | `4f446bc98c9d303d292f487c9ebe36bc8721436a30627dfca55baabae4c6811d` | `6a5641f18914cbf452f027ed33daf64a372a966ebc89ae2a2e859a4de384f818` |

The original source has 39 ordered cases, including 22 migration, 12 cache,
three malformed, and two 1,210-frame loop cases. Its rows contain 4 saves and
216 GPS writes. The comparator checks all original initial state, covariance,
ordered operations, logs, row states, GPS writes, cache decisions, and published
Event fields against freshly generated raw native output. It decodes the native
f64 bit arrays and Cap'n Proto Event packets itself. The comparison follows
`check_paramsd.compare`: exact structure and nonfloat values, 1e-9 tolerance
for f64, and 2e-6 only for Event floats outside debugFilterState. Error-log
suffixes after `:` and `put` Event packet projections follow
`check_paramsd_loop.normalize` for both sides.

The original Python JSON contains one nonstandard `-Infinity` literal at
`migration-13.init.initial[2]`. After verifying both fixture hashes, the
reader substitutes a private marker at precisely that position; the native
reader produces the same marker only from exact negative-infinity f64 bits.
All other nonfinite native floats are rejected. No broad token substitution,
numeric tolerance change, or native-derived expected output is used.

Integrate `paramsd_loop_reference::compare_file` on the **fresh**
`paramsd_loop` raw JSON output, with `PARAMSD_LOOP_NATIVE_RAW` pointing at that
file for the focused test. This leaf module/test needs a crate `lib.rs` export,
test registration, and a cloud job that runs the native generator and loop
against the same current input. Those shared caller/workflow changes belong to
the parent. Keep the historical Python checker until the same-ELF
present/absent retirement gate succeeds.
