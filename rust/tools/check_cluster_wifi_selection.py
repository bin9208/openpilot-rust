"""Temporary original selected main policy blocks plus original FPS reader."""
import argparse
import ast
import hashlib
import itertools
import json
from pathlib import Path
from types import SimpleNamespace


def main():
  parser = argparse.ArgumentParser(); parser.add_argument('--output', type=Path, required=True)
  args = parser.parse_args(); args.output.mkdir(parents=True, exist_ok=False)
  manifest = json.loads((Path(__file__).resolve().parents[1] / 'upstream/cluster-wifi-source.json').read_text())
  item = manifest['files']['openpilot/selfdrive/carrot/cluster/main.py']; data = item['content'].encode()
  assert hashlib.sha256(data).hexdigest() == item['sha256']
  tree = ast.parse(data)
  main_node = next(node for node in tree.body if isinstance(node, ast.FunctionDef) and node.name == 'main')
  blocks = [node for node in main_node.body if isinstance(node, ast.If) and ast.unparse(node.test) == 'wifi_config is not None']
  assert len(blocks) == 2
  reader = next(node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == 'ClusterUsbFpsReader')
  read = next(node for node in reader.body if isinstance(node, ast.FunctionDef) and node.name == 'read')
  scope = {}; exec(compile(ast.Module(body=[read], type_ignores=[]), 'e8e152db:ClusterUsbFpsReader.read', 'exec'), scope)
  original_read = scope['read']
  inputs, expected = [], []
  for tici, live, egpu, fps, quality in itertools.product([False, True], [False, True], [False, True], [1, 5, 10, 15, 20], [20, 50, 90]):
    class Reader:
      def __init__(self, normal_fps, *, egpu_max_fps):
        self.normal_fps, self.egpu_max_fps = normal_fps, egpu_max_fps
        self._params = SimpleNamespace(get_bool=lambda key: egpu)
      read = original_read
    options = SimpleNamespace(usb_codec='h264', usb_jpeg_quality=68, usb_async=False, input='live' if live else 'random')
    scope = dict(args=options, wifi_config=dict(fps=fps, quality=quality), TICI=tici, ClusterUsbFpsReader=Reader)
    exec(compile(ast.Module(body=blocks, type_ignores=[]), 'e8e152db:main S3 selection blocks', 'exec'), scope)
    inputs.append(dict(tici=tici, live=live, egpu=egpu, fps=fps, quality=quality))
    expected.append(dict(codec=options.usb_codec, quality=options.usb_jpeg_quality, asynchronous=options.usb_async, encoder_source=scope['encoder_source'], fps=scope['target_fps'], source=scope['fps_source'], display_fps=scope['usb_display_fps'], automatic=scope['usb_display_fps_auto']))
  (args.output / 'selection.json').write_text(json.dumps(dict(source_commit=manifest['source_commit'], scope='two unchanged main S3 selection blocks and actual reader.read; no full main claim', input=inputs, expected=expected), indent=2) + '\n')
  print(json.dumps(dict(cases=len(inputs), scope='original S3 codec/quality/async/rate selection')))


if __name__ == '__main__': main()
