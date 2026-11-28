use super::super::{
    broadcast::Broadcast,
    bus, clock,
    config::Config,
    parameters::{Write, Writes},
};
use crate::{
    curve::{curve_speed, CurveInput, VisionCurveSpeed},
    geos::Geos,
    owner::Owner,
    route::{RouteInput, RouteUpdate},
    serv::TickInput,
    Error,
};
use openpilot_messaging::runtime::{PubMaster, SubMaster};
use openpilot_params::Params;
use std::{
    sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    time::Duration,
};

/// The tick runs at 20 Hz; Settings (about 20 Params files), the curve factor
/// and IsOnroad were read from disk every tick. They are read once a second
/// (every PARAMS_EVERY ticks, the first tick included) and reused between.
const PARAMS_EVERY: u32 = 20;
static PARAMS_TICK: AtomicU32 = AtomicU32::new(0);
static CURVE_FACTOR_BITS: AtomicU64 = AtomicU64::new(0);
static ONROAD: AtomicBool = AtomicBool::new(false);
pub(super) struct Tick<'a> {
    pub owner: &'a mut Owner,
    pub sub: &'a mut SubMaster,
    pub publisher: &'a mut PubMaster,
    pub vision: &'a mut VisionCurveSpeed,
    pub geos: Option<&'a Geos>,
    pub params: &'a Params,
    pub writes: &'a Writes,
    pub config: &'a Config,
    pub network_connected: &'a AtomicBool,
    pub broadcast: &'a mut Broadcast,
    pub gps: &'a str,
}
pub(super) fn tick(context: Tick<'_>) -> Result<(), Error> {
    let Tick {
        owner,
        sub,
        publisher,
        vision,
        geos,
        params,
        writes,
        config,
        network_connected,
        broadcast,
        gps,
    } = context;
    sub.update(Duration::ZERO)?;
    let refresh = PARAMS_TICK
        .fetch_add(1, Ordering::Relaxed)
        .is_multiple_of(PARAMS_EVERY);
    if refresh {
        ONROAD.store(
            params.get_bool("IsOnroad").unwrap_or(false),
            Ordering::Relaxed,
        );
        CURVE_FACTOR_BITS.store(
            crate::serv::settings::Settings::curve_factor(params)?.to_bits(),
            Ordering::Relaxed,
        );
    }
    let onroad = ONROAD.load(Ordering::Relaxed);
    let inputs = bus::read(&sub.state, gps)?;
    let mono = clock::monotonic();
    network_connected.store(inputs.network_connected, Ordering::Relaxed);
    let navd_updated = inputs.navd_route.is_some();
    if let Some(route) = &inputs.navd_route {
        let coordinates = owner.route.send_routes(route, true)?;
        publisher.send(
            "navRoute",
            &crate::wire::route(&coordinates, clock::timestamp()?)?,
        )?;
    }
    if let Some(payload) = inputs.v2.clone() {
        owner.navigation.accept_v2(payload, mono);
    } else if !inputs.v2_alive {
        owner.navigation.v2_transport_lost(mono);
    }
    let projected = owner.navigation.select(mono).map_err(Error::Contract)?;
    let action = owner.serv.project(projected.clone());
    if let Some(action) = action {
        writes.send(Write::Traffic(action))?;
    }
    let route_update = projected.selection.snapshot.as_ref().map(|s| RouteUpdate {
        session_id: projected.session_id.clone(),
        sequence: i64::try_from(projected.sequences[4]).unwrap_or(i64::MAX),
        present: s.control.route_present,
        polyline: s.control.route_points.clone(),
        force: navd_updated,
        onroad,
    });
    if let Some(coordinates) = owner.route.update(route_update, navd_updated)? {
        publisher.send(
            "navRoute",
            &crate::wire::route(&coordinates, clock::timestamp()?)?,
        )?;
    }
    let factor = f64::from_bits(CURVE_FACTOR_BITS.load(Ordering::Relaxed));
    let curve = if inputs.model_usable {
        inputs.car.as_ref().and_then(|car| {
            curve_speed(
                &inputs.model,
                CurveInput {
                    v_ego: car.v_ego,
                    sensitivity: factor,
                    lower_limit_kph: owner.serv.curve_floor(mono),
                    speed_ratio: car.v_clu_ratio,
                    a_ego: car.a_ego,
                },
            )
        })
    } else {
        None
    };
    // In the target's units (cluster speed): see curve_speed's ratio.
    vision.ego_kph = inputs.car.as_ref().map(|car| {
        let ratio = if car.v_clu_ratio > 0.5 && car.v_clu_ratio <= 1.2 {
            car.v_clu_ratio
        } else {
            1.
        };
        car.v_ego * 3.6 / ratio
    });
    let vturn = vision.update_nanos(
        curve,
        mono,
        inputs.model_usable.then_some(inputs.model_stamp),
    );
    let preview = owner.route.preview(
        RouteInput {
            onroad,
            active_carrot: owner.serv.nav.active_carrot,
            position: (owner.serv.gps.longitude, owner.serv.gps.latitude),
            heading_deg: owner.serv.gps.bearing,
            road_limit: owner.serv.nav.road_limit,
            deceleration: owner.serv.settings.deceleration,
            v_ego: inputs.car.as_ref().map_or(0., |c| c.v_ego),
        },
        geos,
    )?;
    let paths = preview
        .points
        .iter()
        .zip(&preview.distances)
        .map(|(p, d)| format!("{:.2},{:.2},{d:.2}", p.0, p.1))
        .collect::<Vec<_>>()
        .join(";");
    if refresh {
        owner.serv.settings = crate::serv::settings::Settings::read(params)?;
        owner.serv.nav.atc_lanes = if owner.serv.settings.atc_lane_count {
            node_lanes(owner, mono)
        } else {
            None
        };
    }
    let decision = owner.serv.tick(TickInput {
        now: mono,
        car: inputs.car.clone(),
        selfdrive_alive: inputs.selfdrive_alive,
        distance_traveled: inputs.distance_traveled,
        vision_speed: vturn,
        route_speed: preview.speed,
        gps: inputs.gps.clone(),
        nav_instruction: inputs.instruction.clone(),
    });
    let remote = owner
        .peers
        .selected()
        .map_or(String::new(), |a| a.ip().to_string());
    publisher.send(
        "carrotMan",
        &crate::wire::carrot(&owner.serv, &decision, &remote, clock::timestamp()?, &paths)?,
    )?;
    publisher.send(
        "navInstructionCarrot",
        &crate::wire::instruction(
            &owner.serv,
            &decision,
            bus::fallback(&sub.state)?,
            clock::timestamp()?,
        )?,
    )?;
    broadcast.send(owner, &inputs, params, writes, config, mono)?;
    Ok::<_, Error>(())
}

/// Node-link lanes where the car is and 30 m before the next turn along the
/// route (0 when that point is unknown). The file is optional: without it
/// nothing changes; a missing file is looked for again every minute.
fn node_lanes(owner: &mut Owner, mono: f64) -> Option<(u8, u8)> {
    if owner.nodelink.is_none() && mono >= owner.nodelink_retry {
        owner.nodelink_retry = mono + 60.;
        owner.nodelink =
            crate::nodelink::NodeLinks::open(std::path::Path::new(crate::nodelink::PATH)).ok();
    }
    let links = owner.nodelink.as_mut()?;
    let gps = &owner.serv.gps;
    if gps.latitude.abs() < 1. || gps.longitude.abs() < 1. {
        return None;
    }
    let here = links
        .matched(gps.latitude, gps.longitude, gps.bearing, 20., 45.)?
        .lanes;
    let distance = owner.serv.nav.x_turn_distance;
    let before = if distance > 40. {
        owner
            .route
            .ahead((gps.longitude, gps.latitude), distance - 30.)
            .and_then(|((lon, lat), bearing)| links.matched(lat, lon, bearing, 25., 45.))
            .map_or(0, |l| l.lanes)
    } else {
        0
    };
    Some((here, before))
}
