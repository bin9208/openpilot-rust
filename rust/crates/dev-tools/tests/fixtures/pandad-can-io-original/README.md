# Synthetic Panda CAN-I/O original observations

These are synthetic request/response observations, not vehicle captures. The
independent C++ responses remain the oracle; historical Rust responses are
additional regression data. No original Python source or ELF is packaged.
The adjacent LICENSE preserves original tooling licensing.

Repository: bin9208/openpilot-rust. Complete capture: Actions run 39640746985,
artifact 12365930346, ZIP SHA256
`1efa0edb524a893ebda1968a4556c12b3636dfd173ff6301bdc121f6f5e63c94`.
Original capture source: `08f21d4aea8b1be4c59925e2b67a302eda4db623`;
completion source: `282fc87979c16ebac8dc368d9e3b0baf87c8e8f1`.
The complete original manifest SHA256 is
`23534076aec940b91d282c8298609d4b5db46974ab394d0ea05a7c4f3be81531`.
It remains byte-identical in historical-capture-manifest.json, including all
78 original seals, 29 admissions and 12 fresh parser probes. The completed
capture reused successful regular observations from run 39636494184 and
resumed raw native/runner observations; no original corpus rerun is needed.

Two original files were executables: raw-native-elf and raw-runner-elf.
Their original seals remain in the historical manifest and the adapter's
explicit omitted_executable_seals_b64 map. Their bytes are deliberately absent.
The adapter capture-manifest.json seals exactly the remaining 76 unchanged
observation files. Its SHA256 is
`4a4f41552c597d39dcd45ba18b77ff64a1fd584ddcf63981c7523a5ecf4dbd83`.
The native verifier reconstructs and checks the full historical 78-seal roster
from the 76 packaged seals plus the two explicitly omitted executable seals.
This is an explicit data adapter, not a claim that ELF bytes were preserved.

The deterministic gzip/tar has 79 regular members (76 observations, two
manifests, historical-builder.json), 19,299,707 compressed bytes and 87,691,783
unpacked file bytes. Tar names sort, metadata timestamps/IDs are zero, modes
0644; gzip timestamp is zero. Blob SHA256:
`b423db574149629d8b835c4ba563d994452e42e7bd75cd0aba556bf0113262c3`.
fixture.json SHA256:
`cf22be4593fb82f7ce75975bc7ce0c0b8b4bdb407385be52de64ebef5d11a650`.

historical-builder.json records the borrowed C++ build from accepted source
artifact 12358686227, run 39625704015. Its 14 source pins are verified against
the current checkout at staging and immediately before publication. The
historical C++ ELF hash is
`fadf6c458826866584eb66d5cb19794a36aede895d5bbb3f588d2368fecd9ffc`.
Current C++ builds must produce their own source manifest and binary receipt;
never relabel historical-builder.json as a current build receipt.

The native stager verifies bounded expansion, exact membership, every size and
hash, all original seal metadata, safe regular members and current C++ pins.
It rejects links, changed inputs and foreign output, then publishes atomically
without replacement after reserving 25 GiB plus bounded growth. It executes no
captured program and requires no Python or artifact download.

Registration and transition gate (parent-owned): register module
pandad_can_io_fixture under pandad-can-io-native; add bin
stage-pandad-can-io-reference calling its cli(), and integration test
pandad_can_io_fixture. Stage once into an absent directory and load its
environment.env. Keep current C++ source/native build receipts separate.
Update the reference raw-runner setup to copy the current system env executable
rather than the excluded historical ELF. Reference tests must authenticate
this adapter's original manifest and omitted seals explicitly, preserving all
29 admissions, 12 parser probes, full source requests/responses and raw argv.
Run fixture controls plus current CAN-I/O controls/reference comparisons once
in the combined hosted gate. Reuse successful unchanged protocol/device
corpora. No original Python execution belongs in this maintained gate.

After accepted predelete evidence, delete the original caller and shared module
only when both protocol and CAN-I/O callers are native. Run an explicit
postdelete mode requiring actual checkout absence and private-root absence;
retain full normal/MAXOUT and raw-path comparisons and CLI failure effects.
Do not silently skip a missing fixture, restore original sources, or classify
registration or staging alone as functional retirement completion.
