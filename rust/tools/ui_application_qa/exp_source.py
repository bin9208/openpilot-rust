from openpilot.cereal import log


def create(scene,ui):
  from openpilot.selfdrive.ui.onroad.exp_button import ExpButton

  ui.update_params()
  return ExpButton(scene['exp']['button_size'],scene['exp']['icon_size'])


def before(scene,ui,index):
  step=next(step for step in reversed(scene['exp']['steps']) if step['frame']<=index)
  ui.sm['selfdriveState']=log.SelfdriveState.new_message(experimentalMode=step['experimental'],engageable=step['engageable'],enabled=step['enabled'])


def snapshot(widget):
  return {'actual':widget._experimental_mode,'held':widget._held_mode,'end':widget._hold_end_time,
          'param':widget._params.get('ExperimentalMode',encoding='utf-8'),'pressed':widget.is_pressed}
