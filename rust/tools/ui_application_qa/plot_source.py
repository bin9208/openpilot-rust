from openpilot.cereal import log, services

REQUIRED=['carState','longitudinalPlan','carControl','controlsState','modelV2','radarState','liveParameters']


def create(scene,ui):
  from openpilot.selfdrive.ui.mici.onroad.debug_plot import DebugPlot

  ui.sm.alive={name:True for name in REQUIRED}
  ui.sm._owned_receive={name:0.0 for name in REQUIRED}
  for name in REQUIRED:
    event=log.Event.new_message()
    event.init(name)
    ui.sm[name]=getattr(event,name)
  return DebugPlot()


def step(scene,index):
  return next(step for step in reversed(scene['plot']['steps']) if step['frame']<=index)


def before(scene,ui,widget,index):
  current=step(scene,index)
  widget.params.put('ShowPlotMode',str(current['mode']))
  for message in current['messages']:
    with log.Event.from_bytes(bytes(message)) as event:
      name=event.which()
      ui.sm[name]=getattr(event,name).as_builder()
      ui.sm._owned_receive[name]=index/20
  ui.sm.alive={name:index/20-ui.sm._owned_receive[name]<10/services.SERVICE_LIST[name].frequency for name in REQUIRED}


def snapshot(scene,ui,widget,index):
  data,title=widget._make_plot_data(ui.sm,step(scene,index)['mode'])
  return {'samples':{'size':widget.plot_size,'index':widget.plot_index,'time':widget.sample_t,
                     'minimum':widget.plot_min,'maximum':widget.plot_max,'last_sample':widget._last_sample_time,
                     'latest':[widget._get_series_value(series,0) for series in range(3)]},
          'previous':widget.show_plot_mode_prev,'data':data,'title':title}
