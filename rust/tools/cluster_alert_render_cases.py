from __future__ import annotations

from dataclasses import replace
import itertools


def cases(root):
  from cluster_model_types import default_record
  from cluster_scene_compare import source

  _, models, _, base = source(root)
  variants = [("alert-none", "alerts", base)]
  for index, (size, status, title, detail, kind) in enumerate(
    [
      (0, 2, "ignored", "no size", ""),
      (1, 0, "", "", ""),
      (1, 1, "   hello\nworld\x1f  ", "", ""),
      (2, 2, "", "서울 전방을 주시하세요  " * 10, ""),
      (3, 0, "raw", "raw", "clusterSelfdriveStartup"),
      (3, 2, "raw", "raw", "clusterSelfdriveTimeout"),
      (3, 1, "raw", "raw", "clusterSelfdriveReboot"),
      (4, 99, "immediate attention required " * 12, "운전대를 잡으세요 " * 20, ""),
    ]
  ):
    alert = replace(default_record(models.ClusterAlert), size=size, status=status, text1=title, text2=detail, alert_type=kind)
    variants.append((f"alert-{index}", "alerts", replace(base, alert=alert)))
  for index, (vehicle, external, owner, lifecycle, connected) in enumerate(
    [
      (False, False, "", "", False),
      (True, False, "", "", False),
      (True, True, "", "", False),
      (False, False, "", "", True),
      (False, False, "naver_v1", "guiding", False),
      (False, False, "tmap_legacy", "guiding", False),
      (False, False, "unknown", "guiding", False),
      (True, True, "naver_v1", "stale", True),
      (False, True, "", "guiding", True),
    ]
  ):
    state = replace(base, center_clock_text="23:59", vehicle_navi_available=vehicle, external_nav_active=external, navi_owner=owner, navi_lifecycle=lifecycle)
    if connected:
      state = replace(state, navi_dashboard=replace(default_record(models.NaviDashboardState), connected=True))
    variants.append((f"clock-{index}", "clock", state))
  for theme, language, small in itertools.product(["light", "dark"], ["ko", "en"], [False, True]):
    for name, surface, state in variants:
      yield {
        "name": f"{theme}-{language}-{name}-{'swapped' if small else 'normal'}",
        "surface": surface,
        "options": {
          "width": 1280 if small else 1920,
          "height": 320 if small else 480,
          "theme_mode": theme,
          "language": language,
          "is_metric": language == "ko",
          "screen_mode": 0,
          "panel_layout": int(small),
        },
        "state": state,
        "monotonic": 102.5,
        "wall_time": 1791792000.0,
      }
