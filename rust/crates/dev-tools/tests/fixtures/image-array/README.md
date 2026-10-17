# Original image array captures

Synthetic, privately generated images, captured with Pillow 12.3.0 and NumPy
2.5.3 on x86_64 Linux (signed 64-bit `int`). No vehicle data is present.
`original.json` covers the 93 decodable combinations from the initial 182
mode/format save/load matrix. `edges.json` and `tiff-types.json` cover hand-built
TIFF planes, palettes, signed samples, SGI RLE, and IM palettes. `legacy.json`
covers BMP RLE4/8, legacy ICNS raw/RLE/masks, and all 67 rawmodes advertised by
Pillow's IM plugin. `legacy-extra.json` records palette/mode edge cases.

Each accepted case pins the encoded input SHA256, loaded Pillow mode, NumPy
shape, and SHA256 of **every** flattened little-endian i64 sample. Hashing avoids
committing an 89 MiB JSON expansion of the original ICNS arrays. Error cases
assert rejection, including advertised rawmodes that Pillow cannot decode.
`jpeg-errors.json` records valid, malformed, truncated and trailing-data JPEG
acceptance. The original captures and reproducible capture scripts are in the
ignored `.analysis/scratch/2026-10-16-image-array/` working evidence directory.

Reference plugin and unpack behavior: Python-Pillow/Pillow tag `12.3.0`,
`PIL/{BmpImagePlugin,IcnsImagePlugin,ImImagePlugin,Image}.py` and
`src/libImaging/{BitDecode,Unpack}.c`. License and original copyright notices are
retained at `src/image_array/PILLOW-LICENSE`.

`remaining-formats.json`, `extended-formats.json`, and `last-families.json`
add 112 independently captured cases for MCIDAS, Pixar, FTEX, FITS raw/gzip,
PSD composite raw/PackBits, first-frame FLI/FLC chunks, CUR masks, BLP textures,
PhotoCD orientations, IPTC component images and Ghostscript EPS behavior.
`native-formats.json`, `native-variants.json`, and `native-errors.json` cover the
optional pinned AVIF/JPEG2000 path, including indexed JP2, signed/varied bit
precision, raw codestreams, tiles, AVIF chroma/range variants and truncation.
Rows marked `save_error` produced no input and are never counted as decoder
rejections. `compression-tiff.json`, `tiff-openinfo.json` and `tiff-malformed.json`
are included in the production native-SDK suite.

`review-regressions.json` contains 85 additional independent Pillow/NumPy
captures for TGA/CUR probe ambiguity (both colour-map-origin bytes and image
orientations), real CUR truncation errors, EPS header/trailer BoundingBox and
ImageData precedence and rendered-mode admission, and FITS byte-card comments
containing invalid UTF-8 or a multibyte character across the keyword boundary,
short data-record offsets, consecutive header-unit scanning, compressed one-axis
storage and checked storage-offset arithmetic.
EPS captures use the same native Ghostscript dependency as Pillow. Expected
arrays were captured before the Rust repairs; they are not Rust-generated.
