# Cluster source contract

This native port preserves `openpilot/selfdrive/carrot/cluster/cluster_config.py`,
`cluster_scene.py`, `cluster_renderer.py` and their original assets. The existing
screen and its observable state transitions are the approved reference. No new
visual direction, simplified scene, replacement illustration or setting is added.
Browser, React and Lighthouse rules do not apply to this native raylib surface.

## Layout and typography

The source design space is 1920 by 480. Preserve its source viewport scaling,
panel reversal, navigation and full-screen modes, camera projection and scissor
ownership. Renderer constants retain their source names and values: speed panel,
side gauges, navigation area, traffic light panel, TPMS, debug plots and status
labels. Every draw uses those shared constants rather than independent offsets.
The source font assets, weights, glyph sets, Korean base size 32, text measurement,
stroke padding and truncation rules remain authoritative.

## Colors and materials

`ClusterTheme` owns the source light/dark palettes. Light background is
(244,246,248), panel (250,251,252), text (20,24,28), road (218,222,226).
Dark background is (0,0,0), panel (18,23,29), text (238,242,247), road (30,36,43).
Preserve all source alpha values, path layers, lane borders, road-edge backing,
vehicle source colors, model material textures and label shadows. Automatic
theme uses the existing hour/setting policy. Source-colored alert, lane and
signal edges are part of this fidelity contract.

## Geometry and cache ownership

Scene geometry uses source double precision until the existing graphics API
requires float conversion. Preserve path/lane interpolation, dash phase, ego
offset, road/camera coordinate transforms, radar display fusion, road-edge
filtering, vehicle boxes, distance labels and source stable ordering. Real
controls/radar state are inputs; this renderer does not alter driving decisions.
Preserve bounded geometry/style/text caches and their quantization, invalidation
and resource lifetime. A cached display must still respond to every state used
by the original scene key.

## Reusable primitives and states

Scene primitives are Vec3, CameraSpec, MeshStrip, VehicleBox, RadarPointMarker
and ClusterScene. Rendering primitives own meshes, text textures, image textures,
vehicle models, gauges, warning panels, road video and navigation media. Each
resource has one owner and an explicit close path. Native retained camera and
navigation buffers keep their original lifetime and bind external textures
before every draw. Light/dark, offroad/onroad, paused/engaged, turn/blindspot,
alert, source/no-source and all original screen modes remain separate states.

## Motion, scheduling and verification

Preserve original phase/time inputs and animation rules, 10 FPS display work
(5 while USB GPU is active), core/nice scheduling and inactive restoration.
Reference fixtures drive unchanged Python scene generation and actual raylib
rendering. Compare full structured scene output and matched frame sequences,
including both themes, camera modes, panel direction, debug/trip/navigation
surfaces and Korean labels. Screenshots must come from actual geometry and
resources, never from drawing a captured reference. Reference geometry, pixel
differences, resource closure and native dependencies are recorded separately.
Host graphics and owned USB/codec fixtures do not establish device performance
or physical display acceptance.
