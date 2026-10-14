# Original filter reference

`filter-reference.f64le` contains 24,000 outputs evaluated by the original
`FirstOrderFilter` and `BounceFilter`, not by the Rust implementations. The
paired JSON records exact source/trace hashes, source commit, input digest and
output digest. It was captured before retiring the Python comparison tool.
Source licensing/provenance remains in the repository's original source tree
and license notices; this file contains numerical test data only.

The input trace retains all six original timestep/initialization combinations,
2,000 samples per combination, the original deterministic generator, and RC
changes at samples 700 and 1,500. The capture independently verified all outputs
using the former Python checker's absolute/relative tolerance of 1e-12, with a
maximum absolute difference of 3.9968028886505635e-15. No driving data is present.

`check-filter-reference` runs the native trace executable with a deadline and
bounded output, verifies the exact input corpus, and compares every result to
this immutable original-source reference using the same tolerance. Source
changes to the original filters are not silently incorporated: changing the
reference requires a new independently evaluated corpus and provenance review.
