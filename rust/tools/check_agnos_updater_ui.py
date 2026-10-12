"""Source/native updater screens using owned input and Wi-Fi fixtures."""
import argparse
import copy
import json
import os
from pathlib import Path
import subprocess
import sys
import numpy as np
from PIL import Image

parser = argparse.ArgumentParser()
parser.add_argument('--binary', type=Path, required=True)
parser.add_argument('--output', type=Path, required=True)
parser.add_argument('--display', required=True)
parser.add_argument('--filter', default='')
args = parser.parse_args()
args.output.mkdir(parents=True, exist_ok=True)
root = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(root / 'rust/tools'))

def snapshot(networks=None, ssid=None, status='Disconnected', saved=None):
  return {'networks': networks or [], 'wifi_state': {'ssid': ssid, 'status': status}, 'ipv4_address': '192.0.2.2' if status == 'Connected' else '', 'current_network_metered': 'Unknown', 'connecting_to_ssid': ssid if status == 'Connecting' else None, 'connected_ssid': ssid if status == 'Connected' else None, 'tethering_password': 'old-password', 'tethering_active': False, 'saved_ssids': saved or []}

def base(layout, text='downloading_system', progress=37, failure=None, page='progress'):
  return {'config': {'big': layout == 'tici', 'large_viewport': layout == 'tici', 'pc': True, 'scale': 1}, 'state': {'layout': layout, 'text': text, 'progress': progress, 'failure': failure, 'running': failure is None, 'reboot_requested': False, 'logs': []}, 'wifi': snapshot(), 'frames': 70, 'page': page, 'steps': [], 'capture_frames': [0, 4, 10, 30, 69]}

def click(frame, x, y):
  return [{'frame': frame + i, 'events': [{'pos': {'x': x, 'y': y}, 'slot': 0, 'pressed': i == 0, 'released': i == 1, 'down': i == 0, 'time': (frame + i) / 20}]} for i in range(2)]

cases = []
for layout in ['tici', 'mici']:
  for text, percent, name in [('starting update' if layout == 'mici' else 'Starting update...', 0, 'start'), ('waiting for internet', 0, 'waiting'), ('Downloading_SYSTEM' if layout == 'tici' else 'downloading_system', 37, 'progress'), ('Rebooting...' if layout == 'tici' else 'rebooting', 100, 'success')]:
    cases.append((f'{layout}-{name}', base(layout, text, percent)))
  failure = ('Updater' if layout == 'tici' else 'updater') + ' exited with code 9: fatal image hash mismatch'
  scene = base(layout, 'Update failed' if layout == 'tici' else 'verifying_system', 83, failure, 'failed')
  cases.append((f'{layout}-fatal', scene))
  long = copy.deepcopy(scene)
  long['state']['failure'] += ': ' + 'very long verification detail ' * 9
  cases.append((f'{layout}-fatal-long', long))
  cases.append((f'{layout}-network-offline', base(layout, page='network')))
  wifi = base(layout, page='network' if layout == 'tici' else 'wifi')
  wifi['wifi'] = snapshot([{'ssid': 'Owned Cafe', 'strength': 50, 'security_type': 'Open', 'is_tethering': False}, {'ssid': 'Owned secure Wi-Fi', 'strength': 75, 'security_type': 'Wpa2', 'is_tethering': False}])
  wifi['steps'] = [{'frame': 0, 'wifi_events': [{'NetworksUpdated': wifi['wifi']['networks']}]}]
  cases.append((f'{layout}-wifi-cards', wifi))
scene = base('mici', page='root')
cases.append(('mici-root', scene))
scene = base('mici', page='network')
scene['wifi'] = snapshot(ssid='Owned Wi-Fi', status='Connected')
scene['steps'] = [{'frame': 0, 'connected': True}]
cases.append(('mici-network-connected', scene))
scene = base('mici')
scene['steps'] = click(20, 450, 190)
cases.append(('mici-open-network', scene))
scene = base('tici')
scene['steps'] = click(20, 450, 950) + click(45, 180, 950)
cases.append(('tici-network-back', scene))
scene = base('mici', page='network')
scene['steps'] = [{'frame': 15, 'scroll': 1250}] + click(25, 300, 120)
scene['capture_frames'] = [0, 20, 25, 30, 45, 69]
cases.append(('mici-waiting-scroll-wifi', scene))
scene = base('mici', 'verifying_system', 83, 'updater exited with code 9: fatal image hash mismatch', 'failed')
scene['steps'] = [{'frame': 15, 'scroll': 1044}] + click(25, 410, 120)
scene['capture_frames'] = [0, 20, 25, 30, 45, 69]
cases.append(('mici-reboot-confirmation', scene))
scene = base('mici', 'verifying_system', 83, 'updater exited with code 9: fatal image hash mismatch', 'failed')
scene['steps'] = [{'frame': 15, 'scroll': 820}]
cases.append(('mici-fatal-reason-visible', scene))
scene = copy.deepcopy(scene)
scene['state']['failure'] += ': ' + 'very long verification detail ' * 9
cases.append(('mici-fatal-long-visible', scene))
secure = {'ssid': 'Owned secure Wi-Fi', 'strength': 75, 'security_type': 'Wpa2', 'is_tethering': False}
scene = base('mici', page='wifi')
scene['wifi'] = snapshot([secure])
scene['steps'] = click(25, 200, 120)
scene['capture_frames'] = [0, 20, 25, 30, 45, 69]
cases.append(('mici-network-password-dialog', scene))
scene = base('mici', page='wifi')
scene['wifi'] = snapshot([secure], saved=[secure['ssid']])
scene['steps'] = click(25, 360, 170)
scene['capture_frames'] = [0, 20, 25, 30, 45, 69]
cases.append(('mici-network-forget-dialog', scene))
results = []
for name, scene in cases:
  if args.filter and args.filter not in name: continue
  directory = args.output / name
  directory.mkdir(exist_ok=True)
  path = directory / 'scene.json'
  path.write_text(json.dumps(scene))
  (directory / 'owned-manifest.json').write_text('[]')
  updater = directory / 'owned-updater'
  updater.write_text('#!/bin/sh\nprintf "waiting for internet: 0\\n"\nexit 8\n')
  updater.chmod(0o755)
  env = dict(os.environ, DISPLAY=args.display, OFFSCREEN='1')
  for label, command in [('source', [sys.executable, str(root / 'rust/tools/agnos_updater_source.py'), str(path), str(directory / 'source.png')]), ('native', [str(args.binary.resolve()), str(root), str(path), str(directory / 'native.png')])]:
    with (directory / f'{label}.log').open('w') as log:
      subprocess.run(command, cwd=root, env=env, stdout=log, stderr=subprocess.STDOUT, check=True, timeout=40)
  comparisons = []
  for frame in scene['capture_frames'] + [None]:
    suffix = '.png' if frame is None else f'-frame-{frame:04}.png'
    source = np.asarray(Image.open(directory / f'source{suffix}').convert('RGBA')).astype(np.int16)
    native = np.asarray(Image.open(directory / f'native{suffix}').convert('RGBA')).astype(np.int16)
    assert source.shape == native.shape
    difference = np.abs(source - native)
    changed = int(np.count_nonzero(difference.max(axis=2)))
    comparisons.append({'frame': frame, 'changed_pixels': changed, 'max_difference': int(difference.max()), 'mean_difference': float(difference.mean())})
    if changed:
      Image.fromarray(np.clip(difference * 4, 0, 255).astype(np.uint8)).save(directory / f'diff{suffix}')
  result = {'case': name, 'comparisons': comparisons, 'pass': all(item['changed_pixels'] == 0 for item in comparisons)}
  results.append(result)
  (args.output / 'results.json').write_text(json.dumps(results, indent=2))
  print(name, 'PASS' if result['pass'] else 'FAIL', comparisons, flush=True)
assert all(result['pass'] for result in results), 'source/native pixels differ'
