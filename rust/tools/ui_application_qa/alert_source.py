"""Run the unchanged alert widgets against owned cereal data and clock boundaries."""

from dataclasses import asdict
from types import SimpleNamespace


def create(scene, ui):
  if scene['config']['big']:
    from openpilot.selfdrive.ui.onroad.alert_renderer import AlertRenderer
  else:
    from openpilot.selfdrive.ui.mici.onroad.alert_renderer import AlertRenderer
  ui.started_frame = scene['alert'].get('started_frame', 0)
  ui.started_time = 0.0
  ui.sm.recv_frame = {'selfdriveState': 0}
  ui.sm.recv_time = {'selfdriveState': 0.0}
  if scene['alert'].get('initial') is not None:
    ui.sm['selfdriveState'] = reader(scene['alert']['initial'])
  return AlertRenderer()


def reader(alert):
  result = SimpleNamespace(
    alertText1=alert['text1'], alertText2=alert['text2'],
    alertSize=SimpleNamespace(raw=alert['size']), alertStatus=SimpleNamespace(raw=alert['status']),
    alertHudVisual=alert.get('visual_alert', 0), alertType=alert.get('alert_type', ''), enabled=False,
  )
  if alert['size'] == 0:
    result.alertSize = 0
  return result


def monotonic(scene, index):
  step = next(step for step in reversed(scene['alert']['steps']) if step['frame'] <= index)
  return step['now'] + (index - step['frame']) / 20 if 'now' in step else index / 20


def before(scene, ui, index):
  step = next(step for step in reversed(scene['alert']['steps']) if step['frame'] <= index)
  publish = step.get('publish', True)
  if publish:
    ui.sm['selfdriveState'] = reader(step['alert'])
  ui.sm['carState'] = SimpleNamespace(leftBlinker=step.get('left', False), rightBlinker=step.get('right', False))
  ui.sm.updated['selfdriveState'] = publish
  if publish:
    ui.sm.recv_frame['selfdriveState'] = index + 1
    ui.sm.recv_time['selfdriveState'] = monotonic(scene, index)


def snapshot(widget, ui, rendered):
  current = widget.get_alert(ui.sm)
  return {'current': asdict(current) if current is not None else None, 'rendered': rendered}
