use super::{
    geometry::road_world_x,
    math::{clamp, lerp_vec3, smoothstep},
    CameraSpec, Vec3,
};
use crate::{
    config::{
        CLUSTER_CAMERA_VIEW_MODE_EGO_BOTTOM, EGO_FORWARD_M, SURROUND_CAMERA_DISTANCE_M,
        SURROUND_CAMERA_HEIGHT_M, SURROUND_MAX_PITCH_DEG, SURROUND_MAX_YAW_DEG,
        SURROUND_TARGET_FORWARD_M, SURROUND_TARGET_HEIGHT_M,
    },
    models::ClusterUiState,
};

#[must_use]
pub fn ego_anchor_x_m(state: &ClusterUiState, width: f64) -> f64 {
    road_world_x(
        clamp(state.ego_lane_offset, -1.25, 1.25),
        EGO_FORWARD_M,
        state.steering,
        width,
    )
}

#[must_use]
pub fn blend_camera(start: CameraSpec, end: CameraSpec, amount: f64) -> CameraSpec {
    let amount = clamp(amount, 0.0, 1.0);
    CameraSpec {
        position: lerp_vec3(start.position, end.position, amount),
        target: lerp_vec3(start.target, end.target, amount),
        fovy_deg: start.fovy_deg + (end.fovy_deg - start.fovy_deg) * amount,
    }
}

#[must_use]
pub fn scene_camera(state: &ClusterUiState, width: f64, anchor_x: f64) -> CameraSpec {
    let drive = if state.camera_view_mode == CLUSTER_CAMERA_VIEW_MODE_EGO_BOTTOM {
        CameraSpec {
            position: Vec3::new(0.0, -6.0, 5.0),
            target: Vec3::new(0.0, 14.0, -1.0),
            fovy_deg: 44.0,
        }
    } else {
        CameraSpec {
            position: Vec3::new(0.0, -11.0, 6.0),
            target: Vec3::new(0.0, 12.0, -0.2),
            fovy_deg: 31.0,
        }
    };
    if !state.surround_view_active {
        return drive;
    }
    let x = ego_anchor_x_m(state, width) - anchor_x;
    let yaw = clamp(
        state.surround_yaw_deg,
        -SURROUND_MAX_YAW_DEG,
        SURROUND_MAX_YAW_DEG,
    )
    .to_radians();
    let pitch = clamp(
        state.surround_pitch_deg,
        -SURROUND_MAX_PITCH_DEG,
        SURROUND_MAX_PITCH_DEG,
    );
    let target_forward = SURROUND_TARGET_FORWARD_M + pitch * 0.12;
    let orbit = CameraSpec {
        position: Vec3::new(
            x - yaw.sin() * SURROUND_CAMERA_DISTANCE_M,
            EGO_FORWARD_M - yaw.cos() * SURROUND_CAMERA_DISTANCE_M,
            SURROUND_CAMERA_HEIGHT_M + pitch * 0.035,
        ),
        target: Vec3::new(
            x + yaw.sin() * target_forward,
            EGO_FORWARD_M + yaw.cos() * target_forward,
            SURROUND_TARGET_HEIGHT_M,
        ),
        fovy_deg: 40.0,
    };
    blend_camera(
        drive,
        orbit,
        smoothstep(
            (state.surround_yaw_deg.abs() / SURROUND_MAX_YAW_DEG)
                .max(state.surround_pitch_deg.abs() / SURROUND_MAX_PITCH_DEG),
        ),
    )
}
