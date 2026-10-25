"""#557 Actions 전용 실소비 검사. 로컬 기능 실행은 하지 않는다."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import shutil

import pytest


ROOT = Path(__file__).resolve().parents[3]


def digest(path):
  with path.open("rb") as stream:
    return hashlib.file_digest(stream, "sha256").hexdigest()


def frozen(name, expected):
  path = ROOT / "rust/crates/dev-tools/tests/fixtures" / name
  assert digest(path) == expected
  return json.loads(path.read_bytes())


@pytest.fixture
def context():
  assert os.environ.get("GITHUB_ACTIONS") == "true", "Actions-only staging check"
  assert os.environ.get("GITHUB_REPOSITORY") == "bin9208/openpilot-rust"
  assert os.environ.get("RUNNER_ENVIRONMENT") == "github-hosted"
  assert os.environ.get("RUNNER_OS") == "Linux" and os.environ.get("RUNNER_ARCH") == "X64"
  assert Path(os.environ["CI296_SOURCE"]).resolve() == ROOT
  base = Path(os.environ["OPENPILOT_BUILD_ASSET_ROOT"])
  assert base.is_absolute() and base.resolve() == base
  proof = Path(os.environ["CI_JOB_TEMP"]) / "proof/generated-assets-557"
  proof.mkdir(mode=0o700, exist_ok=True)
  before = {str(path.relative_to(ROOT)): digest(path) for path in [
    *sorted((ROOT / "opendbc_repo/opendbc/dbc").rglob("*.dbc")),
    *sorted((ROOT / "openpilot/selfdrive/assets/fonts").glob("Pretendard-*.*")),
  ]}
  outputs = {str(path): digest(path) for name in ("dbc", "fonts")
             for path in (base / name).iterdir() if path.is_file()}
  yield base, proof
  assert {name: digest(ROOT / name) for name in before} == before
  assert {str(path): digest(path) for name in ("dbc", "fonts")
          for path in (base / name).iterdir() if path.is_file()} == outputs


def child(code, *, env=None):
  selected = dict(os.environ)
  if env:
    selected.update(env)
  return subprocess.run([sys.executable, "-c", code], cwd=ROOT, env=selected,
                        capture_output=True, text=True, timeout=60, check=False)


def test_scons_dbc_catalog_matches_frozen_reference(context):
  base, proof = context
  catalog = frozen("dbc-assets/catalog-original.json",
                   "57278d6bf03b3cd7ea0c00238c6082657c37512056adc423079abe80562ea6cc")
  generated = frozen("dbc-generator/generate-all-original.json",
                     "ef1951344a2be6d41be51568432559a55d4271def9249dfa840ba4237f624736")
  actual = {path.name: digest(path) for path in (base / "dbc").glob("*.dbc")}
  assert actual == catalog["radarcan"]["manifest"]["asset_sha256"]
  assert len(actual) == 103
  for name, identity in generated["outputs"].items():
    path = base / "dbc" / (name + ".dbc")
    assert digest(path) == identity["sha256"]
    assert path.stat().st_size == identity["bytes"]
  different = [name for name, expected in catalog["card"]["manifest"]["asset_sha256"].items()
               if actual[name] != expected]
  assert sorted(different) == ["hyundai_canfd_corner_radar_180_generated.dbc",
                               "hyundai_canfd_corner_radar_235_generated.dbc"]
  (proof / "dbc-catalog.json").write_text(json.dumps(actual, sort_keys=True) + "\n")


def test_named_dbc_parser_reads_published_output(context):
  base, proof = context
  assert os.environ["OPENPILOT_DBC_ROOT"] == str(base / "dbc")
  result = child("""
import json, sys
from pathlib import Path
from opendbc.can.dbc import DBC
opened = []
def audit(event, args):
  if event == 'open' and isinstance(args[0], (str, bytes)):
    name = str(args[0])
    if name.endswith('.dbc'): opened.append(name)
sys.addaudithook(audit)
names = ['hyundai_canfd_corner_radar_180_generated',
         'hyundai_canfd_corner_radar_235_generated', 'comma_body']
for name in names:
  parsed = DBC(name)
  assert parsed.msgs
print('GENERATED_DBC_CONSUMER_557_JSON ' + json.dumps(opened))
""")
  (proof / "dbc-consumer.stdout.log").write_text(result.stdout)
  (proof / "dbc-consumer.stderr.log").write_text(result.stderr)
  assert result.returncode == 0, result.stderr
  marker = "GENERATED_DBC_CONSUMER_557_JSON "
  payloads = [line[len(marker):] for line in result.stdout.splitlines() if line.startswith(marker)]
  assert len(payloads) == 1
  opened = json.loads(payloads[0])
  assert len(opened) == 3
  assert all(Path(name).parent == base / "dbc" for name in opened)
  (proof / "dbc-consumer-open.json").write_text(payloads[0] + "\n")


def test_font_output_directory_preserves_bytes_and_glyphs(context, tmp_path):
  base, proof = context
  script = ROOT / "openpilot/selfdrive/assets/fonts/process.py"
  second = tmp_path / "fonts-comparison"
  subprocess.run([sys.executable, str(script), "--output-dir", str(second)],
                 cwd=ROOT, check=True, timeout=180,
                 env={name: os.environ[name] for name in ("PATH", "PYTHONPATH") if name in os.environ})
  first = base / "fonts"
  outputs = lambda directory: {path.name: digest(path) for path in directory.iterdir()
                               if path.suffix in {".fnt", ".png"}}
  assert outputs(first) == outputs(second)
  original = json.loads((first / "manifest.json").read_bytes())
  repeated = json.loads((second / "manifest.json").read_bytes())
  assert original["glyphs"] == repeated["glyphs"]
  assert original["source_sha256"] == repeated["source_sha256"]
  assert original["providers"] == repeated["providers"]
  for path in first.glob("*.fnt"):
    pages = [line for line in path.read_text().splitlines() if line.startswith("page id=0 file=")]
    assert pages == [f'page id=0 file="{path.stem}.png"']
    assert (first / (path.stem + ".png")).is_file()
  (proof / "font-current-comparison.json").write_text(json.dumps({
    "outputs": outputs(first), "glyphs": original["glyphs"],
    "historical_font_equivalence": False,
  }, sort_keys=True) + "\n")


def test_explicit_roots_and_partial_publication_are_rejected(context, tmp_path):
  base, _ = context
  code = "from opendbc.can.dbc import DBC; DBC('comma_body')"
  for value, reason in [(str(tmp_path / "absent"), "DBC root differs from build output"),
                        (str(ROOT / "opendbc_repo/opendbc/dbc"), "DBC root differs from build output")]:
    result = child(code, env={"OPENPILOT_DBC_ROOT": value})
    assert result.returncode != 0 and reason in result.stderr, result.stderr
  negative = tmp_path / "partial"
  negative.mkdir(mode=0o700)
  (negative / "dbc").mkdir(mode=0o700)
  result = child(code, env={"OPENPILOT_BUILD_ASSET_ROOT": str(negative),
                           "OPENPILOT_DBC_ROOT": str(negative / "dbc")})
  assert result.returncode != 0 and "DBC stage is not ready" in result.stderr, result.stderr
  alias = tmp_path / "alias"
  alias.symlink_to(base, target_is_directory=True)
  result = child(code, env={"OPENPILOT_BUILD_ASSET_ROOT": str(alias),
                           "OPENPILOT_DBC_ROOT": str(alias / "dbc")})
  assert result.returncode != 0 and "canonical" in result.stderr, result.stderr
  result = child(code, env={"OPENPILOT_BUILD_ASSET_ROOT": str(ROOT),
                           "OPENPILOT_DBC_ROOT": str(ROOT / "dbc")})
  assert result.returncode != 0 and "overlaps source" in result.stderr, result.stderr
  # 유효한 common context에서 font 선택만 바꾼 독립 거부다.
  result = child("import openpilot.system.ui.lib.application",
                 env={"OPENPILOT_FONT_ROOT": str(tmp_path / "wrong-fonts")})
  assert result.returncode != 0 and "font root differs from build output" in result.stderr, result.stderr
  # 실제 source TTF가 있어도 명시 staged mode의 누락 atlas는 fallback하지 않는다.
  missing = tmp_path / "missing-atlas.fnt"
  shutil.copyfile(ROOT / "openpilot/selfdrive/assets/fonts/Inter-Medium.ttf", missing.with_suffix(".ttf"))
  result = child("""
import os
from pathlib import Path
from openpilot.system.ui.lib.application import gui_app
gui_app._resolve_font_path(Path(os.environ['STAGING_MISSING_FONT_CONTROL']))
""", env={"STAGING_MISSING_FONT_CONTROL": str(missing)})
  assert result.returncode != 0 and "staged font is missing" in result.stderr, result.stderr
  for index, mutation in enumerate(("role", "source", "extra")):
    copied = tmp_path / f"dbc-control-{index}"
    copied.mkdir(mode=0o700)
    shutil.copytree(base / "dbc", copied / "dbc")
    environment = {"OPENPILOT_BUILD_ASSET_ROOT": str(copied), "OPENPILOT_DBC_ROOT": str(copied / "dbc")}
    positive = child(code, env=environment)
    assert positive.returncode == 0, positive.stderr
    manifest_path = copied / "dbc/manifest.json"
    manifest = json.loads(manifest_path.read_bytes())
    if mutation == "role":
      manifest["kind"] = "card"
    elif mutation == "source":
      manifest["source_sha256"] = {}
    else:
      (copied / "dbc/unexpected.dbc").write_text("unexpected input\n")
    manifest_path.write_text(json.dumps(manifest))
    result = child(code, env=environment)
    reason = "DBC output roster or bytes changed" if mutation == "extra" else "DBC source or role differs from stage"
    assert result.returncode != 0 and reason in result.stderr, result.stderr


def test_gui_font_loader_uses_published_atlas(context):
  base, proof = context
  assert os.environ["OPENPILOT_FONT_ROOT"] == str(base / "fonts")
  # 실제 GUI/font loader를 호출한다. Xvfb/GL 미준비는 skip으로 숨기지 않는다.
  result = child("""
import json
from pathlib import Path
from openpilot.system.ui.lib.application import FONT_DIR, FontWeight, gui_app
import pyray as rl
opened = []
real_load = rl.load_font
def observed(path):
  result = real_load(path)
  opened.append(path)
  return result
rl.load_font = observed
try:
  gui_app.init_window('generated-assets-557', font_weights=tuple(FontWeight))
  assert all(gui_app._font_texture_valid(font) for font in gui_app._fonts.values())
  roots = [str(gui_app._resolve_font_path(FONT_DIR / weight)) for weight in FontWeight]
  assert sorted(roots) == sorted(opened)
  print('FONT_ROOTS=' + json.dumps(roots))
finally:
  gui_app.close()
  rl.load_font = real_load
""")
  assert result.returncode == 0, result.stderr
  rows = [line.removeprefix("FONT_ROOTS=") for line in result.stdout.splitlines()
          if line.startswith("FONT_ROOTS=")]
  assert len(rows) == 1
  assert all(Path(name).parent == base / "fonts" and Path(name).suffix == ".fnt"
             for name in json.loads(rows[0]))
  (proof / "gui-current-fonts.txt").write_text(result.stdout + result.stderr)


def test_jotpluggler_shared_resolver_reads_current_variants(context):
  base, proof = context
  binary = Path(os.environ["GENERATED_ASSETS_JOT_LEAF"])
  build = Path(os.environ["CI_JOB_TEMP"]) / "build"
  assert binary.is_absolute() and binary.resolve() == binary and binary.is_relative_to(build)
  with binary.open("rb") as stream:
    assert stream.read(4) == b"\x7fELF"
  before = digest(binary)
  result = subprocess.run([str(binary), str(ROOT), str(base / "dbc"), str(proof / "jot-controls")],
                          cwd=ROOT, capture_output=True, text=True, timeout=30, check=False)
  assert result.returncode == 0, result.stderr
  expected = [str(base / "dbc" / (stem + ".dbc")) for stem in [
    "hyundai_canfd_corner_radar_180_generated", "hyundai_canfd_corner_radar_235_generated"]]
  assert result.stdout.splitlines() == expected + [
    "JOT_DBC_PATHS_557 controls=4 actual_variants=2 foreign_preserved=true"]
  assert before == digest(binary)
  (proof / "jot-current-resolver.json").write_text(json.dumps({
    "leaf_sha256": before, "stdout": result.stdout, "stderr": result.stderr,
    "actual_selected_sha256": {name: digest(Path(name)) for name in expected},
    "gui_execution": False, "traffic_execution": False,
  }, sort_keys=True) + "\n")
