//! Exact renderer tokens derived from `cluster_renderer.py`; original MIT source.

pub const ROUTE_CONTROL_PANEL_X: f64 = 340.0;
pub const ROUTE_CONTROL_PANEL_Y: f64 = 406.0;
pub const ROUTE_CONTROL_PANEL_W: f64 = 1040.0;
pub const ROUTE_CONTROL_PANEL_H: f64 = 34.0;
pub const ROUTE_CONTROL_SEEK_Y: f64 = 424.0;
pub const ROUTE_CONTROL_BAR_X: f64 = 482.0;
pub const ROUTE_CONTROL_BAR_W: f64 = 756.0;
pub const NAVI_LIVE_PANEL_RIGHT: f64 = 1916.0;
pub const NAVI_LIVE_PANEL_W: f64 = 792.0;
pub const NAVI_LIVE_PANEL_X: f64 = 1124.0;
pub const TRIP_REPORT_PANEL_X: f64 = 1124.0;
pub const TRIP_REPORT_PANEL_Y: f64 = 1.0;
pub const TRIP_REPORT_PANEL_W: f64 = 792.0;
pub const TRIP_REPORT_PANEL_H: f64 = 478.0;
pub const TRIP_REPORT_CACHE_REFRESH_SECONDS: f64 = 1.0;
pub const CAMERA_BACKGROUND_X: f64 = 0.0;
pub const CAMERA_BACKGROUND_Y: f64 = 0.0;
pub const CAMERA_BACKGROUND_W: f64 = 1124.0;
pub const CAMERA_BACKGROUND_H: f64 = 480.0;
pub const CAMERA_BACKGROUND_ALPHA: f64 = 220.0;
pub const CAMERA_BACKGROUND_VIGNETTE_ALPHA: f64 = 32.0;
pub const CLUSTER_ALERT_SIZE_SMALL: i64 = 1;
pub const CLUSTER_ALERT_SIZE_MID: i64 = 2;
pub const CLUSTER_ALERT_SIZE_FULL: i64 = 3;
pub const CLUSTER_ALERT_TEXT_STROKE: [u8; 4] = [0, 0, 0, 255];
pub const CAMERA_BACKGROUND_VERTICAL_BIAS: f64 = 0.75;
pub const CAMERA_OVERLAY_VEHICLE_ROAD_HEIGHT_M: f64 = 0.025;
pub const CAMERA_OVERLAY_MIN_DEPTH_M: f64 = 0.5;
pub const CAMERA_OVERLAY_FRAME_MIN_SIZE_PX: f64 = 24.0;
pub const CAMERA_OVERLAY_FRAME_MAX_WIDTH_RATIO: f64 = 0.22;
pub const CAMERA_OVERLAY_FRAME_MAX_HEIGHT_RATIO: f64 = 0.45;
pub const CAMERA_OVERLAY_FRAME_MAX_ASPECT: f64 = 2.4;
pub const CAMERA_OVERLAY_FRAME_EDGE_PAD_PX: f64 = 3.0;
pub const CAMERA_OVERLAY_FRAME_ROUND_SEGMENTS: usize = 4;
pub const CAMERA_OVERLAY_DEFAULT_HEIGHT_M: f64 = 1.22;
pub const CAMERA_OVERLAY_Z_OFFSET_DEFAULT_M: f64 = 0.0;
pub const CAMERA_OVERLAY_Z_OFFSET_MIN_M: f64 = -0.8;
pub const CAMERA_OVERLAY_Z_OFFSET_MAX_M: f64 = 0.4;
pub const CAMERA_OVERLAY_Z_OFFSET_STEP_M: f64 = 0.05;
pub const CAMERA_OVERLAY_TUNE_PANEL_Y: f64 = 368.0;
pub const CAMERA_OVERLAY_PITCH_OFFSET_DEFAULT_DEG: f64 = 0.0;
pub const CAMERA_OVERLAY_PITCH_OFFSET_MIN_DEG: f64 = -3.0;
pub const CAMERA_OVERLAY_PITCH_OFFSET_MAX_DEG: f64 = 3.0;
pub const CAMERA_OVERLAY_PITCH_OFFSET_STEP_DEG: f64 = 0.2;
pub const CAMERA_OVERLAY_PITCH_TUNE_PANEL_Y: f64 = 334.0;
pub const CORNER_RADAR_COIN_HEIGHT_M: f64 = 0.1;
pub const CORNER_RADAR_COIN_SLICES: usize = 28;
pub const CORNER_RADAR_COIN_FILL: [u8; 3] = [52, 210, 230];
pub const CORNER_RADAR_COIN_SIDE: [u8; 3] = [20, 116, 132];
pub const CORNER_RADAR_COIN_RING: [u8; 3] = [185, 248, 255];
pub const CORNER_RADAR_COIN_LEAD_ONE_FILL: [u8; 3] = [255, 170, 36];
pub const CORNER_RADAR_COIN_LEAD_ONE_SIDE: [u8; 3] = [138, 82, 18];
pub const CORNER_RADAR_COIN_LEAD_ONE_RING: [u8; 3] = [255, 190, 54];
pub const CORNER_RADAR_COIN_LEAD_TWO_FILL: [u8; 3] = [255, 218, 64];
pub const CORNER_RADAR_COIN_LEAD_TWO_SIDE: [u8; 3] = [142, 96, 16];
pub const CORNER_RADAR_COIN_LEAD_TWO_RING: [u8; 3] = [255, 242, 132];
pub const CORNER_RADAR_COIN_CUTIN_FILL: [u8; 3] = [198, 78, 238];
pub const CORNER_RADAR_COIN_CUTIN_SIDE: [u8; 3] = [94, 34, 122];
pub const CORNER_RADAR_COIN_CUTIN_RING: [u8; 3] = [238, 174, 255];
pub const CORNER_RADAR_COIN_SLOWER_FILL: [u8; 3] = [54, 136, 255];
pub const CORNER_RADAR_COIN_SLOWER_SIDE: [u8; 3] = [24, 65, 132];
pub const CORNER_RADAR_COIN_SLOWER_RING: [u8; 3] = [146, 198, 255];
pub const CORNER_RADAR_COIN_STOPPED_FILL: [u8; 3] = [132, 140, 148];
pub const CORNER_RADAR_COIN_STOPPED_SIDE: [u8; 3] = [68, 74, 80];
pub const CORNER_RADAR_COIN_STOPPED_RING: [u8; 3] = [205, 212, 218];
pub const CORNER_RADAR_COIN_STOPPED_MAX_SPEED_KPH: f64 = 3.0;
pub const TPMS_LOW_PRESSURE_PSI: f64 = 31.0;
pub const TURN_SIGNAL_LEFT_CENTER_X: f64 = 610.0;
pub const TURN_SIGNAL_RIGHT_CENTER_X: f64 = 1310.0;
pub const TURN_SIGNAL_CENTER_Y: f64 = 72.0;
pub const TURN_SIGNAL_HEAD_HALF_HEIGHT: f64 = 38.0;
pub const TURN_SIGNAL_MID_CENTER_X: f64 = 960.0;
pub const LANE_TURN_SIGNAL_LEFT_CENTER_X: f64 = 760.0;
pub const LANE_TURN_SIGNAL_RIGHT_CENTER_X: f64 = 1160.0;
pub const LANE_TURN_SIGNAL_CENTER_Y: f64 = 350.0;
pub const DRIVE_STATUS_BASE_BOX_SIZE: f64 = 46.0;
pub const DRIVE_STATUS_ROW_HEIGHT: f64 = 76.0;
pub const DRIVE_STATUS_SCALE: f64 = 1.652_173_913_043_478_3;
pub const GEAR_STATUS_CENTER_X: f64 = 900.0;
pub const GEAR_STATUS_CENTER_Y: f64 = 72.0;
pub const GEAR_STATUS_BOX_SIZE: f64 = 62.319_999_999_999_99;
pub const GEAR_STATUS_FONT_SIZE: f64 = 46.062_608_695_652_166;
pub const GEAR_STATUS_OUTLINE_WIDTH: f64 = 3.304_347_826_086_956_5;
pub const FOLLOW_STATUS_CENTER_X: f64 = 1020.0;
pub const FOLLOW_STATUS_W: f64 = 160.0;
pub const FOLLOW_STATUS_H: f64 = 69.391_304_347_826_1;
pub const FOLLOW_STATUS_GAP_BARS: f64 = 4.0;
pub const FOLLOW_GAP_ACTIVE: [u8; 4] = [103, 255, 78, 255];
pub const FOLLOW_GAP_INACTIVE: [u8; 4] = [118, 122, 128, 150];
pub const FOLLOW_GAP_BAR_W: f64 = 5.4;
pub const FOLLOW_GAP_BAR_H: f64 = 7.7;
pub const FOLLOW_GAP_BAR_R: f64 = 1.3;
pub const FOLLOW_GAP_BAR_SCALE: f64 = 2.891_304_347_826_087;
pub const FOLLOW_GAP_BAR_STEP_X: f64 = 6.3;
pub const FOLLOW_GAP_ICON_ASPECT: f64 = 1.6;
pub const FOLLOW_GAP_ICON_H: f64 = 52.869_565_217_391_305;
pub const FOLLOW_GAP_ICON_W: f64 = 84.591_304_347_826_1;
pub const TOP_CRUISE_CENTER_X: f64 = 1222.0;
pub const TOP_CRUISE_FONT_SIZE: f64 = 44.608_695_652_173_914;
pub const TOP_CRUISE_UNIT_FONT_SIZE: f64 = 44.608_695_652_173_914;
pub const WIFI_STATUS_CENTER_X: f64 = 160.0;
pub const WIFI_STATUS_ICON_SIZE: f64 = 48.0;
pub const EGPU_STATUS_CENTER_X: f64 = 455.0;
pub const EGPU_STATUS_W: f64 = 76.0;
pub const EGPU_STATUS_H: f64 = 34.0;
pub const EGPU_STATUS_FONT_SIZE: f64 = 19.0;
pub const NAV_STATUS_CENTER_X: f64 = 138.0;
pub const NAV_STATUS_CENTER_Y: f64 = 99.0;
pub const NAV_STATUS_FONT_SIZE: f64 = 22.0;
pub const LFA_STATUS_CENTER_X: f64 = 70.0;
pub const LFA_STATUS_ICON_SIZE: f64 = 56.173_913_043_478_26;
pub const LFA_LANE_ICON_WIDTH_SCALE: f64 = 2.0;
pub const LFA_LANE_ICON_TOP_OFFSET: f64 = 3.0;
pub const TOP_STATUS_CENTER_Y: f64 = 55.0;
pub const TOP_CLOCK_CENTER_X: f64 = 315.0;
pub const TOP_ICON_SIZE: f64 = 56.173_913_043_478_26;
pub const DRIVE_STATUS_BOX_RADIUS: f64 = 13.217_391_304_347_826;
pub const SPEED_HUD_SCALE: f64 = 0.8;
pub const SPEED_PANEL_X: f64 = 18.0;
pub const SPEED_PANEL_Y: f64 = 295.0;
pub const SPEED_PANEL_W: f64 = 304.0;
pub const SPEED_PANEL_H: f64 = 113.600_000_000_000_01;
pub const SPEED_VALUE_CENTER_X: f64 = 85.2;
pub const SPEED_VALUE_CENTER_Y: f64 = 351.0;
pub const SPEED_VALUE_FONT_SIZE: f64 = 89.600_000_000_000_01;
pub const CRUISE_SPEED_CENTER_Y: f64 = 339.0;
pub const CRUISE_SET_SPEED_CENTER_X: f64 = 249.200_000_000_000_02;
pub const CRUISE_SET_SPEED_FONT_SIZE: f64 = 46.400_000_000_000_006;
pub const CRUISE_OVERRIDE_SPEED_CENTER_X: f64 = 331.6;
pub const CRUISE_OVERRIDE_SPEED_CENTER_Y: f64 = 319.0;
pub const CRUISE_OVERRIDE_LABEL_CENTER_Y: f64 = 291.8;
pub const CRUISE_OVERRIDE_LABEL_FONT_SIZE: f64 = 20.0;
pub const CRUISE_OVERRIDE_SPEED_FONT_SIZE: f64 = 41.6;
pub const SPEED_GAP_CENTER_X: f64 = 297.68;
pub const SPEED_GAP_CENTER_Y: f64 = 389.856;
pub const SPEED_GAP_FONT_SIZE: f64 = 28.0;
pub const SPEED_GEAR_CENTER_X: f64 = 356.0;
pub const SPEED_GEAR_W: f64 = 45.0;
pub const SPEED_GEAR_H: f64 = 58.0;
pub const SPEED_GEAR_CENTER_Y: f64 = 379.6;
pub const SPEED_GEAR_FONT_SIZE: f64 = 48.0;
pub const SPEED_EV_CENTER_X: f64 = 181.0;
pub const SPEED_EV_CENTER_Y: f64 = 351.0;
pub const SPEED_EV_FONT_SIZE: f64 = 28.0;
pub const SPEED_MODEL_TRAFFIC_CENTER_X: f64 = 47.2;
pub const SPEED_MODEL_TRAFFIC_CENTER_Y: f64 = 286.0;
pub const SPEED_MODEL_TRAFFIC_ICON_SIZE: f64 = 34.0;
pub const SPEED_DRIVING_MODE_GAP: f64 = 5.0;
pub const SPEED_DRIVING_MODE_BASE_CENTER_OFFSET_X: f64 = 41.0;
pub const SPEED_DRIVING_MODE_CENTER_X: f64 = 105.2;
pub const SPEED_DRIVING_MODE_CENTER_Y: f64 = 286.0;
pub const SPEED_DRIVING_MODE_FONT_SIZE: f64 = 27.0;
pub const SIDE_GAUGE_TOP: f64 = 88.0;
pub const SIDE_GAUGE_BOTTOM: f64 = 186.0;
pub const SIDE_GAUGE_LOWER_TOP: f64 = 248.0;
pub const SIDE_GAUGE_LOWER_BOTTOM: f64 = 346.0;
pub const SIDE_GAUGE_WIDTH: f64 = 62.0;
pub const SIDE_GAUGE_VALUE_Y: f64 = 64.0;
pub const SIDE_GAUGE_LABEL_OFFSET: f64 = 15.0;
pub const SIDE_GAUGE_LEFT_CENTER_X: f64 = 968.0;
pub const SIDE_GAUGE_COLUMN_GAP: f64 = 78.0;
pub const SIDE_GAUGE_OUTLINE: [u8; 4] = [240, 244, 248, 190];
pub const SPEED_LIMIT_SIGN_CENTER_X: f64 = 80.0;
pub const SPEED_LIMIT_SIGN_CENTER_Y: f64 = 160.0;
pub const SPEED_LIMIT_SIGN_RADIUS: f64 = 42.0;
pub const SPEED_LIMIT_SIGN_RING_WIDTH: f64 = 6.0;
pub const CRUISE_OVERRIDE_APPLY_COLOR: [u8; 3] = [184, 112, 24];
pub const SYSTEM_PANEL_X: f64 = 1416.0;
pub const SYSTEM_PANEL_Y: f64 = 118.0;
pub const SYSTEM_PANEL_W: f64 = 476.0;
pub const NAVI_LIVE_PANEL_Y: f64 = 1.0;
pub const NAVI_LIVE_PANEL_H: f64 = 478.0;
pub const NAVI_WORLD_VIEW_SHIFT_X: f64 = 398.0;
pub const FULLSCREEN_3D_SIDE_WIDGET_OFFSET_X: f64 = 796.0;
pub const FULLSCREEN_3D_CLOCK_CENTER_X: f64 = 960.0;
pub const FULLSCREEN_3D_TRAFFIC_PANEL_RIGHT: f64 = 1892.0;
pub const NAVI_MAP_BACKGROUND: [u8; 4] = [0, 0, 0, 255];
pub const TPMS_STATUS_CENTER_X: f64 = 1062.0;
pub const TPMS_STATUS_VALUE_CENTER_Y: f64 = 429.5;
pub const TPMS_STATUS_CAR_CENTER_Y: f64 = 429.5;
pub const TPMS_STATUS_COLUMN_OFFSET: f64 = 29.5;
pub const TPMS_STATUS_ROW_OFFSET: f64 = 24.0;
pub const TPMS_STATUS_CAR_W: f64 = 38.0;
pub const TPMS_STATUS_CAR_H: f64 = 72.0;
pub const TPMS_STATUS_WHEEL_W: f64 = 32.0;
pub const TPMS_STATUS_WHEEL_H: f64 = 30.0;
pub const TPMS_STATUS_ICON_W: f64 = 104.0;
pub const TPMS_STATUS_ICON_H: f64 = 78.0;
pub const FULLSCREEN_3D_CORE_USAGE_RIGHT_X: f64 = 1788.0;
pub const TPMS_STATUS_FONT_SIZE: f64 = 21.0;
pub const NAVI_LIVE_ICON_X: f64 = 1196.0;
pub const NAVI_LIVE_ICON_Y: f64 = 100.0;
pub const NAVI_LIVE_ICON_SIZE: f64 = 78.0;
pub const NAVI_LIVE_CONTENT_X: f64 = 1260.0;
pub const NAVI_LIVE_CONTENT_W: f64 = 634.0;
pub const NAVI_LIVE_NEXT_Y: f64 = 185.0;
pub const NAVI_LIVE_LANE_Y: f64 = 259.0;
pub const NAVI_LIVE_FOOTER_Y: f64 = 329.0;
pub const NAVI_LIVE_GUIDANCE_MEDIA_SCALE: f64 = 1.2;
pub const NAVI_TURN_LEFT_TYPES: &[i64] = &[
    7, 12, 16, 17, 44, 75, 76, 102, 105, 112, 115, 118, 1000, 1002, 1006,
];
pub const NAVI_TURN_RIGHT_TYPES: &[i64] = &[
    6, 13, 19, 43, 73, 74, 101, 104, 111, 114, 117, 123, 124, 1001, 1003, 1007,
];
pub const NAVI_TURN_ROUNDABOUT_TYPES: &[i64] =
    &[131, 132, 133, 134, 135, 136, 137, 138, 139, 140, 141, 142];
pub const NAVI_TRAFFIC_PANEL_RIGHT: f64 = 1406.0;
pub const NAVI_TRAFFIC_PANEL_H: f64 = 90.0;
pub const NAVI_TRAFFIC_PANEL_Y: f64 = 120.0;
pub const NAVI_TRAFFIC_SIGNAL_SIZE: f64 = 58.0;
pub const NAVI_TRAFFIC_SIGNAL_GAP: f64 = 10.0;
pub const NAVI_TRAFFIC_TEXT_GAP: f64 = 14.0;
pub const NAVI_TRAFFIC_PANEL_PAD_X: f64 = 16.0;
pub const NAVI_TRAFFIC_BG_LIGHT: [u8; 3] = [62, 68, 81];
pub const NAVI_TRAFFIC_BG_DARK: [u8; 3] = [18, 21, 27];
pub const NAVI_TRAFFIC_BG_OUTLINE: [u8; 3] = [238, 241, 246];
pub const NAVI_TRAFFIC_OFF_LIGHT: [u8; 3] = [40, 43, 51];
pub const NAVI_TRAFFIC_OFF_DARK: [u8; 3] = [36, 39, 47];
pub const NAVI_TRAFFIC_OFF_ARROW: [u8; 3] = [58, 61, 70];
pub const NAVI_TRAFFIC_RED: [u8; 3] = [255, 111, 111];
pub const NAVI_TRAFFIC_GREEN: [u8; 3] = [103, 255, 78];
pub const NAVI_GUIDANCE_IMAGE_X: f64 = 1440.0;
pub const NAVI_GUIDANCE_IMAGE_Y: f64 = 328.0;
pub const NAVI_GUIDANCE_IMAGE_W: f64 = 428.0;
pub const NAVI_GUIDANCE_IMAGE_H: f64 = 270.0;
pub const NAVI_MODE_LEFT_W: f64 = 520.0;
pub const NAVI_MODE_MAP_X: f64 = 520.0;
pub const NAVI_MODE_MAP_W: f64 = 880.0;
pub const NAVI_MODE_RIGHT_X: f64 = 1400.0;
pub const NAVI_MODE_RIGHT_W: f64 = 520.0;
pub const KOREAN_FONT_BASE_SIZE: usize = 32;
pub const SYSTEM_STATS_REFRESH_SECONDS: f64 = 1.0;
pub const TEXT_MEASURE_CACHE_LIMIT: usize = 1024;
pub const STROKED_TEXT_TEXTURE_CACHE_LIMIT: usize = 384;
pub const STROKED_TEXT_TEXTURE_PADDING_PX: usize = 2;
pub const TRIANGLE_STRIP_POINT_CACHE_LIMIT: usize = 256;
pub const DEBUG_PLOT_MAX_SAMPLES: usize = 360;
pub const DEBUG_PLOT_SAMPLE_SECONDS: f64 = 0.05;
pub const DEBUG_PLOT_MARGIN: f64 = 18.0;
pub const DEBUG_PLOT_FULL_X: f64 = 500.0;
pub const DEBUG_PLOT_FULL_Y: f64 = 18.0;
pub const DEBUG_PLOT_FULL_W: f64 = 1392.0;
pub const DEBUG_PLOT_FULL_H: f64 = 444.0;
pub const DEBUG_PLOT_RIGHT_X: f64 = 1124.0;
pub const DEBUG_PLOT_RIGHT_Y: f64 = 1.0;
pub const DEBUG_PLOT_RIGHT_W: f64 = 792.0;
pub const DEBUG_PLOT_RIGHT_H: f64 = 478.0;
pub const GIT_STATUS_MARGIN: f64 = 2.0;
pub const GIT_STATUS_BOTTOM_MARGIN: f64 = 12.0;
pub const GIT_STATUS_DOT_RADIUS: f64 = 7.0;
pub const GIT_STATUS_DOT_TEXT_GAP: f64 = 6.0;
pub const GIT_STATUS_MAX_TEXT_W: f64 = 610.0;
pub const FPS_STATUS_MARGIN: f64 = 4.0;
pub const FPS_STATUS_DOT_RADIUS: f64 = 7.0;
pub const FPS_STATUS_DOT_TEXT_GAP: f64 = 6.0;
pub const FPS_STATUS_MAX_TEXT_W: f64 = 220.0;
pub const CLUSTER_CORE_USAGE_MARGIN: f64 = 2.0;
pub const CLUSTER_CORE_USAGE_MAX_TEXT_W: f64 = 760.0;
pub const RADAR_LABEL_DISTANCE_FONT_SIZE: f64 = 16.0;
pub const RADAR_LABEL_SPEED_FONT_SIZE: f64 = 14.0;
pub const VEHICLE_BADGE_DISTANCE_FONT_SIZE: f64 = 17.0;
pub const VEHICLE_BADGE_SPEED_FONT_SIZE: f64 = 15.0;
pub const LEAD_ONE_DISTANCE_FONT_SIZE: f64 = 24.0;
pub const LEAD_ONE_SPEED_FONT_SIZE: f64 = 22.0;
pub const RADAR_LABEL_ANCHOR_Z_OFFSET_M: f64 = 0.3;
pub const VEHICLE_BADGE_ANCHOR_Z_OFFSET_M: f64 = 0.32;
pub const WORLD_LABEL_NEAR_M: f64 = 18.0;
pub const WORLD_LABEL_FAR_M: f64 = 180.0;
pub const WORLD_LABEL_MIN_SCALE: f64 = 0.56;
pub const WORLD_LABEL_TEXTURE_CACHE_LIMIT: usize = 512;
pub const WORLD_LABEL_TEXTURE_SIZE_GRID: f64 = 0.25;
pub const WORLD_LABEL_TEXTURE_PADDING_PX: usize = 4;
pub const DEFAULT_VEHICLE_MATERIAL_COLOR: [u8; 4] = [142, 150, 156, 255];
pub const NV12_PACK_VERTEX_SHADER: &str = r"
attribute vec3 vertexPosition;
attribute vec2 vertexTexCoord;
attribute vec4 vertexColor;

varying vec2 fragTexCoord;
varying vec4 fragColor;

uniform mat4 mvp;

void main() {
    fragTexCoord = vertexTexCoord;
    fragColor = vertexColor;
    gl_Position = mvp * vec4(vertexPosition, 1.0);
}
";
pub const NV12_PACK_FRAGMENT_SHADER: &str = r"
#ifdef GL_ES
precision mediump float;
#endif

varying vec2 fragTexCoord;
varying vec4 fragColor;

uniform sampler2D texture0;
uniform vec2 srcSize;
uniform vec2 packedSize;
uniform int plane;
uniform int flipX;

const float Y_PAD = 0.062745;
const float UV_PAD = 0.501961;

vec3 sampleRgb(float x, float y) {
    if (flipX != 0) {
        // The portrait upload transform maps screen horizontal correction to source Y.
        y = srcSize.y - 1.0 - y;
    }
    vec2 clamped = clamp(vec2(x, y), vec2(0.0), srcSize - vec2(1.0));
    return texture2D(texture0, (clamped + vec2(0.5)) / srcSize).rgb;
}

float y601(vec3 rgb) {
    return clamp(0.062745 + 0.256788 * rgb.r + 0.504129 * rgb.g + 0.097906 * rgb.b, 0.0, 1.0);
}

float u601(vec3 rgb) {
    return clamp(0.501961 - 0.148223 * rgb.r - 0.290993 * rgb.g + 0.439216 * rgb.b, 0.0, 1.0);
}

float v601(vec3 rgb) {
    return clamp(0.501961 + 0.439216 * rgb.r - 0.367788 * rgb.g - 0.071427 * rgb.b, 0.0, 1.0);
}

vec3 sample2x2(float x, float y) {
    return (
        sampleRgb(x, y) +
        sampleRgb(x + 1.0, y) +
        sampleRgb(x, y + 1.0) +
        sampleRgb(x + 1.0, y + 1.0)
    ) * 0.25;
}

float packedY(float x, float y) {
    if (x >= srcSize.x || y >= srcSize.y) {
        return Y_PAD;
    }
    return y601(sampleRgb(x, y));
}

vec2 packedUV(float x, float y) {
    if (x >= srcSize.x || y >= srcSize.y) {
        return vec2(UV_PAD, UV_PAD);
    }
    vec3 rgb = sample2x2(x, y);
    return vec2(u601(rgb), v601(rgb));
}

void main() {
    vec2 packedCoord = min(floor(fragTexCoord * packedSize), packedSize - vec2(1.0));
    float baseX = packedCoord.x * 4.0;
    if (plane == 0) {
        float y = packedCoord.y;
        gl_FragColor = vec4(
            packedY(baseX, y),
            packedY(baseX + 1.0, y),
            packedY(baseX + 2.0, y),
            packedY(baseX + 3.0, y)
        );
    } else {
        float y = packedCoord.y * 2.0;
        vec2 left = packedUV(baseX, y);
        vec2 right = packedUV(baseX + 2.0, y);
        gl_FragColor = vec4(left.x, left.y, right.x, right.y);
    }
}
";
pub const NAVI_YUV420_FRAGMENT_SHADER: &str = r"
#ifdef GL_ES
precision mediump float;
#endif

varying vec2 fragTexCoord;

uniform sampler2D texture0;
uniform sampler2D texture1;
uniform sampler2D texture2;
uniform vec2 uvScaleU;
uniform vec2 uvScaleV;

void main() {
    float y = 1.164383 * (texture2D(texture0, fragTexCoord).r - 0.062745);
    float u = texture2D(texture1, fragTexCoord * uvScaleU).r - 0.501961;
    float v = texture2D(texture2, fragTexCoord * uvScaleV).r - 0.501961;
    vec3 rgb = vec3(
        y + 1.596027 * v,
        y - 0.391762 * u - 0.812968 * v,
        y + 2.017232 * u
    );
    gl_FragColor = vec4(clamp(rgb, 0.0, 1.0), 1.0);
}
";
pub const NAVI_EXTERNAL_VERTEX_SHADER: &str = r"
#version 300 es
precision mediump float;
in vec3 vertexPosition;
in vec2 vertexTexCoord;
in vec4 vertexColor;
uniform mat4 mvp;
out vec2 fragTexCoord;
void main() {
    fragTexCoord = vertexTexCoord;
    gl_Position = mvp * vec4(vertexPosition, 1.0);
}
";
pub const NAVI_EXTERNAL_FRAGMENT_SHADER: &str = r"
#version 300 es
#extension GL_OES_EGL_image_external_essl3 : enable
precision mediump float;
in vec2 fragTexCoord;
uniform samplerExternalOES texture0;
out vec4 fragColor;
void main() {
    fragColor = texture(texture0, fragTexCoord);
}
";
pub const NAVI_EGL_IMAGE_CACHE_LIMIT: usize = 10;
