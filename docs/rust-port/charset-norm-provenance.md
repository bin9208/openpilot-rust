# Registration response detector provenance

Registration uses the native external package `charset-norm` 3.5.1 only when
Requests would need an apparent encoding: no charset declaration, JSON default,
or text default applies. Explicit JSON and text media types keep their UTF-8 and
Latin-1 defaults. Detection does not run project Python.

The exact published package is retained under `rust/vendor/charset-norm`, including
its original MIT license, source, data, examples, tests and package metadata:

- Package: [charset-norm 3.5.1](https://crates.io/crates/charset-norm/3.5.1).
- Archive: [published crate](https://static.crates.io/crates/charset-norm/charset-norm-3.5.1.crate).
- Archive SHA-256: `bffd389b7f5eb8907ca98f54dd832502f54313b837e5f1a30f28ebb1a180cd7b`.
- Upstream revision: [ea979ee8a7d907ba7ce17ef50755d8bfe7c46141](https://github.com/RustedBytes/charset-norm/tree/ea979ee8a7d907ba7ce17ef50755d8bfe7c46141/crates/charset-norm).
- Machine-readable origin: `rust/vendor/charset-norm-provenance.json`.
- Sole compatibility edit: `rust/vendor/charset-norm-msrv.patch` changes the
  normalized package manifest's `rust-version` from `1.98` to `1.94`. No algorithm,
  encoding table or test is edited. The repository toolchain remains Rust 1.94.

A preliminary isolated build showed that the source itself compiles unchanged on
1.94. A second build with only the manifest edit passed **without** an ignore-MSRV
flag, followed by all twelve upstream unit tests and five integration tests.
The package audit compares every retained file with the published archive and
permits exactly that manifest edit. The upstream package is excluded from project
workspace membership; it remains an explicit external dependency.

## Source comparisons and limits

Eight focused `requests.Response.text` fixtures cover ASCII, UTF-8 Latin/Korean,
UTF-8 and UTF-16 BOMs, an undeclared Latin-1 byte and two legacy-encoded JSON
identifiers. The selected native package agrees with the actual pinned
Requests/charset-normalizer 3.5.1 decoded text for those inputs. The registration
loopback matrix repeats them through unchanged registration functions, signed
requests and actual Params persistence, separately from JSON/text defaults.

`charset-normalizer-rs` 1.1.0 was evaluated and rejected: the undeclared Latin-1
`café` fixture became `cafщ`, and a CP949 fixture also differed from the source's
chosen interpretation. The rejected probe and exact bytes are retained in the
local evidence ledger. The selected package's results establish these concrete
comparisons; they are not a claim that every possible Python codec input has
been validated. Its upstream algorithm and tables are retained rather than
adjusting guesses to the fixtures.

Reproduction, with the published archive downloaded outside Git:

```sh
python rust/tools/check_registration_vendor.py CHARSET_NORM_ARCHIVE AUDIT_JSON
cargo test --manifest-path rust/vendor/charset-norm/Cargo.toml --all-targets --locked
python rust/tools/check_registration.py REGISTRATION_TRACE ORIGINAL_PARAMS_SO OUTPUT
```

The registration worktree's `.omo/evidence/registration/evidence.json` records the
normal Rust 1.94 commands, archive audit, upstream tests, detector probes and full
host/ARM registration comparisons. Parent exact-SHA integration remains separate.
