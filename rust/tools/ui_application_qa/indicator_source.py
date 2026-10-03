from types import SimpleNamespace
from openpilot.cereal import car, log


def create(scene, ui):
  import openpilot.selfdrive.ui.ui_state as state

  state.UIStatus=SimpleNamespace(DISENGAGED=0,ENGAGED=1,OVERRIDE=2)
  if scene['kind']=='torque':
    from openpilot.selfdrive.ui.mici.onroad.torque_bar import TorqueBar

    ui.update_params()
    return TorqueBar(scene['indicator']['demo'])
  if scene['kind']=='confidence':
    from openpilot.selfdrive.ui.mici.onroad.confidence_ball import ConfidenceBall

    return ConfidenceBall(scene['indicator']['demo'])
  from openpilot.selfdrive.ui.mici.onroad.traffic_light import TrafficLight

  return TrafficLight()


def before(scene, ui, widget, index):
  step=next(step for step in reversed(scene['indicator']['steps']) if step['frame']<=index)
  ui.status={'disengaged':0,'engaged':1,'override':2}[step['status']]
  if scene['kind']=='torque':
    controls=log.ControlsState.new_message(curvature=step['curvature'],desiredCurvature=step['desired'])
    controls.lateralControlState.init('angleState' if step['angle'] else 'torqueState')
    ui.sm['controlsState']=controls
    ui.sm['carState']=car.CarState.new_message(vEgo=step['speed'])
    ui.sm['carControl']=car.CarControl.new_message(latActive=step['lat_active'])
    ui.sm['liveParameters']=log.LiveParametersData.new_message(roll=step['roll'])
    output=car.CarOutput.new_message()
    output.actuatorsOutput.torque=step['torque']
    ui.sm['carOutput']=output
    if step.get('value') is not None:
      widget.update_filter(step['value'])
    return
  ui.sm['longitudinalPlan']=log.LongitudinalPlan.new_message(trafficState=step['traffic'])
  model=log.ModelDataV2.new_message()
  model.meta.disengagePredictions.brakeDisengageProbs=step['brake']
  model.meta.disengagePredictions.steerOverrideProbs=step['steer']
  ui.sm['modelV2']=model
  if step.get('value') is not None:
    widget.update_filter(step['value'])


def snapshot(scene, widget):
  if scene['kind']=='torque':
    return {'value':float(widget._torque_filter.x),'opacity':float(widget._torque_line_alpha_filter.x)}
  return ({'value':widget._confidence_filter.x} if scene['kind']=='confidence' else
          {'value':widget._alpha_filter.x,'visible':widget.is_visible()})
