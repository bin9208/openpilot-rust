import ast
from collections import deque
from pathlib import Path
from types import SimpleNamespace


def render(steps):
  path=Path(__file__).resolve().parents[3]/'openpilot/selfdrive/ui/mici/onroad/debug_plot.py'
  tree=ast.parse(path.read_text())
  nodes=[node for node in tree.body if isinstance(node,ast.ClassDef)]
  original=next(node for node in nodes if node.name=='DebugPlot')
  method=next(node for node in original.body if isinstance(node,ast.FunctionDef) and node.name=='_render')
  start=next(index for index,node in enumerate(method.body) if isinstance(node,ast.Assign) and
             any(isinstance(target,ast.Name) and target.id=='now' for target in node.targets))
  body=method.body[start:start+3]
  gate=ast.FunctionDef(name='gate',args=ast.arguments(posonlyargs=[],args=[ast.arg(arg=name) for name in ['self','sm','show_plot_mode']],
                                                   kwonlyargs=[],kw_defaults=[],defaults=[]),body=body,decorator_list=[])

  class Widget:
    def set_rect(self,rect):
      self.rect=rect

  clock=[0.0]
  namespace={'deque':deque,'PLOT_MAX':300,'List':list,'Tuple':tuple,'Widget':Widget,'Params':lambda:None,
             'rl':SimpleNamespace(Rectangle=lambda *values:values,get_time=lambda:clock[0],Font=object,Color=object),
             'gui_app':SimpleNamespace(width=2160,height=1080,font=lambda _:None),'FontWeight':SimpleNamespace(DISPLAY=0)}
  exec(compile(ast.fix_missing_locations(ast.Module(body=[*nodes,gate],type_ignores=[])),str(path),'exec'),namespace)
  plot=namespace['DebugPlot']()
  outputs=[]
  for step in steps:
    if step.get('reset'):
      plot._reset_plot()
    clock[0]=step['now']
    plot._make_plot_data=lambda *_args,step=step:(step['values'],'fixture')
    namespace['gate'](plot,None,1)
    snapshot={'size':plot.plot_size,'index':plot.plot_index,'time':plot.sample_t,'minimum':plot.plot_min,
              'maximum':plot.plot_max,'last_sample':plot._last_sample_time,
              'latest':[plot._get_series_value(series,0) for series in range(3)]}
    history=[[plot._get_series_value(series,back) for back in range(plot.plot_size)] for series in range(3)] if step.get('history') else None
    outputs.append({'sample':snapshot,'history':history})
  return outputs
