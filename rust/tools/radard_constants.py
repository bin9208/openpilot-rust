import argparse
import ast
import importlib
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def render(name):
  path = ROOT / "openpilot/selfdrive/carrot/radar_motion" / (name + ".py")
  module = importlib.import_module("openpilot.selfdrive.carrot.radar_motion." + name)
  lines = []
  for node in ast.parse(path.read_text()).body:
    if not isinstance(node, ast.Assign) or len(node.targets) != 1 or not isinstance(node.targets[0], ast.Name):
      continue
    key = node.targets[0].id
    if not key.isupper():
      continue
    value = getattr(module, key)
    match value:
      case float():
        lines.append(f"pub const {key}: f64 = {value!r};")
      case int():
        kind = "usize" if "FRAMES" in key or "SAMPLES" in key else "i32"
        lines.append(f"pub const {key}: {kind} = {value};")
      case tuple() if all(isinstance(item, float) for item in value):
        lines.append(f"pub const {key}: [f64; {len(value)}] = [{', '.join(repr(item) for item in value)}];")
  return "\n".join(lines) + "\n"


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument("--check", action="store_true")
  args = parser.parse_args()
  for name, target in (("primary", "primary"), ("trajectory_cutin", "trajectory_cutin"), ("controller", "controller")):
    path = ROOT / "rust/crates/radard/src" / target / "constants.rs"
    generated = render(name)
    if args.check:
      assert path.read_text() == generated, path
    else:
      path.parent.mkdir(parents=True, exist_ok=True)
      path.write_text(generated)


if __name__ == "__main__":
  main()
