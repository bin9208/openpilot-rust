# Original Carrot Web server (#225)

Source: `openpilot/selfdrive/carrot/carrot_server.py` and
`openpilot/selfdrive/carrot/server/`, retaining the repository's MIT provenance.
The bounded first implementation slice has local host comparison evidence;
it is not a complete server or a normal-startup candidate. No production
selection is changed.

The current slice contains config/state migration, ordered settings group/menu
and brand/gap views, integer-mtime cache, native Params and unregistered-key
fallback, an HTTP settings route, a preliminary static-file boundary and owned
listener/graceful shutdown. JSON/numeric/text behavior reuses the public pure
Carrot Navi JSON value (native feature disabled), logmessaged/runtime-core,
beepd's std::stoi equivalent and calibrationd's std::stof equivalent. Float32
Params write bytes use the source standard-library `%f` behavior through one
bounded libc snprintf call. No new CXX or shared numeric framework is added.

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
composes the actual original settings handler and static middleware/handler
without whole-app registration or background services, using separate real Params roots.
Both peers own loopback listeners and temporary settings/state directories.
Policy and HTTP source/native captures are preserved separately, including raw
bodies/headers and stderr. Dynamic Date is captured but excluded from the
settings header comparison; selected response fields and body bytes are exact.

Local evidence on 2026-10-08 is under `.omo/evidence/carrot-server-225-resume/`:
five filesystem-backed regression tests, 41 exact policy cases (including the
shipped catalog, malformed roots and non-finite coercion), and 17 exact HTTP
observations plus graceful stop passed. HTTP covers live gap/brand changes,
GET/HEAD/method handling, static bytes/cache/gzip/path containment, missing and
malformed settings, and recovery. It caught and corrected the private beepd
module call, missing Last-Modified/Accept-Ranges and source missing-asset response.
Package build, formatting and Clippy passed with incremental compilation off,
two jobs and debug info disabled. Native bindings and the existing Python
oracle environment were reused; no dependency installation was performed.
Space was checked before each build, after recovery to at least 35 GiB.

All 27 route families remain incomplete: bluetooth, static (index/bootstrap,
manifest recovery, complete FileResponse/range/conditional/compression policy),
intro, carrot_navi, stream, support_terminal, ws, settings (snapshot/unit index),
params (bulk/set/history/fingerprint/backup/restore/QR), setting_favorites,
setting_popular_values, setting_profiles, web_settings, ssh_keys, cars, system,
terminal, dashcam, egpu_model, screenrecord, tools, xiaoge, mapbox_tokens,
youtube_live, vision_test, vision_diag and web_sound. Registered TIME coercion,
full malformed catalog iteration cases, complete static precompression/locking,
and exact source I/O/CLI failure text also remain to be closed.

Original app startup still requires broker and serialized msgq polling, raw and
camera hubs, heartbeat, git status, auto update, popular-value upload, periodic
trim and static precompression. These jobs are not replaced by fake success
state in this slice. Full-app fixtures must remap every HTTP/OS/git/update/media
target; binding loopback does not isolate those effects. No vehicle, NAS, LAN,
recipient, system-date, reboot or git-update operation is authorized here.

Parent owns full workflow/catalog/inventory integration. Exact-stack hosted
host/ARM results, complete 27-family coverage, normal startup/upload, executable
and asset packaging and device acceptance remain separate gates. The source
frontend assets are unchanged.
