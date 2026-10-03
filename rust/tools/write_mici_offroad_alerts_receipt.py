# /// script
# dependencies = []
# ///
# How to run: python rust/tools/write_mici_offroad_alerts_receipt.py EVIDENCE_DIR
import hashlib
import json
from pathlib import Path
import subprocess
import sys

root = Path(__file__).resolve().parents[2]
evidence = Path(sys.argv[1]).resolve()
results = json.loads((evidence / 'final/results.json').read_text())
assert len(results) == 26
assert all(row['different_pixels'] == 0 and row['max_channel_difference'] == 0 for row in results)
source_paths = [
  'openpilot/selfdrive/ui/mici/layouts/offroad_alerts.py',
  'openpilot/selfdrive/selfdrived/alerts_offroad.json',
  'openpilot/system/ui/widgets/label.py',
  'openpilot/system/ui/widgets/scroller.py',
  'openpilot/system/ui/lib/scroll_panel2.py',
]
native_paths = [str(path.relative_to(root)) for path in sorted((root / 'rust/crates/ui-application/src/mici/layouts/offroad_alerts').glob('*.rs'))]
native_paths += [
  'rust/crates/ui-application/examples/mici_offroad_alerts.rs',
  'rust/tools/check_ui_mici_offroad_alerts.py',
  'rust/tools/ui_application_qa/mici_offroad_alerts_source.py',
  'rust/tools/write_mici_offroad_alerts_receipt.py',
]
assets = sorted((root / 'openpilot/selfdrive/assets/icons_mici/offroad_alerts').glob('*.png'))
paths = [root / path for path in source_paths + native_paths] + assets
hashes = {str(path.relative_to(root)): hashlib.sha256(path.read_bytes()).hexdigest() for path in paths}
binary = evidence / 'mici_offroad_alerts'
invocation = 'LD_LIBRARY_PATH=/home/bin9/openpilot-rust/.analysis/scratch/2026-10-01-rust-startup-ui/raylib-host/lib /home/bin9/openpilot-rust/.analysis/scratch/2026-10-01-rust-startup-ui/venv/bin/python rust/tools/check_ui_mici_offroad_alerts.py --binary .omo/evidence/mici-offroad-alerts-148/mici_offroad_alerts --output .omo/evidence/mici-offroad-alerts-148/final --display :127'
checks = []
for row in results:
  name = row['scenario']
  artifacts = [evidence / f'final/{name}.json', evidence / f'final/source-{name}.json', evidence / f'final/native-{name}.json']
  artifacts += [evidence / f'final/{lane}-{name}-{frame}.png' for lane in ['source', 'native'] for frame in row['capture_frames']]
  assert all(path.is_file() and path.stat().st_size > 0 for path in artifacts), name
  checks.append({'scenario': name, 'invocation': invocation, 'binary_observable': f"{row['trace_frames']} identical state frames; {len(row['capture_frames'])} exact source/native screenshots; zero differing pixels", 'artifacts': [str(path) for path in artifacts]})
receipt = {
  'scope': 'MiciOffroadAlerts and AlertItem isolated native UI slice for #148; host intermediate evidence',
  'source_revision': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=root, text=True).strip(),
  'binary': {'path': str(binary), 'size_bytes': binary.stat().st_size, 'sha256': hashlib.sha256(binary.read_bytes()).hexdigest()},
  'source_and_native_hashes': hashes,
  'checks': checks,
  'build': {'invocation': 'STARTUP_UI_RAYLIB_ROOT=/home/bin9/openpilot-rust/.analysis/scratch/2026-10-01-rust-startup-ui/raylib-host CARGO_INCREMENTAL=0 CARGO_TARGET_DIR=/home/bin9/openpilot-rust/.analysis/scratch/2026-09-30-rust-process-supervision/worktree/rust/target cargo build --locked -p openpilot-ui-application --example mici_offroad_alerts -j2', 'observable': 'exit 0; inherited openpilot-msgq C++ warnings are recorded in build.log', 'artifact': str(evidence / 'build.log')},
  'limits': [
    'Host raylib/GL rendering at the source compact 536x240 viewport and scale1; no AGNOS/C3X/device acceptance or CPU saving evidence.',
    'Reboot is observed as the native typed effect and source owned callback; no hardware reboot is performed.',
    'Source geometry can clip long alert text until vertically scrolled; unchanged behavior.',
    'English locale does not provide Korean fallback glyphs; both original and native show question marks there. Korean locale uses the original Korean font.',
    'Independent parent-owned whole-UI gate review is required before complete UI acceptance.',
  ],
}
rust_files = [str(root / path) for path in native_paths if path.endswith('.rs')]
static_checks = []
commands = [
  ['/home/bin9/.rustup/toolchains/1.94.0-x86_64-unknown-linux-gnu/bin/rustfmt', '--edition', '2024', '--check', *rust_files],
  ['bash', '/home/bin9/.codex/plugins/cache/sisyphuslabs/omo/5.1.8/skills/programming/scripts/rust/check-no-excuse-rules.sh', *rust_files],
  ['git', 'diff', '--check', '--', *native_paths],
]
for command in commands:
  result = subprocess.run(command, cwd=root, text=True, capture_output=True, check=True)
  static_checks.append({'invocation': command, 'exit_code': result.returncode, 'stdout': result.stdout, 'stderr': result.stderr})
(evidence / 'static-checks.json').write_text(json.dumps(static_checks, indent=2))
receipt['static_checks'] = str(evidence / 'static-checks.json')
(evidence / 'receipt.json').write_text(json.dumps(receipt, indent=2))
print(f'PASS: {len(checks)} evidence-bound scenarios; receipt {evidence / "receipt.json"}')
