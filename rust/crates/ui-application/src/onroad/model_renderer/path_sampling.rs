use super::{
    carrot::Carrot,
    math::{clip, integer},
};
use crate::Error;
pub fn geometric_distances(maximum: f64) -> Vec<f64> {
    let mut distances = Vec::new();
    let mut distance = 2.;
    while distance < maximum {
        distances.push(distance);
        distance += distance * 0.15;
    }
    distances.push(maximum);
    distances
}
pub fn animated_distances(
    carrot: &mut Carrot,
    speed: f64,
    maximum: f64,
) -> Result<Vec<f64>, Error> {
    let kph = speed * 3.6;
    let mut dt = (kph * 0.01).min(if carrot.mode >= 10 { 0.6 } else { 1. });
    if kph < 1. {
        carrot.position_t = 4.;
    } else if dt < 0.2 {
        dt = 0.2;
    }
    carrot.position_t += dt;
    if carrot.position_t > 24. {
        carrot.position_t -= 24.;
    }
    let mut times = vec![carrot.position_t];
    let intervals = match carrot.mode {
        9 => vec![3., 10., 3.],
        10 => vec![3.; 7],
        11 => vec![3.; 5],
        12 => vec![
            3.;
            usize::try_from(integer(clip(kph * 0.058 - 0.5, 0., 7.))?)
                .map_err(|_| Error::Contract("path interval count"))?
        ],
        _ => Vec::new(),
    };
    for interval in intervals {
        let t = times[times.len() - 1] + interval;
        times.push(if t > 24. { t - 24. } else { t });
    }
    let mut next = times
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.total_cmp(b.1))
        .map_or(0, |(i, _)| i);
    let distance = |t: f64| {
        let d = 3. * 1.2_f64.powf(t);
        if d >= maximum {
            maximum
        } else {
            d
        }
    };
    let mut distances = Vec::new();
    let mut exit = false;
    let mut i = 0;
    while i <= times.len() && !exit {
        let t = times[next];
        next = (next + 1) % times.len();
        if t < 3. {
            i += 1;
            continue;
        }
        if distance(t) == maximum {
            exit = true;
        }
        for j in (0..3).rev() {
            distances.push(if exit {
                distance(100.)
            } else {
                distance(t - f64::from(j))
            });
            if exit {
                break;
            }
        }
        i += 1;
    }
    Ok(distances)
}
