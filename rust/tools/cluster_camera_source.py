"""Run unchanged cluster camera methods with original VisionIPC/raylib providers."""

from __future__ import annotations

import json
import os
from pathlib import Path
import sys
import time
from types import SimpleNamespace

import pyray as rl
from msgq.visionipc import VisionIpcClient


def main() -> None:
  root, scene_path, output, synchronization = map(Path, sys.argv[1:])
  scene = json.loads(scene_path.read_text())
  sys.path.insert(0, str(root / 'openpilot/selfdrive/carrot/cluster'))
  import cluster_live_camera as source

  if scene.get('external', False):
    from types import ModuleType

    logging = ModuleType('openpilot.common.swaglog')
    logging.cloudlog = SimpleNamespace(error=print, warning=print, info=print, exception=print)
    sys.modules[logging.__name__] = logging
    source.Path = lambda _path: SimpleNamespace(is_file=lambda: True)

  class OwnedClient:
    def __new__(cls, name: str, stream: int, *, conflate: bool = True):
      assert name == 'camerad'
      return VisionIpcClient('rustcluster', stream, conflate)

    @staticmethod
    def available_streams(name: str, *, block: bool):
      assert name == 'camerad'
      return VisionIpcClient.available_streams('rustcluster', block)

  rl.set_config_flags(32)
  rl.init_window(scene['width'], scene['height'], 'Cluster camera QA')
  rl.set_target_fps(0)
  descriptors = len(os.listdir('/proc/self/fd'))
  camera = source.LiveRoadCamera()
  camera._client_cls = OwnedClient
  camera._client = camera._new_client()
  if os.environ.get('CLUSTER_CAMERA_WRAP_CONTROL') == '1':
    original_textures = camera._ensure_copy_textures

    def clamp_control() -> None:
      original_textures()
      for texture in (camera._texture_y, camera._texture_uv):
        if texture is not None:
          rl.set_texture_wrap(texture, rl.TextureWrap.TEXTURE_WRAP_CLAMP)

    camera._ensure_copy_textures = clamp_control
  clock = 0.0
  source.time = SimpleNamespace(monotonic=lambda: clock)
  traces = []
  try:
    for index, step in enumerate(scene['steps']):
      (synchronization / f'{index}.ready').write_text('ready')
      started = time.monotonic()
      while not (synchronization / f'{index}.allow').is_file():
        assert time.monotonic() - started < 15
        time.sleep(0.001)
      clock = step['now']
      if step['select']:
        camera.select_stream(step['wide'])
      rl.begin_drawing()
      rl.clear_background(rl.Color(22, 33, 44, 255))
      destination = scene['destination']
      drawn = camera.draw(rl.Rectangle(destination['x'], destination['y'], destination['width'], destination['height'])) if step['draw'] else False
      frame = (
        None
        if camera._frame is None
        else {
          'id': camera._client.frame_id,
          'width': camera._frame.width,
          'height': camera._frame.height,
          'stride': camera._frame.stride,
          'uv_offset': camera._frame.uv_offset,
          'sof': camera._client.timestamp_sof,
          'eof': camera._client.timestamp_eof,
        }
      )
      traces.append({'drawn': drawn, 'stream': int(camera._stream_type), 'streams': sorted(map(int, camera._available_streams)), 'frame': frame})
      if scene.get('external', False):
        traces[-1]['zero_copy'] = camera._zero_copy
      rl.take_screenshot(os.path.relpath(output / f'frame-{index:03}.png'))
      rl.end_drawing()
    camera.close()
    if scene.get('external', False):
      (output / 'fd-delta.json').write_text(json.dumps({'delta': len(os.listdir('/proc/self/fd')) - descriptors}))
    result = {'steps': traces, 'cleanup': {'frame': camera._frame is not None, 'streams': len(camera._available_streams)}}
    (output / 'trace.json').write_text(json.dumps(result, indent=2))
  finally:
    camera.close()
    rl.close_window()


if __name__ == '__main__':
  main()
