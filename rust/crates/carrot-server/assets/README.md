# Vision diagnostic defaults

`vision_diag_defaults.rs` is the maintained native declaration of the upload
defaults. The runtime retains the existing obfuscation/decoding contract.

Provenance: `openpilot/selfdrive/carrot/server/config.py`, constants
`VISION_DIAG_DEFAULT_DISCORD_WEBHOOK` and `VISION_DIAG_DEFAULT_DISCORD_KEY`,
at `73a9e01709245de97ffb18b1a079b019bc3f9fd9`. Extracted through the existing
Rust build script without changing either value. Original repository MIT
license and copyright apply. Builds no longer require the Python source.
