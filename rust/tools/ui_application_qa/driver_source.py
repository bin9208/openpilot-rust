import ast
from pathlib import Path
import pyray as rl
from openpilot.cereal import log, messaging
from openpilot.system.ui.widgets import Widget
from openpilot.system.ui.lib.application import gui_app, FontWeight, TextAlignment, TextAlignmentVertical
from openpilot.system.ui.lib.multilang import tr
from openpilot.system.ui.widgets.label import gui_label


def create(scene, ui, CameraView):
  import openpilot.selfdrive.ui.ui_state as state
  from openpilot.selfdrive.ui.mici.onroad.driver_state import DriverStateRenderer

  state.device.set_override_interactive_timeout = lambda value: setattr(state.device, 'override', value)
  root = Path(__file__).resolve().parents[3]

  class OwnedCamera(CameraView):
    def __init__(self, name, stream):
      super().__init__("rustvision", stream)

  namespace = {
    'rl': rl,
    'log': log,
    'messaging': messaging,
    'Widget': Widget,
    'gui_app': gui_app,
    'FontWeight': FontWeight,
    'TextAlignment': TextAlignment,
    'TextAlignmentVertical': TextAlignmentVertical,
    'tr': tr,
    'gui_label': gui_label,
    'ui_state': ui,
    'device': state.device,
    'CameraView': OwnedCamera,
    'DriverStateRenderer': DriverStateRenderer,
    'VisionStreamType': __import__('msgq.visionipc', fromlist=['VisionStreamType']).VisionStreamType,
  }
  tree = ast.parse((root / 'openpilot/selfdrive/ui/mici/onroad/driver_camera_dialog.py').read_text())
  nodes = [node for node in tree.body if isinstance(node, ast.ClassDef) and node.name in ['DriverCameraView', 'BaseDriverCameraDialog']]
  exec(compile(ast.Module(body=nodes, type_ignores=[]), 'driver_camera_dialog.py', 'exec'), namespace)
  options = scene['driver']
  if options['setup']:
    tree = ast.parse((root / 'openpilot/selfdrive/ui/mici/layouts/onboarding.py').read_text())
    nodes = [node for node in tree.body if isinstance(node, ast.ClassDef) and node.name == 'DriverCameraSetupDialog']
    exec(compile(ast.Module(body=nodes, type_ignores=[]), 'onboarding.py', 'exec'), namespace)
  cls = namespace['DriverCameraSetupDialog' if options['setup'] else 'BaseDriverCameraDialog']
  widget = cls()
  ui.started_frame = 0
  ui.sm.recv_frame = {'driverStateV2': 1}
  ui.sm['selfdriveState'] = log.SelfdriveState.new_message()
  drivers = log.DriverStateV2.new_message()
  for name, x in [('leftDriverData', -0.18), ('rightDriverData', 0.18)]:
    data = getattr(drivers, name)
    data.faceOrientation = options['orientation']
    data.faceOrientationStd = [options['deviation'], options['deviation'], 0.1]
    data.facePosition = [x, 0.05]
    data.leftEyeProb, data.rightEyeProb = options['eyes']
    data.sunglassesProb = options['glasses']
  dm = log.DriverMonitoringState.new_message()
  dm.isRHD = options['rhd']
  dm.activePolicy = 'vision'
  dm.visionPolicyState.faceDetected = options['detected']
  dm.visionPolicyState.awarenessPercent = 83
  ui.sm.update(driverStateV2=drivers, driverMonitoringState=dm)

  class Fixture:
    def before(self, index):
      ui.sm.recv_frame['driverStateV2'] = index + 1
      if options['setup']:
        widget.driver_state_renderer.get_driver_data()

  return widget, Fixture()
