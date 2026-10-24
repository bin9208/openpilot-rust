# Independent source API

This test-first contract now has a native implementation and eight passing
foundation/admission tests. Genuine missing-API and semantic REDs preceded it.
The native paired checker/collector controllers are separate work; these tests
do not retire their Python entrypoints or the retained tombstoned loader.

Module: `openpilot_dev_tools::version_source`

```rust
use std::{collections::BTreeMap, ffi::OsString, path::Path};
use openpilot_log_format::JsonValue;

pub struct Session { /* persistent original helper cache and virtual process cwd */ }
pub struct Error { /* original uncaught failure category and useful diagnostic */ }

impl Session {
    pub fn new(cwd: &Path, environment: &BTreeMap<OsString, OsString>)
        -> Result<Self, Error>;
    pub fn request(&mut self, raw_json: &[u8]) -> Result<JsonValue, Error>;
}
impl Error {
    pub fn kind(&self) -> &str;
}
pub fn sources(repository: &Path) -> Result<BTreeMap<String, String>, Error>;
```

`Error` implements `std::error::Error`, `Display`, and `Debug`. Environment input is
an owned-child execution context; the source session never changes its caller's
environment or cwd. Git uses that session's actual environment and virtual cwd.
`chdir` changes subsequent default Git operations in this session only. Its cached
default results remain cached, exactly as in the original persistent source worker.

One `request` handles one JSON line without requiring its trailing newline. All
original caught exceptions are successful protocol responses of the form
`{"error":"TypeName","logs":[...]}`. It returns `Err` only for an exception the
original worker does not catch, such as an unknown operation's `AssertionError`
or a numeric chdir descriptor outside the C-int range (`OverflowError`).

Empty strings retain API-specific semantics: Git subprocess cwd and `chdir` reject
them, while pathlib/`os.path.join` consumers can use the current directory. Numeric
and bool `chdir` inputs denote real C-int descriptors. The Linux reference keeps an
owned `O_PATH` directory handle and performs actual directory/search-permission
checks, preserving both the initial cwd across external renames and an admitted cwd
after the caller closes its descriptor. It
does not change the parent process cwd or environment.
No input/EOF is handled by the later transport and does not call `request`.
Arbitrary-size integers, nonfinite floats, and lone surrogates are lossless through
the existing generic JSON representation. JSON parse errors and integer digit
limit errors retain their original distinct categories.

Every response contains `logs`, including error responses. Metadata responses
include all original dataclass fields and individually captured property results.
No omitted field, default substitution, float tolerance, or log filtering is added.
Test equality follows original NaN normalization and requires preserved source JSON
kinds, since arbitrary metadata values retain their types and property behavior.

`sources` verifies all five entries in `provenance.json.source_sha256`, returns their
actual hashes, and rejects missing or changed inputs. Retired-source absence has
not been authorized for this foundation; it will require a narrow separately
tested transition at retirement. No hash map supplied by an untrusted manifest may
silently omit a required source.

The implementation must independently port the original version/Git policy and
read actual supplied files and Git repositories. Generic process, JSON, and path
mechanics may be reused. Neither the runtime-version candidate nor an expected
fixture may be consulted for source answers. Later native paired checker and
collector controllers remain separate units with their own process/log contracts.
