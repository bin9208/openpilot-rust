"""Compare every module/mask to the locked original Python QR encoder."""

import argparse
import json
from pathlib import Path
import random
import subprocess
import qrcode

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
rng = random.Random(148)
texts = ['', 'http://192.0.2.4:7000', 'http://[2001:db8::1]:7000', 'https://connect.comma.ai/?pair=fixture', '123', 'HELLO WORLD', '한글 와이파이 설정']
for count in [19, 20, 21, 39, 99, 300, 800, 1800]:
  for alphabet in ['0123456789', 'ABCDEF012345', 'aAbBZ0123+./:-=_ 한글']:
    texts.append(''.join(rng.choice(alphabet) for _ in range(count)))
texts += ['prefix' + '1234567890' * 3 + 'middle' + 'ABCDEFGHIJ' * 3 + 'suffix']
cases = [{'data': data, 'correction': correction} for correction in ['low', 'medium'] for data in texts]
(args.output / 'input.json').write_text(json.dumps(cases))
result = subprocess.run([str(args.binary)], input=json.dumps(cases), text=True, capture_output=True, check=True)
(args.output / 'native.json').write_text(result.stdout)
(args.output / 'native.stderr').write_text(result.stderr or '(no stderr)\n')
expected = []
for case in cases:
  qr = qrcode.QRCode(error_correction=qrcode.constants.ERROR_CORRECT_L if case['correction'] == 'low' else qrcode.constants.ERROR_CORRECT_M)
  qr.add_data(case['data'])
  qr.make(fit=True)
  modules = list(qr.modules)
  mask = qr.best_mask_pattern()
  expected.append({'size': len(modules), 'mask': mask, 'modules': [value for row in modules for value in row]})
(args.output / 'source.json').write_text(json.dumps(expected))
for index, (source, native) in enumerate(zip(expected, json.loads(result.stdout), strict=True)):
  assert source == native, (index, cases[index]['data'][:50], source['size'], native['size'], source['mask'], native['mask'])
(args.output / 'result.json').write_text(
  json.dumps({'cases': len(cases), 'exact': True, 'correction': ['L', 'M'], 'source_dependency': 'uv.lock qrcode 8.2'}, indent=2)
)
print(f'PASS: {len(cases)} source/native QR segmentation, mask selection and complete module matrices')
