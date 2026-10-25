//! Direct recipes from the two original fixture sources, with no oracle calls.
use super::{array, json, merge, object, parameters, real, wire, Result, J, V};
use serde_json::json as v;

fn patch(base: &str, updates: serde_json::Value) -> Result<J> {
    merge(&J::parse(base)?, &json(&updates)?)
}
fn rows(count: usize) -> Result<J> {
    array((0..count).map(|i| frame(i as f64)).collect::<Result<_>>()?)
}
fn make_case(
    name: &str,
    fp: &str,
    mode: &str,
    rows: J,
    settings: &str,
    changes: &str,
) -> Result<J> {
    parameters::case(
        name,
        fp,
        mode,
        rows,
        &J::parse(settings)?,
        &J::parse(changes)?,
    )
}
fn change(i: usize, updates: &J, omit: &[&str], settings: Option<&J>) -> Result<J> {
    wire::change(
        &frame(i as f64)?,
        updates,
        &omit.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
        settings,
    )
}

pub(super) fn frame(index: f64) -> Result<J> {
    if !index.is_finite() || index.fract() != 0. || !(-1_000_000. ..=1_000_000.).contains(&index) {
        return Err("fixture index outside bounded integer domain".into());
    }
    let time = 100.0 + index * 0.01;
    let speed = 15.0 + 3.0 * (index * 0.01).sin();
    let active = index >= 3.;
    let data = vec![
        ("onroadEvents", J::parse("[]")?),
        (
            "carState",
            patch(
                r#"{"vEgo":0.0,"aEgo":0.1,"steeringAngleDeg":0.0,"steeringRateDeg":0.2,"canValid":true,"gearShifter":"drive","latEnabled":true,"vCruise":80.0,"vCruiseCluster":80.0,"vCluRatio":1.02,"cruiseState":{"enabled":true,"standstill":false}}"#,
                v!({"vEgo":speed,"steeringAngleDeg":(index * 0.03).sin()}),
            )?,
        ),
        (
            "liveParameters",
            J::parse(
                r#"{"stiffnessFactor":1.0,"steerRatio":15.0,"angleOffsetDeg":0.2,"roll":0.01}"#,
            )?,
        ),
        (
            "liveTorqueParameters",
            J::parse(
                r#"{"useParams":true,"latAccelFactorFiltered":2.6,"latAccelOffsetFiltered":0.01,"frictionCoefficientFiltered":0.12}"#,
            )?,
        ),
        ("liveDelay", J::parse(r#"{"lateralDelay":0.2}"#)?),
        (
            "modelV2",
            object(vec![
                (
                    "action".into(),
                    json(&v!({"desiredCurvature":0.001 * (index * 0.02).sin()}))?,
                ),
                (
                    "orientation".into(),
                    J::parse(&format!(
                        r#"{{"x":{},"y":{}}}"#,
                        serde_json::to_string(&vec![0.01; 33])?,
                        serde_json::to_string(&vec![0.02; 33])?
                    ))?,
                ),
                (
                    "acceleration".into(),
                    json(&v!({"y":(0..33).map(|j|0.1 + j as f64 * 0.003).collect::<Vec<_>>()}))?,
                ),
                ("meta".into(), json(&v!({"desireState":vec![0.0;8]}))?),
            ])?,
        ),
        (
            "longitudinalPlan",
            patch(
                r#"{"aTarget":0.0,"vTargetNow":0.0,"jTargetNow":0.05,"speeds":[20.0,20.0,20.0,20.0,20.0,20.0,20.0,20.0,20.0,20.0,20.0,20.0,20.0,20.0,20.0,20.0,20.0],"hasLead":true,"cruiseTarget":80.0}"#,
                v!({"aTarget":0.2*(index*0.01).sin(),"vTargetNow":speed+0.2}),
            )?,
        ),
        (
            "lateralPlan",
            object(vec![
                ("useLaneLines".into(), json(&false)?),
                ("psis".into(), json(&vec![0.0; 17])?),
                ("curvatures".into(), json(&vec![0.0; 17])?),
                (
                    "distances".into(),
                    json(&(0..17).map(|j| j as f64).collect::<Vec<_>>())?,
                ),
            ])?,
        ),
        (
            "radarState",
            J::parse(
                r#"{"leadOne":{"status":true,"dRel":25.0,"vRel":-0.1,"radar":true,"dPath":0.1}}"#,
            )?,
        ),
        (
            "carOutput",
            J::parse(
                r#"{"actuatorsOutput":{"torque":0.0,"curvature":0.0,"steeringAngleDeg":0.0}}"#,
            )?,
        ),
        (
            "carrotMan",
            J::parse(r#"{"desiredSpeed":70.0,"vTurnSpeed":130.0,"activeCarrot":2}"#)?,
        ),
        (
            "driverMonitoringState",
            J::parse(r#"{"alertLevel":"none"}"#)?,
        ),
        (
            "driverAssistance",
            J::parse(r#"{"leftLaneDeparture":false,"rightLaneDeparture":true}"#)?,
        ),
        ("livePose", J::parse("{}")?),
        (
            "liveCalibration",
            J::parse(r#"{"rpyCalib":[0.01,-0.02,0.03],"calStatus":"calibrated"}"#)?,
        ),
        (
            "selfdriveState",
            object(vec![
                ("enabled".into(), json(&active)?),
                ("active".into(), json(&active)?),
                (
                    "state".into(),
                    J::text(if active { "enabled" } else { "disabled" }),
                ),
                ("personality".into(), J::text("standard")),
            ])?,
        ),
    ];
    let packets = data
        .into_iter()
        .map(|(name, data)| {
            let packet = if name == "livePose" {
                crate::paramsd_fixture_generate::pose(time, Default::default())
            } else {
                wire::event_packet(name, time, &data)?
            };
            json(&packet)
        })
        .collect::<Result<Vec<_>>>()?;
    object(vec![
        ("time".into(), real(time)?),
        ("messages".into(), array(packets)?),
    ])
}
pub(super) fn fixture_cases() -> Result<J> {
    let base_rows = rows(200)?;
    let mut out = Vec::new();
    for (fp, mode) in [
        ("HONDA_CIVIC", "pid"),
        ("TOYOTA_RAV4", "torque"),
        ("VOLKSWAGEN_ID4_MK1", "angle"),
        ("HYUNDAI_PALISADE", "torque"),
        ("GMC_ACADIA", "torque"),
        ("CHEVROLET_VOLT", "torque"),
    ] {
        let settings = if fp == "HYUNDAI_PALISADE" {
            r#"{"NNFF":"1"}"#
        } else if fp == "GMC_ACADIA" {
            r#"{"NNFFLite":"1"}"#
        } else {
            "{}"
        };
        // fixture.cases serializes parameters() directly; scenarios.case always
        // copies it through as_builder a second time. Keep that distinction.
        let settings = merge(&J::parse(parameters::SETTINGS)?, &J::parse(settings)?)?;
        let mut params = super::entries(&settings)?
            .into_iter()
            .map(|(k, v)| Ok((k, json(super::text(&v)?.as_bytes())?)))
            .collect::<Result<Vec<_>>>()?;
        params.push((
            "CarParams".into(),
            json(&parameters::parameters(fp, mode, &settings)?)?,
        ));
        out.push(object(vec![
            ("name".into(), J::text(&format!("{fp}-{mode}"))),
            ("params".into(), object(params)?),
            ("simulation".into(), json(&false)?),
            ("frames".into(), base_rows.clone()),
        ])?);
    }
    array(out)
}
pub(super) fn scenario_cases() -> Result<J> {
    let mut out = Vec::new();
    let family = rows(40)?;
    for (fp, mode) in [
        ("COMMA_BODY", "pid"),
        ("CHRYSLER_PACIFICA_2018", "pid"),
        ("FORD_ESCAPE_MK4", "angle"),
        ("MAZDA_3", "pid"),
        ("MOCK", "pid"),
        ("NISSAN_LEAF", "angle"),
        ("RIVIAN_R1_GEN1", "torque"),
        ("SUBARU_ASCENT", "pid"),
        ("TESLA_MODEL_3", "angle"),
    ] {
        out.push(make_case(
            &format!("family-{fp}"),
            fp,
            mode,
            family.clone(),
            "{}",
            "{}",
        )?);
    }
    let mut transitions = Vec::new();
    for i in 0..620 {
        let phase = i / 20;
        let speed = [
            0., 0.3, 0.300001, 1., 4.999, 5., 5.001, 10., 10.001, -1., 30.,
        ][phase % 11];
        let values = json(&v!({
            "carState":{"vEgo":speed,"standstill":i<40,"brakePressed":(60..80).contains(&i),"gasPressed":(100..120).contains(&i),"softHoldActive":i32::from((160..190).contains(&i)),"steeringPressed":(220..340).contains(&i),"steeringAngleDeg":if (220..340).contains(&i){46.}else{0.},"steerFaultTemporary":(420..430).contains(&i),"steerFaultPermanent":(430..440).contains(&i),"latEnabled":!(440..460).contains(&i),"gearShifter":if i<200{["neutral","park","reverse","unknown","drive"][(i/100)%5]}else{"drive"},"cruiseState":{"enabled":true,"standstill":(120..140).contains(&i)}},
            "longitudinalPlan":{"shouldStop":(120..210).contains(&i),"aTarget":if (120..200).contains(&i){-0.7}else{0.2}},
            "selfdriveState":{"enabled":(3..500).contains(&i),"active":(3..500).contains(&i),"state":if (400..420).contains(&i){"softDisabling"}else{"enabled"}},
            "modelV2":{"jetlink":{"lossLatched":(380..390).contains(&i),"source":if (390..400).contains(&i){"jetlink"}else{"native"}}}
        }))?;
        let settings = json(
            &v!({"StoppingAccel":(["-200","-50","nan","-70"][phase%4]),"SteerRatioRate":([29,30,100,200,201][phase%5]),"CustomSR":if phase.is_multiple_of(3){150}else{0},"AlwaysLateral":i32::from(i<560),"DisableDM":phase%2,"SpeedFromPCM":phase%4}),
        )?;
        transitions.push(change(i, &values, &[], Some(&settings))?);
    }
    let transitions = array(transitions)?;
    for (fp, mode) in [
        ("HONDA_CIVIC", "pid"),
        ("TOYOTA_RAV4", "torque"),
        ("HYUNDAI_PALISADE", "torque"),
        ("VOLKSWAGEN_ID4_MK1", "angle"),
        ("TESLA_MODEL_3", "angle"),
    ] {
        out.push(make_case(
            &format!("transitions-{fp}"),
            fp,
            mode,
            transitions.clone(),
            "{}",
            r#"{"startingState":true,"steerAtStandstill":true}"#,
        )?);
    }
    // Original resets Random(150) for each vehicle. Reuse those identical rows.
    let mut rng = crate::journald_check::Random::new(150);
    let mut neural = Vec::new();
    for i in 0..1000 {
        let mut uniform = |lower: f64, upper: f64| lower + (upper - lower) * rng.random();
        let acceleration = (0..33).map(|_| uniform(-3., 3.)).collect::<Vec<_>>();
        let steering = uniform(-3., 3.);
        let ego = uniform(-1., 1.);
        let orientation = (0..33).map(|_| uniform(-0.1, 0.1)).collect::<Vec<_>>();
        let desired = uniform(-0.015, 0.015);
        let values = json(
            &v!({"carState":{"steeringPressed":i%60<10,"steeringAngleDeg":steering,"aEgo":ego},"modelV2":{"orientation":{"x":orientation,"y":vec![0.03;33]},"acceleration":{"y":acceleration},"action":{"desiredCurvature":desired}}}),
        )?;
        let custom = match i {
            100 => Some(1),
            200 => Some(0),
            300 => Some(2),
            400 => Some(0),
            _ => None,
        };
        let settings=custom.map(|custom|json(&v!({"LateralTorqueCustom":custom,"LateralTorqueAccelFactor":2800,"LateralTorqueFriction":130,"LateralTorqueKpV":80,"LateralTorqueKiV":12,"LateralTorqueKf":100,"LateralTorqueKd":1}))).transpose()?;
        neural.push(change(i, &values, &[], settings.as_ref())?);
    }
    let neural = array(neural)?;
    for (fp, settings) in [
        ("HYUNDAI_PALISADE", r#"{"NNFF":"1"}"#),
        ("GMC_ACADIA", r#"{"NNFFLite":"1"}"#),
        ("CHEVROLET_VOLT", "{}"),
    ] {
        out.push(make_case(
            &format!("neural-history-{fp}"),
            fp,
            "torque",
            neural.clone(),
            settings,
            r#"{"useSteeringAngle":false}"#,
        )?);
    }
    let mut coasting = Vec::new();
    for i in 0..420 {
        let phase = i / 20;
        let values = json(
            &v!({"carState":{"vEgo":20.5,"vCruise":80.0,"brakePressed":phase==4,"gasPressed":phase==5,"carrotCruise":i32::from(phase==6),"cruiseState":{"standstill":phase==7}},"longitudinalPlan":{"aTarget":-0.7,"vTargetNow":20.0,"cruiseCoastingTarget":20.0,"cruiseCoastingPercent":5,"longitudinalPlanSource":if phase==8{"lead0"}else{"cruise"},"fcw":phase==9,"cruiseTarget":if phase==10{79.0}else{80.0}},"radarState":{"leadOne":{"status":phase==11},"leadTwo":{"status":phase==12},"leadCutInRisk":{"status":phase==13}}}),
        )?;
        coasting.push(change(
            i,
            &values,
            if phase == 14 {
                &["longitudinalPlan"]
            } else {
                &[]
            },
            None,
        )?);
    }
    out.push(make_case(
        "coasting-all-vetoes",
        "HONDA_CIVIC",
        "pid",
        array(coasting)?,
        "{}",
        "{}",
    )?);
    let mut lanes = Vec::new();
    for i in 0..260 {
        let phase = i / 20;
        let values = json(
            &v!({"carrotMan":{"desiredSpeed":([0.,-1.,250.,251.,60.][phase%5])},"lateralPlan":{"useLaneLines":true,"psis":(0..17).map(|j|0.001*j as f64).collect::<Vec<_>>(),"curvatures":vec![0.001;17],"distances":(0..17).map(|j|j as f64*0.5).collect::<Vec<_>>()},"modelV2":{"meta":{"laneChangeState":"laneChangeStarting","laneChangeDirection":if !phase.is_multiple_of(2){"left"}else{"right"},"desireState":[0.,if phase.is_multiple_of(4){0.2}else{0.},if phase%4==1{0.2}else{0.},if phase%4==2{0.2}else{0.},if phase%4==3{0.2}else{0.}]}}}),
        )?;
        let settings = json(
            &v!({"UseLaneLineCurveSpeed":0,"LatSmoothSec":if !phase.is_multiple_of(2){0}else{30}}),
        )?;
        lanes.push(change(
            i,
            &values,
            if i > 60 { &["carrotMan"] } else { &[] },
            Some(&settings),
        )?);
    }
    out.push(make_case(
        "lanes-carrot-stale",
        "HONDA_CIVIC",
        "pid",
        array(lanes)?,
        "{}",
        "{}",
    )?);
    let navigation = J::parse(
        r#"[{"vTurnSpeed":20.0,"xSpdType":1,"xSpdLimit":40},{"vTurnSpeed":-20.0,"xSpdType":22,"xSpdLimit":30},{"vTurnSpeed":130.0,"atcType":"turn left"},{"vTurnSpeed":130.0,"atcType":"fork right","nRoadLimitSpeed":60},{"vTurnSpeed":130.0,"xTurnInfo":5,"xDistToTurn":100},{"vTurnSpeed":130.0,"szSdiDescr":"Bottleneck point"},{"vTurnSpeed":130.0,"desiredSource":"road","desiredSpeed":60,"nRoadLimitSpeed":80},{"vTurnSpeed":130.0,"atcType":"prepare turn left"}]"#,
    )?;
    let V::Array(navigation) = navigation.view() else {
        unreachable!()
    };
    let mut nav = Vec::new();
    for i in 0..160 {
        nav.push(change(
            i,
            &object(vec![(
                "carrotMan".into(),
                navigation[(i / 20) % navigation.len()].clone(),
            )])?,
            &[],
            Some(&json(&v!({"SpeedFromPCM":(i/10)%4}))?),
        )?);
    }
    out.push(make_case(
        "meb-navigation-events",
        "VOLKSWAGEN_ID4_MK1",
        "angle",
        array(nav)?,
        "{}",
        "{}",
    )?);
    let mut nonfinite = Vec::new();
    for (i, value) in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, 0., 0.1]
        .into_iter()
        .enumerate()
    {
        let update = object(vec![(
            "longitudinalPlan".into(),
            object(vec![("jTargetNow".into(), real(value)?)])?,
        )])?;
        nonfinite.push(change(i, &update, &[], None)?);
    }
    out.push(make_case(
        "nonfinite-actuator-sanitation",
        "HONDA_CIVIC",
        "pid",
        array(nonfinite)?,
        "{}",
        "{}",
    )?);
    out.push(make_case(
        "zero-neural-jerk-time",
        "GMC_ACADIA",
        "torque",
        rows(30)?,
        r#"{"NNFFLite":"1","SteerActuatorDelay":"-30"}"#,
        "{}",
    )?);
    for camera in ["0", "1"] {
        out.push(make_case(
            &format!("hyundai-hda2-bus-{camera}"),
            "HYUNDAI_IONIQ_5",
            "torque",
            rows(35)?,
            &format!(r#"{{"HyundaiCameraSCC":"{camera}"}}"#),
            r#"{"flags":8193}"#,
        )?);
    }
    array(out)
}
