# Original Carrot Web server (#225)

Source: `openpilot/selfdrive/carrot/carrot_server.py` and
`openpilot/selfdrive/carrot/server/`, retaining the repository's MIT provenance.
The bounded settings/state/static composition has local host comparison evidence;
it is not a complete server or a normal-startup candidate. No production
selection is changed.

The current slice contains config/state migration, ordered settings group/menu
and brand/gap views, integer-mtime cache, native Params and unregistered-key
fallback, unit-index/favorites/web-settings persistence, bulk/set/history/
fingerprint and profile preview/apply routes, intro state/presets, shared static
assets/manifest/precompression and actual index bootstrap composition. The
listener and shutdown are exercised through owned host fixtures. JSON/numeric/text behavior reuses the public pure
Carrot Navi JSON value (native feature disabled), logmessaged/runtime-core,
beepd's std::stoi equivalent and calibrationd's std::stof equivalent. Float32
Params write bytes use the source standard-library `%f` behavior through one
bounded libc snprintf call. No new CXX or shared numeric framework is added.
TIME conversion compiles the existing pure UI datetime parser directly and
retains its source dependency. Safe backup/intro reads use complete typed
conversion; ordinary registered numeric reads retain the original fatal C++
fault boundary. Registered native writes discard filesystem status after
validation, matching Cython; successful responses can therefore accompany
partial write failures. Unregistered atomic-writer errors still propagate.

Focused comparison recipe (also suitable for an isolated CI job):

```sh
cargo fmt --manifest-path rust/Cargo.toml -p openpilot-carrot-server -- --check
cargo clippy --manifest-path rust/Cargo.toml -p openpilot-carrot-server --all-targets --locked -- -D warnings
cargo test --manifest-path rust/Cargo.toml -p openpilot-carrot-server --locked
cargo build --manifest-path rust/Cargo.toml -p openpilot-carrot-server --bins --examples --locked
PYTHONPATH=rust/tools:. python rust/tools/carrot_server_policy.py --binary rust/target/debug/examples/carrot-server-policy --output "$RUNNER_TEMP/carrot-server/policy"
PYTHONPATH="$RUNNER_TEMP/carrot-server/python:rust/tools:." python rust/tools/carrot_server_http.py --binary rust/target/debug/examples/carrot-server-http-slice --binding "$CARROT_SERVER_PARAMS_BINDING" --output "$RUNNER_TEMP/carrot-server/http"
```

The HTTP helper requires the existing original Params/msgq bindings and aiohttp
environment. Reuse preserved bindings or prepare them with the repository's
existing binding helpers. The Params binding path is passed explicitly. It
composes the actual original settings/state/Params/profile/intro handlers and static middleware/handler
without whole-app registration or background services, using separate real Params roots.
Both peers own loopback listeners and temporary settings/state directories.
Policy and HTTP source/native captures are preserved separately, including raw
bodies/headers and stderr. Dynamic Date is captured but excluded from the
header comparison; selected response fields and body bytes are exact. Real
profile creation checks UUID/UTC properties and four calls to an owned wrapper
that delegates to real Git. Profile JSON is reserialized after normalizing
generated UUID/time fields; original wire/state bytes and Git markers remain captured.

Local evidence on 2026-10-08 is under `.omo/evidence/carrot-server-225-resume/`:
the retained five filesystem-backed regression tests and 41 policy cases,
66 supported Params/backup comparisons, six owned SIGABRT subprocess pairs,
and 103 actual HTTP observations plus graceful stop. The current HTTP gate
includes full bootstrap dependencies, missing-index/no-intro-write ordering,
profile rejection before Git calls, actual Git metadata, validated profile
apply/history, and Latin-1 including C1 byte 0x80. Backup comparison preserves
the exact 208-key/value mapping; original unordered C++ iteration is retained
as a raw artifact rather than treated as a canonical ordering. Separate owner
receipts cover 702 web-settings cases, 73 profile/restore/history service cases,
61 static cases plus transport/FFI boundaries, and 70 intro cases plus two
Params-unavailable pairs. These are scoped receipts, not full-app startup.
Package build, formatting and Clippy passed with incremental compilation off,
two jobs and debug info disabled. Native bindings and the existing Python
oracle environment were reused; no dependency installation was performed.
Space was checked before each build, after recovery to at least 35 GiB.

The complete 27-family server remains unfinished. Remaining families include
bluetooth, carrot_navi, stream, support_terminal, ws, settings snapshots,
Params backup download/multipart/JSON/QR restore and QR provider lifecycle,
setting_popular_values, ssh_keys, cars, system,
terminal, dashcam, egpu_model, screenrecord, tools, xiaoge, mapbox_tokens,
youtube_live, vision_test, vision_diag and web_sound. Unexported next-wave QR
and cars sources are outside this checkpoint. Full malformed catalog iteration,
non-UTF-8/Latin-1/ASCII request codecs, request compression, real live-broker
engagement, executable runtime-root discovery and complete CLI failure cases
remain explicit gaps.

Original app startup still requires broker and serialized msgq polling, raw and
camera hubs, heartbeat, git status, auto update, popular-value upload, periodic
trim. Static precompression now shares the application asset locks; the other
jobs are not replaced by fake success
state in this slice. Full-app fixtures must remap every HTTP/OS/git/update/media
target; binding loopback does not isolate those effects. No vehicle, NAS, LAN,
recipient, system-date, reboot or git-update operation is authorized here.

Parent owns full workflow/catalog/inventory integration. Exact-stack hosted
host/ARM results, complete 27-family coverage, normal startup/upload, executable
and asset packaging and device acceptance remain separate gates. The source
frontend assets are unchanged.
