import argparse
import ast
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
from types import SimpleNamespace


READERS = {
  'theme': 'ClusterThemeParamReader',
  'display': 'ClusterDisplayPreferencesParamReader',
  'clock_visible': 'ClusterClockVisibilityParamReader',
  'usb_fps': 'ClusterUsbFpsReader',
  'brightness': 'ClusterHudBrightnessParamReader',
  'orientation': 'ClusterHudOrientationParamReader',
  'mirror': 'ClusterHudMirrorParamReader',
  'screen_mode': 'ClusterScreenModeParamReader',
  'camera_view_mode': 'ClusterCameraViewModeParamReader',
  'panel_layout': 'ClusterPanelLayoutParamReader',
  'radar_info': 'ClusterRadarInfoParamReader',
  'radar_display': 'ClusterRadarDisplayParamReader',
  'radar_color': 'ClusterRadarSourceColorParamReader',
  'hud_mode': 'ClusterHudModeParamReader',
  'encoder': 'ClusterHudEncoderParamReader',
  'gate': 'ClusterHudOutputGateParamReader',
}


def original(root):
  sys.path[:0] = [str(root), str(root / 'rust/tools'), str(root / 'openpilot/selfdrive/carrot/cluster')]
  import cluster_config
  import cluster_display

  path = root / 'openpilot/selfdrive/carrot/cluster/main.py'
  tree = ast.parse(path.read_text())
  selected = [ast.ImportFrom(module='__future__', names=[ast.alias(name='annotations')], level=0)]
  selected += [
    node
    for node in tree.body
    if (isinstance(node, ast.ClassDef) and node.name in READERS.values()) or (isinstance(node, ast.FunctionDef) and node.name == 'apply_cluster_encoder_param')
  ]
  scope = {**vars(cluster_config), **vars(cluster_display)}
  exec(compile(ast.fix_missing_locations(ast.Module(selected, type_ignores=[])), str(path), 'exec'), scope)
  return scope, path


def source_rows(root, binding, cases, output):
  scope, path = original(root)
  classes = {name: scope[name] for name in READERS.values()}
  from original_params_binding import load

  params_module, logging = load(binding, 'ipc://' + str(output / 'source-log'), output / 'logs')
  rows = []
  with tempfile.TemporaryDirectory(dir=output) as folder:
    params = params_module.Params(folder)
    current = set()
    for case in cases:
      for key in current:
        params.remove(key)
      values = case['values'] or {}
      current = set(values)
      for key, value in values.items():
        (Path(params.get_param_path()) / key).write_bytes(bytes(value))
      readers = {}
      for key, name in READERS.items():
        reader = classes[name].__new__(classes[name])
        reader._params = params if case['values'] is not None else None
        readers[key] = reader
      value = {key: reader.read() for key, reader in readers.items() if key != 'gate'}
      value['display'] = {'language': value['display'][0], 'is_metric': value['display'][1]}
      value['radar'] = {'info': value.pop('radar_info'), 'display': value.pop('radar_display'), 'color': value.pop('radar_color')}
      value['gate'] = {'mode': readers['gate'].read_mode(), 'allowed': readers['gate'].allowed()}
      arguments = case['args']

      def option(name, default, values=arguments):
        return values[values.index(name) + 1] if name in values else default

      args = SimpleNamespace(
        output=option('--output', 'usb'),
        usb_codec=option('--usb-codec', 'jpeg'),
        usb_codec_from_cli='--usb-codec' in arguments,
        usb_h264_backend=option('--usb-h264-backend', 'native'),
        usb_h264_backend_from_cli='--usb-h264-backend' in arguments,
        usb_h264_ffmpeg_encoder=option('--usb-h264-ffmpeg-encoder', 'libx264'),
        usb_h264_ffmpeg_encoder_from_cli='--usb-h264-ffmpeg-encoder' in arguments,
      )
      scope['ClusterHudEncoderParamReader'] = lambda owner=readers['encoder']: owner
      source = scope['apply_cluster_encoder_param'](args)
      rows.append(
        {
          'preferences': value,
          'encoder_source': source,
          'codec': args.usb_codec,
          'backend': args.usb_h264_backend,
          'ffmpeg_encoder': args.usb_h264_ffmpeg_encoder,
        }
      )
  logging.ipchandler.close()
  return rows, hashlib.sha256(path.read_bytes()).hexdigest()


def main():
  parser = argparse.ArgumentParser()
  parser.add_argument('root', type=Path)
  parser.add_argument('output', type=Path)
  parser.add_argument('--binding', type=Path, required=True)
  parser.add_argument('--binary', type=Path)
  args = parser.parse_args()
  output = args.output.resolve()
  output.mkdir(parents=True, exist_ok=False)
  os.environ['OPENPILOT_PREFIX'] = 'fixture'
  keys = [
    'ClusterHudTheme',
    'ShowDateTime',
    'ClusterHudBrightness',
    'ClusterHudOrientation',
    'ClusterHudMirror',
    'ClusterHudScreenMode',
    'ClusterHudCameraViewMode',
    'ClusterHudPanelLayout',
    'ClusterHudRadarInfo',
    'ClusterHudRadarDisplay',
    'ClusterHudRadarSourceColor',
    'ClusterHud',
    'ClusterHudDebug',
    'ClusterHudEncoder',
  ]
  cases = [{'values': None, 'args': []}, {'values': {}, 'args': []}]
  for key in keys:
    for value in [b'', b'0', b'1', b'2', b'3', b'4', b'5', b'6', b'-1', b'-7', b'101', b'2147483647', b'-2147483648', b' +2tail']:
      cases.append({'values': {key: list(value)}, 'args': []})
  for language in [None, b'', b'en', b'ko', b' main_english ', b'main_kor', b'unknown', b'en\xff']:
    for metric in [None, b'', b'1', b'0', b'true', b'off']:
      values = {key: list(value) for key, value in [('LanguageSetting', language), ('IsMetric', metric)] if value is not None}
      cases.append({'values': values, 'args': []})
  for active in [b'0', b'1', b'false', b'01']:
    for onroad in [b'0', b'1']:
      for mode in [b'0', b'1', b'2', b'3', b'-1', b'4']:
        cases.append({'values': {'UsbGpuActive': list(active), 'IsOnroad': list(onroad), 'ClusterHudDebug': list(mode)}, 'args': []})
  for mode in range(-1, 5):
    for options in [
      [],
      ['--output', 'window'],
      ['--usb-codec', 'png'],
      ['--usb-h264-backend', 'ffmpeg'],
      ['--usb-h264-backend', 'auto'],
      ['--usb-h264-ffmpeg-encoder', 'h264_v4l2m2m'],
    ]:
      cases.append({'values': {'ClusterHudEncoder': list(str(mode).encode())}, 'args': options})
  expected, source_hash = source_rows(args.root.resolve(), args.binding.resolve(), cases, output)
  (output / 'input.json').write_text(json.dumps(cases))
  (output / 'source.json').write_text(json.dumps(expected))
  result = {'cases': len(cases), 'source_sha256': source_hash}
  if args.binary:
    binary = args.binary.resolve()
    completed = subprocess.run([str(binary)], input=json.dumps(cases), text=True, capture_output=True, timeout=30, check=True)
    (output / 'native.stdout').write_text(completed.stdout)
    (output / 'native.stderr').write_text(completed.stderr)
    actual = json.loads(completed.stdout)
    failures = [{'case': index, 'source': left, 'native': right} for index, (left, right) in enumerate(zip(expected, actual, strict=True)) if left != right]
    result.update({'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(), 'matched': len(cases) - len(failures), 'failures': failures})
  (output / 'result.json').write_text(json.dumps(result, indent=2) + '\n')
  print(json.dumps({key: value for key, value in result.items() if key != 'failures'}))
  if result.get('failures'):
    raise SystemExit(1)


if __name__ == '__main__':
  main()
