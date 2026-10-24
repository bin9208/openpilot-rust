# Startup adapter ASan orchestration oracle

`original.json` freezes 74 independent original safety/resource driver cases,
using pinned CPython 3.12.15. It records complete raw invocation-prefix logs,
ordered argv and selected environment bytes, disk checks, stdout, and partial
artifacts. Exact original source SHA-256 and the source line ranges for
`Context`, `require`, and `adapter_asan` are embedded in the fixture. Controlled
captures execute those unchanged AST nodes and each unchanged standalone body;
unused PIL/Xlib imports are excluded only in this controlled capture harness.

Actual original validation executes both complete standalone files and imports
with verified python-xlib 0.33/six 1.17 wheels unpacked only into private scratch
PYTHONPATH. The primary Python environment is read-only. Dependency hashes,
original/CXX/runtime/raylib source identities and actual compiler/probe receipts
are under `.analysis/scratch/2026-10-15-startup-adapter-sanitizers/`.

Cases cover missing artifacts, unsorted equal-mtime selection, non-directory
prefixes, 26 actual non-root permission cases with valid alternate headers,
Safety output/library/target/raylib symlink-loop resolution failures, dangling headers, directory-shaped rlibs, every compiler/probe failure,
spawn failure, original 180-second timeout with partial log bytes, relative
paths and raw Unix bytes in root/target/output/raylib. Original missing screenshot
behavior is retained: the drivers do not independently stat `asan.png` before
writing success (the actual CXX program owns its assertions). Equal-mtime tests
assert the original candidate multiset and fixed command positions, then adapt
only the tied `--extern` slot and its invocation prefix to actual unsorted
filesystem enumeration. Production selection retains first-enumerated maxima.

Safety preserves its strict initial `free > 25.25 GiB` check before output
creation, output resolve/mkdir before library, target and raylib resolution, canonical paths, OFFSCREEN=1 and the Params parser probe. Resources
preserves uncanonicalized pathlib-style paths and inherited OFFSCREEN, and
writes each compiler's disk receipt before enforcing `free >= 26 GiB`.

The native safety CLI additionally checks the repository's 25 GiB reserve plus
1 GiB estimated growth before every compiler. This disclosed policy is separate
from the original-controlled `check` contract. A dedicated regression verifies
all three compiler guards, inclusive equality, and that refusal starts no child
and creates no invocation log or false success receipt. Original 180-second
command deadlines remain unchanged; owned process-group cleanup also terminates
descendants on timeout or failure. External raylib and Rust CXX runtime remain
uninstrumented; graphics-driver leak detection remains disabled.

The shared selector scans every literal and wildcard directory component, skips
selector OSError branches, preserves unsorted DFS order and retains final
dangling entries for the caller’s strict stat. Six additional complete original
model/training captures in `../ui-adapter-sanitizers/permissions.json` verify
those older consumers select valid alternates under search-only, read-only and
no-access prefixes. Permission tests require non-root execution.

Complete original and native safety/resource runs both passed with actual CXX
ASan/UBSan compilation and probes on owned private Xvfb displays. Both families'
bridge executable, runtime archive and PNG were byte-identical; the safety
Params executable was also byte-identical. Complete result receipts and process
stdout/stderr matched byte-for-byte. All invocation/program logs matched after
replacing only each family's output directory. Resource disk receipts retain
actual measured free bytes (which naturally differ between runs) and identical
1,073,741,824-byte growth estimates; those free values were not normalized.
Native Xvfb was stopped and reaped. The detailed comparison and artifact hashes
are in scratch `actual-comparison.json` alongside full original/native records.
