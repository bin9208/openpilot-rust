"""Load unchanged compact onboarding card classes, avoiding unrelated installer I/O."""

import ast
from pathlib import Path
from types import SimpleNamespace
import numpy as np
import qrcode


def classes(root, gui_app, ui_state):
  import pyray as rl
  from openpilot.system.ui.widgets import Widget
  from openpilot.system.ui.widgets.scroller import Scroller, NavScroller
  from openpilot.system.ui.widgets.label import gui_label
  from openpilot.system.ui.lib.application import FontWeight, TextAlignment
  from openpilot.system.ui.lib.multilang import tr
  from openpilot.selfdrive.ui.mici.widgets.button import BigButton, GreyBigButton
  from openpilot.selfdrive.ui.mici.widgets.dialog import BigConfirmationCircleButton
  from collections.abc import Callable

  namespace = {
    "gui_app": gui_app,
    "ui_state": ui_state,
    "rl": rl,
    "Widget": Widget,
    "Scroller": Scroller,
    "NavScroller": NavScroller,
    "gui_label": gui_label,
    "FontWeight": FontWeight,
    "TextAlignment": TextAlignment,
    "tr": tr,
    "BigButton": BigButton,
    "GreyBigButton": GreyBigButton,
    "BigConfirmationCircleButton": BigConfirmationCircleButton,
    "Callable": Callable,
  }
  namespace.update(np=np, qrcode=qrcode)

  def load(path, names):
    tree = ast.parse((root / path).read_text())
    selected = [node for node in tree.body if isinstance(node, ast.ClassDef) and node.name in names]
    assert {node.name for node in selected} == set(names)
    exec(compile(ast.Module(body=selected, type_ignores=[]), str(path), 'exec'), namespace)

  from openpilot.system.ui.lib.application import TextAlignmentVertical

  namespace['TextAlignmentVertical'] = TextAlignmentVertical
  load(Path('openpilot/system/ui/mici_setup.py'), ['BigPillButton'])
  load(
    Path('openpilot/selfdrive/ui/mici/layouts/onboarding.py'),
    ['TermsPage', 'QRCodeWidget', 'TrainingGuideAttentionNotice', 'TrainingGuidePreDMTutorial', 'TrainingGuideRecordFront', 'DMBadFaceDetected'],
  )
  return SimpleNamespace(**namespace)
