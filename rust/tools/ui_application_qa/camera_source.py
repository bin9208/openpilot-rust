import os
from pathlib import Path
import time
import pyray as rl
import numpy as np
from msgq.visionipc import VisionStreamType


def create(scene, ui):
  import openpilot.system.hardware as hardware

  hardware.TICI = False
  import openpilot.selfdrive.ui.ui_state as state
  from types import SimpleNamespace

  state.UIStatus = SimpleNamespace(DISENGAGED=0, ENGAGED=1, OVERRIDE=2)
  if scene['config']['big']:
    from openpilot.selfdrive.ui.onroad.cameraview import CameraView
  else:
    from openpilot.selfdrive.ui.mici.onroad.cameraview import CameraView
  options = scene['camera']
  if scene.get('driver'):
    from driver_source import create

    widget, preview = create(scene, ui, CameraView)
  else:
    preview = None
    widget = CameraView('rustvision', VisionStreamType(options['stream']))
  camera = widget if preview is None else widget._camera_view
  if preview is None:
    camera._set_placeholder_color(rl.Color(22, 33, 44, 255))
  base = camera._calc_frame_matrix

  def transform(rect):
    kind = options.get('transform', 0)
    if kind == 1:
      w, h = (camera.frame.width, camera.frame.height) if camera.frame else (1928, 1208)
      zy = h * 2.0 / w
      return np.array([[zy * rect.height / rect.width * w / h, 0, 0], [0, zy, 0], [0, 0, 1]])
    if kind == 2:
      matrix = base(rect)
      matrix[0, 0] *= 1.5
      matrix[1, 1] *= 1.5
      return matrix
    if kind == 3:
      return np.array([[1.3, 0, 0.1], [0, 1.1, -0.3], [0, 0, 1]])
    return base(rect)

  if preview is None:
    camera._calc_frame_matrix = transform
  ui.status = 1 if options.get('engaged') else 0

  class Fixture:
    def before(self, index):
      if preview is not None:
        preview.before(index)
      for frame, stream in options.get('switches', []):
        if frame == index:
          widget.switch_stream(VisionStreamType(stream))
      directory = Path(os.environ['UI_CAMERA_SYNC'])
      (directory / f'{index}.ready').write_text('ready')
      start = time.perf_counter()
      while not (directory / f'{index}.allow').exists():
        assert time.perf_counter() - start < 10
        time.sleep(0.001)

    def snapshot(self):
      if preview is not None:
        return {
          "frame": camera.frame is not None,
          "rhd": widget.driver_state_renderer.is_rhd,
          "enabled": ui.params.get_bool("IsDriverViewEnabled"),
          "distracted": ui.params.get("DriverTooDistracted") is not None,
          "timeout": state.device.override,
        }
      return {
        'frame': widget.client.frame_id if camera.frame else None,
        'stream': int(widget.stream_type),
        'streams': sorted(int(s) for s in widget.available_streams),
      }

  return widget, Fixture()
