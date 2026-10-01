# Startup UI source contract

The unchanged `openpilot/system/ui/{spinner,text}.py` and their application,
scroll-panel, Widget and Button implementations are the visual reference. This
is a native port, not a redesign. Web/React/Lighthouse and external design-gallery
recommendations do not apply to this existing raylib surface.

## Layout and tokens

Logical viewport is 2160×1080 when hardware is tici/tizi or BIG=1; otherwise
536×240. BIG alone controls FONT_SCALE (1.242 vs 1.16), NORMAL Inter font weight
(Regular vs Medium), and TextWindow layout. Preserve the source distinction.
SCALE changes the even-sized physical framebuffer and input/scissor coordinates.
The source auto-scale is min(monitor/logical)*0.95, clamped to at least 0.3 when
monitor space is insufficient; it is otherwise 1.

Spinner large/small: texture 360/140; bar 1000×20 / 268×10; wrapped gap 50/10;
center gap 150/20; horizontal margin 100/20; text 96/28; line height 104/32;
IP 72/36 at top36/12; status48/28, gap14/4. Logo and track use original images;
track alpha is premultiplied, assets resized preserving aspect, bilinear filtered.
Background black, text/progress white, track darkgray(55), IP(230,230,230,235),
status lightgray(200). The ring rotates360 degrees/sec, retaining source frame time.

TextWindow BIG/small: margin50/20, spacing40/30, text72/25, lineheight80/25,
button310×160 /150×80. IP large-viewport/small: size50/18, reserved band80/28.
The body alone scrolls and is scissored; IP and button stay fixed. Content starts
at its bottom. Button bottom-right, radius10, white2px border, black fill,
Inter-Medium text(228), centered with source floor positioning and font scale.
Its label is Exit on PC, Reboot on board. Preserve the source IP measurement/draw
font-size distinction and fixed layout even where its spacing is imperfect.

## Typography and resources

Prefer existing .fnt atlas, then matching .ttf, then .otf, then original missing
.fnt fallback. Source glyph sets: ASCII32..126 for ordinary fonts, plus Hangul
AC00..D7A3 and Han4E00..9FFF/3400..4DBF for DISPLAY/UNIFONT. Raster sizes200,
DISPLAY48, UNIFONT16. Generate mipmaps/trilinear filtering except UNIFONT.
Spinner loads NORMAL+Pretendard only; TextWindow loads source font weights.
Font fallback uses DISPLAY only for th/zh-CHT/zh-CHS/ko/ja and if loaded.
Assets and fonts remain original licensed external files; no substituted artwork.

## States and interaction

Spinner stdin accepts available newline records and stops each read batch at a
blank line/EOF. Digits latch clamped0..100 progress; text updates last status,
wrapping only before progress first becomes active. Progress retains last status;
status collapses whitespace and binary-search truncates with literal three dots.
Wrap behavior preserves source indentation, paragraph blanks and hyphen splits.

Scroll uses wheel50px, drag threshold12px, offset filter tau0.1, velocity filter
0.05 at20FPS, exponential friction5/sec, out-of-bounds movement /3 and stronger
friction, settle tolerance0.01. Button tracks press-start inside, cancels outside,
permits reentry, and fires only on release inside after a tracked press. Slot0 is
the active button/scroll input. PC events update each frame; board input follows
the source two-slot140Hz polling/evdev boundary. Exit/reboot follows request-close.
Network label is current IPv4:6999 or `no network`; five-second monitor retains
last IP for two failures and clears on the third. UI timing/policy is unchanged.

## Verification and limitations

Compare original source policy and actual raylib rendering in an isolated X server,
including small/large geometry, progress/status, error text/scroll and exit input.
Screenshots must show actual drawn text/textures/controls, never a pasted reference.
External raylib/GL/EGL/DRM, fonts/assets and board input ABI remain explicit; generic
host rendering is not AGNOS/device acceptance. Parent reviews screenshots independently.
