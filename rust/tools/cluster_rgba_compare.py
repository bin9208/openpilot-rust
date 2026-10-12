# /// script
# requires-python = ">=3.11"
# dependencies = []
# ///
# python cluster_rgba_compare.py ROOT REFERENCE BINARY OUTPUT
"""Check output pixels, borrowed callback cleanup and copied RGBA bytes."""
from __future__ import annotations

import json
from pathlib import Path
import subprocess
import sys


def main() -> None:
    root, reference, binary, output = (Path(value).resolve() for value in sys.argv[1:])
    command = [sys.executable, str(root / "rust/tools/cluster_navigation_render_compare.py"), str(root), str(reference), str(binary), str(output)]
    status = subprocess.run(command, check=False).returncode
    expected = json.loads((reference / "source-result.json").read_text())["captures"]
    actual = json.loads((output / "native-result.json").read_text())["captures"]
    errors = []
    for source, native in zip(expected, actual, strict=True):
        assert (source["name"], source["warm"]) == (native["name"], native["warm"])
        fields = [key for key in ("profile_names", "error_profile_names") if source[key] != native[key]]
        raw_equal = Path(source["path"]).with_suffix(".rgba").read_bytes() == Path(native["path"]).with_suffix(".rgba").read_bytes()
        if fields or not raw_equal:
            errors.append({"name": source["name"], "warm": source["warm"], "fields": fields, "rgba_equal": raw_equal})
        if source["error_profile_names"]:
            assert source["error_profile_names"][-1] == native["error_profile_names"][-1] == "render_to_rgba.unload_image"
    (output / "callbacks.json").write_text(json.dumps({"frames": len(expected), "differences": errors}, indent=2))
    print(json.dumps({"frames": len(expected), "callback_or_byte_differences": len(errors)}))
    raise SystemExit(status or bool(errors))


if __name__ == "__main__":
    main()
