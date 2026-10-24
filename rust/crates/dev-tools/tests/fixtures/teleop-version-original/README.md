# Independent teleop release-tool observations

Source-only hosted run [39669805189](https://github.com/bin9208/openpilot-rust/actions/runs/39669805189)
checks out c810f7159ccecde0826e51c1ccc6364d0081cd8e under workflow
afd6013619f46781332a494ecf14718d9fee79b0. Artifact12378144762 is9,170bytes,
seven CRC-checked members, SHA256
720f521346ff6b5456fbabb701856d7f0f10845471b4c8a3378515333221e42e.

The unchanged original two Python snippets produce34 computation observations;
the original shell script produces10 scenarios in disposable hosted Git
repositories. No native output supplies expected values. The parent verifies
all five original member hashes and both source pins unchanged before/after.
The exact original manifest SHA256 is
e61842b4db2ddfa2760ec878b33ea11f0556845465963f2b17902e961225456a.

This durable subset retains only the original manifest and the two result-data
JSON files. The manifest also describes three provenance files in the complete
capture; their executable shell/snippet copies are deliberately absent here.
Native comparisons authenticate both retained data files against that unchanged
manifest and check its original source hashes. They never execute old snippets.

Successful computations compare exact exit/stdout; failures compare exit/stdout
and original exception category, not Python traceback formatting. All10 script
scenarios compare exact output/effects, with ordered error categories for failure
stderr. Git/tag/commit operations are restricted to hosted disposable fixtures.

The supported release interface uses a string PEP621 project.version. Arbitrary
non-string TOML printing and out-of-spec large TOML integers remain explicitly
unsupported. Decimal conversion uses the default4,300-digit CPython bound.
Source capture exposes embedded CR as input rather than a Unix line terminator;
the native implementation retains that failure behavior.
