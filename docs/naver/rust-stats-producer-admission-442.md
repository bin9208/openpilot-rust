# Stats producer historical source admission (#442)

Issue: https://github.com/bin9208/openpilot-rust/issues/442. This is a #296 developer-checker defect, not evidence of a vehicle runtime failure.

At20a8f52e `sources()` omitted absent retired `check_stats_producer.py` and `stats_source_producer.py`; later verification only revisited admitted files. A helper reappearing during one of the four worker scenarios could therefore leave `result.json` reporting PASS. The new guard keeps absent paths and real parent-directory identities, rechecks them through CLI output/handler teardown, and only commits its own inode-bound no-clobber result. Rollback preserves a foreign replacement receipt.

The focused regression uses the actual controller with a held private worker: all four mode outputs must exist, then the old controller must fail the specific assertion because it accepted reappearance. Compilation errors, missing inputs and unrelated failures do not count as old behavioral evidence. Current tests include unchanged admission, reappearance, parent replacement and foreign receipt rollback; the existing actual StatLog fork/drop/pressure/types comparisons remain mandatory.

Source-ready only. GitHub-hosted old RED/current GREEN, original wire comparisons and strict Clippy are pending. Local checks are syntax, formatting and diff review. Final filesystem checking narrows the window; it does not claim atomic protection from mutation after commit.
