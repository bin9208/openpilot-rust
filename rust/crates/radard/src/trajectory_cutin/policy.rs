use super::{
    constants::*,
    history::{
        front_confidence, front_noise, horizon, median_slope, motion, values_since, LatchInput,
        Track,
    },
    support::{existence_supported, reported_inward, vision_brackets, vision_supports},
    Estimate, Frame,
};
use crate::{
    math::{maximum, minimum},
    model::VisionLead,
    path::Projection,
    point::{Point, TrackId},
    scope::turning_entry_allowed,
    Error,
};

pub struct Evaluation<'a, 'b> {
    pub frame: &'a Frame<'b>,
    pub point: &'a Point,
    pub cross: Option<&'a Point>,
    pub projection: Projection,
    pub vision: Option<VisionLead>,
    pub speed: f64,
    pub lane_supported: bool,
    pub sensitivity: i32,
}

pub fn evaluate(state: &mut Track, input: Evaluation<'_, '_>) -> Result<Estimate, Error> {
    let Evaluation {
        frame,
        point,
        cross,
        projection,
        vision,
        speed,
        lane_supported,
        sensitivity,
    } = input;
    let now = frame.time_s;
    let history_s = state
        .observations
        .back()
        .zip(state.observations.front())
        .filter(|_| state.observations.len() >= 2)
        .map_or(0., |(last, first)| last.time_s - first.time_s);
    let long = motion(&state.observations, LONG_MOTION_WINDOW_S, |value| {
        value.d_path
    });
    let short = motion(&state.observations, SHORT_MOTION_WINDOW_S, |value| {
        value.d_path
    });
    let side = 1_f64.copysign(if projection.d_path != 0. {
        projection.d_path
    } else if point.y_rel != 0. {
        point.y_rel
    } else {
        1.
    });
    let short_inward = maximum(short.inward_rate, maximum(0., -side * short.rate));
    let recent = values_since(&state.observations, 0.50);
    let recent_v_rel_min = recent
        .iter()
        .map(|value| value.v_rel)
        .reduce(minimum)
        .unwrap_or(point.v_rel);
    let recent_v_rel_spread = recent
        .iter()
        .map(|value| value.v_rel)
        .reduce(maximum)
        .unwrap_or(point.v_rel)
        - recent_v_rel_min;
    let recent_abs_yaw_max = values_since(&state.observations, 0.75)
        .iter()
        .map(|value| value.yaw_rate_rad_s.abs())
        .reduce(maximum)
        .unwrap_or(frame.yaw_rate.abs());
    let reported = reported_inward(point, projection, frame.yaw_rate);
    let supported_inward = maximum(long.inward_rate, minimum(short_inward, reported + 0.35));
    let mut horizon_s = horizon(speed);
    let unstable_fast = point.v_rel.abs() >= 5.
        && (long.jittering
            || long.consistency < 0.75
            || (long.inward_rate - reported).abs() > 0.75);
    if unstable_fast {
        horizon_s *= maximum(0.50, 1. - 0.07 * (point.v_rel.abs() - 5.));
    }
    let target_speed = median_slope(
        &state.observations.iter().copied().collect::<Vec<_>>(),
        |value| value.global_path_s,
        LONG_MOTION_WINDOW_S,
    );
    let relative_speed = if history_s >= 0.20 {
        target_speed - speed
    } else {
        point.v_rel
    };
    let relative_speed = maximum(point.v_rel - 3., minimum(point.v_rel + 3., relative_speed));
    let future_distance = point.d_rel + relative_speed * horizon_s;
    let future_path = projection.d_path + long.rate * horizon_s;
    let clearance = maximum(0., projection.d_path.abs() - PATH_OVERLAP_HALF_WIDTH_M);
    let overlap_time = if clearance <= 0. {
        Some(0.)
    } else if supported_inward > 0.05 {
        Some(clearance / supported_inward)
    } else {
        None
    };
    let vision_supported = vision_supports(point, frame.model, false)?;
    let cross_supported = cross.is_some();
    let paired_front_overlap = point.corner()
        && cross.is_some_and(|front| {
            front.source == "frontRadar"
                && (front.y_rel.abs() <= 2.15
                    || (front.y_rel.abs() <= PAIRED_CLOSE_BODY_HALF_WIDTH_M
                        && reported >= PAIRED_OUTER_BODY_MIN_REPORTED_INWARD_MPS
                        && point.v_rel <= PAIRED_OUTER_BODY_MAX_VREL_MPS
                        && recent_abs_yaw_max < PAIRED_OUTER_BODY_MAX_ABS_YAW_RATE_RAD_S
                        && long.inward_progress >= PAIRED_OUTER_BODY_MIN_INWARD_PROGRESS_M))
        });
    let exists = existence_supported(point, history_s, vision_supported, cross_supported);
    let started_outside = state.observations.iter().any(|value| {
        (if point.source == "frontRadar" {
            value.y_rel
        } else {
            value.d_path
        })
        .abs()
            >= PATH_OVERLAP_HALF_WIDTH_M + OUTSIDE_HISTORY_MARGIN_M
    });
    if point.measured
        && projection.d_path.abs() >= PATH_OVERLAP_HALF_WIDTH_M + OUTSIDE_HISTORY_MARGIN_M
    {
        state.outside_until_s = now + SLOW_ENTRY_OUTSIDE_MEMORY_S;
        state.outside_side = side;
    }
    let current_overlap = projection.d_path.abs() <= PATH_OVERLAP_HALF_WIDTH_M;
    let paired_inward = point.corner()
        && cross.is_some_and(|front| front.source == "frontRadar")
        && history_s >= 0.75
        && long.inward_progress >= 0.15
        && short.inward_progress >= 0.06
        && long.inward_rate >= MIN_INWARD_RATE_MPS
        && short_inward >= MIN_INWARD_RATE_MPS
        && long.consistency >= 0.75
        && short.consistency >= 0.75
        && reported >= MIN_INWARD_RATE_MPS
        && (reported - long.inward_rate).abs() <= 0.50;
    let independent_support =
        paired_inward && vision_brackets(point, cross, vision, frame.primary, frame.path);
    if independent_support {
        state.paired_motion_support_until_s = now + MAX_OBSERVATION_GAP_S;
    } else if !paired_inward {
        state.paired_motion_support_until_s = f64::NEG_INFINITY;
    }
    let paired_inward_supported = now <= state.paired_motion_support_until_s;
    let ambiguous = point.corner()
        && cross.is_some_and(|front| {
            front.source == "frontRadar"
                && point.d_rel <= CROSS_SENSOR_SLOT_HANDOFF_MAX_DREL_M
                && point.v_rel < -0.1
                && recent_abs_yaw_max >= PAIRED_OUTER_BODY_RANGE_CHECK_MIN_ABS_YAW_RATE_RAD_S
                && !vision_supported
                && !paired_inward_supported
                && !current_overlap
                && front.y_rel.abs() > 2.15
                && (point.d_rel - front.d_rel).abs() > PAIRED_OUTER_BODY_MAX_DREL_DELTA_M
        });
    if ambiguous {
        state.outer_body_ambiguous_until_s = now + CROSS_SENSOR_ALIAS_HOLD_S;
    } else if vision_supported
        || paired_inward_supported
        || current_overlap
        || cross.is_some_and(|front| front.y_rel.abs() <= 2.15)
    {
        state.outer_body_ambiguous_until_s = f64::NEG_INFINITY;
    }
    let ambiguous = now <= state.outer_body_ambiguous_until_s;
    let rear_pass = point.corner()
        && cross_supported
        && !vision_supported
        && !current_overlap
        && state.minimum_d_rel <= PAIRED_REAR_PASS_MAX_INITIAL_DREL_M
        && point.v_rel >= PAIRED_REAR_PASS_MIN_PULL_AWAY_MPS
        && long.inward_progress < PAIRED_REAR_PASS_MAX_INWARD_PROGRESS_M
        && reported < PAIRED_REAR_PASS_MAX_REPORTED_INWARD_MPS;
    let confirmed_continues = now <= state.cutin_until_s
        && long.inward_rate >= MIN_INWARD_RATE_MPS
        && short_inward >= MIN_INWARD_RATE_MPS
        && long.consistency >= 0.75
        && short.consistency >= 0.75;
    let parallel = point.corner()
        && cross_supported
        && !vision_supported
        && !current_overlap
        && history_s >= PAIRED_PARALLEL_MIN_HISTORY_S
        && point.v_rel.abs() <= PAIRED_PARALLEL_MAX_ABS_VREL_MPS
        && long.inward_progress < PAIRED_PARALLEL_MAX_INWARD_PROGRESS_M
        && reported < PAIRED_PARALLEL_MAX_REPORTED_INWARD_MPS
        && !confirmed_continues;
    let non_entry = rear_pass || parallel;
    let uncorroborated_close_front =
        point.source == "frontRadar" && point.d_rel <= 8. && !vision_supported && !cross_supported;
    let front_half_width = if uncorroborated_close_front {
        PATH_OVERLAP_HALF_WIDTH_M
    } else if point.d_rel <= 20. {
        2.15
    } else {
        PATH_OVERLAP_HALF_WIDTH_M
    };
    let raw_body_overlap = point.y_rel.abs() <= front_half_width;
    let ahead_at_overlap =
        overlap_time.is_some_and(|time| point.d_rel + relative_speed * time > 0.5);
    let predicted_overlap = overlap_time.is_some_and(|time| time <= horizon_s && ahead_at_overlap);
    let recent_overlap_time = if short_inward > 0.05 {
        clearance / short_inward
    } else {
        f64::INFINITY
    };
    let recent_predicted_overlap = recent_overlap_time <= horizon_s
        && point.d_rel + relative_speed * recent_overlap_time > 0.5;
    let consistent = long.inward_progress >= MIN_INWARD_PROGRESS_M
        && supported_inward >= MIN_INWARD_RATE_MPS
        && long.consistency >= MIN_DIRECTION_CONSISTENCY;
    let strong_prediction = long.inward_progress >= MIN_PREDICTED_INWARD_PROGRESS_M
        && long.inward_rate >= MIN_INWARD_RATE_MPS
        && long.consistency >= MIN_DIRECTION_CONSISTENCY;
    let front_history = frame.vision_required_front
        && point.source == "frontRadar"
        && !vision_supported
        && history_s >= 0.50
        && point.d_rel <= 20.
        && projection.d_path.abs() <= 3.20
        && recent_abs_yaw_max < 0.020
        && point.v_rel.abs() <= 5.
        && recent_v_rel_min >= 0.50
        && recent_v_rel_spread <= 1.
        && ((short.inward_progress >= 0.18 && short_inward >= 0.35 && short.consistency >= 0.85)
            || (long.inward_progress >= 0.35
                && supported_inward >= 0.35
                && long.consistency >= 0.75));
    let front_motion = point.source != "frontRadar"
        || vision_supported
        || cross_supported
        || front_history
        || front_confidence(point.d_rel, &long, &short) >= FRONT_LATERAL_CONFIDENCE_MIN;
    let mut body_entry = false;
    if let Some(front) = cross.filter(|front| {
        point.corner()
            && point.measured
            && front.measured
            && front.source == "frontRadar"
            && state.front_observations.len() >= 3
            && state
                .front_observations
                .back()
                .zip(state.front_observations.front())
                .is_some_and(|(last, first)| last.time_s - first.time_s >= 0.50)
            && current_overlap
            && paired_front_overlap
            && 0.8 < point.d_rel
            && point.d_rel <= 8.
            && speed <= 12.
            && -5. <= point.v_rel
            && point.v_rel < -0.1
            && point.d_rel / -point.v_rel >= 1.20
            && now <= state.outside_until_s
            && side == state.outside_side
            && history_s >= 0.75
            && recent_abs_yaw_max < 0.020
            && reported >= 0.10
            && vision.is_some_and(|vision| vision.probability >= 0.90)
    }) {
        if vision_supports(point, frame.model, true)?
            || vision_brackets(point, cross, vision, frame.primary, frame.path)
        {
            let body = motion(&state.observations, LONG_MOTION_WINDOW_S, |value| {
                value.y_rel
            });
            let front_motion = motion(&state.front_observations, LONG_MOTION_WINDOW_S, |value| {
                value.y_rel
            });
            let paired_projection = frame.path.project(front.d_rel, front.y_rel);
            body_entry = body.inward_progress >= 0.10
                && body.net_fraction >= 0.65
                && body.consistency >= 0.80
                && front_motion.inward_progress >= 0.20
                && front_motion.net_fraction >= 0.50
                && front_motion.consistency >= 0.75
                && reported_inward(front, paired_projection, frame.yaw_rate) >= 0.10;
        }
    }
    let jitter_override = body_entry
        || front_history
        || ((vision_supported || cross_supported)
            && long.inward_progress >= 0.45
            && long.consistency >= 0.65);
    let reliable = !long.jittering || jitter_override;
    let fast_pass = point.corner()
        && -point.v_rel >= PAIRED_FAST_PASS_MIN_CLOSING_SPEED_MPS
        && point.d_rel / maximum(-point.v_rel, 0.1) <= PAIRED_FAST_PASS_MAX_TTC_S
        && history_s < PAIRED_FAST_PASS_MIN_HISTORY_S;
    let mut close_entry = point.corner()
        && cross_supported
        && paired_front_overlap
        && point.d_rel <= 8.
        && projection.d_path.abs() <= 2.75
        && long.inward_progress >= 0.06
        && supported_inward >= 0.15
        && long.consistency >= 0.75
        && !fast_pass;
    let direct_entry = point.corner()
        && point.d_rel <= 8.
        && projection.d_path.abs() <= 2.85
        && history_s >= 0.50
        && long.inward_progress >= 0.20
        && supported_inward >= 0.25
        && reported >= 0.20
        && long.consistency >= 0.85
        && predicted_overlap
        && !fast_pass;
    let curve_alias = !turning_entry_allowed(point, projection.d_path, frame.yaw_rate, false)
        || (point.corner()
            && recent_abs_yaw_max >= CORNER_CURVE_ALIAS_MIN_ABS_YAW_RATE_RAD_S
            && point.y_rel.abs() >= CORNER_CURVE_ALIAS_MIN_ABS_YREL_M
            && point.y_rel.abs() - projection.d_path.abs()
                >= CORNER_CURVE_ALIAS_MIN_OFFSET_DISCREPANCY_M
            && !vision_supported
            && reported < PAIRED_OUTER_BODY_MIN_REPORTED_INWARD_MPS)
        || (point.corner()
            && recent_abs_yaw_max >= CORNER_CURVE_ALIAS_MIN_ABS_YAW_RATE_RAD_S
            && reported > CORNER_CURVE_MAX_REPORTED_INWARD_MPS
            && reported - long.inward_rate >= CORNER_CURVE_MIN_RATE_DISAGREEMENT_MPS);
    let front_curve_motion = point.source != "frontRadar"
        || recent_abs_yaw_max < FRONT_CURVE_ALIAS_MIN_ABS_YAW_RATE_RAD_S
        || vision_supported
        || cross_supported
        || reported >= PAIRED_OUTER_BODY_MIN_REPORTED_INWARD_MPS;
    let turning_corner = point.corner() && frame.yaw_rate.abs() >= 0.10;
    let volatile = point.corner() && point.track_id < TrackId(1000);
    let curve_motion = !turning_corner
        || ((cross_supported || vision_supported)
            && (0.15..=3.).contains(&reported)
            && long.inward_progress >= 0.35);
    let slot_motion = !volatile || close_entry || (0.25..=3.).contains(&reported);
    let plausible = !point.corner()
        || vision_supported
        || cross_supported
        || long.inward_rate <= maximum(2., reported + 1.);
    let away_range = !point.corner()
        || vision_supported
        || cross_supported
        || point.v_rel <= 0.5
        || point.d_rel <= 30.;
    let front_range = point.source != "frontRadar"
        || point.d_rel >= FRONT_NEW_CUTIN_MIN_DREL_M
        || state
            .observations
            .iter()
            .any(|value| value.d_rel >= FRONT_NEW_CUTIN_MIN_DREL_M);
    let close_front = point.source != "frontRadar"
        || vision_supported
        || cross_supported
        || point.d_rel >= FRONT_CLOSE_BORN_MIN_DREL_M
        || state
            .observations
            .iter()
            .any(|value| value.d_rel >= FRONT_CLOSE_BORN_MIN_DREL_M)
        || (recent_abs_yaw_max < 0.020
            && reported >= 0.50
            && long.inward_progress >= 0.30
            && long.consistency >= 0.75);
    let mut common = exists
        && !ambiguous
        && (started_outside || body_entry)
        && front_range
        && close_front
        && (point.source != "frontRadar"
            || !frame.vision_required_front
            || (recent_abs_yaw_max < 0.020 && point.v_rel.abs() <= 5.))
        && (point.source != "frontRadar"
            || !frame.vision_required_front
            || vision_supported
            || front_history)
        && MIN_MOVING_VLEAD_MPS < point.v_lead
        && 0.8 < point.d_rel
        && point.d_rel <= CUTIN_MAX_DREL_M
        && reliable
        && (!point.corner()
            || point.d_rel <= 8.
            || vision_supports(point, frame.model, true)?
            || point.y_rel.abs() <= 1.
            || lane_supported)
        && front_curve_motion
        && !curve_alias
        && curve_motion
        && slot_motion
        && plausible
        && away_range
        && (!point.corner() || vision_supported || cross_supported || reported >= 0.50);
    let closing_speed = maximum(maximum(-point.v_rel, -relative_speed), 0.);
    let pass_time = if closing_speed > 0.1 {
        point.d_rel / closing_speed
    } else {
        f64::INFINITY
    };
    let low_speed_entry = speed <= 12. && point.d_rel <= 6. && pass_time >= 1.20;
    let front_entry = front_motion
        && ((raw_body_overlap && pass_time >= 1.20 && consistent)
            || (vision_supported
                && predicted_overlap
                && long.inward_progress >= 0.18
                && supported_inward >= 0.20
                && long.consistency >= 0.75)
            || (front_history && recent_predicted_overlap));
    let approach = point.v_rel <= 0.5
        || current_overlap
        || projection.d_path.abs() <= 2.35
        || reported >= 0.43;
    let commitment = current_overlap
        || reported >= 0.43
        || projection.d_path.abs() <= 2.30
        || (long.inward_progress >= 0.60 && long.consistency >= 0.90);
    let projected_corner = point.d_rel > 8. || vision_supported;
    let corner_entry = body_entry
        || (approach
            && commitment
            && ((current_overlap && consistent)
                || (predicted_overlap && strong_prediction && projected_corner)));
    let paired_ahead = overlap_time.is_some_and(|time| {
        point.d_rel + minimum(minimum(point.v_rel, relative_speed), recent_v_rel_min) * time > 0.5
    });
    let mut front_entry_ahead = false;
    let mut stationary_alias = false;
    if let Some(front) = cross.filter(|_| point.corner()) {
        let front_projection = frame.path.project(front.d_rel, front.y_rel);
        let mut front_inward = reported_inward(front, front_projection, frame.yaw_rate);
        let front_motion = motion(&state.front_observations, LONG_MOTION_WINDOW_S, |value| {
            value.d_path
        });
        if state.front_observations.len() >= 3
            && front_motion.inward_progress >= 0.75 * front_noise(front.d_rel)
            && front_motion.consistency >= 0.75
        {
            front_inward = maximum(
                front_inward,
                -1_f64.copysign(front_projection.d_path) * front_motion.rate,
            );
        }
        let front_clearance = maximum(
            0.,
            front_projection.d_path.abs() - PATH_OVERLAP_HALF_WIDTH_M,
        );
        if front_clearance == 0. {
            front_entry_ahead = front.d_rel > 0.5;
        } else if front_inward >= MIN_INWARD_RATE_MPS {
            let time = front_clearance / front_inward;
            front_entry_ahead = time <= horizon_s && front.d_rel + front.v_rel * time > 0.5;
        }
        stationary_alias = front.source == "frontRadar"
            && front.v_lead.abs() <= MIN_MOVING_VLEAD_MPS
            && !vision_supported
            && !current_overlap
            && !paired_front_overlap
            && front_clearance > 0.
            && front_inward < MIN_INWARD_RATE_MPS
            && reported < MIN_INWARD_RATE_MPS
            && long.inward_rate - reported > 0.75;
    }
    common = common && !stationary_alias;
    let entry_ahead = current_overlap || (ahead_at_overlap && paired_ahead) || front_entry_ahead;
    close_entry = close_entry && entry_ahead;
    let bracket_support = vision_brackets(point, cross, vision, frame.primary, frame.path)
        && history_s >= 1.
        && long.inward_progress >= PAIRED_OUTER_BODY_MIN_INWARD_PROGRESS_M
        && long.inward_rate >= 0.20
        && short_inward >= 0.20
        && long.consistency >= 0.85
        && short.consistency >= 0.75
        && long.net_fraction >= 0.65
        && (recent_abs_yaw_max < PAIRED_OUTER_BODY_RANGE_CHECK_MIN_ABS_YAW_RATE_RAD_S
            || paired_inward_supported)
        && point.v_rel <= -0.5
        && predicted_overlap
        && paired_ahead
        && cross
            .zip(overlap_time)
            .is_some_and(|(front, time)| front.d_rel + front.v_rel * time > 0.5);
    let bracket_entry = bracket_support && overlap_time.is_some_and(|time| time <= 1.90);
    let passing_before = point.corner()
        && cross_supported
        && point.d_rel <= 8.
        && overlap_time.is_some()
        && !entry_ahead;
    let strong_consistent =
        long.inward_progress >= 0.60 && (long.inward_rate - reported).abs() <= 0.30;
    let committed = long.inward_progress >= 0.60 && long.consistency >= 0.90;
    let weak_nonclosing = point.corner()
        && point.d_rel > 15.
        && -0.1 <= point.v_rel
        && point.v_rel <= 2.
        && !vision_supported
        && !current_overlap
        && projection.d_path.abs() > 2.30
        && !committed;
    let raw_cutin = common
        && !non_entry
        && !weak_nonclosing
        && (!point.corner()
            || point.d_rel <= 8.
            || !cross_supported
            || recent_abs_yaw_max < 0.020
            || vision_supported
            || current_overlap
            || paired_front_overlap
            || front_entry_ahead)
        && if point.source == "frontRadar" {
            front_entry
        } else {
            corner_entry || close_entry || direct_entry || bracket_support
        };
    let confirmation = if close_entry || body_entry || front_history {
        0.
    } else {
        maximum(
            0.,
            if current_overlap {
                CUTIN_CURRENT_OVERLAP_CONFIRMATION_S
            } else {
                CUTIN_CONFIRMATION_S
            } + 0.05 * f64::from(3 - sensitivity),
        )
    };
    let withdrawn = !raw_cutin
        && !current_overlap
        && !paired_front_overlap
        && !vision_supported
        && history_s >= 0.75
        && !recent_predicted_overlap
        && short_inward < maximum(MIN_INWARD_RATE_MPS, 0.5 * long.inward_rate)
        && reported < maximum(MIN_INWARD_RATE_MPS, 0.5 * long.inward_rate);
    let mut confirmed = state.latch(
        now,
        LatchInput {
            raw: raw_cutin,
            risk: false,
            confirmation,
            hold: CUTIN_HOLD_S,
        },
    );
    let reversed = !raw_cutin
        && !current_overlap
        && !predicted_overlap
        && !recent_predicted_overlap
        && short.inward_progress < 0.05
        && short.consistency < 0.35
        && !vision_supported
        && !cross_supported;
    if reversed
        || withdrawn
        || non_entry
        || curve_alias
        || !front_curve_motion
        || ambiguous
        || passing_before
        || stationary_alias
    {
        state.cutin_until_s = f64::NEG_INFINITY;
        confirmed = false;
    }
    if frame.vision_required_front
        && point.source == "frontRadar"
        && (recent_abs_yaw_max >= 0.020 || point.v_rel.abs() > 5.)
    {
        confirmed = false;
    }
    let closing_time = if point.v_rel < -0.1 {
        point.d_rel / -point.v_rel
    } else {
        f64::INFINITY
    };
    let relevant =
        point.v_rel >= 0.5 || point.d_rel <= 15. || closing_time <= 5. || strong_consistent;
    let control = confirmed
        && relevant
        && (!point.corner()
            || point.d_rel <= 8.
            || vision_supported
            || (long.consistency >= 0.90 && long.net_fraction >= 0.80))
        && if point.source == "frontRadar" {
            raw_body_overlap
        } else {
            current_overlap
                || close_entry
                || direct_entry
                || bracket_entry
                || overlap_time.is_some_and(|time| time <= 1.90 && commitment)
        };
    let raw_risk = common
        && !non_entry
        && front_motion
        && 2. < point.d_rel
        && point.d_rel <= 45.
        && point.v_rel <= -0.5
        && overlap_time.is_some_and(|time| time <= 3.)
        && (ahead_at_overlap || low_speed_entry)
        && (long.inward_progress >= 0.25 || bracket_support || (confirmed && control))
        && supported_inward >= 0.12
        && long.consistency >= 0.60
        && (point.source == "frontRadar"
            || reported >= 0.40
            || projection.d_path.abs() <= 2.30
            || long.inward_progress >= 0.60
            || point.d_rel <= 6.);
    let mut risk = state.latch(
        now,
        LatchInput {
            raw: raw_risk,
            risk: true,
            confirmation: maximum(0., RISK_CONFIRMATION_S + 0.05 * f64::from(3 - sensitivity)),
            hold: RISK_HOLD_S,
        },
    );
    if risk
        && (withdrawn
            || point.v_rel >= -0.1
            || overlap_time.is_none()
            || curve_alias
            || !front_curve_motion
            || ambiguous
            || stationary_alias)
    {
        state.risk_until_s = f64::NEG_INFINITY;
        risk = false;
    }
    let current_path = exists
        && history_s >= 0.25
        && projection.d_path.abs() <= 1.
        && point.v_lead > 2.
        && point.d_rel > 0.8;
    let support_score = maximum(f64::from(vision_supported), f64::from(cross_supported));
    let confidence = minimum(
        1.,
        maximum(
            0.,
            0.25 * minimum(history_s / 0.50, 1.)
                + 0.25 * minimum(long.inward_progress / 0.50, 1.)
                + 0.20 * long.consistency
                + 0.15 * minimum(supported_inward / 0.75, 1.)
                + 0.15 * support_score,
        ),
    );
    let reason = if confirmed {
        "confirmed trajectory CUT-IN"
    } else if risk {
        "trajectory pre-deceleration"
    } else if withdrawn {
        "entry withdrawn outside path"
    } else if passing_before {
        "side pass before path entry"
    } else if rear_pass {
        "close-born rear pass"
    } else if parallel {
        "parallel side drift"
    } else if stationary_alias {
        "stationary pair motion disagreement"
    } else if ambiguous {
        "ambiguous outer-body pair"
    } else if !close_front {
        "uncorroborated close front"
    } else if !front_motion {
        "front lateral uncertainty"
    } else if long.jittering {
        "corner lateral jitter"
    } else if current_path {
        "current path"
    } else {
        "tracking"
    };
    Ok(Estimate {
        point: point.clone(),
        continuity_id: state.continuity_id,
        d_path: projection.d_path,
        d_path_rate: long.rate,
        future_d_rel: future_distance,
        future_d_path: future_path,
        horizon_s,
        time_to_overlap_s: overlap_time,
        inward_rate: supported_inward,
        reported_inward_rate: reported,
        inward_progress: long.inward_progress,
        recent_inward_progress: short.inward_progress,
        lateral_travel: long.travel,
        lateral_net_fraction: long.net_fraction,
        direction_consistency: long.consistency,
        recent_direction_consistency: short.consistency,
        recent_v_rel_min,
        recent_v_rel_spread,
        recent_abs_yaw_max,
        history_s,
        confidence,
        vision_supported,
        cross_sensor_supported: cross_supported,
        cross_sensor_track_id: cross.map(|front| front.track_id),
        current_path,
        raw_cutin,
        confirmed_cutin: confirmed,
        control_eligible: control,
        predecel_risk: risk,
        jittering: long.jittering,
        unstable_fast_motion: unstable_fast,
        rear_pass,
        parallel_drift: parallel,
        front_history_supported: front_history,
        close_front_supported: close_front,
        curve_alias,
        reason,
        passing_before_overlap: passing_before,
        vision_bracket_supported: bracket_support,
        paired_inward_motion_supported: paired_inward_supported,
        entry_withdrawn: withdrawn,
        paired_body_entry: body_entry,
        stationary_pair_alias: stationary_alias,
    })
}
