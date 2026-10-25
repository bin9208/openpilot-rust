#!/usr/bin/env python3
from pathlib import Path
import json
import argparse
import hashlib
import os
import stat
import sys

import pyray as rl

FONT_DIR = Path(__file__).resolve().parent
SELFDRIVE_DIR = FONT_DIR.parents[1]
TRANSLATIONS_DIR = SELFDRIVE_DIR / "ui" / "translations"
LANGUAGES_FILE = TRANSLATIONS_DIR / "languages.json"

GLYPH_PADDING = 6
EXTRA_CHARS = "–‑✓×°§•X⚙✕◀▶✔⌫⇧␣○●↳çêüñ–‑✓×°§•€£¥"
UNIFONT_LANGUAGES = {"th", "zh-CHT", "zh-CHS", "ko", "ja"}
KR_FONTS = {"KaiGenGothicKR-Bold"}


def _hash(path):
  descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
  with os.fdopen(descriptor, "rb") as stream:
    before = os.fstat(stream.fileno())
    if not stat.S_ISREG(before.st_mode) or before.st_size > 512 * 1024 * 1024:
      raise ValueError(f"not a bounded regular font/provider: {path}")
    digest = hashlib.file_digest(stream, "sha256").hexdigest()
    identity = lambda value: (value.st_dev, value.st_ino, value.st_mode, value.st_size, value.st_mtime_ns, value.st_ctime_ns)
    if identity(before) != identity(os.fstat(stream.fileno())) or identity(before) != identity(path.lstat()):
      raise ValueError(f"font/provider changed while reading: {path}")
    return digest


def _inputs():
  root = FONT_DIR.parents[3]
  files = [Path(__file__).resolve(), SELFDRIVE_DIR / "ui/SConscript", LANGUAGES_FILE,
           *FONT_DIR.glob("*.ttf"), *FONT_DIR.glob("*.otf"), *TRANSLATIONS_DIR.glob("*.po")]
  return {str(path.relative_to(root)): _hash(path) for path in sorted(files)}


def _providers():
  files = {Path(sys.executable).resolve()}
  for name, module in list(sys.modules.items()):
    if name.startswith(("pyray", "raylib", "_raylib", "_cffi")) and getattr(module, "__file__", None):
      files.add(Path(module.__file__).resolve())
  maps = Path("/proc/self/maps")
  if maps.exists():
    for row in maps.read_text().splitlines():
      columns = row.split(maxsplit=5)
      if len(columns) == 6 and columns[5].startswith("/") and ".so" in columns[5]:
        files.add(Path(columns[5]).resolve())
  return {"python_version": sys.version, "maps_scope": "sampled" if maps.exists() else "unavailable",
          "files": {str(path): _hash(path) for path in sorted(files)}}


def _output_directory(value):
  path = Path(value)
  repository = FONT_DIR.parents[3]
  if not path.is_absolute() or path.resolve() != path or ".." in path.parts:
    raise ValueError("font output must be absolute and canonical")
  if path == repository or path.is_relative_to(repository) or repository.is_relative_to(path):
    raise ValueError("font output overlaps source")
  path.mkdir(mode=0o700, parents=True, exist_ok=True)
  metadata = path.lstat()
  if any((item.stat().st_dev, item.stat().st_ino) == (metadata.st_dev, metadata.st_ino)
         for item in (repository, FONT_DIR, FONT_DIR.parent, TRANSLATIONS_DIR)):
    raise ValueError("font output aliases source identity")
  if not stat.S_ISDIR(metadata.st_mode) or metadata.st_uid != os.getuid() or stat.S_IMODE(metadata.st_mode) != 0o700:
    raise ValueError("font output must be an owned private directory")
  entries = {file.name for file in path.iterdir()}
  if entries:
    manifest_path = path / "manifest.json"
    _hash(manifest_path)
    if manifest_path.stat().st_size > 4 * 1024 * 1024:
      raise ValueError("previous font manifest exceeds bound")
    old = json.loads(manifest_path.read_bytes())
    assets = old.get("asset_sha256", {})
    if old.get("kind") != "fonts" or not isinstance(assets, dict) or entries - {".ready"} != set(assets) | {"manifest.json"}:
      raise ValueError("font output contains unowned entries")
    if not all(Path(name).name == name and Path(name).suffix in {".fnt", ".png"} for name in assets):
      raise ValueError("previous font roster is invalid")
    if any(_hash(path / name) != digest for name, digest in assets.items()):
      raise ValueError("previous font output changed")
    if ".ready" in entries:
      ready = path / ".ready"
      _hash(ready)
      if ready.stat().st_size != len(b"openpilot-font-stage-v1\n") or ready.read_bytes() != b"openpilot-font-stage-v1\n":
        raise ValueError("previous font ready role differs")
  return path

def _languages():
  if not LANGUAGES_FILE.exists():
    return {}
  with LANGUAGES_FILE.open(encoding="utf-8") as f:
    return json.load(f)


def _char_sets():
  base = set(map(chr, range(32, 127))) | set(EXTRA_CHARS)
  unifont = set(base)

  unifont.update(map(chr, range(0xAC00, 0xD7A4)))
  for language, code in _languages().items():
    unifont.update(language)
    po_path = TRANSLATIONS_DIR / f"app_{code}.po"
    try:
      chars = set(po_path.read_text(encoding="utf-8"))
    except FileNotFoundError:
      continue
    (unifont if code in UNIFONT_LANGUAGES else base).update(chars)

  return tuple(sorted(ord(c) for c in base)), tuple(sorted(ord(c) for c in unifont))


def _glyph_metrics(glyphs, rects, glyph_count: int):
  entries = []
  min_offset_y, max_extent = None, 0
  for idx in range(glyph_count):
    glyph = glyphs[idx]
    rect = rects[idx]
    width = int(round(rect.width))
    height = int(round(rect.height))
    offset_y = int(round(glyph.offsetY))
    min_offset_y = offset_y if min_offset_y is None else min(min_offset_y, offset_y)
    max_extent = max(max_extent, offset_y + height)
    entries.append({
      "id": glyph.value,
      "x": int(round(rect.x)),
      "y": int(round(rect.y)),
      "width": width,
      "height": height,
      "xoffset": int(round(glyph.offsetX)),
      "yoffset": offset_y,
      "xadvance": int(round(glyph.advanceX)),
    })

  if min_offset_y is None:
    raise RuntimeError("No glyphs were generated")

  line_height = int(round(max_extent - min_offset_y))
  base = int(round(max_extent))
  return entries, line_height, base


def _write_bmfont(path: Path, font_size: int, face: str, atlas_name: str, line_height: int, base: int, atlas_size, entries):
  # TODO: why doesn't raylib calculate these metrics correctly?
  if line_height != font_size:
    print("using font size for line height", atlas_name)
    line_height = font_size
  lines = [
    f"info face=\"{face}\" size=-{font_size} bold=0 italic=0 charset=\"\" unicode=1 stretchH=100 smooth=0 aa=1 padding=0,0,0,0 spacing=0,0 outline=0",
    f"common lineHeight={line_height} base={base} scaleW={atlas_size[0]} scaleH={atlas_size[1]} pages=1 packed=0 alphaChnl=0 redChnl=4 greenChnl=4 blueChnl=4",
    f"page id=0 file=\"{atlas_name}\"",
    f"chars count={len(entries)}",
  ]
  for entry in entries:
    lines.append(
      ("char id={id:<4} x={x:<5} y={y:<5} width={width:<5} height={height:<5} " +
       "xoffset={xoffset:<5} yoffset={yoffset:<5} xadvance={xadvance:<5} page=0  chnl=15").format(**entry)
    )
  path.write_text("\n".join(lines) + "\n")


def _process_font(font_path: Path, codepoints: tuple[int, ...], output_dir: Path = FONT_DIR):
  print(f"Processing {font_path.name}...")

  #font_size = {
  #  "unifont.otf": 16,  # unifont is only 16x8 or 16x16 pixels per glyph
  #}.get(font_path.name, 200)
  font_size = 200
  padding = GLYPH_PADDING

  if font_path.name == "unifont.otf":
    font_size = 16
    padding = GLYPH_PADDING

  if font_path.stem in KR_FONTS:
    font_size = 48
    padding = 2
    
  data = font_path.read_bytes()
  file_buf = rl.ffi.new("unsigned char[]", data)
  cp_buffer = rl.ffi.new("int[]", codepoints)
  cp_ptr = rl.ffi.cast("int *", cp_buffer)
  glyph_count = rl.ffi.new("int *", len(codepoints))
  glyphs = rl.load_font_data(
    rl.ffi.cast("unsigned char *", file_buf), len(data), font_size, cp_ptr, len(codepoints),
    rl.FontType.FONT_DEFAULT, glyph_count
  )
  if glyphs == rl.ffi.NULL:
    raise RuntimeError("raylib failed to load font data")

  rects_ptr = rl.ffi.new("Rectangle **")
  image = rl.gen_image_font_atlas(glyphs, rects_ptr, glyph_count[0], font_size, padding, 0)
  if image.width == 0 or image.height == 0:
    raise RuntimeError("raylib returned an empty atlas")

  rects = rects_ptr[0]
  atlas_name = f"{font_path.stem}.png"
  atlas_path = output_dir / atlas_name
  entries, line_height, base = _glyph_metrics(glyphs, rects, glyph_count[0])

  if not rl.export_image(image, atlas_path.as_posix()):
    raise RuntimeError("Failed to export atlas image")

  _write_bmfont(output_dir / f"{font_path.stem}.fnt", font_size, font_path.stem, atlas_name, line_height, base, (image.width, image.height), entries)
  return {"requested": list(codepoints), "returned": [entry["id"] for entry in entries],
          "metrics_sha256": hashlib.sha256(json.dumps(entries, sort_keys=True).encode()).hexdigest()}


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument("--output-dir")
  args = parser.parse_args()
  output = _output_directory(args.output_dir) if args.output_dir else FONT_DIR
  if args.output_dir:
    (output / ".ready").unlink(missing_ok=True)
    inputs, providers = _inputs(), _providers()
  base_cp, unifont_cp = _char_sets()
  fonts = sorted(FONT_DIR.glob("*.ttf")) + sorted(FONT_DIR.glob("*.otf"))
  if args.output_dir:
    expected = {font.stem + suffix for font in fonts if "emoji" not in font.name.lower() for suffix in (".fnt", ".png")}
    # 직전 manifest/hash로 소유를 확인한 오래된 출력만 제거한다.
    for previous in output.iterdir():
      if previous.suffix in {".fnt", ".png"} and previous.name not in expected:
        previous.unlink()
  generated = {}
  for font in fonts:
    if "emoji" in font.name.lower():
      continue
    glyphs = unifont_cp if font.stem.lower().startswith("unifont") or font.stem in KR_FONTS else base_cp
    generated[font.stem] = _process_font(font, glyphs, output)
  if args.output_dir:
    inputs_after, providers_after = _inputs(), _providers()
    if inputs != inputs_after or providers != providers_after:
      raise ValueError("font source or provider changed during generation")
    manifest = {"format_version": 1, "kind": "fonts", "source_sha256": inputs,
                "source_after_sha256": inputs_after, "providers": providers,
                "providers_after": providers_after, "glyphs": generated,
                "asset_sha256": {path.name: _hash(path) for path in sorted(output.iterdir())
                                 if path.suffix in {".fnt", ".png"}}}
    (output / "manifest.json").write_text(json.dumps(manifest, sort_keys=True, indent=2) + "\n")
    (output / ".ready").write_text("openpilot-font-stage-v1\n")
  return 0


if __name__ == "__main__":
  raise SystemExit(main())
