use crate::{
    native::{
        actor::{Action, Handle},
        config::Config,
    },
    serv::CarrotServ,
    Error,
};
use std::{collections::BTreeMap, net::UdpSocket, sync::atomic::Ordering, thread, time::Duration};

#[derive(Clone, Debug)]
enum Value {
    Integer(i64),
    Text(String),
}
#[derive(Clone, Debug)]
pub struct Kisa {
    values: BTreeMap<String, Value>,
}
fn parse(bytes: &[u8]) -> Kisa {
    let mut values = BTreeMap::new();
    if let Ok(text) = std::str::from_utf8(bytes) {
        for part in text.split('/') {
            if let Some((key, value)) = part.split_once(':') {
                let value = value
                    .trim()
                    .parse::<i64>()
                    .map(Value::Integer)
                    .unwrap_or_else(|_| Value::Text(value.into()));
                values.insert(key.into(), value);
            }
        }
    }
    Kisa { values }
}

pub fn apply_kisa(serv: &mut CarrotServ, data: Kisa) -> Result<(), Error> {
    serv.nav.active_kisa_count = 100;
    if let Some(value) = data.values.get("kisawazeroadspdlimit") {
        let Value::Integer(limit) = value else {
            return Err(Error::Contract("KISA road limit is not numeric"));
        };
        let limit = num_traits::ToPrimitive::to_f64(limit)
            .ok_or(Error::Contract("KISA speed conversion"))?;
        if limit > 0. {
            serv.nav.road_limit = if serv.settings.is_metric {
                limit
            } else {
                limit * 1.609344
            };
        }
    }
    if let Some(value) = data.values.get("kisawazeroadname") {
        serv.nav.road_name = match value {
            Value::Integer(value) => value.to_string(),
            Value::Text(value) => value.clone(),
        };
    }
    if let (Some(report), Some(distance)) = (
        data.values.get("kisawazereportid"),
        data.values.get("kisawazealertdist"),
    ) {
        let (Value::Text(report), Value::Text(distance)) = (report, distance) else {
            return Err(Error::Contract("KISA report and distance must be text"));
        };
        let distance = distance.to_lowercase();
        let digits: String = distance
            .chars()
            .skip_while(|c| !c.is_ascii_digit())
            .take_while(char::is_ascii_digit)
            .collect();
        let distance = digits.parse::<i64>().unwrap_or(0);
        let distance = num_traits::ToPrimitive::to_f64(&distance).unwrap_or(0.);
        let distance = if serv.settings.is_metric {
            distance
        } else {
            (distance * 0.3048).trunc()
        };
        let kind = if report.contains("camera") {
            101
        } else if report.contains("police") {
            100
        } else {
            -1
        };
        if kind >= 0 {
            serv.nav.speed_limit = if serv.nav.road_limit > 0. {
                serv.nav.road_limit * serv.settings.safety_factor
            } else {
                0.
            };
            serv.nav.speed_distance = distance;
            serv.nav.speed_type = kind;
        }
    }
    Ok(())
}

pub fn run(config: Config, handle: Handle) {
    while !handle.stop.load(Ordering::Relaxed) {
        let result = (|| {
            let socket = UdpSocket::bind((config.bind, config.kisa_port))?;
            socket.set_broadcast(true)?;
            socket.set_read_timeout(Some(Duration::from_secs(10)))?;
            while !handle.stop.load(Ordering::Relaxed) {
                let mut bytes = [0; 4096];
                match socket.recv_from(&mut bytes) {
                    Ok((0, _)) => break,
                    Ok((length, _)) => {
                        if let Err(error) = handle.call(Action::Kisa(parse(&bytes[..length]))) {
                            eprintln!("carrot_man KISA data: {error}");
                        }
                    }
                    Err(error)
                        if matches!(
                            error.kind(),
                            std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                        ) =>
                    {
                        thread::sleep(Duration::from_secs(1))
                    }
                    Err(error) => return Err(Error::Io(error)),
                }
            }
            Ok::<_, Error>(())
        })();
        if let Err(error) = result {
            eprintln!("carrot_man KISA retry: {error}");
            thread::sleep(Duration::from_secs(2));
        } else {
            thread::sleep(Duration::from_secs(1));
        }
    }
}
