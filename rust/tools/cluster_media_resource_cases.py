from dataclasses import asdict, replace


def cases(root):
  from cluster_navigation_render_cases import serializable
  from cluster_scene_compare import source
  _, models, _, base = source(root)
  records = []
  for theme in ("light", "dark"):
    options = dict(width=1920, height=480, theme_mode=theme, language="en", screen_mode=4, panel_layout=0, is_metric=True)
    previous = None
    for index, (label, width, height, strides, mode) in enumerate([
      ("basic", 8, 4, (8, 4, 4), "valid"),
      ("padded", 8, 6, (12, 8, 6), "valid"),
      ("held-sequence", 8, 6, (12, 8, 6), "held"),
      ("odd-width", 7, 6, (12, 8, 6), "valid"),
      ("short-stride", 8, 6, (7, 4, 4), "valid"),
      ("short-plane", 8, 6, (12, 8, 6), "short"),
      ("updated", 8, 6, (12, 8, 6), "valid"),
      ("clear", 8, 6, (12, 8, 6), "clear"),
      ("reappear", 8, 6, (12, 8, 6), "valid"),
      ("short-chroma", 8, 6, (12, 3, 4), "valid"),
      ("smaller", 4, 2, (4, 2, 2), "valid"),
      ("zero-size", 0, 2, (4, 2, 2), "valid"),
    ]):
      planes = tuple(bytes((16 + (x * (13 + i*7) + y*9 + index*15) % 220) for y in range(height if i == 0 else height//2) for x in range(stride)) for i, stride in enumerate(strides))
      if mode == "short":
        planes = (planes[0], planes[1], planes[2][:-1])
      sequence = previous.sequence if mode == "held" else index + 1
      frame = models.NaviMediaFrame(key="owned-yuv", sequence=sequence, present=mode != "clear", mime="image/yuv420p", width=width, height=height, plane_data=planes, plane_strides=strides)
      previous = frame
      records.append(dict(name=f"{theme}-{label}", action="media", options=options, state=base, overlay=None, frame=frame, now=100.0+index, setters=index == 0, reopen=False))
    for index, (label, width, height, sequence, payload) in enumerate([
      ("rgba-first", 8, 4, 20, bytes([220, 80, 40, 255]*32)),
      ("rgba-update", 8, 4, 21, bytes([40, 220, 80, 180]*32)),
      ("rgba-held", 8, 4, 21, bytes([40, 80, 220, 255]*32)),
      ("rgba-short", 8, 4, 22, b"bad"),
      ("rgba-resize", 4, 8, 23, bytes([40, 80, 220, 255]*32)),
      ("encoded-bad", 4, 8, 24, b"bad PNG"),
    ]):
      frame = models.NaviMediaFrame(key="owned-yuv", sequence=sequence, present=True, mime="image/png" if label == "encoded-bad" else "image/rgba", width=width, height=height, data=payload)
      records.append(dict(name=f"{theme}-{label}", action="media", options=options, state=base, overlay=None, frame=frame, now=200.0+index, setters=False, reopen=False))
  encoded = [serializable(dict(row, frame=asdict(row["frame"]))) for row in records]
  return records, encoded
