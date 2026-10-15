# dev-md web assets and setting guides

The final upstream web delta is applied from
`e8e152dbcfd9574a1791ed12026d1880e87689b3`, preserving native branch changes.
Of 59 upstream changed web paths, five overlap native changes: three translation
files merged cleanly and generated outputs were rebuilt from merged sources.
The catalog retains the native JetlinkMode entry and its guarded activation
semantics; it does not authorize any new model or enable Jetlink by default.

Web asset build passed; all **834 browser-module tests** passed on 2026-10-15.
The first run had one environment failure because the reference projection test
invoked an unavailable `python` command. Setting PYTHON to the existing project
venv and rerunning the suite passed without changing the fixture or threshold.
Dependencies came from the unchanged package lock in an ignored scratch staging
area. No site or remote server was published.

Twenty paired Korean/English guides were merged from the source setting changes,
retaining native edits. Two settings-summary overlaps were reconciled to keep the
native VW gain correction and add the final Toyota/Lexus fixed-gain description.
A source documentation inconsistency was corrected in both DM guides: Carrot
Vision works with the external HUD, as the source/native manager predicates and
web tests establish. Driver monitoring and web video remain independent.

The native check-user-docs command passed for the settings/DM guide change set.
The original Wiki generator/validator staged 591 pages and its 25 focused tests
passed. No remote Wiki was changed. Six DM enable/Carrot Vision manual regions
were reviewed in Korean, English and Chinese; the reusable manual-only patch is
`docs/naver/dev-md-wiki-manual.patch`. It corrects the stale HUD exclusion and
explains persistent enable versus temporary CANCEL state, restore exclusions and
explicit reset behavior. Generator-owned AUTO blocks were regenerated, not
hand-edited. Staged pages and reports are private scratch evidence, not a claim
that production Wiki publishing is enabled on this independent repository.

Pending integration remains explicit: native onroad/cluster UI, native YouTube
service/encoder removal, per-upload scope/screenrecord backend, signal producers
and model/recovery support still require completion. Passing static web tests
alone does not establish those native behaviors or complete-runtime acceptance.
Detailed logs: `.analysis/scratch/2026-10-15-dev-md-web/`.
