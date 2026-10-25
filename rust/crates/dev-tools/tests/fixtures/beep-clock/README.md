# Independent beep clock corpus

The original `check_beepd_clock.py` main supplied all 1,011 fixed-seed inputs.
Before using any native candidate, CPython 3.12.15 `_PyTime_AsSecondsDouble`
produced the expected IEEE754 bit patterns, including signed minimum time and
integer/fractional second boundaries. The manifest records original checker
identity, complete CPython provenance, size, counts and both corpus hashes.
The checker replaces only this fixed-input gate; no Python wrapper or generic
runtime clock oracle is retained.

Fifteen controlled original subprocess captures cover exact artifacts and failure
ordering, including malformed integers before writes, wrong cardinality after
native.stdout/inputs.json, and mismatch reports before assertion. Each output
starts as `keep`, making premature truncation observable. The seven original CLI
captures preserve argparse exit status and side-effect-free help/errors.
The native tool reuses the lightweight stats clock line/integer parser, including
its exhaustive CPython Unicode digit and whitespace regression.

Three further original filesystem fault captures make each receipt pathname a
directory in turn, verifying that a failed artifact write leaves later receipts
unchanged. These are in `io-failures.json.gz`.
