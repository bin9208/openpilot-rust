# Original release file selection

`original.json` was captured from the unchanged `release/release_files.py` at
`b55552dbb7bb16cf5dae0cfc0f2571d14ed8643a`; its SHA-256 is recorded in the file.
The script ran in a synthetic tree containing the listed regular files,
symbolic links, directory links, a dangling link, a loop and a FIFO. The script
itself was copied to `release/release_files.py` so its original root discovery
ran unmodified. Rust tests create that path as inert fixture data.

Expected paths come from original stdout, not the native selection algorithm.
The deliberately unescaped original regular expressions are retained, including
exclusions such as `widgetXgit` and `xgithub/workflows/hidden`. Filesystem
iteration order is not part of the release-copy contract. No release script,
Git mutation, build, network call or device operation ran during capture.
