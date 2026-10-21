# Original Panda orchestrator CLI admissions

This is the unchanged Python `rust/tools/check_pandad_ci.py` output from GitHub-hosted
Actions [run 39567432880](https://github.com/bin9208/openpilot-rust/actions/runs/39567432880),
job 119112995269, checkout `7f8de4a33cba5bbab85eff4821561d1a1f500cc5`.
The original-only capture step completed before a separate strict-Clippy failure;
no native CLI comparison or raw-path full run occurred in this job.

- Artifact: `12331125685`, ZIP SHA-256 `144ee4bcc3e212707ef801e626330e285a2a15510989837ae866db4f9143437a`
- This `original-cli.json` SHA-256: `02aa67da5d511a312fe30b7ebc058da5a8ada66098e14aef7b05a8b63eef700b`
- Original checker source SHA-256: `21dd66f08fb279f0d8bdee35b97fd22027e82275486c5a8510704cf3847676c9`
- Original checker test source SHA-256: `46575991a4a668d12828da5ee02832db015e5aaa159e3c617d1926b09329cb22`

All eleven captured `argv` vectors and stdout/stderr streams are preserved as
base64 bytes with exit codes, stream hashes, and observed output side effects.
The first eight cases are argument/help admissions; the last three reach an
owned output directory then fail on a deliberately absent build-messages file.
The saved paths are the original hosted-run paths; consumers should compare
behavior after substituting their own rooted paths, not require matching
workspace spellings. This fixture does not establish full 38-phase behavior or
post-deletion source absence.
