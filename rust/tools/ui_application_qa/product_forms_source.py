from openpilot.system.ui.lib.application import gui_app
from openpilot.system.ui.lib.multilang import multilang


def create(scene, effects, dialog_results):
  if 'dialog' in scene:
    from openpilot.selfdrive.ui.mici.widgets.dialog import BigDialog, BigConfirmationDialog, BigInputDialog

    options = scene['dialog']
    gui_app.pop_widget = lambda *args, **kwargs: None
    if scene['kind'] == 'dialog-info':
      widget = BigDialog(options['title'], options.get('description', ''))
    elif scene['kind'] == 'dialog-confirm':
      icon = gui_app.texture('icons_mici/settings/device/reboot.png', 64, 64)
      widget = BigConfirmationDialog(
        options['title'], icon, lambda: dialog_results.append('confirm'), exit_on_confirm=not options.get('stay', False), red=options.get('red', False)
      )
    else:
      widget = BigInputDialog(options['title'], options.get('text', ''), confirm_callback=lambda text: dialog_results.append(text))
  elif scene['kind'] == 'language':
    from openpilot.system.ui.widgets.option_dialog import MultiOptionDialog
    from openpilot.system.ui.lib.application import FontWeight
    from openpilot.system.ui.lib.multilang import tr
    from openpilot.system.ui.widgets import DialogResult

    def select_language(result):
      if result == DialogResult.CONFIRM:
        code = multilang.languages[widget.selection]
        multilang.change_language(code)
        effects.effects.append({'language': code})

    gui_app.pop_widget = lambda: effects.effects.append({'pop': True})
    widget = MultiOptionDialog(
      tr('Select a language'), multilang.languages, multilang.codes[multilang.language], option_font_weight=FontWeight.UNIFONT, callback=select_language
    )
  return widget


def redirect_ssh(scene):
  import requests

  original_get = requests.get

  def owned_get(url, *args, **kwargs):
    assert url.startswith('https://github.com/')
    return original_get(scene['ssh_host'] + url.removeprefix('https://github.com'), *args, **kwargs)

  requests.get = owned_get
