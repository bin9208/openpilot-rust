use openpilot_carrot_man::{
    curve::{curve_speed, CurveInput, CurveSpeed, ModelPath, VisionCurveSpeed},
    geometry,
    geos::Geos,
    navigation::{parse_legacy, v2, NavigationRuntime},
    route::{RouteInput, RouteState, RouteUpdate},
    serv::{settings::Settings, CarrotServ, TickInput, TrafficAction},
    sources::{
        safety::{choose_safety, SafetyPolicy},
        Snapshot, Source, SourceStore,
    },
    wire,
};
use serde::Deserialize;
use serde_json::json;
use std::io::{self, BufRead};

#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
enum Request {
    Curve {
        model: ModelPath,
        input: CurveInput,
        now: f64,
        model_time: Option<f64>,
    },
    CurveUpdate {
        result: Option<CurveSpeed>,
        now: f64,
        model_time: Option<f64>,
    },
    Geometry {
        start: i64,
        points: Vec<geometry::Point>,
        position: geometry::Point,
        distance: f64,
        heading: f64,
    },
    Route {
        update: Option<RouteUpdate>,
        input: RouteInput,
        geos: bool,
    },
    Sources {
        snapshot: Option<Snapshot>,
        now: f64,
        loss: Option<(Source, String)>,
        policy: SafetyPolicy,
        hda: (f64, f64),
    },
    Configure {
        root: String,
        values: std::collections::BTreeMap<String, String>,
    },
    Tick {
        input: TickInput,
        snapshot: Option<Snapshot>,
        v2: Option<v2::Payload>,
        legacy: Option<serde_json::Value>,
        refresh: bool,
    },
    Ingress {
        bytes: Vec<u8>,
        now: f64,
        strict: bool,
    },
    TimeSet {
        epoch: i64,
        timezone: String,
        path: String,
        now_millis: i64,
        outcomes: Vec<bool>,
    },
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut vision = VisionCurveSpeed::default();
    let mut route = RouteState {
        sequence: -1,
        ..RouteState::default()
    };
    let mut sources = SourceStore::default();
    let mut navigation = NavigationRuntime::default();
    let mut serv = None;
    let mut params: Option<openpilot_params::Params> = None;
    let mut memory = None;
    let geos = std::env::var_os("CARROT_GEOS_LIBRARY")
        .map(|p| Geos::open(std::path::Path::new(&p)))
        .transpose()?;
    for line in io::stdin().lock().lines() {
        let request: Request = serde_json::from_str(&line?)?;
        let output = match request {
            Request::TimeSet {
                epoch,
                timezone,
                path,
                now_millis,
                outcomes,
            } => {
                let p = params.as_ref().ok_or("Params are required")?;
                let _ = p.remove("TimezoneName");
                let _ = p.remove("TimezoneSource");
                let mut commands = Vec::new();
                let mut outcomes = outcomes.into_iter();
                openpilot_carrot_man::native::time_set::apply(
                    epoch,
                    &timezone,
                    p,
                    std::path::Path::new(&path),
                    now_millis,
                    |name, args| {
                        commands.push(json!({"name":name,"args":args}));
                        Ok(outcomes.next().unwrap_or(true))
                    },
                )?;
                json!({"commands":commands,"timezone":p.get("TimezoneName")?.map(|b|String::from_utf8_lossy(&b).into_owned()),"source":p.get("TimezoneSource")?.map(|b|String::from_utf8_lossy(&b).into_owned())})
            }
            Request::Curve {
                model,
                input,
                now,
                model_time,
            } => {
                let result = curve_speed(&model, input);
                let speed = vision.update(result, now, model_time);
                json!({"result": result, "speed": speed, "state": vision})
            }
            Request::CurveUpdate {
                result,
                now,
                model_time,
            } => {
                let speed = vision.update(result, now, model_time);
                json!({"speed": speed, "state": vision})
            }
            Request::Geometry {
                start,
                points,
                position,
                distance,
                heading,
            } => {
                let path = geometry::path_after_distance(start, &points, position, distance);
                let relative = path
                    .closest
                    .map(|reference| geometry::relative_xy(&path.points, reference, heading));
                json!({"path": path, "relative": relative})
            }
            Request::Route {
                update,
                input,
                geos: active_geos,
            } => {
                let force = update.as_ref().is_some_and(|u| u.force);
                let coordinates = route.update(update, force)?;
                let output =
                    route.preview(input, if active_geos { geos.as_ref() } else { None })?;
                json!({"coordinates": coordinates, "output": output, "state": route})
            }
            Request::Sources {
                snapshot,
                now,
                loss,
                policy,
                hda,
            } => {
                let accepted = snapshot.map(|s| sources.accept(s, now));
                let lost = loss
                    .map(|(source, session)| sources.record_transport_loss(source, &session, now));
                let selection = sources.select(now)?;
                let safety = choose_safety(&selection, policy, hda);
                json!({"accepted": accepted, "lost": lost, "selection": selection, "safety": safety})
            }
            Request::Configure { root, values } => {
                let p = openpilot_params::Params::open(std::path::Path::new(&root), "d")?;
                for (key, value) in values {
                    p.put(&key, value.as_bytes())?;
                }
                let settings = Settings::read(&p)?;
                memory = Some(openpilot_params::Params::open(
                    &std::path::Path::new(&root).join("memory"),
                    "d",
                )?);
                serv = Some(CarrotServ::new(settings));
                params = Some(p);
                json!({"configured": true})
            }
            Request::Tick {
                input,
                snapshot,
                v2,
                legacy,
                refresh,
            } => {
                let serv = serv.as_mut().ok_or("Configure is required")?;
                let p = params.as_ref().ok_or("Params are required")?;
                let mem = memory.as_ref().ok_or("memory Params are required")?;
                if refresh {
                    serv.settings = Settings::read(p)?;
                }
                if let Some(snapshot) = snapshot {
                    navigation.accept(snapshot);
                }
                if let Some(v2) = v2 {
                    navigation.accept_v2(v2, input.now);
                }
                if let Some(legacy) = legacy {
                    if let Some(fields) = parse_legacy(&legacy, input.now) {
                        navigation.accept_legacy(fields, "tmap-test", input.now);
                    }
                }
                let projected = navigation.select(input.now)?;
                let action = serv.project(projected);
                match action {
                    Some(TrafficAction::Put { distance, lamp, remain, source, ts }) => mem.put("TrafficLight", serde_json::to_string(&json!({"distance":distance,"lamp":lamp,"remain":remain,"source":source,"ts":ts}))?.as_bytes())?,
                    Some(TrafficAction::Remove) => { if let Err(error) = mem.remove("TrafficLight") { if !matches!(error, openpilot_params::Error::Io(ref e) if e.kind() == std::io::ErrorKind::NotFound) { return Err(error.into()); } } }
                    None => {}
                }
                let timestamp =
                    num_traits::ToPrimitive::to_u64(&(input.now * 1e9)).ok_or("timestamp range")?;
                let decision = serv.tick(input);
                let carrot = wire::carrot(serv, &decision, "127.0.0.1", timestamp, "")?;
                let instruction = wire::instruction(serv, &decision, None, timestamp)?;
                let traffic = mem
                    .get("TrafficLight")?
                    .map(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes))
                    .transpose()?;
                json!({"carrot": carrot, "instruction": instruction, "traffic": traffic, "decision": decision, "state": serv})
            }
            Request::Ingress { bytes, now, strict } => {
                match openpilot_carrot_man::ingress::json::parse(&bytes, strict) {
                    Ok(value) if value.get("schema").is_some() => {
                        match openpilot_carrot_man::ingress::naver::parse(&value, now) {
                            Ok(snapshot) => json!({"snapshot":snapshot}),
                            Err(error) => json!({"error":error.0}),
                        }
                    }
                    Ok(value) => {
                        json!({"value":serde_json::from_str::<serde_json::Value>(&value.to_json()?).ok()})
                    }
                    Err(error) => json!({"error":error.0}),
                }
            }
        };
        println!("{}", serde_json::to_string(&output)?);
    }
    Ok(())
}
