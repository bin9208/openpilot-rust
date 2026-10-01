"""Compare native color-emoji rasters with the actual source emoji_tex function."""
import argparse
import importlib.util
from io import BytesIO
import json
from pathlib import Path
import subprocess
import sys
from types import ModuleType

from PIL import Image
import numpy as np

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]
app = ModuleType('openpilot.system.ui.lib.application')
app.FONT_DIR = root / 'openpilot/selfdrive/assets/fonts'
sys.modules[app.__name__] = app
rl = ModuleType('pyray')
rl.Texture = Image.Image
rl.load_image_from_memory = lambda ext, data, length: Image.open(BytesIO(data)).convert('RGBA')
rl.load_texture_from_image = lambda image: image
sys.modules['pyray'] = rl
spec = importlib.util.spec_from_file_location('source_emoji', root / 'openpilot/system/ui/lib/emoji.py')
source = importlib.util.module_from_spec(spec)
spec.loader.exec_module(source)
results = []
for index, text in enumerate(['😀', '🇰🇷', '👨\u200d👩\u200d👧', '❤️', '😀😀', '⚠️', '👍🏽']):
  original = source.emoji_tex(text)
  source_path = args.output / f'emoji-{index}-source.png'
  native_path = args.output / f'emoji-{index}-native.png'
  original.save(source_path)
  subprocess.run([str(args.binary), str(app.FONT_DIR / 'NotoColorEmoji.ttf'), text, str(native_path)], check=True)
  a, b = np.asarray(original).astype(int), np.asarray(Image.open(native_path)).astype(int)
  results.append({'text': text, 'different_pixels': int(np.any(a != b, axis=-1).sum()), 'max_channel_difference': int(abs(a - b).max())})
(args.output / 'emoji-pixels.json').write_text(json.dumps(results, indent=2, ensure_ascii=False))
assert not any(item['different_pixels'] for item in results), results
print(f'PASS: {len(results)} actual-source emoji rasters equal pixel-for-pixel, including flags, ZWJ, selectors, adjacent emoji and skin tone')
