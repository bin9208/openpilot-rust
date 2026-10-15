use openpilot_can::{Frame, Packet};
use openpilot_radarcan::{
    base::Base,
    databases::Databases,
    decoder::toyota::ars510::{interface::NativeInterface, receiver::Receiver},
    decoder::{Config, Interface, Kind},
    numerics::Numerics,
    Error,
};
use serde_json::{json, Value};
type RawPackets = Vec<(u64, Vec<(u32, Vec<u8>, u8)>)>;
fn fixture() -> &'static [u8] {
    if cfg!(target_arch = "aarch64") {
        include_bytes!("fixtures/ars510-receiver-final-arm.jsonl.zst")
    } else {
        include_bytes!("fixtures/ars510-receiver-final.jsonl.zst")
    }
}
fn number(v: &Value) -> f64 {
    v.as_f64().unwrap_or_else(|| match v.as_str().unwrap() {
        "NaN" => f64::NAN,
        "Infinity" => f64::INFINITY,
        "-Infinity" => f64::NEG_INFINITY,
        _ => panic!("number"),
    })
}
fn compare(a: &Value, b: &Value, path: &str) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(
            a.as_f64().unwrap().to_bits(),
            b.as_f64().unwrap().to_bits(),
            "{path}: {a} != {b}"
        ),
        (Value::Array(a), Value::Array(b)) => {
            assert_eq!(a.len(), b.len(), "{path}");
            for (i, (a, b)) in a.iter().zip(b).enumerate() {
                compare(a, b, &format!("{path}/{i}"));
            }
        }
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(
                a.keys().collect::<Vec<_>>(),
                b.keys().collect::<Vec<_>>(),
                "keys/{path}"
            );
            for (k, a) in a {
                compare(a, &b[k], &format!("{path}/{k}"));
            }
        }
        _ => assert_eq!(a, b, "{path}"),
    }
}
fn interface() -> Interface {
    let mut databases = Databases::new(std::path::PathBuf::from("/not-used-for-raw-ars510"));
    let config:Config=serde_json::from_value(json!({"candidate":"TOYOTA_RAV4_TSS2_2022","brand":"toyota","flags":64,"delay":0.2,"period":0.,"unavailable":true})).unwrap();
    let mut lines = vec![];
    let interface = Interface::new(config, &mut databases, &mut || 0, &mut |line: &str| {
        lines.push(line.to_owned())
    })
    .unwrap();
    assert!(lines.is_empty());
    assert!(interface.reader.is_none());
    assert!(interface.supports_display());
    interface
}
fn snapshot(receiver: &Receiver, b: &Base) -> Value {
    json!({"receiver":receiver,"diagnostics":receiver.diagnostics(b),"base":{"frame":b.frame,"v_ego_hist":b.v_ego_hist,"a_ego_hist":b.a_ego_hist,"v_ego":b.v_ego,"a_ego":b.a_ego,"last_timestamp":b.last_timestamp,"dt":b.dt,"init_samples":b.init_samples,"init_done":b.init_done,"pts":b.pts,"pts_order":b.pts.keys().collect::<Vec<_>>(),"tracks":b.tracks,"tracks_order":b.tracks.keys().collect::<Vec<_>>(),"history_maxlen":b.history_maxlen}})
}
#[test]
fn full_receiver_cold_fault_recovery_display_namespace_and_real_mytrack_states_match_source() {
    let dir = std::env::var("RADARCAN_NUMERICS_DIR")
        .expect("explicit verified ILP64 provider directory required");
    let mut numerics = Numerics::load(std::path::Path::new(&dir)).unwrap();
    let decoder = zstd::stream::read::Decoder::new(fixture()).unwrap();
    let rows = serde_json::Deserializer::from_reader(std::io::BufReader::new(decoder))
        .into_iter::<Value>();
    let mut interface = interface();
    let mut count = 0;
    for (index, row) in rows.enumerate() {
        let row = row.unwrap();
        let mut warnings = vec![];
        if row["op"] == "new" {
            interface = self::interface();
        } else {
            count += 1;
            let input = &row["input"];
            let op = input["op"].as_str().unwrap();
            let result = match op {
                "rebuild" => {
                    let old = interface;
                    interface = self::interface();
                    interface.recover_from_input_fault(&old).unwrap();
                    Ok(None)
                }
                "guard_generation" => {
                    let Kind::Ars510(receiver) = &mut interface.kind else {
                        panic!("ARS factory")
                    };
                    for model in [&mut receiver.decoder, &mut receiver.display_decoder] {
                        let mut state = serde_json::to_value(&*model).unwrap();
                        state["_guard_gen"]["1"] = input["value"].clone();
                        *model = serde_json::from_value::<NativeInterface>(state).unwrap();
                    }
                    Ok(None)
                }
                "update" | "update_carrot" => {
                    let raw: RawPackets = serde_json::from_value(input["packets"].clone()).unwrap();
                    let packets: Vec<_> = raw
                        .into_iter()
                        .map(|(mono_time, frames)| Packet {
                            mono_time,
                            frames: frames
                                .into_iter()
                                .map(|(address, data, bus)| Frame { address, data, bus })
                                .collect(),
                        })
                        .collect();
                    let mut emit = |line: &str| warnings.push(line.to_owned());
                    if op == "update" {
                        interface.update(&packets, &numerics)
                    } else {
                        interface.update_carrot(
                            number(&input["speed"]),
                            number(&input["accel"]),
                            number(&input["time"]),
                            &packets,
                            &mut numerics,
                            &mut emit,
                        )
                    }
                }
                _ => panic!("operation {op}"),
            };
            warnings.extend(interface.take_warnings());
            match result {
                Ok(value) => {
                    assert!(row["error"].is_null(), "expected error/{index}");
                    compare(
                        &serde_json::to_value(value).unwrap(),
                        &row["result"],
                        &format!("result/{index}"),
                    );
                }
                Err(error) => {
                    let kind = match error {
                        Error::ArsValue(_) => "ValueError",
                        Error::DivisionByZero => "ZeroDivisionError",
                        Error::PowerOverflow => "OverflowError",
                        _ => "NativeBoundaryError",
                    };
                    compare(
                        &json!({"kind":kind,"message":error.to_string()}),
                        &row["error"],
                        &format!("error/{index}"),
                    );
                }
            }
        }
        let Kind::Ars510(receiver) = &interface.kind else {
            panic!("ARS factory")
        };
        compare(
            &snapshot(receiver, &interface.base),
            &row["expected"],
            &format!("state/{index}"),
        );
        assert_eq!(json!(warnings), row["warnings"], "warnings/{index}");
    }
    assert_eq!(count, 2697);
}

#[test]
fn production_engine_preserves_display_support_and_recovery_namespace_on_batch_fault() {
    use openpilot_radarcan::{
        batch::Ego,
        data::Data,
        databases::Databases,
        decoder::Config,
        runtime::{Engine, Io, Reason},
    };
    struct Host {
        now: u64,
        numerics: Numerics,
        publications: Vec<Data>,
        errors: usize,
    }
    impl Io for Host {
        fn monotonic_ns(&mut self) -> u64 {
            self.now
        }
        fn create_interface(&mut self, c: &Config) -> Result<Interface, Error> {
            Interface::new(
                c.clone(),
                &mut Databases::new("/unused-ars510".into()),
                &mut || 0,
                &mut |_| {},
            )
        }
        fn track_flip(&mut self) -> Result<bool, Error> {
            Ok(true)
        }
        fn update(
            &mut self,
            s: &mut Interface,
            e: Ego,
            p: &[Packet],
        ) -> Result<Option<Data>, Error> {
            s.update_carrot(
                e.v_ego,
                e.a_ego,
                e.receive_ns as f64 * 1e-9,
                p,
                &mut self.numerics,
                &mut |_| {},
            )
        }
        fn publish(&mut self, d: Data, _valid: bool) -> Result<(), Error> {
            self.publications.push(d);
            Ok(())
        }
        fn input_error(&mut self, _reason: Reason) -> Result<(), Error> {
            self.errors += 1;
            Ok(())
        }
    }
    let decoder = zstd::stream::read::Decoder::new(fixture()).unwrap();
    let rows = serde_json::Deserializer::from_reader(std::io::BufReader::new(decoder))
        .into_iter::<Value>();
    let mut batches = vec![];
    for row in rows {
        let row = row.unwrap();
        if row["case"] == 1 && row.get("input").is_some() {
            let raw: RawPackets = serde_json::from_value(row["input"]["packets"].clone()).unwrap();
            let packets: Vec<_> = raw
                .into_iter()
                .map(|(mono_time, frames)| Packet {
                    mono_time,
                    frames: frames
                        .into_iter()
                        .map(|(address, data, bus)| Frame { address, data, bus })
                        .collect(),
                })
                .collect();
            batches.push(packets);
            if batches.len() == 2 {
                break;
            }
        }
    }
    let mut io = Host {
        now: 1_000_000_000,
        numerics: Numerics::load(std::path::Path::new(
            &std::env::var("RADARCAN_NUMERICS_DIR").unwrap(),
        ))
        .unwrap(),
        publications: vec![],
        errors: 0,
    };
    let config:Config=serde_json::from_value(json!({"candidate":"TOYOTA_RAV4_TSS2_2022","brand":"toyota","flags":64,"delay":0.2,"period":0.,"unavailable":true})).unwrap();
    let mut engine = Engine::new(config, &mut io, false).unwrap();
    let add = |engine: &mut Engine, io: &mut Host, p: Vec<Packet>| {
        let receive = p.last().unwrap().mono_time;
        io.now = receive;
        engine.add_state(Ego {
            first_can_ns: p.first().unwrap().mono_time,
            last_can_ns: receive,
            packet_count: p.len() as u32,
            receive_ns: receive,
            v_ego: 15.,
            a_ego: 0.,
        });
        engine.batches.add_can(p);
        engine.process(io).unwrap();
    };
    add(&mut engine, &mut io, batches.remove(0));
    let first = io.publications.last().unwrap();
    assert!(first.has_display_data);
    assert!(!first.points.is_empty());
    assert!(!first.display_points.is_empty());
    let old_id = first.points[0].track_id;
    assert!(first.radar_track_flipped);
    io.now = 1_040_000_000;
    engine.add_state(Ego {
        first_can_ns: 0,
        last_can_ns: 0,
        packet_count: 0,
        receive_ns: 0,
        v_ego: 15.,
        a_ego: 0.,
    });
    engine.process(&mut io).unwrap();
    let failure = io.publications.last().unwrap();
    assert!(failure.errors.can_error);
    assert!(failure.has_display_data);
    assert!(failure.points.is_empty());
    assert!(failure.display_points.is_empty());
    assert!(failure.radar_track_flipped);
    assert!(engine.needs_reset);
    add(&mut engine, &mut io, batches.remove(0));
    let recovered = io.publications.last().unwrap();
    assert!(!recovered.errors.any());
    assert!(recovered.has_display_data);
    assert!(recovered.points[0].track_id > old_id);
    assert!(recovered.display_points[0].track_id > old_id);
    assert!(!engine.needs_reset);
    assert_eq!(io.errors, 1);
}
