The two `.jsonl.gz` files contain the complete 24,585-line request and expected
reference streams written by the unchanged Python DesireHelper checker on the
checked-out source `6e34f78ae77734dac46967cc605836b950c99db9`. The cloud
capture selected only the checker's original writer block, stopping before its
candidate invocation. They were compressed with `gzip -n -9`; `manifest.json`
records the original raw hashes and source closure, and `provenance.json`
records the capture run, artifact ZIP, and compressed hashes.

These files do not establish native parity. The Rust controller must execute a
fresh native candidate and compare every actual row against the retained
reference, preserving the original `1e-10` numerical bound and exact discrete
fields. The original checker remains available for a live cloud pair until its
separate caller and absence proof are accepted.
