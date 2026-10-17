`nonunicode.json` contains twelve independent original Python captures: model
and training commands with one invalid UTF-8 byte in each of root, target,
selected libcxx filename, selected bridge-header directory, raylib, and evidence.
Original source SHA-256 values are embedded. Compiler peers record `os.fsencode`
argument bytes, create controlled products and a capture, and leave execution
inside owned scratch. `BASE` normalizes the temporary ancestor while preserving
every byte beneath it. Receipts are the exact Python `json.dumps(indent=2)` text,
including surrogateescape encodings such as `\udcff`.

`raw-parser.json` captures thirteen additional original UI parser cases before
any glob or subprocess work. Cases exercise non-UTF-8 values and option names,
inline values, negative-looking values, unknown space-containing values,
recognized short-help clusters, explicit help values, and deferred unknowns.
The valid Unicode marker path is a collision-control case for the lossless
native wrapper. The parser source is the model script hashed above.

Native regressions also execute a raw-argv shell subprocess and a complete CLI
with a non-UTF-8 working root and controlled compiler/probe executables, checking
actual child argument bytes and exact receipt bytes. No compiler, GPU, display,
network, or device work is performed by those regressions.

`glob.json` records eighteen additional original captures under pinned CPython
3.12.15: model/training crossed with non-directory prefix entries, dangling
prefix links, prefix links to files, an intermediate regular file, dangling or
valid-symlink fixed-final headers, header directories, and dangling or directory
wildcard libcxx entries. Original source hashes and Python version are embedded.
Only ENOTDIR for the fixed header lookup is newly skipped. A dangling final
header remains a candidate and fails its later stat, as does a dangling wildcard
libcxx entry. For two header aliases with identical target mtimes, the test
adapts only that include slot to live unsorted directory order, preserving
Python max's first-candidate rule. Other command positions and receipts remain
frozen; non-tied artifact mtimes are explicit.
