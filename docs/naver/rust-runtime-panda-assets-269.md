# Native runtime Panda firmware assets (#269)

The installed pandad binding now requires the F4/H7 applications and bootstubs,
instead of admitting an empty `panda/board/obj` directory. Required files must
be regular, nonempty bytes and cannot be Git LFS pointers. The manager inspection
schema is `{"file":true,"path":...}`; the package sealer includes these paths.
The same gate includes the existing two Xiaoge ONNX assets and two web alert SVGs.

`rust/tools/build_runtime_panda.py` builds an owned snapshot of the unchanged
Panda/safety sources with SCons 4.11.1, PyCryptodome 3.23.0 and the uv.lock-pinned
Arm GNU compiler 13.2.1. The compiler dependency recipe is commaai/dependencies
`15a616d4f08f6b8ecaa9b2390c75d2fe0c0fffb8`; the verified x86_64 build-tool wheel
SHA256 is `a30364fcd198b1eab442c5d57d1555073f289d8d4b3ee8e92209d581ad12f9d1`.
The source default remains `DEV-1de1c7b1f187-DEBUG`, including `ALLOW_DEBUG` and
the existing development certificate. No safety policy or signing format changed.

| File in `panda/board/obj` | Bytes | SHA256 |
| --- | ---: | --- |
| panda.bin.signed | 58160 | 9af58e47888c3d445536ce151218bdec7dec4b9fe4e56d9f5fb5ded3e3470a88 |
| panda_h7.bin.signed | 85928 | db99218a885a7438ed4a8923fd146f2af961abbc533ab937d8c5ee310080ffeb |
| bootstub.panda.bin | 13796 | 52ac7ba40cb123fe52ef1ba609417eb602ad924c9bff2cdc6d1009a464908670 |
| bootstub.panda_h7.bin | 18608 | 0c77563bec36e9836706d7dd18b1fcd1e4f74651bdc5b2d17950daf588cac7ed |

Both independent builds produced identical four files. Source manifests cover 251
unchanged inputs. Public-key checks verify the source SETLEN/VERS=2/raw-SHA1 RSA
signatures; ELF metadata and boot vectors confirm the Cortex-M applications and
bootstubs. Six corrupted body/signature/vector controls were rejected. Staging
rejects changed original/snapshot sources and existing conflicting files before
writing; repeat staging is idempotent. The original Panda license and build
provenance are retained in `rust/bin/licenses/panda` without private key material.

Local evidence: `.omo/evidence/269-panda-firmware/receipt.json`. It contains the
exact build commands/logs, source hashes, expected failure controls, six passing
catalog tests, strict catalog/manager checks and the freshly built manager's
actual inspection. That inspection reports all eight required source files with
their current hashes. Neither firmware nor the supervisor was executed against
hardware. Build/signature validation does not establish flashing, Cortex-M
execution, vehicle acceptance or completion of the entire runtime candidate.
