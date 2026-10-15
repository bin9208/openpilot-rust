# Original JPEG sanitizer checker characterization

`original.json` captures `rust/tools/check_jpeg_sanitizers.py` unchanged at SHA256
`ccd19b4cae11294931a8160067740bf8c13ef91fcff715a0376c3cafd07c7b66`.
The original Python 3.12 entry point was executed in temporary filesystems with
controlled subprocess/disk responses. The records pin command arguments, selected
environment overrides, command order, artifacts present before rejection, and all
success output. `$ROOT` is the temporary fixture and `$RUST` the repository Rust
workspace. Capture script is private scratch under
`.analysis/scratch/2026-10-15-jpeg-check/capture.py`.

The fifteen independent cases cover success; one byte below the original 26 GiB
admission threshold; nonzero Cargo; malformed Cargo JSON; zero/two executable
artifacts; failed libasan lookup; failed ownership executable; zero/two codec
CMake caches; missing full codec sanitizer flags; process spawn failure; false executable metadata ignored alongside a valid artifact; Unicode record line separators; and rejection of a non-string executable after libasan lookup.
The non-string argv characterization includes the attempted Python subprocess call; the native runner accepts only string argv, so its assertion pins the two preceding calls and rejection before spawn.

Native result JSON is compared as data; process logs remain byte-exact after
original text-mode newline translation. Build stdout and stderr stay separate;
`run.log` concatenates stdout then stderr, as in the original.

The native QA runner introduces bounded build/compiler execution and process-group
cleanup; the ownership executable retains the original 60-second deadline.
The production checker must also be validated with the real instrumented codec
and ownership test; synthetic command fixtures do not establish sanitizer coverage.
