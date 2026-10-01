from types import SimpleNamespace
from openpilot.cereal import log


def create(scene, ui):
  import openpilot.selfdrive.ui.ui_state as state

  state.UIStatus=SimpleNamespace(DISENGAGED=0,ENGAGED=1,OVERRIDE=2)
  if scene['kind']=='confidence':
    from openpilot.selfdrive.ui.mici.onroad.confidence_ball import ConfidenceBall

    return ConfidenceBall(scene['indicator']['demo'])
  from openpilot.selfdrive.ui.mici.onroad.traffic_light import TrafficLight

  return TrafficLight()


def before(scene, ui, widget, index):
  step=next(step for step in reversed(scene['indicator']['steps']) if step['frame']<=index)
  ui.status={'disengaged':0,'engaged':1,'override':2}[step['status']]
  ui.sm['longitudinalPlan']=log.LongitudinalPlan.new_message(trafficState=step['traffic'])
  model=log.ModelDataV2.new_message()
  model.meta.disengagePredictions.brakeDisengageProbs=step['brake']
  model.meta.disengagePredictions.steerOverrideProbs=step['steer']
  ui.sm['modelV2']=model
  if step.get('value') is not None:
    widget.update_filter(step['value'])


def snapshot(scene, widget):
  return ({'value':widget._confidence_filter.x} if scene['kind']=='confidence' else
          {'value':widget._alpha_filter.x,'visible':widget.is_visible()})
