# External media notices for issue 269

The media stager retains 33 complete source notice files, including the pinned
raylib root license and notices from bundled headers. The ARM wheel SHA256 is
`d9b9bebb39167442bacf627958d57ae95e452550ced1b68337cbf2a73375856a`;
the matching comma raylib source revision is
`caa64e15ac20a47804a8048029c921ac091fef12`, pinned by its dependency build.
The plugin links only the comma raylib archive and external system graphics
primitives. Mesa/LLVM/Python wheel providers are not packaged. Retaining notices
conservatively does not assert every optional header/backend is enabled.

The original FreeSans font matches the Pygame 2.6.1 tagged binary exactly:
`bf9251269f11e4b35582a99376096deee3d0c4973a459cbd97b0bebc01f440ad`.
Its embedded copyright, GPL grant, source URL and revision are retained. The
embedded grant does not specify a GPL version; general GNU GPL2/3 texts are
retained without assigning a version or inferring a modern font exception.
The font is not relabeled as Pygame's library LGPL. A matching preferred
FontForge SFD source was not identified in the wheel/tag metadata.

The missing raylib root license was observed before the narrow stager loop.
Actual staging and an idempotent rerun preserve all 39 existing library/font
paths byte-for-byte and retain 54 SDL/raylib/FFmpeg/vpx notice/provenance files.
The receipt with primary-source URLs, exact hashes and commands is
`.omo/evidence/269-media-license-audit/receipt.json`. This is scoped retention
evidence, not a claim that the unresolved historical font source is identified.
