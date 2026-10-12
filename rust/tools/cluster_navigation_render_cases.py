from __future__ import annotations

import base64
from dataclasses import asdict, replace
from io import BytesIO
import itertools


def cases(root):
  from cluster_model_types import default_record
  from cluster_scene_compare import source
  from PIL import Image, ImageDraw

  _, models, _, base = source(root)
  meta = models.NaviItemMeta(sequence=1, source_timestamp_ms=1234, received_mono_s=100.0)
  current = replace(
    default_record(models.NaviGuidanceInfo),
    meta=meta,
    distance_m=250,
    time_s=18,
    turn_type=12,
    road_name="서울 중앙로",
    main_text="중앙로 우회전",
    near_direction="시청 방향",
  )
  next_guidance = replace(current, distance_m=1500, turn_type=13, main_text="다음 교차로 좌회전")
  route = replace(
    default_record(models.NaviRouteInfo),
    meta=meta,
    remaining_distance_m=12345,
    remaining_time_s=630,
    moved_distance_m=3450,
    total_distance_m=15795,
    polyline=((37.0, 127.0), (37.02, 127.01), (37.01, 127.03), (37.04, 127.04)),
  )
  vehicle = replace(
    default_record(models.NaviVehicleInfo), meta=meta, latitude=37.01, longitude=127.03, road_name="서울특별시 중앙대로 북쪽 차로", speed_kph=88.5
  )
  lane = replace(default_record(models.NaviLaneInfo), meta=meta, count=4, visible=True, current_lane=2, available=(0, 1, 1, 0), turn_info=(1, 2, 3, 4))
  speed = replace(
    default_record(models.NaviSpeedInfo),
    meta=meta,
    road_limit_kph=80,
    sdi_type=2,
    sdi_speed_limit_kph=60,
    sdi_distance_m=135,
    secondary_sdi_type=4,
    secondary_sdi_speed_limit_kph=50,
    secondary_sdi_distance_m=610,
    section_active=True,
    section_average_kph=64.5,
    section_remaining_distance_m=810.0,
  )
  navi = replace(
    default_record(models.NaviLiveState),
    session_id="fixture",
    vehicle=vehicle,
    current=current,
    next=next_guidance,
    route=route,
    lane_current=lane,
    speed=speed,
    crossroad=replace(default_record(models.NaviCrossroadInfo), meta=meta, visible=True, distance_m=345),
    status=replace(default_record(models.NaviStatusInfo), meta=meta, guidance_active=True, off_route=True),
  )
  dashboard = replace(
    default_record(models.NaviDashboardState),
    connected=True,
    app_version="2.7.1",
    manifest_revision=17,
    received_count=29,
    last_received_age_ms=25,
    peer="fixture:7714",
    app_status="앱 연결됨",
    camera_status="카메라 상태 정상",
    composition_status="화면 준비 완료",
  )
  state = replace(base, speed_kph=88.5, speed_limit_kph=70, navi_live=navi, navi_dashboard=dashboard)
  image = Image.new("RGBA", (320, 160), (10, 50, 100, 210))
  draw = ImageDraw.Draw(image)
  draw.rectangle((12, 15, 140, 140), fill=(210, 60, 20, 255))
  draw.ellipse((180, 20, 300, 140), fill=(30, 230, 80, 255))
  draw.line((0, 0, 319, 159), fill=(255, 255, 255, 255), width=5)
  encoded = BytesIO()
  image.save(encoded, format="PNG")
  png = encoded.getvalue()

  def media(key, sequence=1, width=320, height=160, payload=png):
    return replace(default_record(models.NaviMediaFrame), key=key, sequence=sequence, present=True, mime="image/png", width=width, height=height, data=payload)

  all_media = tuple(
    media(key)
    for key in [
      "render:map_main",
      "image:crossroad_expanded",
      "image:crossroad_minimized",
      "image:tbt_current_full",
      "image:tbt_current_compact",
      "image:tbt_next",
      "image:lane_bottom",
      "image:traffic_signal",
      "image:center_tbt_icon",
      "image:center_tbt_text",
      "image:center_tbt_fee",
      "image:safety_primary",
      "image:safety_secondary",
      "image:safety_section",
    ]
  )
  media_state = replace(state, navi_dashboard=replace(dashboard, media=all_media))
  variants = [
    ("dashboard-empty", "dashboard", replace(base, navi_live=None, navi_dashboard=None)),
    ("dashboard-values", "dashboard", state),
    ("dashboard-media", "dashboard", media_state),
    ("dashboard-minimized", "dashboard", replace(state, navi_dashboard=replace(dashboard, media=(media("image:crossroad_minimized"),)))),
    (
      "dashboard-stalled",
      "dashboard",
      replace(media_state, navi_dashboard=replace(media_state.navi_dashboard, map_stream_stalled=True, map_frame_age_ms=3456)),
    ),
    ("dashboard-error", "dashboard", replace(state, navi_dashboard=replace(dashboard, connected=False, error="owned fixture decode failed"))),
    ("live-empty", "live", replace(base, navi_live=None, navi_dashboard=None)),
    ("live-values", "live", state),
    ("live-media", "live", media_state),
    (
      "live-next-no-current",
      "live",
      replace(media_state, navi_dashboard=replace(dashboard, media=tuple(v for v in all_media if not v.key.startswith("image:tbt_current")))),
    ),
    ("live-stalled", "live", replace(state, navi_dashboard=replace(dashboard, map_stream_stalled=True, map_frame_age_ms=6789))),
    ("live-stalled-no-age", "live", replace(state, navi_dashboard=replace(dashboard, map_stream_stalled=True))),
    ("live-disconnected", "live", replace(state, navi_dashboard=replace(dashboard, connected=False))),
    (
      "live-lane-ahead",
      "live",
      replace(state, navi_live=replace(navi, current=None, lane_current=None, lane_ahead=(replace(lane, count=12, current_lane=0, available=(1,) * 10),))),
    ),
    ("debug-empty", "debug", base),
  ]
  for severity in ["normal", "stop", "go", "warning", "caution", "CUSTOM"]:
    info = replace(
      default_record(models.NaviDebugInfo),
      title="내비게이션 안내 상태",
      lines=("왼쪽 차로를 이용하세요", "긴 안내문 " * 20, "route 7714 received", "fourth detail", "hidden fifth line"),
      severity=severity,
      guidance_image=replace(
        default_record(models.NaviGuidanceImage), image_base64=base64.b64encode(png).decode(), image_mime="image/png", image_hash="owned-png"
      ),
    )
    variants.append(("debug-" + severity, "debug", replace(base, navi_debug=info)))
  for turn in [0, 12, 13, 14, 153, 154, 249, 132, 133, 134, -1]:
    variants.append((f"turn-{turn}", "live", replace(state, navi_live=replace(navi, current=replace(current, turn_type=turn)))))
  for index, values in enumerate(
    [
      {},
      {"red_s": 12, "red_on": True, "straight_s": 5, "straight_on": False},
      {"red_s": 12, "red_on": False, "left_s": 6, "left_on": True, "straight_s": 4, "straight_on": True},
      {"uturn_s": 9, "uturn_on": True},
      {"left_s": 0, "left_on": False, "uturn_s": 9, "uturn_on": True, "right_s": 5, "right_on": True},
      {"red_s": 0, "red_on": True, "straight_s": 123, "right_s": 7, "right_on": False},
    ]
  ):
    traffic = replace(default_record(models.NaviTrafficLightInfo), meta=meta, **values)
    variants.append((f"traffic-{index}", "traffic", replace(base, navi_debug=replace(default_record(models.NaviDebugInfo), traffic_light=traffic))))
  for theme, language in itertools.product(["light", "dark"], ["ko", "en"]):
    for name, surface, value in variants:
      yield {
        "name": f"{theme}-{language}-{name}",
        "surface": surface,
        "options": {
          "width": 1920,
          "height": 480,
          "theme_mode": theme,
          "language": language,
          "is_metric": language == "ko",
          "screen_mode": 6 if surface == "dashboard" else 0,
          "panel_layout": 0,
        },
        "state": value,
        "monotonic": 102.5,
        "wall_time": 1791792000.0,
      }
    for name, surface, value in variants:
      if name in ["live-media", "live-values", "live-disconnected", "debug-caution"]:
        yield {
          "name": f"{theme}-{language}-{name}-swapped",
          "surface": surface,
          "options": {
            "width": 1280,
            "height": 320,
            "theme_mode": theme,
            "language": language,
            "is_metric": language == "ko",
            "screen_mode": 0,
            "panel_layout": 1,
          },
          "state": value,
          "monotonic": 102.5,
          "wall_time": 1791792000.0,
        }


def serializable(case):
  value = {**case, "state": asdict(case["state"])}

  def normalize(item):
    if isinstance(item, bytes):
      return list(item)
    if isinstance(item, dict):
      return {key: normalize(value) for key, value in item.items()}
    if isinstance(item, (list, tuple)):
      return [normalize(value) for value in item]
    return item

  return normalize(value)
