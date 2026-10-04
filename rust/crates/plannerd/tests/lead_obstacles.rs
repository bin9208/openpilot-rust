use openpilot_plannerd::{
    lead::Lead,
    lead_obstacles::{apply_predecel, cutout_relief, predecel_limit, FollowGeometry},
};

fn lead() -> Lead {
    Lead {
        status: true,
        radar: true,
        radar_track_id: 12,
        d_rel: 40.,
        v_rel: 2.,
        v_lead: 22.,
        cut_out_time: 1.,
        cut_out_confidence: 0.8,
        ..Lead::default()
    }
}

#[test]
fn credit_preserves_every_obstacle_before_clearance() {
    // Given: a confirmed lead predicted to clear after one second plus the safety margin.
    let geometry = FollowGeometry {
        speed: 20.,
        follow_time: 1.45,
        stop_distance: 6.,
    };
    // When: the full obstacle horizon spans both sides of clearance.
    let relief = cutout_relief(&lead(), geometry, &[0., 1., 1.3, 1.55, 1.8, 3.]);
    // Then: no early obstacle moves, and the final credit remains bounded.
    assert_eq!(&relief[..3], &[0., 0., 0.]);
    assert_eq!(relief[3], 3.2);
    assert_eq!(&relief[4..], &[6.4, 6.4]);
}

#[test]
fn heavy_lead_braking_prevents_cutout_relief() {
    // Given: lateral departure evidence alongside significant lead braking.
    let mut lead = lead();
    lead.a_lead_k = -2.5001;
    // When: requesting relief throughout the horizon.
    let relief = cutout_relief(
        &lead,
        FollowGeometry {
            speed: 20.,
            follow_time: 1.45,
            stop_distance: 6.,
        },
        &[0., 1., 2., 3.],
    );
    // Then: the measured obstacle remains intact.
    assert_eq!(relief, [0.; 4]);
}

#[test]
fn strong_cutin_limits_acceleration_by_the_source_step() {
    // Given: a measured corner risk exactly at the activation score threshold.
    let lead = Lead {
        status: true,
        radar: true,
        score: 0.15,
        d_rel: 10.,
        v_rel: -10.,
        ..Lead::default()
    };
    // When: applying its bounded acceleration ceiling from zero acceleration.
    let output = apply_predecel(1.6, 0., predecel_limit(&lead));
    // Then: the per-plan step remains 0.15, not the full requested braking ceiling.
    assert_eq!(output, -0.15);
}
