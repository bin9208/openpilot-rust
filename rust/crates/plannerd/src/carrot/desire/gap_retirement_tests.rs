//! 원본 SHA3bee725a의 adapter/TF assertion을 실제 CarrotPlanner 호출로 보존한다.
use super::{CarrotPlanner, Input};
use crate::{
    car_state::CarState,
    lead::Lead,
    model::{Model, ModelMeta},
    parameters::Parameters,
    radar::Radar,
    types::Personality,
    Error,
};
use openpilot_cereal::log_capnp::LaneChangeState;

struct StartupParameters;

impl Parameters for StartupParameters {
    fn integer(&mut self, key: &'static str) -> Result<i32, Error> {
        match key {
            "MyDrivingMode" => Ok(3),
            "MyDrivingModeAuto" => Ok(0),
            _ => Err(Error::Contract(
                "unexpected gap regression startup parameter",
            )),
        }
    }

    fn float(&mut self, _key: &'static str) -> Result<f64, Error> {
        Err(Error::Contract("unexpected gap regression float parameter"))
    }
}

#[test]
fn selected_pair_adapter_ignores_unselected_radar_lead() {
    // 원본은 leadOne/leadTwo만 있고 side 속성이 없는 SubMaster를 쓴다.
    // Rust에서는 같은 빈 selected pair와 유효한 비선택 lead를 각각 전달해 오독을 검출한다.
    for unselected in [
        Lead::default(),
        Lead {
            status: true,
            radar: true,
            radar_track_id: 99,
            d_rel: 35.,
            v_lead: 15.,
            ..Lead::default()
        },
    ] {
        let mut planner = CarrotPlanner::new(&mut StartupParameters).unwrap();
        let car = CarState {
            right_blinker: true,
            v_ego: 26.,
            ..CarState::default()
        };
        let model = Model {
            meta: ModelMeta {
                lane_change_state: LaneChangeState::LaneChangeFinishing,
                ..ModelMeta::default()
            },
            ..Model::default()
        };
        let radar = Radar {
            lead_cut_in_risk: unselected,
            ..Radar::default()
        };
        let input = Input {
            car: &car,
            model: &model,
            radar: &radar,
            personality: Personality::Standard,
            navigation: None,
            navigation_receive_time: None,
            mode_checks: true,
            lane_valid: true,
            model_ns: 1_000_000_000,
            radar_ns: 1_000_000_000,
            pose_ns: 1_000_000_000,
            pose_valid: true,
            yaw_rate: 0.01,
        };

        planner.update_desire(&input).unwrap();

        assert!(planner.lane_change_active);
        assert!(planner.lane_change_gap.targets.is_empty());
        assert_eq!(planner.lane_change_gap.entry_ids, [-1, -1]);
    }
}

#[test]
fn four_original_lane_change_cases_keep_common_following_time_for_100_updates() {
    // 원본4×100회 get_T_FOLLOW 회귀의 base=1.3, mode_factor=1,
    // speed/감속 boost 없음, t_follow_last=1.3을 실제 following_time에 적용한다.
    for (changing, ratio) in [(true, 0.2), (true, 0.8), (true, 1.), (false, 1.)] {
        let mut planner = CarrotPlanner::new(&mut StartupParameters).unwrap();
        planner.config.gaps.gaps[Personality::Standard.index()] = 1.3;
        planner.config.gaps.speed_factor = 10;
        planner.config.gaps.decel_boost = 0.;
        planner.follow_factor = 1.;
        planner.following.value = 1.3;
        planner.following.decel_extra = 0.;
        planner.following.jerk_factor = 0.7;
        planner.lane_change_active = changing;
        planner.config.lane_change_ratio = ratio;
        planner.lead_response = 0;
        planner.desire_state = 1.;
        planner.desire_state_count = 1;

        for iteration in 0..100 {
            let tf = planner
                .following_time(Personality::Standard, 0., 0.)
                .unwrap();
            assert_eq!(
                tf, 1.3,
                "changing={changing} ratio={ratio} iteration={iteration}"
            );
            assert_eq!(
                planner.following.value, 1.3,
                "t_follow_last: changing={changing} ratio={ratio} iteration={iteration}"
            );
        }
    }
}
